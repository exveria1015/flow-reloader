use flow_reload::{FlowRunner, RunStatus};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let logic_path = logic_library_path()?;
    let mut state = flow_reload::AppState::default();
    let mut context = 777u64;
    // SAFETY: every generation's init expects u64; it copies the value and
    // retains no reference. The host keeps context alive through runner cleanup.
    let mut runner = unsafe { FlowRunner::with_context(&logic_path, &mut context) }?;

    loop {
        let status = runner.run(&mut state)?;
        if status == RunStatus::Error {
            eprintln!("Logic returned error status.");
        }
        print_state(&state);
        if runner.reload_if_needed()? {
            println!("Reloaded logic.");
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

fn logic_library_path() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    if let Ok(path) = std::env::var("LOGIC_LIB_PATH") {
        return Ok(std::path::PathBuf::from(path));
    }

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let logic_manifest_dir = manifest_dir.join("..").join("sync_logic");
    let target_dir = flow_reload::internal::default_target_dir(&logic_manifest_dir);
    let status =
        flow_reload::internal::build_logic("sync_logic", &logic_manifest_dir, &target_dir)?;
    if !status.success() {
        return Err("failed to build sync_logic".into());
    }

    Ok(flow_reload::internal::library_path_in(
        &target_dir,
        "sync_logic",
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
