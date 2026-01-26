use flow_reload::{FlowRunner, RunStatus};
use std::sync::atomic::{AtomicUsize, Ordering};

static TICK_COUNT: AtomicUsize = AtomicUsize::new(0);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let logic_path = logic_library_path()?;
    let mut state = flow_reload::AppState::default();
    let mut context = 777u64;
    let mut runner = FlowRunner::with_context(&logic_path, &mut context)?;

    loop {
        let status = runner.run(&mut state)?;
        if status == RunStatus::Error {
            eprintln!("Logic returned error status.");
        }
        print_state(&state);
        if runner.reload_if_needed()? {
            println!("Reloaded logic.");
        }
    }
}

fn logic_library_path() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    if let Ok(path) = std::env::var("LOGIC_LIB_PATH") {
        return Ok(std::path::PathBuf::from(path));
    }

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let logic_manifest_dir = manifest_dir.join("..").join("snapshot_logic");
    let target_dir = flow_reload::internal::default_target_dir(&logic_manifest_dir);
    let status = flow_reload::internal::build_logic(
        "snapshot_logic",
        &logic_manifest_dir,
        &target_dir,
    )?;
    if !status.success() {
        return Err("failed to build snapshot_logic".into());
    }

    Ok(flow_reload::internal::library_path_in(
        &target_dir,
        "snapshot_logic",
    ))
}

fn print_state(state: &flow_reload::AppState) {
    let tick = TICK_COUNT.fetch_add(1, Ordering::Relaxed);
    if tick % 10 != 0 {
        return;
    }
    let snapshot = state.cartridge_bytes();
    let decoded: Result<snapshot_logic::CartState, _> = bincode::deserialize(snapshot);
    match decoded {
        Ok(state) => {
            println!("State => counter: {}, message: {}", state.counter, state.message);
        }
        Err(err) => {
            println!("State => snapshot decode error: {err}");
        }
    }
}
