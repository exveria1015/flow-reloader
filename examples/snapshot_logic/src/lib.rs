use flow_reload::{allocator, reload_init, reload_snapshot};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

struct YieldOnce {
    yielded: bool,
}

impl YieldOnce {
    fn new() -> Self {
        Self { yielded: false }
    }
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            Poll::Pending
        }
    }
}

allocator!();

static mut HOST_CONTEXT: u64 = 0;

#[derive(Default, Serialize, Deserialize)]
pub struct CartState {
    pub counter: u32,
    pub message: String,
}

#[reload_init]
pub fn init(ctx: &mut u64) {
    unsafe {
        HOST_CONTEXT = *ctx;
    }
}

#[reload_snapshot]
pub async fn update(state: &mut CartState) {
    let ctx = unsafe { HOST_CONTEXT };
    state.counter += 1;
    state.message = format!("ctx={ctx} async tick");
    YieldOnce::new().await;
    state.counter += 1;
    state.message = format!("ctx={ctx} done");
}
