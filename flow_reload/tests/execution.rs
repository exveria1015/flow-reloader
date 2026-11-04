use flow_reload::{AttachedFuture, AttachedFutureVTable, Execution, HostWaker, RunStatus};
use std::cell::RefCell;
use std::ffi::c_void;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

struct State(Rc<RefCell<Vec<&'static str>>>);
impl Drop for State {
    fn drop(&mut self) {
        self.0.borrow_mut().push("state");
    }
}

unsafe extern "C" fn make(state: *mut State) -> AttachedFuture {
    AttachedFuture {
        data: Box::into_raw(Box::new(state)).cast(),
        vtable: &VTABLE,
    }
}
unsafe extern "C" fn poll(_: *mut c_void, _: *const HostWaker) -> RunStatus {
    RunStatus::Pending
}
unsafe extern "C" fn destroy(data: *mut c_void) {
    let state = Box::from_raw(data.cast::<*mut State>());
    (&**state).0.borrow_mut().push("future");
}
static VTABLE: AttachedFutureVTable = AttachedFutureVTable {
    poll,
    drop: destroy,
};

#[test]
fn owned_generic_state_outlives_future_on_drop_and_explicit_cancellation() {
    for cancel in [false, true] {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut task = unsafe { Execution::from_ffi(State(events.clone()), make) }
            .ok()
            .unwrap();
        assert_eq!(
            Pin::new(&mut task).poll(&mut Context::from_waker(std::task::Waker::noop())),
            Poll::Pending
        );
        assert!(task.state().is_none());
        if cancel {
            let state = task.cancel();
            assert_eq!(*events.borrow(), ["future"]);
            drop(state);
        } else {
            drop(task);
        }
        assert_eq!(*events.borrow(), ["future", "state"]);
    }
}
