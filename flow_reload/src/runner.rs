#[cfg(feature = "async-std")]
use crate::runtime::AsyncStdAdapter;
#[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
use crate::runtime::RuntimeAdapter;
#[cfg(feature = "smol")]
use crate::runtime::SmolAdapter;
#[cfg(feature = "tokio")]
use crate::runtime::TokioAdapter;
#[cfg(any(target_os = "windows", test))]
use crate::ALLOCATOR_TAG_SYSTEM;
use crate::{AppState, FuncType, RunStatus, RuntimeHandle, ABI_VERSION};
use interface::MakeAttachedFutureFn;
#[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
use interface::{AttachRuntimeFn, AttachedSupportedFn};
use interface::{EntryFn, InitFn, Metadata, MetadataFn, ResetAttachedFn};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::error::Error;
use std::ffi::c_void;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::SystemTime;
#[cfg(test)]
use std::time::UNIX_EPOCH;

mod blocking;
mod generation;
pub(crate) use blocking::run_borrowed;
pub(crate) use generation::GenerationOwner;
pub use generation::{collect_retired_libraries, UnloadPolicy};

pub struct FlowRunner {
    load_id: u64,
    loaded: Option<LoadedLib>,
    reload_watcher: Option<ReloadWatcher>,
    init_context: Option<*mut c_void>,
    runtime: RuntimeHandle,
    unload_policy: UnloadPolicy,
}

#[derive(Debug)]
pub enum RunnerError {
    Io(std::io::Error),
    LibLoading(libloading::Error),
    Notify(notify::Error),
    AbiVersion {
        expected: u32,
        found: u32,
    },
    StateSize {
        expected: u32,
        found: u32,
    },
    AllocatorTag {
        expected: u32,
        found: u32,
    },
    InitFailed,
    AsyncRequiresRun,
    InitError {
        status: RunStatus,
    },
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    AttachedUnavailable,
    AttachedInvalid,
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    AttachedAttachFailed,
}

impl fmt::Display for RunnerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunnerError::Io(err) => write!(f, "io error: {err}"),
            RunnerError::LibLoading(err) => write!(f, "library load error: {err}"),
            RunnerError::Notify(err) => write!(f, "file watcher error: {err}"),
            RunnerError::AbiVersion { expected, found } => {
                write!(f, "ABI version mismatch: host {expected}, logic {found}")
            }
            RunnerError::StateSize { expected, found } => {
                write!(f, "AppState size mismatch: host {expected}, logic {found}")
            }
            RunnerError::AllocatorTag { expected, found } => {
                write!(
                    f,
                    "Allocator mismatch: expected tag {expected}, logic {found}"
                )
            }
            RunnerError::InitFailed => write!(f, "hot_init failed"),
            RunnerError::AsyncRequiresRun => write!(f, "async logic requires run() or run_until_done(); run_once() cannot retain a borrowed state"),
            RunnerError::InitError { status } => write!(f, "hot_init error: {status:?}"),
            #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
            RunnerError::AttachedUnavailable => write!(f, "attached async entry not available"),
            RunnerError::AttachedInvalid => write!(f, "attached async entry invalid"),
            #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
            RunnerError::AttachedAttachFailed => write!(f, "failed to attach runtime"),
        }
    }
}

impl Error for RunnerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RunnerError::Io(err) => Some(err),
            RunnerError::LibLoading(err) => Some(err),
            RunnerError::Notify(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for RunnerError {
    fn from(value: std::io::Error) -> Self {
        RunnerError::Io(value)
    }
}

impl From<libloading::Error> for RunnerError {
    fn from(value: libloading::Error) -> Self {
        RunnerError::LibLoading(value)
    }
}

impl From<notify::Error> for RunnerError {
    fn from(value: notify::Error) -> Self {
        RunnerError::Notify(value)
    }
}

impl FlowRunner {
    /// The default keeps retired libraries loaded, including after runner drop.
    ///
    /// # Safety
    /// The DLL and every future replacement loaded from this path must be
    /// trusted code satisfying this SDK's ABI and task ownership contracts.
    /// This includes module initializers, finalizers, metadata functions and
    /// escaped callbacks. Metadata checks cannot verify native code behavior.
    pub unsafe fn new<P: AsRef<Path>>(logic_path: P) -> Result<Self, RunnerError> {
        Self::load_with_policy(
            logic_path,
            std::ptr::null_mut(),
            RuntimeHandle::std(),
            UnloadPolicy::KeepLoaded,
        )
    }

    /// Loads native logic with a blocking runtime adapter.
    ///
    /// # Safety
    /// The DLL and its replacements must satisfy `FlowRunner::new`'s contract.
    pub unsafe fn with_runtime<P: AsRef<Path>>(
        logic_path: P,
        runtime: RuntimeHandle,
    ) -> Result<Self, RunnerError> {
        Self::load_with_policy(
            logic_path,
            std::ptr::null_mut(),
            runtime,
            UnloadPolicy::KeepLoaded,
        )
    }

    /// Low-level legacy initialization context. Prefer context-free execution.
    ///
    /// # Safety
    /// The context's concrete type and layout must match hot_init in every
    /// loaded generation. It must remain valid and exclusively accessible for
    /// every DLL access, including reload, cleanup and escaped callbacks/tasks.
    /// KeepLoaded retains code, not the context. Failed initialization must also
    /// obey this contract. No Rust lifetime or runtime type check enforces it.
    /// The DLL and replacements must also satisfy `FlowRunner::new`'s contract.
    pub unsafe fn with_context<P: AsRef<Path>, T>(
        logic_path: P,
        context: &mut T,
    ) -> Result<Self, RunnerError> {
        Self::with_context_ptr_and_runtime(
            logic_path,
            context as *mut T as *mut c_void,
            RuntimeHandle::std(),
        )
    }

    /// Low-level legacy initialization context. Prefer context-free execution.
    ///
    /// # Safety
    /// The context's concrete type and layout must match hot_init in every
    /// loaded generation. It must remain valid and exclusively accessible for
    /// every DLL access, including reload, cleanup and escaped callbacks/tasks.
    /// KeepLoaded retains code, not the context. Failed initialization must also
    /// obey this contract. No Rust lifetime or runtime type check enforces it.
    /// The DLL and replacements must also satisfy `FlowRunner::new`'s contract.
    pub unsafe fn with_context_and_runtime<P: AsRef<Path>, T>(
        logic_path: P,
        context: &mut T,
        runtime: RuntimeHandle,
    ) -> Result<Self, RunnerError> {
        Self::with_context_ptr_and_runtime(logic_path, context as *mut T as *mut c_void, runtime)
    }

    /// Low-level legacy initialization context. Prefer context-free execution.
    ///
    /// # Safety
    /// The context's concrete type and layout must match hot_init in every
    /// loaded generation. It must remain valid and exclusively accessible for
    /// every DLL access, including reload, cleanup and escaped callbacks/tasks.
    /// KeepLoaded retains code, not the context. Failed initialization must also
    /// obey this contract. No Rust lifetime or runtime type check enforces it.
    /// The DLL and replacements must also satisfy `FlowRunner::new`'s contract.
    pub unsafe fn with_context_ptr<P: AsRef<Path>>(
        logic_path: P,
        context: *mut c_void,
    ) -> Result<Self, RunnerError> {
        Self::with_context_ptr_and_runtime(logic_path, context, RuntimeHandle::std())
    }

    /// Low-level legacy initialization context. Prefer context-free execution.
    ///
    /// # Safety
    /// The context's concrete type and layout must match hot_init in every
    /// loaded generation. It must remain valid and exclusively accessible for
    /// every DLL access, including reload, cleanup and escaped callbacks/tasks.
    /// KeepLoaded retains code, not the context. Failed initialization must also
    /// obey this contract. No Rust lifetime or runtime type check enforces it.
    /// The DLL and replacements must also satisfy `FlowRunner::new`'s contract.
    pub unsafe fn with_context_ptr_and_runtime<P: AsRef<Path>>(
        logic_path: P,
        context: *mut c_void,
        runtime: RuntimeHandle,
    ) -> Result<Self, RunnerError> {
        Self::load_with_policy(logic_path, context, runtime, UnloadPolicy::KeepLoaded)
    }

    fn load_with_policy<P: AsRef<Path>>(
        logic_path: P,
        context: *mut c_void,
        runtime: RuntimeHandle,
        unload_policy: UnloadPolicy,
    ) -> Result<Self, RunnerError> {
        let logic_path = logic_path.as_ref().canonicalize()?;
        let reload_watcher = start_reload_watcher(&logic_path);
        let init_context = if context.is_null() {
            None
        } else {
            Some(context)
        };
        let loaded = load_library(&logic_path, 0, init_context, unload_policy)?;

        Ok(Self {
            load_id: 0,
            loaded: Some(loaded),
            reload_watcher,
            init_context,
            runtime,
            unload_policy,
        })
    }

    pub fn unload_policy(&self) -> UnloadPolicy {
        self.unload_policy
    }

    /// Changes retirement of the current generation and subsequent loads.
    /// Already retired KeepLoaded generations remain loaded.
    ///
    /// # Safety
    /// Before collecting WhenIdle generations, the caller must establish that
    /// independent tasks/threads and external callbacks have stopped and all
    /// plugin calls have returned. Include data/TLS destructors, initialization,
    /// and failed candidate loads in this artifact lifetime contract. Wakers
    /// cloned through the supplied HostWaker ABI are tracked; arbitrary spawn
    /// is not tracked. Setting this policy never unloads a library by itself.
    pub unsafe fn set_unload_policy(&mut self, policy: UnloadPolicy) {
        self.unload_policy = policy;
        self.loaded
            .as_mut()
            .expect("loaded library missing")
            .image
            .set_policy(policy);
    }

    pub fn run(&mut self, state: &mut AppState) -> Result<RunStatus, RunnerError> {
        self.run_until_done(state)
    }

    /// Runs synchronous logic once. Async logic must use `run` so its state
    /// borrow cannot escape into a library-owned pending future.
    pub fn run_once(&mut self, state: &mut AppState) -> Result<RunStatus, RunnerError> {
        if self.metadata_checked()?.func_type == FuncType::Async {
            return Err(RunnerError::AsyncRequiresRun);
        }
        let loaded = self.loaded_ref();
        let status = unsafe { (loaded.entry)(state) };
        Ok(status)
    }

    #[cfg(feature = "tokio")]
    pub fn run_attached_with_tokio(
        &mut self,
        state: &mut AppState,
        handle: &tokio::runtime::Handle,
    ) -> Result<RunStatus, RunnerError> {
        self.run_attached_with_adapter::<TokioAdapter>(state, handle)
    }

    #[cfg(feature = "async-std")]
    pub fn run_attached_with_async_std(
        &mut self,
        state: &mut AppState,
    ) -> Result<RunStatus, RunnerError> {
        let handle = ();
        self.run_attached_with_adapter::<AsyncStdAdapter>(state, &handle)
    }

    #[cfg(feature = "smol")]
    pub fn run_attached_with_smol(
        &mut self,
        state: &mut AppState,
    ) -> Result<RunStatus, RunnerError> {
        let handle = ();
        self.run_attached_with_adapter::<SmolAdapter>(state, &handle)
    }

    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    fn run_attached_with_adapter<A: RuntimeAdapter>(
        &mut self,
        state: &mut AppState,
        handle: &A::Handle,
    ) -> Result<RunStatus, RunnerError> {
        use crate::AttachedFutureTask;

        let metadata = self.metadata_checked()?;
        if metadata.func_type != FuncType::Async {
            return self.run_until_done(state);
        }
        let (supported, attach_runtime, make_future) = {
            let loaded = self.loaded_ref();
            (
                loaded
                    .attached_supported
                    .map(|func| unsafe { func() })
                    .unwrap_or(false),
                loaded.attach_runtime,
                loaded.make_future,
            )
        };
        if !supported {
            return Err(RunnerError::AttachedUnavailable);
        }
        let attach = attach_runtime.ok_or(RunnerError::AttachedUnavailable)?;

        let vtable = A::vtable(handle);
        let lease = self.loaded_ref().image.lease();
        A::with_entered(handle, || {
            // Drop order: task -> runtime attachment -> table/handle -> library.
            let _attachment = unsafe { crate::AttachedRuntimeGuard::new(attach, &vtable) }
                .ok_or(RunnerError::AttachedAttachFailed)?;
            let attached = unsafe { make_future(state) };
            // SAFETY: the synchronous call retains the state borrow and loaded
            // library until block_on has completed or dropped the task on unwind.
            let task = unsafe { AttachedFutureTask::with_keep_alive(attached, lease) }
                .ok_or(RunnerError::AttachedInvalid)?;
            Ok(A::block_on(handle, task))
        })
    }

    pub fn run_until_done(&mut self, state: &mut AppState) -> Result<RunStatus, RunnerError> {
        let metadata = self.metadata_checked()?;
        let loaded = self.loaded_ref();
        unsafe {
            run_borrowed(
                metadata.func_type,
                loaded.entry,
                loaded.make_future,
                state,
                &self.runtime,
                Some(loaded.image.lease()),
            )
        }
    }

    /// Starts an independent execution with owned state. The handle does not
    /// borrow this runner, so it may finish after reload or runner destruction.
    /// This path does not attach the legacy host runtime callback table.
    pub fn start(
        &mut self,
        mut state: AppState,
    ) -> Result<crate::Execution<AppState>, RunnerError> {
        let metadata = self.metadata_checked()?;
        let loaded = self.loaded_ref();
        let lease = loaded.image.lease();
        if metadata.func_type == FuncType::Sync {
            let status = unsafe { (loaded.entry)(&mut state) };
            return Ok(crate::Execution::completed(state, status, lease));
        }
        unsafe { crate::Execution::with_generation(state, loaded.make_future, Some(lease)) }
            .map_err(|_| RunnerError::AttachedInvalid)
    }

    pub fn reload_if_needed(&mut self) -> Result<bool, RunnerError> {
        let reload_requested = reload_requested(&self.reload_watcher);
        if !should_reload(self.loaded_ref(), reload_requested) {
            return Ok(false);
        }

        let source_path = self.loaded_ref().source_path.clone();
        let next_load_id = self.load_id.saturating_add(1);
        let loaded = load_library(
            &source_path,
            next_load_id,
            self.init_context,
            self.unload_policy,
        )?;
        let current = self
            .loaded
            .replace(loaded)
            .expect("FlowRunner loaded library missing");

        drop(current);
        self.load_id = next_load_id;
        Ok(true)
    }

    fn metadata_checked(&mut self) -> Result<Metadata, RunnerError> {
        loop {
            let metadata = unsafe { (self.loaded_ref().get_type)() };
            if let Err(err) = validate_metadata(metadata) {
                if self.reload_if_needed()? {
                    continue;
                }
                return Err(err);
            }

            return Ok(metadata);
        }
    }

    fn loaded_ref(&self) -> &LoadedLib {
        self.loaded
            .as_ref()
            .expect("FlowRunner loaded library missing")
    }
}

impl Drop for FlowRunner {
    fn drop(&mut self) {
        drop(self.loaded.take());
    }
}

struct LoadedLib {
    entry: EntryFn,
    get_type: MetadataFn,
    _init: Option<InitFn>,
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    attached_supported: Option<AttachedSupportedFn>,
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    attach_runtime: Option<AttachRuntimeFn>,
    make_future: MakeAttachedFutureFn,
    reset_attached: Option<ResetAttachedFn>,
    source_path: PathBuf,
    source_mtime: SystemTime,
    image: GenerationOwner,
}

impl Drop for LoadedLib {
    fn drop(&mut self) {
        // Run module cleanup while code is live. Independent executions own
        // their futures and leases; resetting a module must not destroy them.
        if let Some(reset) = self.reset_attached {
            unsafe { reset() };
        }
    }
}

struct LibraryCopy {
    path: PathBuf,
    id: crate::ArtifactId,
}

impl Drop for LibraryCopy {
    fn drop(&mut self) {
        let error = std::fs::remove_file(&self.path)
            .err()
            .filter(|e| e.kind() != std::io::ErrorKind::NotFound)
            .map(|e| crate::CopyRemovalError {
                id: self.id,
                path: self.path.clone(),
                message: e.to_string(),
            });
        crate::retention::removed(self.id, error);
    }
}

struct ReloadWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<()>,
}

fn load_library(
    source_path: &Path,
    load_id: u64,
    init_context: Option<*mut c_void>,
    unload_policy: UnloadPolicy,
) -> Result<LoadedLib, RunnerError> {
    let source_mtime = std::fs::metadata(source_path)?.modified()?;
    let image = GenerationOwner::open(copy_library(source_path, load_id)?, unload_policy)?;
    let lib = image.library();
    let get_type: MetadataFn = unsafe { *lib.get(b"hot_type")? };
    let entry: EntryFn = unsafe { *lib.get(b"hot_entry")? };
    let init = unsafe { lib.get::<InitFn>(b"hot_init") }
        .ok()
        .map(|sym| *sym);
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    let attached_supported = unsafe { lib.get::<AttachedSupportedFn>(b"hot_attached_supported") }
        .ok()
        .map(|sym| *sym);
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    let attach_runtime = unsafe { lib.get::<AttachRuntimeFn>(b"hot_attach_runtime") }
        .ok()
        .map(|sym| *sym);
    let make_future: MakeAttachedFutureFn = unsafe { *lib.get(b"hot_make_future")? };
    let reset_attached = unsafe { lib.get::<ResetAttachedFn>(b"hot_reset") }
        .ok()
        .map(|sym| *sym);

    let metadata = unsafe { get_type() };
    validate_metadata(metadata)?;

    if let (Some(ctx), Some(init_fn)) = (init_context, init) {
        let status = unsafe { init_fn(ctx) };
        if status == RunStatus::Error {
            return Err(RunnerError::InitFailed);
        }
        if status != RunStatus::Done {
            return Err(RunnerError::InitError { status });
        }
    }

    image.activate();
    Ok(LoadedLib {
        entry,
        get_type,
        _init: init,
        #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
        attached_supported,
        #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
        attach_runtime,
        make_future,
        reset_attached,
        source_path: source_path.to_path_buf(),
        source_mtime,
        image,
    })
}

fn copy_library(source_path: &Path, _load_id: u64) -> Result<LibraryCopy, RunnerError> {
    let result = (|| {
        let directory = crate::cache::session_directory()?.join("copies");
        let id = crate::retention::next_id()?;
        let ext = source_path
            .extension()
            .and_then(|s| s.to_str())
            .filter(|s| matches!(*s, "dll" | "so" | "dylib"))
            .unwrap_or("bin");
        let path = directory.join(format!("{:016x}.{ext}", id.0));
        let mut source = std::fs::File::open(source_path)?;
        let mut dest = crate::cache::new_file(&path)?;
        crate::retention::copied(id, source_path.to_path_buf(), path.clone(), 0);
        let copy = LibraryCopy { path, id };
        let bytes = std::io::copy(&mut source, &mut dest)?;
        crate::retention::update(id, |v| v.copy_bytes = bytes);
        drop(dest);
        Ok(copy)
    })();
    if result.is_err() {
        crate::retention::copy_failed();
    }
    result
}

pub(crate) fn validate_metadata(metadata: Metadata) -> Result<(), RunnerError> {
    if metadata.abi_version != ABI_VERSION {
        return Err(RunnerError::AbiVersion {
            expected: ABI_VERSION,
            found: metadata.abi_version,
        });
    }

    let expected_state_size = std::mem::size_of::<AppState>() as u32;
    if metadata.state_size != expected_state_size {
        return Err(RunnerError::StateSize {
            expected: expected_state_size,
            found: metadata.state_size,
        });
    }

    #[cfg(target_os = "windows")]
    {
        if metadata.allocator_tag != ALLOCATOR_TAG_SYSTEM {
            return Err(RunnerError::AllocatorTag {
                expected: ALLOCATOR_TAG_SYSTEM,
                found: metadata.allocator_tag,
            });
        }
    }

    Ok(())
}

fn should_reload(current: &LoadedLib, reload_requested: bool) -> bool {
    if reload_requested {
        return true;
    }
    let Ok(metadata) = std::fs::metadata(&current.source_path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    modified > current.source_mtime
}

fn reload_requested(watcher: &Option<ReloadWatcher>) -> bool {
    let Some(watcher) = watcher else {
        return false;
    };

    let mut triggered = false;
    while watcher.rx.try_recv().is_ok() {
        triggered = true;
    }
    triggered
}

fn start_reload_watcher(source_path: &Path) -> Option<ReloadWatcher> {
    let file_name = source_path.file_name()?.to_os_string();
    let watch_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
    let (tx, rx) = mpsc::channel();
    let mut watcher =
        match notify::recommended_watcher(move |event: Result<notify::Event, notify::Error>| {
            let Ok(event) = event else {
                return;
            };
            let is_target = event.paths.iter().any(|path| {
                path.file_name()
                    .map(|name| name == file_name.as_os_str())
                    .unwrap_or(false)
            });
            if !is_target {
                return;
            }
            if matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
            ) {
                let _ = tx.send(());
            }
        }) {
            Ok(watcher) => watcher,
            Err(err) => {
                eprintln!("Failed to start file watcher: {err}");
                return None;
            }
        };

    if let Err(err) = watcher.watch(watch_dir, RecursiveMode::NonRecursive) {
        eprintln!("Failed to watch {}: {err}", watch_dir.display());
        return None;
    }

    Some(ReloadWatcher {
        _watcher: watcher,
        rx,
    })
}

#[cfg(test)]
mod tests;
