use crate::{AttachedFuture, AttachedFutureTask, RunStatus};
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll};

/// One execution owns its state and foreign future. It stays on its creating
/// thread; waking it from another thread is supported by the host Waker ABI.
///
/// Forgetting an execution leaks its dependencies together, rather than leaving
/// a foreign future borrowing released stack data.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<flow_reload::Execution<()>>();
/// ```
pub struct Execution<S: 'static> {
    task: Option<AttachedFutureTask>,
    state: Option<StateStorage<S>>,
    status: Option<RunStatus>,
    // State and foreign destructors run while the generation is still live.
    _generation: Option<Arc<dyn Send + Sync>>,
    _local: PhantomData<Rc<()>>,
}

struct StateStorage<S>(*mut S, PhantomData<Box<S>>);

impl<S> StateStorage<S> {
    fn new(state: S) -> Self {
        Self(Box::into_raw(Box::new(state)), PhantomData)
    }

    fn into_inner(self) -> S {
        let pointer = self.0;
        std::mem::forget(self);
        // The foreign future has already been destroyed by the caller.
        unsafe { *Box::from_raw(pointer) }
    }
}

impl<S> Drop for StateStorage<S> {
    fn drop(&mut self) {
        unsafe { drop(Box::from_raw(self.0)) };
    }
}

impl<S: 'static> Execution<S> {
    /// Creates a task borrowing a stable allocation containing `state`.
    ///
    /// # Safety
    /// `make` must return a uniquely owned, valid foreign handle (or a fully
    /// null handle on failure). Its callbacks and library must remain live until
    /// task destruction and any escaped Wakers/callbacks have finished. The
    /// foreign destructor must end every borrow of `state`, including on error
    /// and cancellation. Callbacks must not unwind over FFI. `S` must remain a
    /// host-owned type that can be moved and returned independently of the
    /// library after destruction. Do not establish permanent pinning or
    /// self-references in S. On failure, release all state borrows before
    /// returning the null handle.
    pub unsafe fn from_ffi(
        state: S,
        make: unsafe extern "C" fn(*mut S) -> AttachedFuture,
    ) -> Result<Self, S> {
        Self::with_generation(state, make, None)
    }

    pub(crate) unsafe fn with_generation(
        state: S,
        make: unsafe extern "C" fn(*mut S) -> AttachedFuture,
        generation: Option<Arc<dyn Send + Sync>>,
    ) -> Result<Self, S> {
        // Convert to a raw owner BEFORE lending the allocation to the future.
        // Moving a Box after that borrow can invalidate the borrow under Miri.
        let state = StateStorage::new(state);
        let raw = make(state.0);
        let Some(task) = AttachedFutureTask::with_optional_keep_alive(raw, generation.clone())
        else {
            return Err(state.into_inner());
        };
        Ok(Self {
            task: Some(task),
            state: Some(state),
            status: None,
            _generation: generation,
            _local: PhantomData,
        })
    }

    pub(crate) fn completed(state: S, status: RunStatus, generation: Arc<dyn Send + Sync>) -> Self {
        Self {
            task: None,
            state: Some(StateStorage::new(state)),
            status: Some(status),
            _generation: Some(generation),
            _local: PhantomData,
        }
    }

    /// State is observable only after the foreign future's destructor returned.
    pub fn state(&self) -> Option<&S> {
        self.status
            .map(|_| unsafe { &*self.state.as_ref().unwrap().0 })
    }

    pub fn status(&self) -> Option<RunStatus> {
        self.status
    }

    /// Destroys this future synchronously and returns its state. This does not
    /// stop independently spawned tasks or roll back partially applied changes.
    pub fn cancel(mut self) -> S {
        self.task = None;
        self.state.take().unwrap().into_inner()
    }
}

impl<S> Future for Execution<S> {
    type Output = RunStatus;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<RunStatus> {
        let this = self.get_mut();
        if let Some(status) = this.status {
            return Poll::Ready(status);
        }
        match Pin::new(this.task.as_mut().unwrap()).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(status) => {
                this.task = None;
                this.status = Some(status);
                Poll::Ready(status)
            }
        }
    }
}
