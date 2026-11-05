use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};

pub use crate::attached::{poll_attached_future, poll_attached_result_future};
pub use crate::environment::validate_init_data;
pub use crate::request::make_handler_future;

/// Runs statically linked SDK entrypoints through the same execution path as
/// FlowRunner. State is inaccessible to tick callbacks until the task is gone.
///
/// # Safety
/// All functions must belong to the same compatible, trusted logic module and
/// obey ABI v4's ownership, poll, and destruction contracts.
pub unsafe fn run_static(
    state: &mut crate::AppState,
    runtime: &crate::RuntimeHandle,
    metadata: interface::MetadataFn,
    entry: interface::EntryFn,
    make: interface::MakeAttachedFutureFn,
) -> Result<crate::RunStatus, crate::RunnerError> {
    let metadata = metadata();
    crate::runner::validate_metadata(metadata)?;
    crate::runner::run_borrowed(metadata.func_type, entry, make, state, runtime, None)
}

pub struct SourceWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<()>,
}

pub fn library_file_name(package_name: &str) -> String {
    let base_name = package_name.replace('-', "_");
    if cfg!(target_os = "windows") {
        format!("{base_name}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{base_name}.dylib")
    } else {
        format!("lib{base_name}.so")
    }
}

pub fn library_path_in(target_dir: &Path, package_name: &str) -> PathBuf {
    target_dir
        .join("debug")
        .join(library_file_name(package_name))
}

pub fn default_library_path(package_name: &str) -> PathBuf {
    library_path_in(&PathBuf::from("target"), package_name)
}

pub fn default_target_dir(manifest_dir: &Path) -> PathBuf {
    manifest_dir.join("target").join("hot-reload-build")
}

pub fn resolve_logic_manifest_dir(host_manifest_dir: &Path, package_name: &str) -> PathBuf {
    if let Ok(path) = std::env::var("FLOW_RELOAD_LOGIC_MANIFEST_DIR") {
        let candidate = PathBuf::from(path);
        if candidate
            .file_name()
            .map(|name| name == "Cargo.toml")
            .unwrap_or(false)
        {
            if let Some(parent) = candidate.parent() {
                return parent.to_path_buf();
            }
        }
        if candidate.join("Cargo.toml").exists() {
            return candidate;
        }
        eprintln!(
            "FLOW_RELOAD_LOGIC_MANIFEST_DIR does not point to a crate directory: {}",
            candidate.display()
        );
    }

    let sibling = host_manifest_dir.join("..").join(package_name);
    if sibling.join("Cargo.toml").exists() {
        return sibling;
    }

    host_manifest_dir.join(package_name)
}

pub fn build_logic(
    package_name: &str,
    manifest_dir: &Path,
    target_dir: &Path,
) -> io::Result<std::process::ExitStatus> {
    Command::new("cargo")
        .arg("build")
        .arg("--lib")
        .arg("--package")
        .arg(package_name)
        .arg("--target-dir")
        .arg(target_dir)
        .arg("--jobs")
        .arg("1")
        .env("CARGO_INCREMENTAL", "0")
        .env("CARGO_PROFILE_DEV_INCREMENTAL", "false")
        .env("CARGO_BUILD_JOBS", "1")
        .current_dir(manifest_dir)
        .status()
}

pub fn start_source_watcher(manifest_dir: &Path) -> Option<SourceWatcher> {
    let src_dir = manifest_dir.join("src");
    let cargo_manifest = manifest_dir.join("Cargo.toml");
    let build_script = manifest_dir.join("build.rs");
    let (tx, rx) = mpsc::channel();
    let mut watcher =
        match notify::recommended_watcher(move |event: Result<notify::Event, notify::Error>| {
            let Ok(event) = event else {
                return;
            };
            if matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
            ) {
                let _ = tx.send(());
            }
        }) {
            Ok(watcher) => watcher,
            Err(err) => {
                eprintln!("Failed to start source watcher: {err}");
                return None;
            }
        };

    let mut watched_any = false;

    if cargo_manifest.exists() {
        match watcher.watch(&cargo_manifest, RecursiveMode::NonRecursive) {
            Ok(()) => watched_any = true,
            Err(err) => {
                eprintln!("Failed to watch {}: {err}", cargo_manifest.display());
            }
        }
    }

    if build_script.exists() {
        match watcher.watch(&build_script, RecursiveMode::NonRecursive) {
            Ok(()) => watched_any = true,
            Err(err) => {
                eprintln!("Failed to watch {}: {err}", build_script.display());
            }
        }
    }

    if src_dir.exists() {
        match watcher.watch(&src_dir, RecursiveMode::Recursive) {
            Ok(()) => watched_any = true,
            Err(err) => {
                eprintln!("Failed to watch {}: {err}", src_dir.display());
            }
        }
    }

    if !watched_any {
        eprintln!(
            "Failed to start source watcher: no watchable paths in {}",
            manifest_dir.display()
        );
        return None;
    }

    Some(SourceWatcher {
        _watcher: watcher,
        rx,
    })
}

pub fn source_changed(watcher: &mut Option<SourceWatcher>) -> bool {
    let Some(watcher) = watcher.as_mut() else {
        return false;
    };
    let mut triggered = false;
    while watcher.rx.try_recv().is_ok() {
        triggered = true;
    }
    triggered
}

/// Shared development loop used by all `main!` variants.
#[cfg(debug_assertions)]
pub fn run_debug(
    state: &mut crate::AppState,
    host_manifest_dir: &Path,
    package_name: &str,
    runtime: crate::RuntimeHandle,
    mut make_runner: impl FnMut(
        &Path,
        crate::RuntimeHandle,
    ) -> Result<crate::FlowRunner, crate::RunnerError>,
    mut tick: impl FnMut(&crate::AppState),
) {
    use crate::{FlowRunner, RunStatus, RunnerError};
    use std::time::{Duration, Instant};

    let logic_manifest_dir = resolve_logic_manifest_dir(host_manifest_dir, package_name);
    let target_dir = default_target_dir(&logic_manifest_dir);
    let mut source_watcher = start_source_watcher(&logic_manifest_dir);
    let mut runner: Option<FlowRunner> = None;
    let mut schedule = BuildSchedule::default();

    loop {
        if schedule.should_build(Instant::now(), source_changed(&mut source_watcher)) {
            match build_logic(package_name, &logic_manifest_dir, &target_dir) {
                Ok(status) if status.success() => {
                    let logic_path = library_path_in(&target_dir, package_name);
                    match runner.as_mut() {
                        Some(active) => match active.reload_if_needed() {
                            Ok(true) => println!("Reloaded logic."),
                            Ok(false) => {}
                            Err(err) => eprintln!("FlowRunner reload error: {err}"),
                        },
                        None => match make_runner(&logic_path, runtime.clone()) {
                            Ok(new_runner) => runner = Some(new_runner),
                            Err(err) => eprintln!("Failed to start FlowRunner: {err}"),
                        },
                    }
                }
                Ok(_) => eprintln!("\x1b[31mBuild failed; waiting for changes...\x1b[0m"),
                Err(err) => eprintln!("\x1b[31mBuild error: {err}; waiting for changes...\x1b[0m"),
            }
        }

        if let Some(active) = runner.as_mut() {
            match active.run(state) {
                Ok(status) => {
                    if status == RunStatus::Error {
                        eprintln!("Logic returned error status.");
                    }
                    tick(state);
                }
                Err(err) => {
                    if matches!(
                        err,
                        RunnerError::AbiVersion { .. }
                            | RunnerError::StateSize { .. }
                            | RunnerError::AllocatorTag { .. }
                    ) {
                        eprintln!("\x1b[31mABI/state mismatch detected. Please rebuild and restart the host.\x1b[0m");
                        return;
                    }
                    eprintln!("FlowRunner error: {err}");
                    runtime.sleep(Duration::from_secs(1));
                    continue;
                }
            }
            match active.reload_if_needed() {
                Ok(true) => println!("Reloaded logic."),
                Ok(false) => {}
                Err(err) => {
                    eprintln!("FlowRunner reload error: {err}");
                    runtime.sleep(Duration::from_secs(1));
                }
            }
        }
        runtime.sleep(Duration::from_millis(16));
    }
}

#[cfg(any(debug_assertions, test))]
#[derive(Default)]
struct BuildSchedule {
    attempted: bool,
    last_change: Option<std::time::Instant>,
}

#[cfg(any(debug_assertions, test))]
impl BuildSchedule {
    fn should_build(&mut self, now: std::time::Instant, changed: bool) -> bool {
        if !self.attempted {
            self.attempted = true;
            return true;
        }
        if changed {
            self.last_change = Some(now);
        }
        if self
            .last_change
            .is_some_and(|last| now.duration_since(last) >= std::time::Duration::from_millis(300))
        {
            self.last_change = None;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn initial_build_is_attempted_once_until_sources_change() {
        let mut schedule = BuildSchedule::default();
        let now = Instant::now();
        assert!(schedule.should_build(now, false));
        assert!(!schedule.should_build(now + Duration::from_secs(1), false));
    }

    #[test]
    fn changes_are_debounced_from_the_last_event() {
        let mut schedule = BuildSchedule::default();
        let now = Instant::now();
        assert!(schedule.should_build(now, false));
        assert!(!schedule.should_build(now, true));
        assert!(!schedule.should_build(now + Duration::from_millis(200), true));
        assert!(!schedule.should_build(now + Duration::from_millis(300), false));
        assert!(schedule.should_build(now + Duration::from_millis(500), false));
        assert!(!schedule.should_build(now + Duration::from_secs(1), false));
    }
}
