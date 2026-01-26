use async_std::net::TcpListener;
use async_std::task;
use flow_reload::{FlowRunner, RunStatus};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let logic_path = logic_library_path()?;
    let mut state = flow_reload::AppState::default();
    let mut runner = FlowRunner::new(&logic_path)?;
    start_echo_server();

    loop {
        let status = runner.run_attached_with_async_std(&mut state)?;
        if status == RunStatus::Error {
            eprintln!("Logic returned error status.");
        }
        print_state(&state);
        if runner.reload_if_needed()? {
            println!("Reloaded logic.");
        }
    }
}

fn start_echo_server() {
    task::spawn(async move {
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
            task::spawn(async move {
                let mut buf = vec![0u8; 256];
                let Ok(n) = async_std::io::ReadExt::read(&mut socket, &mut buf).await else {
                    return;
                };
                if n == 0 {
                    return;
                }
                let _ = async_std::io::WriteExt::write_all(&mut socket, &buf[..n]).await;
            });
        }
    });
}

fn logic_library_path() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    if let Ok(path) = std::env::var("LOGIC_LIB_PATH") {
        return Ok(std::path::PathBuf::from(path));
    }

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let logic_manifest_dir = manifest_dir.join("..").join("async_std_logic");
    let target_dir = flow_reload::internal::default_target_dir(&logic_manifest_dir);
    let status = flow_reload::internal::build_logic(
        "async_std_logic",
        &logic_manifest_dir,
        &target_dir,
    )?;
    if !status.success() {
        return Err("failed to build async_std_logic".into());
    }

    Ok(flow_reload::internal::library_path_in(
        &target_dir,
        "async_std_logic",
    ))
}

fn print_state(state: &flow_reload::AppState) {
    let message = read_message(&state.message);
    println!("State => counter: {}, message: {}", state.counter, message);
}

fn read_message(bytes: &[u8; 256]) -> String {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..len]).to_string()
}
