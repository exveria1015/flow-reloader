use flow_reload::{allocator, reload_direct, reload_init};

allocator!();

static mut HOST_CONTEXT: u64 = 0;

#[reload_init]
pub fn init(ctx: &mut u64) {
    unsafe {
        HOST_CONTEXT = *ctx;
    }
}

#[reload_direct]
pub fn update(state: &mut flow_reload::AppState) {
    let ctx = unsafe { HOST_CONTEXT };
    state.counter += 1;
    state.set_message(&format!("sync tick ctx={ctx}"));
}
