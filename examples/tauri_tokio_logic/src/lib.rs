use flow_reload::{
    allocator, reload_direct, spawn_detached_tcp_request, AttachedSleep, AttachedTcpRequest,
    RunStatus,
};

allocator!();

#[reload_direct]
pub async fn update(state: &mut flow_reload::AppState) {
    state.counter += 1;
    state.set_message("tauri: Update!");
    AttachedSleep::new(50).await;
    let mut out = [0u8; 256];
    match AttachedTcpRequest::new("127.0.0.1:4001", b"ping", &mut out).await {
        Ok(len) => {
            let msg = std::str::from_utf8(&out[4..4 + len]).unwrap_or("<invalid>");
            state.set_message(&format!("tauri tcp ASDF: {msg}"));
        }
        Err(RunStatus::Error) => state.set_message("tauri tcp error"),
        Err(RunStatus::Pending) => state.set_message("tauri tcp pending"),
        Err(RunStatus::Done) => state.set_message("tauri tcp done"),
    }
    if !spawn_detached_tcp_request("127.0.0.1:4001", b"fire") {
        state.set_message("tauri detached tcp unavailable");
    }
    state.counter += 1;
}
