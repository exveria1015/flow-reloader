use flow_reload::{AppState, CartridgeState, RunStatus};
use std::cell::{Cell, RefCell};
use std::task::Poll;

flow_reload::allocator!();

thread_local! {
    static MODE: Cell<u8> = const { Cell::new(0) };
    static EVENTS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

#[derive(Default)]
struct SnapshotState {
    value: u8,
}

impl Drop for SnapshotState {
    fn drop(&mut self) {
        EVENTS.with(|events| events.borrow_mut().push("state"));
    }
}

impl CartridgeState for SnapshotState {
    fn snapshot(&self, out: &mut Vec<u8>) -> Result<(), String> {
        EVENTS.with(|events| events.borrow_mut().push("snapshot"));
        if MODE.with(Cell::get) == 3 {
            panic!("snapshot panic");
        }
        out.push(self.value);
        Ok(())
    }

    fn restore(&mut self, input: &[u8]) -> Result<(), String> {
        if MODE.with(Cell::get) == 4 {
            panic!("restore panic");
        }
        self.value = input[0];
        Ok(())
    }
}

struct BorrowGuard<'a>(&'a mut SnapshotState);

impl Drop for BorrowGuard<'_> {
    fn drop(&mut self) {
        self.0.value += 1;
        EVENTS.with(|events| events.borrow_mut().push("future"));
    }
}

#[flow_reload::reload_snapshot]
async fn update(state: &mut SnapshotState) {
    let _guard = BorrowGuard(state);
    match MODE.with(Cell::get) {
        0 => std::future::pending::<()>().await,
        1 => panic!("logic panic"),
        5 => {
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
        _ => {}
    }
}

#[test]
fn future_is_destroyed_before_snapshot_or_state_and_panics_are_contained() {
    let mut app = AppState::default();
    for mode in 0..=5 {
        MODE.with(|cell| cell.set(mode));
        EVENTS.with(|events| events.borrow_mut().clear());
        use std::future::Future;
        let mut task = unsafe { flow_reload::Execution::from_ffi(app, hot_make_future) }
            .ok()
            .unwrap();
        let mut poll = || {
            std::pin::Pin::new(&mut task)
                .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        };
        let status = poll();
        match mode {
            0 => assert_eq!(status, Poll::Pending),
            1 | 3 | 4 => assert_eq!(status, Poll::Ready(RunStatus::Error)),
            5 => {
                assert_eq!(status, Poll::Pending);
                assert_eq!(poll(), Poll::Ready(RunStatus::Done));
            }
            _ => assert_eq!(status, Poll::Ready(RunStatus::Done)),
        }
        app = task.cancel();
        if mode == 2 {
            assert_eq!(app.cartridge_bytes(), &[1]);
        }
        if mode == 5 {
            assert_eq!(app.cartridge_bytes(), &[2]);
        }
        let expected = match mode {
            0 | 1 => vec!["future", "state"],
            4 => vec!["state"],
            _ => vec!["future", "snapshot", "state"],
        };
        EVENTS.with(|events| assert_eq!(*events.borrow(), expected));
        hot_reset();
        EVENTS.with(|events| assert_eq!(*events.borrow(), expected));
    }
}
