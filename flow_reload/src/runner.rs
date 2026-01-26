use crate::{AppState, FuncType, RunStatus, RuntimeHandle, ABI_VERSION, ALLOCATOR_TAG_SYSTEM};
#[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
use crate::runtime::RuntimeAdapter;
#[cfg(feature = "tokio")]
use crate::runtime::TokioAdapter;
#[cfg(feature = "async-std")]
use crate::runtime::AsyncStdAdapter;
#[cfg(feature = "smol")]
use crate::runtime::SmolAdapter;
use interface::{EntryFn, InitFn, Metadata, MetadataFn};
#[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
use interface::{
    AttachRuntimeFn, AttachedSupportedFn, MakeAttachedFutureFn, ResetAttachedFn,
};
use libloading::Library;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::error::Error;
use std::fmt;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, SystemTime};

pub struct FlowRunner {
    load_id: u64,
    loaded: Option<LoadedLib>,
    reload_watcher: Option<ReloadWatcher>,
    init_context: Option<*mut c_void>,
    runtime: RuntimeHandle,
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    attached_runtime: Option<Box<interface::HostRuntimeVTable>>,
}

#[derive(Debug)]
pub enum RunnerError {
    Io(std::io::Error),
    LibLoading(libloading::Error),
    Notify(notify::Error),
    AbiVersion { expected: u32, found: u32 },
    StateSize { expected: u32, found: u32 },
    AllocatorTag { expected: u32, found: u32 },
    InitFailed,
    InitError { status: RunStatus },
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    AttachedUnavailable,
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
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
                write!(f, "Allocator mismatch: expected tag {expected}, logic {found}")
            }
            RunnerError::InitFailed => write!(f, "hot_init failed"),
            RunnerError::InitError { status } => write!(f, "hot_init error: {status:?}"),
            #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
            RunnerError::AttachedUnavailable => write!(f, "attached async entry not available"),
            #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
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
    pub fn new<P: AsRef<Path>>(logic_path: P) -> Result<Self, RunnerError> {
        Self::with_context_ptr_and_runtime(logic_path, std::ptr::null_mut(), RuntimeHandle::std())
    }

    pub fn with_runtime<P: AsRef<Path>>(
        logic_path: P,
        runtime: RuntimeHandle,
    ) -> Result<Self, RunnerError> {
        Self::with_context_ptr_and_runtime(logic_path, std::ptr::null_mut(), runtime)
    }

    pub fn with_context<P: AsRef<Path>, T>(
        logic_path: P,
        context: &mut T,
    ) -> Result<Self, RunnerError> {
        Self::with_context_ptr_and_runtime(
            logic_path,
            context as *mut T as *mut c_void,
            RuntimeHandle::std(),
        )
    }

    pub fn with_context_and_runtime<P: AsRef<Path>, T>(
        logic_path: P,
        context: &mut T,
        runtime: RuntimeHandle,
    ) -> Result<Self, RunnerError> {
        Self::with_context_ptr_and_runtime(logic_path, context as *mut T as *mut c_void, runtime)
    }

    pub fn with_context_ptr<P: AsRef<Path>>(
        logic_path: P,
        context: *mut c_void,
    ) -> Result<Self, RunnerError> {
        Self::with_context_ptr_and_runtime(logic_path, context, RuntimeHandle::std())
    }

    pub fn with_context_ptr_and_runtime<P: AsRef<Path>>(
        logic_path: P,
        context: *mut c_void,
        runtime: RuntimeHandle,
    ) -> Result<Self, RunnerError> {
        let logic_path = logic_path.as_ref().to_path_buf();
        let reload_watcher = start_reload_watcher(&logic_path);
        let init_context = if context.is_null() { None } else { Some(context) };
        let loaded = load_library(&logic_path, 0, init_context)?;

        Ok(Self {
            load_id: 0,
            loaded: Some(loaded),
            reload_watcher,
            init_context,
            runtime,
            #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
            attached_runtime: None,
        })
    }

    pub fn run(&mut self, state: &mut AppState) -> Result<RunStatus, RunnerError> {
        self.run_until_done(state)
    }

    pub fn run_once(&mut self, state: &mut AppState) -> Result<RunStatus, RunnerError> {
        let _metadata = self.metadata_checked()?;
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
    pub fn run_attached_with_smol(&mut self, state: &mut AppState) -> Result<RunStatus, RunnerError> {
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
        if let Some(attach) = attach_runtime {
            if self.attached_runtime.is_none() {
                let vtable = A::vtable(handle);
                self.attached_runtime = Some(Box::new(vtable));
            }
            let vtable_ptr = self
                .attached_runtime
                .as_ref()
                .expect("attached runtime missing")
                .as_ref()
                as *const _;
            let ok = unsafe { attach(vtable_ptr) };
            if !ok {
                return Err(RunnerError::AttachedAttachFailed);
            }
        }
        let make_future = make_future.ok_or(RunnerError::AttachedUnavailable)?;
        let attached = A::with_entered(handle, || unsafe { make_future(state) });
        let task = AttachedFutureTask::new(attached).ok_or(RunnerError::AttachedInvalid)?;
        let status = A::block_on(handle, task);
        Ok(status)
    }

    pub fn run_until_done(&mut self, state: &mut AppState) -> Result<RunStatus, RunnerError> {
        let metadata = self.metadata_checked()?;
        let loaded = self.loaded_ref();

        match metadata.func_type {
            FuncType::Sync => {
                let status = unsafe { (loaded.entry)(state) };
                Ok(status)
            }
            FuncType::Async => loop {
                let status = unsafe { (loaded.entry)(state) };
                match status {
                    RunStatus::Pending => self.runtime.sleep(Duration::from_millis(16)),
                    _ => return Ok(status),
                }
            },
        }
    }

    pub fn reload_if_needed(&mut self) -> Result<bool, RunnerError> {
        let reload_requested = reload_requested(&self.reload_watcher);
        if !should_reload(self.loaded_ref(), reload_requested) {
            return Ok(false);
        }

        let current = self
            .loaded
            .take()
            .expect("FlowRunner loaded library missing");
        #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
        if let Some(reset) = current.reset_attached {
            unsafe { reset() };
        }
        self.load_id += 1;
        let loaded = reload_library(current, self.load_id, self.init_context)?;
        self.loaded = Some(loaded);
        Ok(true)
    }

    fn metadata_checked(&mut self) -> Result<Metadata, RunnerError> {
        loop {
            let metadata = unsafe { (self.loaded_ref().get_type)() };
            if metadata.abi_version != ABI_VERSION {
                if self.reload_if_needed()? {
                    continue;
                }
                return Err(RunnerError::AbiVersion {
                    expected: ABI_VERSION,
                    found: metadata.abi_version,
                });
            }

            let expected_state_size = std::mem::size_of::<AppState>() as u32;
            if metadata.state_size != expected_state_size {
                if self.reload_if_needed()? {
                    continue;
                }
                return Err(RunnerError::StateSize {
                    expected: expected_state_size,
                    found: metadata.state_size,
                });
            }

            #[cfg(target_os = "windows")]
            {
                if metadata.allocator_tag != ALLOCATOR_TAG_SYSTEM {
                    if self.reload_if_needed()? {
                        continue;
                    }
                    return Err(RunnerError::AllocatorTag {
                        expected: ALLOCATOR_TAG_SYSTEM,
                        found: metadata.allocator_tag,
                    });
                }
            }

            return Ok(metadata);
        }
    }

    fn loaded_ref(&self) -> &LoadedLib {
        self.loaded.as_ref().expect("FlowRunner loaded library missing")
    }
}

struct LoadedLib {
    _lib: Library,
    entry: EntryFn,
    get_type: MetadataFn,
    _init: Option<InitFn>,
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    attached_supported: Option<AttachedSupportedFn>,
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    attach_runtime: Option<AttachRuntimeFn>,
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    make_future: Option<MakeAttachedFutureFn>,
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    reset_attached: Option<ResetAttachedFn>,
    source_path: PathBuf,
    source_mtime: SystemTime,
    copy_path: PathBuf,
}

struct ReloadWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<()>,
}

fn load_library(
    source_path: &Path,
    load_id: u64,
    init_context: Option<*mut c_void>,
) -> Result<LoadedLib, RunnerError> {
    let source_mtime = std::fs::metadata(source_path)?.modified()?;
    let copy_path = copy_library(source_path, load_id)?;
    let lib = unsafe { Library::new(&copy_path)? };
    let get_type: MetadataFn = unsafe { *lib.get(b"hot_type")? };
    let entry: EntryFn = unsafe { *lib.get(b"hot_entry")? };
    let init = unsafe { lib.get::<InitFn>(b"hot_init") }.ok().map(|sym| *sym);
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    let attached_supported = unsafe { lib.get::<AttachedSupportedFn>(b"hot_attached_supported") }
        .ok()
        .map(|sym| *sym);
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    let attach_runtime = unsafe { lib.get::<AttachRuntimeFn>(b"hot_attach_runtime") }
        .ok()
        .map(|sym| *sym);
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    let make_future = unsafe { lib.get::<MakeAttachedFutureFn>(b"hot_make_future") }
        .ok()
        .map(|sym| *sym);
    #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
    let reset_attached = unsafe { lib.get::<ResetAttachedFn>(b"hot_reset") }
        .ok()
        .map(|sym| *sym);

    if let (Some(ctx), Some(init_fn)) = (init_context, init) {
        let status = unsafe { init_fn(ctx) };
        if status == RunStatus::Error {
            return Err(RunnerError::InitFailed);
        }
        if status != RunStatus::Done {
            return Err(RunnerError::InitError { status });
        }
    }

    Ok(LoadedLib {
        _lib: lib,
        entry,
        get_type,
        _init: init,
        #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
        attached_supported,
        #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
        attach_runtime,
        #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
        make_future,
        #[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
        reset_attached,
        source_path: source_path.to_path_buf(),
        source_mtime,
        copy_path,
    })
}

fn reload_library(
    current: LoadedLib,
    load_id: u64,
    init_context: Option<*mut c_void>,
) -> Result<LoadedLib, RunnerError> {
    let source_path = current.source_path.clone();
    let old_copy = current.copy_path.clone();
    drop(current);
    let _ = std::fs::remove_file(old_copy);
    load_library(&source_path, load_id, init_context)
}

fn copy_library(source_path: &Path, load_id: u64) -> Result<PathBuf, RunnerError> {
    let copy_dir = PathBuf::from("target").join("hot");
    std::fs::create_dir_all(&copy_dir)?;
    let ext = source_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("dll");
    let file_name = format!("logic_hot_{load_id}.{}", ext);
    let dest = copy_dir.join(file_name);
    std::fs::copy(source_path, &dest)?;
    Ok(dest)
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
    let Some(file_name) = source_path.file_name().map(|name| name.to_os_string()) else {
        return None;
    };
    let watch_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
    let (tx, rx) = mpsc::channel();
    let mut watcher = match notify::recommended_watcher(
        move |event: Result<notify::Event, notify::Error>| {
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
            if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_))
            {
                let _ = tx.send(());
            }
        },
    ) {
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

    Some(ReloadWatcher { _watcher: watcher, rx })
}
