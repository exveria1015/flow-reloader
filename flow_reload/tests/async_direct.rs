use flow_reload::{AppState, RunStatus};
use std::task::Poll;

flow_reload::allocator!();

struct BorrowGuard<'a>(&'a mut i32);

impl Drop for BorrowGuard<'_> {
    fn drop(&mut self) {
        *self.0 += 10;
    }
}

#[flow_reload::reload_direct]
async fn update(state: &mut AppState) {
    let mode = state.counter;
    let _guard = BorrowGuard(&mut state.counter);
    match mode {
        0 => std::future::pending::<()>().await,
        1 => panic!("logic panic"),
        _ => {
            let mut first = true;
            std::future::poll_fn(|_| {
                if std::mem::take(&mut first) {
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
        }
    }
}

fn poll(task: &mut flow_reload::Execution<AppState>) -> Poll<RunStatus> {
    use std::future::Future;
    std::pin::Pin::new(task).poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
}

#[test]
fn generated_future_releases_state_on_completion_panic_and_cancellation() {
    for mode in [0, 1, 2] {
        let state = AppState {
            counter: mode,
            ..AppState::default()
        };
        let mut task = unsafe { flow_reload::Execution::from_ffi(state, hot_make_future) }
            .ok()
            .unwrap();
        assert!(task.state().is_none());
        let first = poll(&mut task);
        match mode {
            0 => assert_eq!(first, Poll::Pending),
            1 => assert_eq!(first, Poll::Ready(RunStatus::Error)),
            _ => {
                assert_eq!(first, Poll::Pending);
                assert_eq!(poll(&mut task), Poll::Ready(RunStatus::Done));
                assert_eq!(poll(&mut task), Poll::Ready(RunStatus::Done));
            }
        }
        let state = task.cancel();
        assert_eq!(state.counter, mode + 10);
    }
}

#[test]
fn two_pending_executions_on_one_thread_are_independent() {
    let make = |counter| {
        unsafe {
            flow_reload::Execution::from_ffi(
                AppState {
                    counter,
                    ..AppState::default()
                },
                hot_make_future,
            )
        }
        .ok()
        .unwrap()
    };
    let mut first = make(0);
    let mut second = make(2);
    assert_eq!(poll(&mut first), Poll::Pending);
    assert_eq!(poll(&mut second), Poll::Pending);
    hot_reset(); // There is no global slot to clear.
    assert_eq!(poll(&mut second), Poll::Ready(RunStatus::Done));
    assert_eq!(second.state().unwrap().counter, 12);
    assert_eq!(first.cancel().counter, 10);
    assert_eq!(second.cancel().counter, 12);
}
