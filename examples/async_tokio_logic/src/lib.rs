use flow_reload::{allocator, reload_direct, AttachedSleep, AttachedTcpRequest, RunStatus};

allocator!();

#[reload_direct]
pub async fn update(state: &mut flow_reload::AppState) {
    state.counter += 1;
    state.set_message("attached: sleep...");
    AttachedSleep::new(50).await;
    let mut out = [0u8; 256];
    match AttachedTcpRequest::new("127.0.0.1:4001", b"ping", &mut out).await {
        Ok(len) => {
            let msg = std::str::from_utf8(&out[4..4 + len]).unwrap_or("<invalid>");
            state.set_message(&format!("tcp ok: {msg}"));
        }
        Err(RunStatus::Error) => state.set_message("tcp error"),
        Err(RunStatus::Pending) => state.set_message("tcp pending"),
        Err(RunStatus::Done) => state.set_message("tcp done"),
    }
    state.counter += 1;
}
