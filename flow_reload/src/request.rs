use crate::Checkpoint;
use crate::{AttachedFuture, AttachedFutureVTable, HostWaker, RunStatus, Services};
use interface::services::{
    HandlerHeader, HostTaskControl, HostTaskVTable, TASK_CONTROL_ABI_VERSION,
};
use interface::services::{HandlerRequest, ResourceId, ServiceError, ServiceKey};
use std::ffi::c_void;
use std::future::Future;
use std::pin::Pin;

struct TaskControl(HostTaskControl);
impl TaskControl {
    unsafe fn retain(raw: HostTaskControl) -> Result<Self, ServiceError> {
        if raw.data.is_null() || raw.vtable.is_null() {
            return Err(ServiceError::INVALID_ARGUMENT);
        }
        let header = raw.vtable.cast::<HandlerHeader>().read();
        if header.abi_version != TASK_CONTROL_ABI_VERSION
            || header.struct_size < std::mem::size_of::<HostTaskVTable>() as u32
        {
            return Err(ServiceError::CONTRACT_MISMATCH);
        }
        let data = ((*raw.vtable).retain)(raw.data);
        if data.is_null() {
            return Err(ServiceError::FAILED);
        }
        Ok(Self(HostTaskControl { data, ..raw }))
    }
}
impl Drop for TaskControl {
    fn drop(&mut self) {
        unsafe { ((*self.0.vtable).release)(self.0.data) };
    }
}

/// DLL-local owned message and retained host services. No reference to the
/// host's request descriptor or input buffer survives construction.
pub struct RequestContext {
    generation: u64,
    resource: ResourceId,
    service: ServiceKey,
    services: Services,
    input: Vec<u8>,
    control: TaskControl,
    checkpoint: Option<Checkpoint>,
}

impl RequestContext {
    /// # Safety
    /// request and its input must be readable for this call. Services must obey
    /// Services::clone_from_raw's lifetime, ABI and owner-thread contracts.
    pub unsafe fn from_raw(request: *const HandlerRequest) -> Result<Self, ServiceError> {
        if request.is_null() {
            return Err(ServiceError::INVALID_ARGUMENT);
        }
        let request = &*request;
        let services = Services::clone_from_raw(request.services)?;
        let control = TaskControl::retain(request.control)?;
        let checkpoint = if request.state_schema.id == 0 {
            None
        } else {
            if request.state_len > 0 && request.state.is_null() {
                return Err(ServiceError::INVALID_ARGUMENT);
            }
            let bytes = if request.state_len == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(request.state, request.state_len).to_vec()
            };
            Some(Checkpoint::new(request.state_schema, bytes))
        };
        let input = if request.input_len == 0 {
            Vec::new()
        } else {
            if request.input.is_null() {
                return Err(ServiceError::INVALID_ARGUMENT);
            }
            std::slice::from_raw_parts(request.input, request.input_len).to_vec()
        };
        Ok(Self {
            generation: request.generation,
            resource: request.resource,
            service: request.service,
            services,
            input,
            control,
            checkpoint,
        })
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn resource(&self) -> ResourceId {
        self.resource
    }
    pub fn service(&self) -> ServiceKey {
        self.service
    }
    pub fn services(&self) -> &Services {
        &self.services
    }
    pub fn input(&self) -> &[u8] {
        &self.input
    }

    pub fn stop_requested(&self) -> bool {
        unsafe { ((*self.control.0.vtable).stop_requested)(self.control.0.data) }
    }

    /// Woken by the owning RequestTask when the router requests shutdown.
    pub async fn cancelled(&self) {
        std::future::poll_fn(|_| {
            if self.stop_requested() {
                std::task::Poll::Ready(())
            } else {
                std::task::Poll::Pending
            }
        })
        .await
    }

    pub fn checkpoint(&self) -> Option<&Checkpoint> {
        self.checkpoint.as_ref()
    }

    /// Stores a bounded, committed checkpoint in the host. Not an IO rollback.
    pub fn save_checkpoint(&self, schema: ServiceKey, bytes: &[u8]) -> Result<(), ServiceError> {
        let status = unsafe {
            ((*self.control.0.vtable).checkpoint)(
                self.control.0.data,
                schema,
                bytes.as_ptr(),
                bytes.len(),
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(ServiceError(status))
        }
    }

    pub fn call(
        &self,
        operation: u32,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<usize, ServiceError> {
        self.services
            .call(self.resource, self.service, operation, input, output)
    }
}

struct HandlerFuture(Pin<Box<dyn Future<Output = RunStatus>>>);

/// SDK support for reload_handler. User code first runs when the task is polled.
#[doc(hidden)]
pub fn make_handler_future(
    future: Pin<Box<dyn Future<Output = Result<(), ServiceError>>>>,
) -> AttachedFuture {
    let future = Box::pin(async move {
        match future.await {
            Ok(()) => RunStatus::Done,
            Err(_) => RunStatus::Error,
        }
    });
    AttachedFuture {
        data: Box::into_raw(Box::new(HandlerFuture(future))).cast(),
        vtable: &VTABLE,
    }
}

unsafe extern "C" fn poll(data: *mut c_void, waker: *const HostWaker) -> RunStatus {
    let future = &mut *data.cast::<HandlerFuture>();
    crate::attached::poll_attached_result_future(future.0.as_mut(), waker)
}
unsafe extern "C" fn destroy(data: *mut c_void) {
    drop(Box::from_raw(data.cast::<HandlerFuture>()));
}
static VTABLE: AttachedFutureVTable = AttachedFutureVTable {
    poll,
    drop: destroy,
};
