use std::sync::Arc;
use std::time::Duration;

#[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
use crate::{AttachedFutureTask, HostRuntimeVTable, RunStatus};

pub trait RuntimeBridge: Send + Sync {
    fn sleep(&self, duration: Duration);
}

#[derive(Clone)]
pub struct RuntimeHandle(Arc<dyn RuntimeBridge>);

impl RuntimeHandle {
    pub fn new<R: RuntimeBridge + 'static>(runtime: R) -> Self {
        Self(Arc::new(runtime))
    }

    pub fn std() -> Self {
        Self::new(StdRuntime::default())
    }

    pub fn sleep(&self, duration: Duration) {
        self.0.sleep(duration);
    }
}

impl<R: RuntimeBridge + 'static> From<R> for RuntimeHandle {
    fn from(runtime: R) -> Self {
        Self::new(runtime)
    }
}

#[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
pub trait RuntimeAdapter {
    type Handle;

    fn with_entered<F, R>(handle: &Self::Handle, f: F) -> R
    where
        F: FnOnce() -> R;

    fn vtable(handle: &Self::Handle) -> HostRuntimeVTable;

    fn block_on(handle: &Self::Handle, task: AttachedFutureTask) -> RunStatus;
}

#[derive(Default)]
pub struct StdRuntime;

impl RuntimeBridge for StdRuntime {
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[cfg(feature = "tokio")]
#[derive(Clone)]
#[allow(dead_code)]
pub struct TokioRuntimeHandle(pub tokio::runtime::Handle);

#[cfg(feature = "tokio")]
impl RuntimeBridge for TokioRuntimeHandle {
    fn sleep(&self, duration: Duration) {
        self.0.block_on(tokio::time::sleep(duration));
    }
}

#[cfg(feature = "async-std")]
#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct AsyncStdRuntime;

#[cfg(feature = "async-std")]
impl RuntimeBridge for AsyncStdRuntime {
    fn sleep(&self, duration: Duration) {
        async_std::task::block_on(async_std::task::sleep(duration));
    }
}

#[cfg(feature = "smol")]
#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct SmolRuntime;

#[cfg(feature = "smol")]
impl RuntimeBridge for SmolRuntime {
    fn sleep(&self, duration: Duration) {
        smol::block_on(async { smol::Timer::after(duration).await });
    }
}

#[cfg(feature = "async-std")]
pub struct AsyncStdAdapter;

#[cfg(feature = "async-std")]
impl RuntimeAdapter for AsyncStdAdapter {
    type Handle = ();

    fn with_entered<F, R>(_handle: &Self::Handle, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        f()
    }

    fn vtable(_handle: &Self::Handle) -> HostRuntimeVTable {
        crate::attached::async_std_runtime_vtable()
    }

    fn block_on(_handle: &Self::Handle, task: AttachedFutureTask) -> RunStatus {
        async_std::task::block_on(task)
    }
}

#[cfg(feature = "smol")]
pub struct SmolAdapter;

#[cfg(feature = "smol")]
impl RuntimeAdapter for SmolAdapter {
    type Handle = ();

    fn with_entered<F, R>(_handle: &Self::Handle, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        f()
    }

    fn vtable(_handle: &Self::Handle) -> HostRuntimeVTable {
        crate::attached::smol_runtime_vtable()
    }

    fn block_on(_handle: &Self::Handle, task: AttachedFutureTask) -> RunStatus {
        smol::block_on(task)
    }
}

#[cfg(feature = "tokio")]
pub struct TokioAdapter;

#[cfg(feature = "tokio")]
impl RuntimeAdapter for TokioAdapter {
    type Handle = tokio::runtime::Handle;

    fn with_entered<F, R>(handle: &Self::Handle, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = handle.enter();
        f()
    }

    fn vtable(handle: &Self::Handle) -> HostRuntimeVTable {
        crate::tokio_runtime_vtable(handle)
    }

    fn block_on(handle: &Self::Handle, task: AttachedFutureTask) -> RunStatus {
        handle.block_on(task)
    }
}
