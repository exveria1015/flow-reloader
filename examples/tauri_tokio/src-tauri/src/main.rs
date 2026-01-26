use flow_reload::{FlowRunner, RunStatus};
use serde::Serialize;
use tauri::Emitter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone, Serialize)]
struct HotState {
    counter: i32,
    message: String,
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                if let Err(err) = run_hot_loop(app_handle) {
                    eprintln!("hot loop failed: {err}");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn run_hot_loop(app_handle: tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let logic_path = logic_library_path()?;
    let mut state = flow_reload::AppState::default();
    let mut runner = FlowRunner::new(&logic_path)?;
    let rt = tokio::runtime::Runtime::new()?;
    let handle = rt.handle().clone();
    start_echo_server(&handle);

    loop {
        let status = runner.run_attached_with_tokio(&mut state, &handle)?;
        if status == RunStatus::Error {
            eprintln!("Logic returned error status.");
        }
        let payload = HotState {
            counter: state.counter,
            message: read_message(&state.message),
        };
        let _ = app_handle.emit("hot_state", payload);
        if runner.reload_if_needed()? {
            let _ = app_handle.emit("hot_reload", ());
        }
    }
}

fn start_echo_server(handle: &tokio::runtime::Handle) {
    let handle = handle.clone();
    handle.spawn(async move {
        let listener = match TcpListener::bind("127.0.0.1:4001").await {
            Ok(listener) => listener,
            Err(err) => {
                eprintln!("Failed to bind echo server: {err}");
                return;
            }
        };

        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("Echo accept failed: {err}");
                    continue;
                }
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 256];
                let Ok(n) = socket.read(&mut buf).await else {
                    return;
                };
                if n == 0 {
                    return;
                }
                let _ = socket.write_all(&buf[..n]).await;
            });
        }
    });
}

fn logic_library_path() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    if let Ok(path) = std::env::var("LOGIC_LIB_PATH") {
        return Ok(std::path::PathBuf::from(path));
    }

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let examples_dir = manifest_dir.join("..").join("..");
    let logic_manifest_dir = examples_dir.join("tauri_tokio_logic");
    let target_dir = flow_reload::internal::default_target_dir(&logic_manifest_dir);
    let status = flow_reload::internal::build_logic(
        "tauri_tokio_logic",
        &logic_manifest_dir,
        &target_dir,
    )?;
    if !status.success() {
        return Err("failed to build tauri_tokio_logic".into());
    }

    Ok(flow_reload::internal::library_path_in(
        &target_dir,
        "tauri_tokio_logic",
    ))
}

fn read_message(bytes: &[u8; 256]) -> String {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..len]).to_string()
}
