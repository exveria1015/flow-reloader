// Compatibility adapter for the AppState entrypoints. The generic Execution<S>
// core does not depend on this sample-shaped ABI or a blocking runtime.
use crate::{AttachedFutureTask, RunStatus};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// Common blocking path for dynamically loaded and statically linked logic.
/// No caller may observe state until this function destroys the foreign task.
pub(crate) unsafe fn run_borrowed(
    kind: crate::FuncType,
    entry: interface::EntryFn,
    make: interface::MakeAttachedFutureFn,
    state: &mut crate::AppState,
    runtime: &crate::RuntimeHandle,
    generation: Option<Arc<dyn Send + Sync>>,
) -> Result<RunStatus, crate::RunnerError> {
    if kind == crate::FuncType::Sync {
        return Ok(entry(state));
    }
    let raw = make(state);
    let mut task = AttachedFutureTask::with_optional_keep_alive(raw, generation)
        .ok_or(crate::RunnerError::AttachedInvalid)?;
    let mut cx = Context::from_waker(std::task::Waker::noop());
    loop {
        match Pin::new(&mut task).poll(&mut cx) {
            Poll::Ready(status) => return Ok(status),
            Poll::Pending => runtime.sleep(std::time::Duration::from_millis(16)),
        }
    }
}
