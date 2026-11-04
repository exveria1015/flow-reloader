use crate::runner::GenerationOwner;
use crate::{
    AttachedFutureTask, HandlerApi, HandlerHeader, HandlerRequest, HostResources, ResourceId,
    RunStatus, RunnerError, ServiceError, ServiceKey, HANDLER_ABI_VERSION,
};
use interface::services::HandlerApiFn;
use std::cell::{Cell, RefCell};
use std::fmt;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

mod control;
mod limits;
mod restart;
pub use control::Checkpoint;
use control::Control;
pub use limits::{GenerationBudget, RouterOptions};
use restart::Worker;
pub use restart::{Restart, WorkerId};

static NEXT_ROUTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub enum RouteError {
    Load(RunnerError),
    Abi {
        version: u32,
        size: u32,
    },
    InvalidApi,
    ContractMismatch {
        expected: ServiceKey,
        found: ServiceKey,
    },
    Resource(ServiceError),
    WrongRouter,
    StaleCandidate,
    Paused,
    InvalidTask,
    GenerationExhausted,
    GenerationLimit {
        limit: usize,
    },
    InvalidLimits,
    StateSchemaMismatch,
    CheckpointTooLarge,
    WorkerExists(WorkerId),
    WorkerFailed(WorkerId),
    Migration {
        worker: WorkerId,
        error: ServiceError,
    },
    RestartInProgress,
}

impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(error) => write!(f, "handler load failed: {error}"),
            Self::Abi { version, size } => {
                write!(f, "handler ABI mismatch: version {version}, size {size}")
            }
            Self::InvalidApi => write!(f, "invalid handler API or service contract"),
            Self::ContractMismatch { expected, found } => write!(
                f,
                "service contract mismatch: expected {expected:?}, found {found:?}"
            ),
            Self::Resource(error) => write!(f, "request resource unavailable: {error}"),
            Self::WrongRouter => write!(f, "candidate belongs to another router"),
            Self::StaleCandidate => {
                write!(f, "candidate was prepared for an older active generation")
            }
            Self::Paused => write!(f, "request admission is paused"),
            Self::InvalidTask => write!(f, "handler returned an invalid task"),
            Self::GenerationExhausted => write!(f, "generation or request count exhausted"),
            Self::GenerationLimit { limit } => {
                write!(f, "retained generation budget reached ({limit})")
            }
            Self::InvalidLimits => write!(f, "generation and checkpoint limits must be positive"),
            Self::StateSchemaMismatch => {
                write!(f, "checkpoint schema mismatch; state migration required")
            }
            Self::CheckpointTooLarge => write!(f, "checkpoint exceeds configured byte limit"),
            Self::WorkerExists(id) => write!(f, "worker {} already registered", id.0),
            Self::WorkerFailed(id) => write!(f, "worker {} did not stop successfully", id.0),
            Self::Migration { worker, error } => {
                write!(f, "worker {} migration failed: {error}", worker.0)
            }
            Self::RestartInProgress => write!(f, "restart is already in progress"),
        }
    }
}
impl std::error::Error for RouteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Load(error) => Some(error),
            Self::Resource(error) => Some(error),
            _ => None,
        }
    }
}

struct Generation {
    id: u64,
    api: HandlerApi,
    in_flight: Cell<usize>,
    image: Option<GenerationOwner>,
    controls: RefCell<Vec<Weak<Control>>>,
    completion: Rc<RefCell<Option<std::task::Waker>>>,
}

struct AdmissionCount {
    generation: Rc<Generation>,
    armed: bool,
}
impl Drop for AdmissionCount {
    fn drop(&mut self) {
        if self.armed {
            self.generation
                .in_flight
                .set(self.generation.in_flight.get() - 1);
        }
    }
}

/// An inert candidate. Preparation does not start request handlers or alter
/// routing. Native module initialization must itself obey the unsafe load contract.
pub struct PreparedHandler {
    router: u64,
    base: u64,
    api: HandlerApi,
    image: Option<GenerationOwner>,
}

impl PreparedHandler {
    pub fn based_on(&self) -> u64 {
        self.base
    }
    pub fn service(&self) -> ServiceKey {
        self.api.service
    }
}

/// Thread-bound message admission. Activation and dispatch are serialized by
/// Rust borrows; already admitted tasks stay bound to their original generation.
pub struct RequestRouter {
    id: u64,
    resources: HostResources,
    active: Rc<Generation>,
    retired: Vec<Weak<Generation>>,
    accepting: bool,
    options: RouterOptions,
    workers: Vec<Worker>,
    restarting: bool,
}

impl RequestRouter {
    /// # Safety
    /// The module must be trusted, including initializers/finalizers. Its API
    /// must obey HandlerRequest's copy/retain contract and the owned FFI future
    /// protocol. All calls, polling and destruction occur on this thread.
    pub unsafe fn load(
        path: impl AsRef<Path>,
        resources: HostResources,
    ) -> Result<Self, RouteError> {
        Self::load_with_options(path, resources, RouterOptions::default())
    }

    /// # Safety
    /// Every callback and any escaped Waker must remain valid independently of
    /// the router (normally statically linked code). The same task and thread
    /// contracts as load apply; no DLL lease is added for this extension point.
    pub unsafe fn from_api(api: HandlerApi, resources: HostResources) -> Result<Self, RouteError> {
        Self::from_api_with_options(api, resources, RouterOptions::default())
    }

    /// # Safety
    /// Same native code and lifetime contract as load. The shared budget also
    /// counts rejected loads and can be reused across router reconstruction.
    pub unsafe fn load_with_options(
        path: impl AsRef<Path>,
        resources: HostResources,
        options: RouterOptions,
    ) -> Result<Self, RouteError> {
        if options.max_checkpoint_bytes == 0 {
            return Err(RouteError::InvalidLimits);
        }
        options.generations.reserve()?;
        let (api, image) = load(path.as_ref(), 1)?;
        Self::create(api, Some(image), resources, options)
    }

    /// # Safety
    /// Same static callback lifetime contract as from_api.
    pub unsafe fn from_api_with_options(
        api: HandlerApi,
        resources: HostResources,
        options: RouterOptions,
    ) -> Result<Self, RouteError> {
        validate(api)?;
        if options.max_checkpoint_bytes == 0 {
            return Err(RouteError::InvalidLimits);
        }
        options.generations.reserve()?;
        Self::create(api, None, resources, options)
    }

    fn create(
        api: HandlerApi,
        image: Option<GenerationOwner>,
        resources: HostResources,
        options: RouterOptions,
    ) -> Result<Self, RouteError> {
        validate(api)?;
        let id = NEXT_ROUTER
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| RouteError::GenerationExhausted)?;
        Ok(Self {
            id,
            resources,
            active: Rc::new(Generation {
                id: 1,
                api,
                image,
                in_flight: Cell::new(0),
                controls: RefCell::new(Vec::new()),
                completion: Rc::new(RefCell::new(None)),
            }),
            retired: Vec::new(),
            accepting: true,
            options,
            workers: Vec::new(),
            restarting: false,
        })
    }

    /// Loads and validates an inert candidate; old requests and admission continue.
    ///
    /// # Safety
    /// The candidate must obey load's native code, initialization and ABI contracts.
    pub unsafe fn prepare(&self, path: impl AsRef<Path>) -> Result<PreparedHandler, RouteError> {
        let next = self
            .active
            .id
            .checked_add(1)
            .ok_or(RouteError::GenerationExhausted)?;
        self.options.generations.reserve()?;
        let (api, image) = load(path.as_ref(), next)?;
        self.candidate(api, Some(image))
    }

    /// # Safety
    /// The statically supplied callbacks must obey from_api's lifetime contract.
    pub unsafe fn prepare_api(&self, api: HandlerApi) -> Result<PreparedHandler, RouteError> {
        validate(api)?;
        self.check_service(api.service)?;
        self.options.generations.reserve()?;
        self.candidate(api, None)
    }

    fn candidate(
        &self,
        api: HandlerApi,
        image: Option<GenerationOwner>,
    ) -> Result<PreparedHandler, RouteError> {
        validate(api)?;
        self.check_service(api.service)?;
        Ok(PreparedHandler {
            router: self.id,
            base: self.active.id,
            api,
            image,
        })
    }

    /// Drain cutover: only requests admitted after this call use the new API.
    /// Paused admission stays paused. A stale/wrong-router candidate is rejected.
    pub fn activate(&mut self, candidate: PreparedHandler) -> Result<u64, RouteError> {
        if self.restarting {
            return Err(RouteError::RestartInProgress);
        }
        if candidate.api.state_schema != self.active.api.state_schema {
            return Err(RouteError::StateSchemaMismatch);
        }
        self.commit(candidate)
    }

    fn commit(&mut self, candidate: PreparedHandler) -> Result<u64, RouteError> {
        if candidate.router != self.id {
            return Err(RouteError::WrongRouter);
        }
        if candidate.base != self.active.id {
            return Err(RouteError::StaleCandidate);
        }
        self.check_service(candidate.api.service)?;
        let id = self
            .active
            .id
            .checked_add(1)
            .ok_or(RouteError::GenerationExhausted)?;
        let next = Rc::new(Generation {
            id,
            api: candidate.api,
            image: candidate.image,
            in_flight: Cell::new(0),
            controls: RefCell::new(Vec::new()),
            completion: self.active.completion.clone(),
        });
        self.retired
            .retain(|generation| generation.strong_count() != 0);
        self.retired.push(Rc::downgrade(&self.active));
        self.active = next;
        Ok(id)
    }

    pub fn dispatch(&self, resource: ResourceId, input: &[u8]) -> Result<RequestTask, RouteError> {
        if !self.accepting {
            return Err(RouteError::Paused);
        }
        self.spawn(&self.active, resource, input, None)
    }

    fn spawn(
        &self,
        generation: &Rc<Generation>,
        resource: ResourceId,
        input: &[u8],
        checkpoint: Option<Checkpoint>,
    ) -> Result<RequestTask, RouteError> {
        let service = self
            .resources
            .service(resource)
            .map_err(RouteError::Resource)?;
        if service != generation.api.service {
            return Err(RouteError::ContractMismatch {
                expected: generation.api.service,
                found: service,
            });
        }
        let count = generation
            .in_flight
            .get()
            .checked_add(1)
            .ok_or(RouteError::GenerationExhausted)?;
        let services = self.resources.services();
        let control = Control::new(
            generation.api.state_schema,
            checkpoint.clone(),
            self.options.max_checkpoint_bytes,
        );
        let (state_schema, state) = checkpoint
            .as_ref()
            .map(|state| (state.schema, state.bytes.as_slice()))
            .unwrap_or((ServiceKey::new(0, 0), &[]));
        let request = HandlerRequest {
            generation: generation.id,
            resource,
            service,
            services: services.as_raw(),
            input: input.as_ptr(),
            input_len: input.len(),
            control: control.raw(),
            state_schema,
            state: state.as_ptr(),
            state_len: state.len(),
        };
        generation.in_flight.set(count);
        let mut admission = AdmissionCount {
            generation: generation.clone(),
            armed: true,
        };
        // The validated native contract requires copying input and retaining
        // services synchronously. Neither stack descriptor escapes this call.
        let raw = unsafe { (generation.api.make_request)(&request) };
        let lease = generation.image.as_ref().map(GenerationOwner::lease);
        let task = unsafe { AttachedFutureTask::with_optional_keep_alive(raw, lease) }
            .ok_or(RouteError::InvalidTask)?;
        admission.armed = false;
        let mut controls = generation.controls.borrow_mut();
        controls.retain(|control| control.strong_count() != 0);
        controls.push(Rc::downgrade(&control));
        Ok(RequestTask {
            task: Some(task),
            generation: generation.clone(),
            resource,
            status: None,
            control,
        })
    }

    pub fn options(&self) -> &RouterOptions {
        &self.options
    }

    pub fn pause(&mut self) {
        self.accepting = false;
    }
    pub fn resume(&mut self) -> Result<(), RouteError> {
        if self.restarting {
            return Err(RouteError::RestartInProgress);
        }
        self.accepting = true;
        Ok(())
    }
    pub fn is_accepting(&self) -> bool {
        self.accepting
    }
    pub fn generation(&self) -> u64 {
        self.active.id
    }
    pub fn service(&self) -> ServiceKey {
        self.active.api.service
    }

    /// Counts managed unfinished requests, not escaped callbacks or retained DLLs.
    pub fn in_flight(&self) -> usize {
        self.retired
            .iter()
            .filter_map(Weak::upgrade)
            .fold(self.active.in_flight.get(), |count, generation| {
                count.saturating_add(generation.in_flight.get())
            })
    }

    fn check_service(&self, service: ServiceKey) -> Result<(), RouteError> {
        if service != self.active.api.service {
            return Err(RouteError::ContractMismatch {
                expected: self.active.api.service,
                found: service,
            });
        }
        Ok(())
    }
}

pub struct RequestTask {
    task: Option<AttachedFutureTask>,
    generation: Rc<Generation>,
    resource: ResourceId,
    status: Option<RunStatus>,
    control: Rc<Control>,
}

impl RequestTask {
    pub fn generation(&self) -> u64 {
        self.generation.id
    }
    pub fn resource(&self) -> ResourceId {
        self.resource
    }
    pub fn status(&self) -> Option<RunStatus> {
        self.status
    }

    /// Destroys this request future. Independent tasks and side effects are not
    /// rolled back. No request is automatically retried.
    pub fn cancel(mut self) {
        self.finish(RunStatus::Error);
    }

    fn finish(&mut self, status: RunStatus) {
        if self.status.is_none() {
            self.control.waker.borrow_mut().take();
            self.task = None;
            self.status = Some(status);
            self.generation
                .in_flight
                .set(self.generation.in_flight.get() - 1);
            let wake = self.generation.completion.borrow().clone();
            if let Some(wake) = wake {
                wake.wake();
            }
        }
    }
}
impl Future for RequestTask {
    type Output = RunStatus;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<RunStatus> {
        if let Some(status) = self.status {
            return Poll::Ready(status);
        }
        *self.control.waker.borrow_mut() = Some(cx.waker().clone());
        match Pin::new(self.task.as_mut().unwrap()).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(status) => {
                self.finish(status);
                Poll::Ready(status)
            }
        }
    }
}
impl Drop for RequestTask {
    fn drop(&mut self) {
        self.finish(RunStatus::Error);
    }
}

fn validate(api: HandlerApi) -> Result<(), RouteError> {
    let header = api.header;
    if header.abi_version != HANDLER_ABI_VERSION
        || header.struct_size < std::mem::size_of::<HandlerApi>() as u32
    {
        return Err(RouteError::Abi {
            version: header.abi_version,
            size: header.struct_size,
        });
    }
    if api.service.id == 0 || api.service.version == 0 || api.service.reserved != 0 {
        return Err(RouteError::InvalidApi);
    }
    let schema = api.state_schema;
    if !((schema.id == 0 && schema.version == 0 && schema.reserved == 0)
        || (schema.id != 0 && schema.version != 0 && schema.reserved == 0))
    {
        return Err(RouteError::InvalidApi);
    }
    Ok(())
}

unsafe fn load(path: &Path, generation: u64) -> Result<(HandlerApi, GenerationOwner), RouteError> {
    let image = GenerationOwner::load(path, generation).map_err(RouteError::Load)?;
    let entry = image
        .library()
        .get::<HandlerApiFn>(b"hot_handler_api")
        .map_err(|error| RouteError::Load(RunnerError::LibLoading(error)))?;
    let pointer = entry();
    if pointer.is_null() {
        return Err(RouteError::InvalidApi);
    }
    // Read only the stable header before accessing the rest of the table.
    let header = pointer.cast::<HandlerHeader>().read();
    if header.abi_version != HANDLER_ABI_VERSION
        || header.struct_size < std::mem::size_of::<HandlerApi>() as u32
    {
        return Err(RouteError::Abi {
            version: header.abi_version,
            size: header.struct_size,
        });
    }
    let api = pointer.read();
    validate(api)?;
    Ok((api, image))
}
