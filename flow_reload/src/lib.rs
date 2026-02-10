mod attached;
mod runtime;
mod runner;

pub use hot_macro::{
    hot_init, hot_reload, hot_swap, reload_direct, reload_init, reload_snapshot, reloadable,
};
pub use interface::{
    AppState, AttachRuntimeFn, AttachedFuture, AttachedFutureVTable, AttachedSupportedFn,
    CartridgeState, FuncType, HostRuntimeVTable, HostWaker, HostWakerVTable, InitFn,
    MakeAttachedFutureFn, Metadata, ResetAttachedFn, RunStatus, ABI_VERSION,
    CARTRIDGE_CAPACITY,
};
pub use runner::{FlowRunner, RunnerError};
pub use attached::{
    attach_tokio_runtime, attached_runtime_available, clear_attached_host_waker,
    set_attached_host_waker, tokio_runtime_vtable, AttachedFutureTask, AttachedRuntimeGuard,
    AttachedSleep, AttachedTcpRequest, spawn_detached_sleep, spawn_detached_tcp_request,
};
pub use runtime::{RuntimeBridge, RuntimeHandle, StdRuntime};

pub const ALLOCATOR_TAG_SYSTEM: u32 = 1;

pub mod internal {
    use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
    use std::io;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::mpsc::{self, Receiver};

    pub struct SourceWatcher {
        _watcher: RecommendedWatcher,
        rx: Receiver<()>,
    }

    #[cfg(debug_assertions)]
    pub fn assert_context_type<T: 'static>(_: &mut T) {
        use std::any::type_name;
        if type_name::<T>() == "()" {
            panic!("context type must not be () when using context variants");
        }
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

    pub fn library_path_in(target_dir: &PathBuf, package_name: &str) -> PathBuf {
        target_dir.join("debug").join(library_file_name(package_name))
    }

    pub fn default_library_path(package_name: &str) -> PathBuf {
        library_path_in(&PathBuf::from("target"), package_name)
    }

    pub fn default_target_dir(manifest_dir: &PathBuf) -> PathBuf {
        manifest_dir.join("target").join("hot-reload-build")
    }

    pub fn resolve_logic_manifest_dir(host_manifest_dir: &PathBuf, package_name: &str) -> PathBuf {
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
        manifest_dir: &PathBuf,
        target_dir: &PathBuf,
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

    pub fn start_source_watcher(manifest_dir: &PathBuf) -> Option<SourceWatcher> {
        let src_dir = manifest_dir.join("src");
        let cargo_manifest = manifest_dir.join("Cargo.toml");
        let build_script = manifest_dir.join("build.rs");
        let (tx, rx) = mpsc::channel();
        let mut watcher = match notify::recommended_watcher(
            move |event: Result<notify::Event, notify::Error>| {
                let Ok(event) = event else {
                    return;
                };
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    let _ = tx.send(());
                }
            },
        ) {
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

        Some(SourceWatcher { _watcher: watcher, rx })
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

}

#[macro_export]
macro_rules! allocator {
    () => {
        #[cfg(target_os = "windows")]
        #[global_allocator]
        static FLOW_RELOAD_ALLOCATOR: std::alloc::System = std::alloc::System;

        #[cfg(target_os = "windows")]
        #[allow(dead_code)]
        const FLOW_RELOAD_ALLOCATOR_TAG: u32 = $crate::ALLOCATOR_TAG_SYSTEM;
    };
}

#[macro_export]
macro_rules! main {
    ($logic:ident) => {
        $crate::__main_impl!(@no_context $logic, $crate::AppState, {}, |_: &$crate::AppState| {});
    };
    ($logic:ident, runtime: $runtime:expr) => {
        $crate::__main_impl!(@no_context_rt $logic, $crate::AppState, $runtime, {}, |_: &$crate::AppState| {});
    };
    ($logic:ident, $state:ty) => {
        $crate::__main_impl!(@no_context $logic, $state, {}, |_: &$state| {});
    };
    ($logic:ident, $state:ty, runtime: $runtime:expr) => {
        $crate::__main_impl!(@no_context_rt $logic, $state, $runtime, {}, |_: &$state| {});
    };
    ($logic:ident, $state:ty, $init:block) => {
        $crate::__main_impl!(@no_context $logic, $state, $init, |_: &$state| {});
    };
    ($logic:ident, $state:ty, runtime: $runtime:expr, $init:block) => {
        $crate::__main_impl!(@no_context_rt $logic, $state, $runtime, $init, |_: &$state| {});
    };
    ($logic:ident, $state:ty, $init:block, $tick:expr) => {
        $crate::__main_impl!(@no_context $logic, $state, $init, $tick);
    };
    ($logic:ident, $state:ty, runtime: $runtime:expr, $init:block, $tick:expr) => {
        $crate::__main_impl!(@no_context_rt $logic, $state, $runtime, $init, $tick);
    };
    ($logic:ident, $state:ty, runtime: $runtime:expr, $tick:expr) => {
        $crate::__main_impl!(@no_context_rt $logic, $state, $runtime, {}, $tick);
    };
    ($logic:ident, $state:ty, $context:expr) => {
        $crate::__main_impl!(@with_context $logic, $state, $context, {}, |_: &$state| {});
    };
    ($logic:ident, $state:ty, $context:expr, runtime: $runtime:expr) => {
        $crate::__main_impl!(@with_context_rt $logic, $state, $context, $runtime, {}, |_: &$state| {});
    };
    ($logic:ident, $state:ty, $context:expr, $init:block) => {
        $crate::__main_impl!(@with_context $logic, $state, $context, $init, |_: &$state| {});
    };
    ($logic:ident, $state:ty, $context:expr, runtime: $runtime:expr, $init:block) => {
        $crate::__main_impl!(@with_context_rt $logic, $state, $context, $runtime, $init, |_: &$state| {});
    };
    ($logic:ident, $state:ty, $context:expr, $tick:expr) => {
        $crate::__main_impl!(@with_context $logic, $state, $context, {}, $tick);
    };
    ($logic:ident, $state:ty, $context:expr, runtime: $runtime:expr, $tick:expr) => {
        $crate::__main_impl!(@with_context_rt $logic, $state, $context, $runtime, {}, $tick);
    };
    ($logic:ident, $state:ty, $context:expr, $init:block, $tick:expr) => {
        $crate::__main_impl!(@with_context $logic, $state, $context, $init, $tick);
    };
    ($logic:ident, $state:ty, $context:expr, runtime: $runtime:expr, $init:block, $tick:expr) => {
        $crate::__main_impl!(@with_context_rt $logic, $state, $context, $runtime, $init, $tick);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __main_impl {
    (@no_context $logic:ident, $state:ty, $init:block, $tick:expr) => {
        fn main() {
            let mut state: $state = Default::default();
            let runtime = $crate::RuntimeHandle::std();
            $init

            #[cfg(debug_assertions)]
            {
                let host_manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                let logic_manifest_dir = $crate::internal::resolve_logic_manifest_dir(
                    &host_manifest_dir,
                    stringify!($logic),
                );
                let target_dir = $crate::internal::default_target_dir(&logic_manifest_dir);
                let mut source_watcher = $crate::internal::start_source_watcher(&logic_manifest_dir);
                let mut runner: Option<$crate::FlowRunner> = None;
                let mut built_once = false;
                let mut last_change: Option<std::time::Instant> = None;
                let debounce = std::time::Duration::from_millis(300);

                loop {
                    let mut build_now = false;
                    if !built_once {
                        build_now = true;
                    } else if $crate::internal::source_changed(&mut source_watcher) {
                        last_change = Some(std::time::Instant::now());
                    }

                    if let Some(instant) = last_change {
                        if instant.elapsed() >= debounce {
                            build_now = true;
                            last_change = None;
                        }
                    }

                    if build_now {
                        match $crate::internal::build_logic(
                            stringify!($logic),
                            &logic_manifest_dir,
                            &target_dir,
                        ) {
                            Ok(status) if status.success() => {
                                built_once = true;
                                let logic_path =
                                    $crate::internal::library_path_in(&target_dir, stringify!($logic));
                                match runner.as_mut() {
                                    Some(active) => {
                                        match active.reload_if_needed() {
                                            Ok(true) => println!("Reloaded logic."),
                                            Ok(false) => {}
                                            Err(err) => {
                                                eprintln!("FlowRunner reload error: {err}");
                                                runtime.sleep(std::time::Duration::from_secs(1));
                                                continue;
                                            }
                                        }
                                    }
                                    None => match $crate::FlowRunner::new(&logic_path) {
                                        Ok(new_runner) => {
                                            runner = Some(new_runner);
                                        }
                                        Err(err) => {
                                            eprintln!("Failed to start FlowRunner: {err}");
                                            runtime.sleep(std::time::Duration::from_secs(1));
                                            continue;
                                        }
                                    },
                                }
                            }
                            Ok(_) => {
                                eprintln!("\x1b[31mBuild failed; waiting for changes...\x1b[0m");
                                runtime.sleep(std::time::Duration::from_secs(1));
                                continue;
                            }
                            Err(err) => {
                                eprintln!("\x1b[31mBuild error: {err}\x1b[0m");
                                runtime.sleep(std::time::Duration::from_secs(1));
                                continue;
                            }
                        }
                    }

                    let Some(active) = runner.as_mut() else {
                        runtime.sleep(std::time::Duration::from_millis(16));
                        continue;
                    };

                    match active.run(&mut state) {
                        Ok(status) => {
                            if status == $crate::RunStatus::Error {
                                eprintln!("Logic returned error status.");
                            }
                            ($tick)(&state);
                        }
                        Err(err) => {
                            let requires_restart = matches!(
                                err,
                                $crate::RunnerError::AbiVersion { .. }
                                    | $crate::RunnerError::StateSize { .. }
                                    | $crate::RunnerError::AllocatorTag { .. }
                            );
                            if requires_restart {
                                eprintln!(
                                    "\x1b[31mABI/state mismatch detected. Please rebuild and restart the host.\x1b[0m"
                                );
                                return;
                            }
                            eprintln!("FlowRunner error: {err}");
                            runtime.sleep(std::time::Duration::from_secs(1));
                            continue;
                        }
                    }

                    match active.reload_if_needed() {
                        Ok(true) => println!("Reloaded logic."),
                        Ok(false) => {}
                        Err(err) => {
                            eprintln!("FlowRunner reload error: {err}");
                            runtime.sleep(std::time::Duration::from_secs(1));
                            continue;
                        }
                    }
                    runtime.sleep(std::time::Duration::from_millis(16));
                }
            }

            #[cfg(not(debug_assertions))]
            {
                loop {
                    let status = unsafe { $logic::hot_entry(&mut state) };
                    match status {
                        $crate::RunStatus::Pending => {
                            runtime.sleep(std::time::Duration::from_millis(16));
                        }
                        $crate::RunStatus::Error => {
                            eprintln!("Logic returned error status.");
                        }
                        $crate::RunStatus::Done => {}
                    }
                    ($tick)(&state);
                }
            }
        }
    };
    (@no_context_rt $logic:ident, $state:ty, $runtime:expr, $init:block, $tick:expr) => {
        fn main() {
            let mut state: $state = Default::default();
            let runtime: $crate::RuntimeHandle = $runtime.into();
            $init

            #[cfg(debug_assertions)]
            {
                let host_manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                let logic_manifest_dir = $crate::internal::resolve_logic_manifest_dir(
                    &host_manifest_dir,
                    stringify!($logic),
                );
                let target_dir = $crate::internal::default_target_dir(&logic_manifest_dir);
                let mut source_watcher = $crate::internal::start_source_watcher(&logic_manifest_dir);
                let mut runner: Option<$crate::FlowRunner> = None;
                let mut built_once = false;
                let mut last_change: Option<std::time::Instant> = None;
                let debounce = std::time::Duration::from_millis(300);

                loop {
                    let mut build_now = false;
                    if !built_once {
                        build_now = true;
                    } else if $crate::internal::source_changed(&mut source_watcher) {
                        last_change = Some(std::time::Instant::now());
                    }

                    if let Some(instant) = last_change {
                        if instant.elapsed() >= debounce {
                            build_now = true;
                            last_change = None;
                        }
                    }

                    if build_now {
                        match $crate::internal::build_logic(
                            stringify!($logic),
                            &logic_manifest_dir,
                            &target_dir,
                        ) {
                            Ok(status) if status.success() => {
                                built_once = true;
                                let logic_path =
                                    $crate::internal::library_path_in(&target_dir, stringify!($logic));
                                match runner.as_mut() {
                                    Some(active) => {
                                        match active.reload_if_needed() {
                                            Ok(true) => println!("Reloaded logic."),
                                            Ok(false) => {}
                                            Err(err) => {
                                                eprintln!("FlowRunner reload error: {err}");
                                                runtime.sleep(std::time::Duration::from_secs(1));
                                                continue;
                                            }
                                        }
                                    }
                                    None => match $crate::FlowRunner::with_runtime(
                                        &logic_path,
                                        runtime.clone(),
                                    ) {
                                        Ok(new_runner) => {
                                            runner = Some(new_runner);
                                        }
                                        Err(err) => {
                                            eprintln!("Failed to start FlowRunner: {err}");
                                            runtime.sleep(std::time::Duration::from_secs(1));
                                            continue;
                                        }
                                    },
                                }
                            }
                            Ok(_) => {
                                eprintln!("\x1b[31mBuild failed; waiting for changes...\x1b[0m");
                                runtime.sleep(std::time::Duration::from_secs(1));
                                continue;
                            }
                            Err(err) => {
                                eprintln!("\x1b[31mBuild error: {err}\x1b[0m");
                                runtime.sleep(std::time::Duration::from_secs(1));
                                continue;
                            }
                        }
                    }

                    let Some(active) = runner.as_mut() else {
                        runtime.sleep(std::time::Duration::from_millis(16));
                        continue;
                    };

                    match active.run(&mut state) {
                        Ok(status) => {
                            if status == $crate::RunStatus::Error {
                                eprintln!("Logic returned error status.");
                            }
                            ($tick)(&state);
                        }
                        Err(err) => {
                            let requires_restart = matches!(
                                err,
                                $crate::RunnerError::AbiVersion { .. }
                                    | $crate::RunnerError::StateSize { .. }
                                    | $crate::RunnerError::AllocatorTag { .. }
                            );
                            if requires_restart {
                                eprintln!(
                                    "\x1b[31mABI/state mismatch detected. Please rebuild and restart the host.\x1b[0m"
                                );
                                return;
                            }
                            eprintln!("FlowRunner error: {err}");
                            runtime.sleep(std::time::Duration::from_secs(1));
                            continue;
                        }
                    }

                    match active.reload_if_needed() {
                        Ok(true) => println!("Reloaded logic."),
                        Ok(false) => {}
                        Err(err) => {
                            eprintln!("FlowRunner reload error: {err}");
                            runtime.sleep(std::time::Duration::from_secs(1));
                            continue;
                        }
                    }
                    runtime.sleep(std::time::Duration::from_millis(16));
                }
            }

            #[cfg(not(debug_assertions))]
            {
                loop {
                    let status = unsafe { $logic::hot_entry(&mut state) };
                    match status {
                        $crate::RunStatus::Pending => {
                            runtime.sleep(std::time::Duration::from_millis(16));
                        }
                        $crate::RunStatus::Error => {
                            eprintln!("Logic returned error status.");
                        }
                        $crate::RunStatus::Done => {}
                    }
                    ($tick)(&state);
                }
            }
        }
    };
    (@with_context $logic:ident, $state:ty, $context:expr, $init:block, $tick:expr) => {
        fn main() {
            let mut state: $state = Default::default();
            let mut context = $context;
            let runtime = $crate::RuntimeHandle::std();
            #[cfg(debug_assertions)]
            $crate::internal::assert_context_type(&mut context);
            $init

            #[cfg(debug_assertions)]
            {
                let host_manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                let logic_manifest_dir = $crate::internal::resolve_logic_manifest_dir(
                    &host_manifest_dir,
                    stringify!($logic),
                );
                let target_dir = $crate::internal::default_target_dir(&logic_manifest_dir);
                let mut source_watcher = $crate::internal::start_source_watcher(&logic_manifest_dir);
                let mut runner: Option<$crate::FlowRunner> = None;
                let mut built_once = false;
                let mut last_change: Option<std::time::Instant> = None;
                let debounce = std::time::Duration::from_millis(300);

                loop {
                    let mut build_now = false;
                    if !built_once {
                        build_now = true;
                    } else if $crate::internal::source_changed(&mut source_watcher) {
                        last_change = Some(std::time::Instant::now());
                    }

                    if let Some(instant) = last_change {
                        if instant.elapsed() >= debounce {
                            build_now = true;
                            last_change = None;
                        }
                    }

                    if build_now {
                        match $crate::internal::build_logic(
                            stringify!($logic),
                            &logic_manifest_dir,
                            &target_dir,
                        ) {
                            Ok(status) if status.success() => {
                                built_once = true;
                                let logic_path =
                                    $crate::internal::library_path_in(&target_dir, stringify!($logic));
                                match runner.as_mut() {
                                    Some(active) => {
                                        match active.reload_if_needed() {
                                            Ok(true) => println!("Reloaded logic."),
                                            Ok(false) => {}
                                            Err(err) => {
                                                eprintln!("FlowRunner reload error: {err}");
                                                runtime.sleep(std::time::Duration::from_secs(1));
                                                continue;
                                            }
                                        }
                                    }
                                    None => match $crate::FlowRunner::with_context_and_runtime(
                                        &logic_path,
                                        &mut context,
                                        runtime.clone(),
                                    ) {
                                        Ok(new_runner) => {
                                            runner = Some(new_runner);
                                        }
                                        Err(err) => {
                                            eprintln!("Failed to start FlowRunner: {err}");
                                            runtime.sleep(std::time::Duration::from_secs(1));
                                            continue;
                                        }
                                    },
                                }
                            }
                            Ok(_) => {
                                eprintln!("\x1b[31mBuild failed; waiting for changes...\x1b[0m");
                                runtime.sleep(std::time::Duration::from_secs(1));
                                continue;
                            }
                            Err(err) => {
                                eprintln!("\x1b[31mBuild error: {err}\x1b[0m");
                                runtime.sleep(std::time::Duration::from_secs(1));
                                continue;
                            }
                        }
                    }

                    let Some(active) = runner.as_mut() else {
                        runtime.sleep(std::time::Duration::from_millis(16));
                        continue;
                    };

                    match active.run(&mut state) {
                        Ok(status) => {
                            if status == $crate::RunStatus::Error {
                                eprintln!("Logic returned error status.");
                            }
                            ($tick)(&state);
                        }
                        Err(err) => {
                            let requires_restart = matches!(
                                err,
                                $crate::RunnerError::AbiVersion { .. }
                                    | $crate::RunnerError::StateSize { .. }
                                    | $crate::RunnerError::AllocatorTag { .. }
                            );
                            if requires_restart {
                                eprintln!(
                                    "\x1b[31mABI/state mismatch detected. Please rebuild and restart the host.\x1b[0m"
                                );
                                return;
                            }
                            eprintln!("FlowRunner error: {err}");
                            runtime.sleep(std::time::Duration::from_secs(1));
                            continue;
                        }
                    }

                    match active.reload_if_needed() {
                        Ok(true) => println!("Reloaded logic."),
                        Ok(false) => {}
                        Err(err) => {
                            eprintln!("FlowRunner reload error: {err}");
                            runtime.sleep(std::time::Duration::from_secs(1));
                            continue;
                        }
                    }
                    runtime.sleep(std::time::Duration::from_millis(16));
                }
            }

            #[cfg(not(debug_assertions))]
            {
                let ctx_ptr = (&mut context as *mut _) as *mut std::ffi::c_void;
                let status = unsafe { $logic::hot_init(ctx_ptr) };
                if status != $crate::RunStatus::Done {
                    eprintln!("hot_init failed in release mode: {status:?}");
                    return;
                }
                loop {
                    let status = unsafe { $logic::hot_entry(&mut state) };
                    match status {
                        $crate::RunStatus::Pending => {
                            runtime.sleep(std::time::Duration::from_millis(16));
                        }
                        $crate::RunStatus::Error => {
                            eprintln!("Logic returned error status.");
                        }
                        $crate::RunStatus::Done => {}
                    }
                    ($tick)(&state);
                }
            }
        }
    };
    (@with_context_rt $logic:ident, $state:ty, $context:expr, $runtime:expr, $init:block, $tick:expr) => {
        fn main() {
            let mut state: $state = Default::default();
            let mut context = $context;
            let runtime: $crate::RuntimeHandle = $runtime.into();
            #[cfg(debug_assertions)]
            $crate::internal::assert_context_type(&mut context);
            $init

            #[cfg(debug_assertions)]
            {
                let host_manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                let logic_manifest_dir = $crate::internal::resolve_logic_manifest_dir(
                    &host_manifest_dir,
                    stringify!($logic),
                );
                let target_dir = $crate::internal::default_target_dir(&logic_manifest_dir);
                let mut source_watcher = $crate::internal::start_source_watcher(&logic_manifest_dir);
                let mut runner: Option<$crate::FlowRunner> = None;
                let mut built_once = false;
                let mut last_change: Option<std::time::Instant> = None;
                let debounce = std::time::Duration::from_millis(300);

                loop {
                    let mut build_now = false;
                    if !built_once {
                        build_now = true;
                    } else if $crate::internal::source_changed(&mut source_watcher) {
                        last_change = Some(std::time::Instant::now());
                    }

                    if let Some(instant) = last_change {
                        if instant.elapsed() >= debounce {
                            build_now = true;
                            last_change = None;
                        }
                    }

                    if build_now {
                        match $crate::internal::build_logic(
                            stringify!($logic),
                            &logic_manifest_dir,
                            &target_dir,
                        ) {
                            Ok(status) if status.success() => {
                                built_once = true;
                                let logic_path =
                                    $crate::internal::library_path_in(&target_dir, stringify!($logic));
                                match runner.as_mut() {
                                    Some(active) => {
                                        match active.reload_if_needed() {
                                            Ok(true) => println!("Reloaded logic."),
                                            Ok(false) => {}
                                            Err(err) => {
                                                eprintln!("FlowRunner reload error: {err}");
                                                runtime.sleep(std::time::Duration::from_secs(1));
                                                continue;
                                            }
                                        }
                                    }
                                    None => match $crate::FlowRunner::with_context_and_runtime(
                                        &logic_path,
                                        &mut context,
                                        runtime.clone(),
                                    ) {
                                        Ok(new_runner) => {
                                            runner = Some(new_runner);
                                        }
                                        Err(err) => {
                                            eprintln!("Failed to start FlowRunner: {err}");
                                            runtime.sleep(std::time::Duration::from_secs(1));
                                            continue;
                                        }
                                    },
                                }
                            }
                            Ok(_) => {
                                eprintln!("\x1b[31mBuild failed; waiting for changes...\x1b[0m");
                                runtime.sleep(std::time::Duration::from_secs(1));
                                continue;
                            }
                            Err(err) => {
                                eprintln!("\x1b[31mBuild error: {err}\x1b[0m");
                                runtime.sleep(std::time::Duration::from_secs(1));
                                continue;
                            }
                        }
                    }

                    let Some(active) = runner.as_mut() else {
                        runtime.sleep(std::time::Duration::from_millis(16));
                        continue;
                    };

                    match active.run(&mut state) {
                        Ok(status) => {
                            if status == $crate::RunStatus::Error {
                                eprintln!("Logic returned error status.");
                            }
                            ($tick)(&state);
                        }
                        Err(err) => {
                            let requires_restart = matches!(
                                err,
                                $crate::RunnerError::AbiVersion { .. }
                                    | $crate::RunnerError::StateSize { .. }
                                    | $crate::RunnerError::AllocatorTag { .. }
                            );
                            if requires_restart {
                                eprintln!(
                                    "\x1b[31mABI/state mismatch detected. Please rebuild and restart the host.\x1b[0m"
                                );
                                return;
                            }
                            eprintln!("FlowRunner error: {err}");
                            runtime.sleep(std::time::Duration::from_secs(1));
                            continue;
                        }
                    }

                    match active.reload_if_needed() {
                        Ok(true) => println!("Reloaded logic."),
                        Ok(false) => {}
                        Err(err) => {
                            eprintln!("FlowRunner reload error: {err}");
                            runtime.sleep(std::time::Duration::from_secs(1));
                            continue;
                        }
                    }
                    runtime.sleep(std::time::Duration::from_millis(16));
                }
            }

            #[cfg(not(debug_assertions))]
            {
                let ctx_ptr = (&mut context as *mut _) as *mut std::ffi::c_void;
                let status = unsafe { $logic::hot_init(ctx_ptr) };
                if status != $crate::RunStatus::Done {
                    eprintln!("hot_init failed in release mode: {status:?}");
                    return;
                }
                loop {
                    let status = unsafe { $logic::hot_entry(&mut state) };
                    match status {
                        $crate::RunStatus::Pending => {
                            runtime.sleep(std::time::Duration::from_millis(16));
                        }
                        $crate::RunStatus::Error => {
                            eprintln!("Logic returned error status.");
                        }
                        $crate::RunStatus::Done => {}
                    }
                    ($tick)(&state);
                }
            }
        }
    };
}
