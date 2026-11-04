// Execute the actual Release macro expansion, rather than only compiling it.
#[cfg(not(debug_assertions))]
flow_reload::allocator!();

#[cfg(not(debug_assertions))]
mod host {
    use flow_reload::{AppState, RuntimeBridge, RuntimeHandle};
    use std::task::Poll;

    struct NoSleep;
    impl RuntimeBridge for NoSleep {
        fn sleep(&self, _: std::time::Duration) {}
    }

    mod logic {
        use super::*;

        #[flow_reload::reload_direct]
        async fn update(state: &mut AppState) {
            let counter = &mut state.counter;
            *counter = 1;
            let mut first = true;
            std::future::poll_fn(|_| {
                if std::mem::take(&mut first) {
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
            *counter = 2;
        }
    }

    struct Finished;

    flow_reload::main!(logic, AppState, runtime: RuntimeHandle::new(NoSleep), {}, |state: &AppState| {
        assert_eq!(state.counter, 2, "tick observed a pending future's state");
        std::panic::panic_any(Finished);
    });

    #[test]
    fn tick_runs_only_after_pending_future_has_released_state() {
        let result = std::panic::catch_unwind(main);
        assert!(result.unwrap_err().is::<Finished>());
    }
}
