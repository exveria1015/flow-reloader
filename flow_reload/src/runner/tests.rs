// Native fixtures and their replacements are built below with the SDK ABI;
// each unsafe constructor is limited to those trusted test artifacts.
use super::*;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn leaked_counter() -> &'static AtomicUsize {
    // Keep DLL termination probes valid even if a retained module closes only
    // during process shutdown or an assertion fails before explicit cleanup.
    Box::leak(Box::new(AtomicUsize::new(0)))
}

fn observe_unload(runner: &FlowRunner, counter: &'static AtomicUsize) {
    unsafe {
        let set = runner
            .loaded_ref()
            .image
            .library()
            .get::<unsafe extern "C" fn(*mut AtomicUsize)>(b"test_set_unload_counter")
            .unwrap();
        set((counter as *const AtomicUsize).cast_mut());
    }
}

fn wait_for(counter: &AtomicUsize, value: usize) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while counter.load(Ordering::Acquire) != value {
        assert!(
            std::time::Instant::now() < deadline,
            "worker did not reach expected state"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn default_retains_dll_while_unregistered_thread_outlives_reload_and_runner() {
    let (_dir, path) = async_fixture("unmanaged_thread");
    let mut runner = unsafe { FlowRunner::new(&path) }.unwrap();
    assert_eq!(runner.unload_policy(), UnloadPolicy::KeepLoaded);
    let unloads = leaked_counter();
    observe_unload(&runner, unloads);
    let old_copy = runner.loaded_ref().image.copy_path().to_path_buf();
    let started = leaked_counter();
    let gate = leaked_counter();
    let finished = leaked_counter();
    unsafe {
        let spawn = runner.loaded_ref().image.library().get::<unsafe extern "C" fn(*const AtomicUsize, *const AtomicUsize, *const AtomicUsize)>(b"test_spawn_unmanaged").unwrap();
        spawn(started, gate, finished);
    }
    wait_for(started, 1);
    runner.loaded.as_mut().unwrap().source_mtime = UNIX_EPOCH;
    assert!(runner.reload_if_needed().unwrap());
    assert_eq!(unloads.load(Ordering::Relaxed), 0);
    assert!(old_copy.exists());
    drop(runner);
    unsafe {
        collect_retired_libraries();
    }
    assert_eq!(unloads.load(Ordering::Relaxed), 0);
    gate.store(1, Ordering::Release);
    wait_for(finished, 1);
    assert!(old_copy.exists());
}

#[test]
fn default_retains_failed_initialization_that_started_unregistered_work() {
    #[repr(C)]
    struct SpawnSignals {
        started: *const AtomicUsize,
        gate: *const AtomicUsize,
        finished: *const AtomicUsize,
    }
    let (_dir, path) = async_fixture("init_spawns_then_fails");
    let started = leaked_counter();
    let gate = leaked_counter();
    let finished = leaked_counter();
    let mut signals = SpawnSignals {
        started,
        gate,
        finished,
    };
    assert!(matches!(
        unsafe { FlowRunner::with_context(&path, &mut signals) },
        Err(RunnerError::InitFailed)
    ));
    wait_for(started, 1);
    // Only KeepLoaded generations were created on this test thread.
    assert_eq!(unsafe { collect_retired_libraries() }, 0);
    gate.store(1, Ordering::Release);
    wait_for(finished, 1);
}

#[test]
fn metadata_mismatches_are_rejected() {
    let valid = Metadata {
        abi_version: ABI_VERSION,
        func_type: FuncType::Sync,
        state_size: std::mem::size_of::<AppState>() as u32,
        allocator_tag: ALLOCATOR_TAG_SYSTEM,
    };
    assert!(validate_metadata(valid).is_ok());
    assert!(matches!(
        validate_metadata(Metadata {
            abi_version: ABI_VERSION + 1,
            ..valid
        }),
        Err(RunnerError::AbiVersion { .. })
    ));
    assert!(matches!(
        validate_metadata(Metadata {
            state_size: 0,
            ..valid
        }),
        Err(RunnerError::StateSize { .. })
    ));
    #[cfg(target_os = "windows")]
    assert!(matches!(
        validate_metadata(Metadata {
            allocator_tag: 0,
            ..valid
        }),
        Err(RunnerError::AllocatorTag { .. })
    ));
}

#[test]
fn library_lifecycle_cleans_up_and_preserves_active_logic_on_failed_reload() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture_dir =
        std::env::temp_dir().join(format!("flow_reload_{}_{nonce}", std::process::id()));
    std::fs::create_dir(&fixture_dir).unwrap();
    let dir_guard = FixtureDirectory(fixture_dir);
    let source = dir_guard
        .0
        .join(crate::internal::library_file_name(&format!(
            "fixture_{nonce}"
        )));
    let compile = |cfg| compile_fixture(&source, cfg);

    compile(None);
    let mut context = 0u64;
    let mut state = AppState::default();
    let mut runner = FlowRunner::load_with_policy(
        &source,
        (&mut context as *mut u64).cast(),
        RuntimeHandle::std(),
        UnloadPolicy::WhenIdle,
    )
    .unwrap();
    assert_eq!(context, 10);
    assert_eq!(runner.run(&mut state).unwrap(), RunStatus::Done);
    assert_eq!(state.counter, 1);
    let first_copy = runner.loaded_ref().image.copy_path().to_path_buf();
    assert!(first_copy.exists());

    compile(Some("bad_abi"));
    runner.loaded.as_mut().unwrap().source_mtime = UNIX_EPOCH;
    assert!(matches!(
        runner.reload_if_needed(),
        Err(RunnerError::AbiVersion { .. })
    ));
    assert_eq!(runner.run(&mut state).unwrap(), RunStatus::Done);
    assert_eq!(state.counter, 2);
    assert!(first_copy.exists());

    compile(None);
    assert!(runner.reload_if_needed().unwrap());
    let second_copy = runner.loaded_ref().image.copy_path().to_path_buf();
    unsafe {
        collect_retired_libraries();
    }
    assert!(!first_copy.exists());
    assert_eq!(context, 120);
    drop(runner);
    unsafe {
        collect_retired_libraries();
    }
    assert_eq!(context, 220);
    assert!(!second_copy.exists());

    context = u64::MAX;
    assert!(matches!(
        FlowRunner::load_with_policy(
            &source,
            (&mut context as *mut u64).cast(),
            RuntimeHandle::std(),
            UnloadPolicy::WhenIdle
        ),
        Err(RunnerError::InitFailed)
    ));
    compile(Some("missing_entry"));
    assert!(matches!(
        FlowRunner::load_with_policy(
            &source,
            std::ptr::null_mut(),
            RuntimeHandle::std(),
            UnloadPolicy::WhenIdle
        ),
        Err(RunnerError::LibLoading(_))
    ));
    std::fs::write(&source, b"not a dynamic library").unwrap();
    assert!(matches!(
        FlowRunner::load_with_policy(
            &source,
            std::ptr::null_mut(),
            RuntimeHandle::std(),
            UnloadPolicy::WhenIdle
        ),
        Err(RunnerError::LibLoading(_))
    ));

    unsafe {
        collect_retired_libraries();
    }
    // No rejected or successfully unloaded copy from this fixture may remain.
    let marker = format!("fixture_{nonce}");
    for entry in std::fs::read_dir(second_copy.parent().unwrap()).unwrap() {
        assert!(!entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(&marker));
    }
}

struct FixtureDirectory(PathBuf);

impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn compile_fixture(source: &Path, cfg: Option<&str>) {
    let mut command = Command::new("rustc");
    command
        .args([
            "--crate-type",
            "cdylib",
            "--edition",
            "2021",
            "--crate-name",
            "reload_fixture",
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/logic.rs"))
        .env(
            "FLOW_RELOAD_INTERFACE_SOURCE",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../interface/src/lib.rs"),
        )
        .arg("-o")
        .arg(source);
    if let Some(cfg) = cfg {
        command.arg("--cfg").arg(cfg);
    }
    let output = command
        .output()
        .expect("rustc must be available to build the test fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn async_fixture(cfg: &str) -> (FixtureDirectory, PathBuf) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = FixtureDirectory(
        std::env::temp_dir().join(format!("flow_reload_async_{}_{nonce}", std::process::id())),
    );
    std::fs::create_dir(&dir.0).unwrap();
    let path = dir
        .0
        .join(crate::internal::library_file_name("async_fixture"));
    compile_fixture(&path, Some(cfg));
    (dir, path)
}

#[test]
fn async_single_step_is_rejected_and_host_unwind_cancels_before_releasing_state() {
    struct PanickingRuntime;
    impl crate::RuntimeBridge for PanickingRuntime {
        fn sleep(&self, _: Duration) {
            panic!("host sleep panic");
        }
    }
    let (_dir, path) = async_fixture("async_entry");
    let mut runner =
        unsafe { FlowRunner::with_runtime(&path, RuntimeHandle::new(PanickingRuntime)) }.unwrap();
    let mut state = AppState::default();
    assert!(matches!(
        runner.run_once(&mut state),
        Err(RunnerError::AsyncRequiresRun)
    ));
    assert_eq!(state.counter, 0);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runner.run(&mut state)));
    assert!(result.is_err());
    assert_eq!(
        state.counter, 11,
        "pending future was not reset before state was released"
    );
    drop(runner);
    assert_eq!(state.counter, 11);
    let mut runner = unsafe { FlowRunner::new(&path) }.unwrap();
    state.counter = 0;
    assert_eq!(runner.run(&mut state).unwrap(), RunStatus::Done);
    assert_eq!(state.counter, 2);
}

#[test]
fn independent_executions_keep_retired_generation_until_completion_or_cancellation() {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, Waker};

    let (_dir, path) = async_fixture("async_entry");
    let mut runner = unsafe { FlowRunner::new(&path) }.unwrap();
    // Fixture has no TLS or unmanaged work. All calls return on this thread
    // before explicit collection below.
    unsafe {
        runner.set_unload_policy(UnloadPolicy::WhenIdle);
    }
    let unloads = leaked_counter();
    observe_unload(&runner, unloads);
    let mut old = runner.start(AppState::default()).unwrap();
    let mut cancelled = runner.start(AppState::default()).unwrap();
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(Pin::new(&mut old).poll(&mut cx), Poll::Pending);
    assert_eq!(Pin::new(&mut cancelled).poll(&mut cx), Poll::Pending);
    runner.loaded.as_mut().unwrap().source_mtime = UNIX_EPOCH;
    assert!(runner.reload_if_needed().unwrap());
    let mut new = runner.start(AppState::default()).unwrap();
    drop(runner);
    assert_eq!(unsafe { collect_retired_libraries() }, 0);
    assert_eq!(cancelled.cancel().counter, 11);
    assert_eq!(
        Pin::new(&mut old).poll(&mut cx),
        Poll::Ready(RunStatus::Done)
    );
    assert_eq!(old.state().unwrap().counter, 2);
    drop(old);
    assert_eq!(unsafe { collect_retired_libraries() }, 1);
    assert_eq!(unloads.load(Ordering::Relaxed), 1);
    assert_eq!(Pin::new(&mut new).poll(&mut cx), Poll::Pending);
    assert_eq!(
        Pin::new(&mut new).poll(&mut cx),
        Poll::Ready(RunStatus::Done)
    );
    drop(new);
    assert_eq!(unsafe { collect_retired_libraries() }, 1);
}

#[cfg(any(feature = "tokio", feature = "async-std", feature = "smol"))]
mod attached {
    use super::*;
    use crate::{
        AttachedFuture, AttachedFutureTask, AttachedFutureVTable, HostRuntimeVTable, HostWaker,
    };
    use std::cell::Cell;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, Waker};

    #[derive(Default)]
    struct Handle {
        calls: Cell<usize>,
        drops: Cell<usize>,
        panic: bool,
    }

    unsafe extern "C" fn make_sleep(ctx: *mut c_void, _: u64) -> AttachedFuture {
        let handle = &*ctx.cast::<Handle>();
        handle.calls.set(handle.calls.get() + 1);
        AttachedFuture {
            data: Box::into_raw(Box::new(ctx)).cast(),
            vtable: &VTABLE,
        }
    }

    unsafe extern "C" fn poll(_: *mut c_void, _: *const HostWaker) -> RunStatus {
        RunStatus::Done
    }

    unsafe extern "C" fn drop_future(data: *mut c_void) {
        let ctx = Box::from_raw(data.cast::<*mut c_void>());
        let handle = &*(*ctx).cast::<Handle>();
        handle.drops.set(handle.drops.get() + 1);
    }

    static VTABLE: AttachedFutureVTable = AttachedFutureVTable {
        poll,
        drop: drop_future,
    };

    struct Adapter;
    impl RuntimeAdapter for Adapter {
        type Handle = Handle;
        fn with_entered<F: FnOnce() -> R, R>(_: &Handle, f: F) -> R {
            f()
        }
        fn vtable(handle: &Handle) -> HostRuntimeVTable {
            let mut table = crate::attached::async_std_runtime_vtable();
            table.ctx = (handle as *const Handle).cast_mut().cast();
            table.sleep_ms = make_sleep;
            table
        }
        fn block_on(handle: &Handle, mut task: AttachedFutureTask) -> RunStatus {
            assert!(!handle.panic, "executor panic");
            match Pin::new(&mut task).poll(&mut Context::from_waker(Waker::noop())) {
                Poll::Ready(status) => status,
                Poll::Pending => panic!("probe task unexpectedly pending"),
            }
        }
    }

    #[test]
    fn retained_dll_waker_delays_actual_unload_until_owner_thread_collection() {
        let (_dir, path) = async_fixture("retained_waker");
        let mut runner = unsafe { FlowRunner::new(&path) }.unwrap();
        // This fixture has no independent work; its only escaped resource is a
        // HostWaker clone, explicitly released below before owner collection.
        unsafe {
            runner.set_unload_policy(UnloadPolicy::WhenIdle);
        }
        let unloads = leaked_counter();
        observe_unload(&runner, unloads);
        let old_copy = runner.loaded_ref().image.copy_path().to_path_buf();
        let (wake, release) = unsafe {
            let lib = runner.loaded_ref().image.library();
            (
                *lib.get::<unsafe extern "C" fn()>(b"test_wake_retained")
                    .unwrap(),
                *lib.get::<unsafe extern "C" fn()>(b"test_release_waker")
                    .unwrap(),
            )
        };
        let mut state = AppState::default();
        assert_eq!(
            runner
                .run_attached_with_adapter::<Adapter>(&mut state, &Handle::default())
                .unwrap(),
            RunStatus::Done
        );
        runner.loaded.as_mut().unwrap().source_mtime = UNIX_EPOCH;
        assert!(runner.reload_if_needed().unwrap());
        drop(runner);
        // The replacement has no leases and can close; the old image cannot.
        assert_eq!(unsafe { collect_retired_libraries() }, 1);
        assert_eq!(unloads.load(Ordering::Relaxed), 0);
        assert!(old_copy.exists());
        // Run callbacks inside the retired DLL, then try collecting from that
        // foreign thread. It must not unload code on its own callback stack.
        let foreign_count = std::thread::spawn(move || {
            unsafe {
                wake();
                release();
            }
            unsafe { collect_retired_libraries() }
        })
        .join()
        .unwrap();
        assert_eq!(foreign_count, 0);
        assert_eq!(unloads.load(Ordering::Relaxed), 0);
        assert_eq!(unsafe { collect_retired_libraries() }, 1);
        assert_eq!(
            unloads.load(Ordering::Relaxed),
            1,
            "DLL termination routine was not called"
        );
        assert!(!old_copy.exists());
    }

    #[cfg(feature = "tokio")]
    #[test]
    fn unregistered_tokio_task_can_poll_and_drop_dll_future_after_runner_drop() {
        let (_dir, path) = async_fixture("unmanaged_future");
        let mut runner = unsafe { FlowRunner::new(&path) }.unwrap();
        let gate = leaked_counter();
        let finished = leaked_counter();
        let unloads = leaked_counter();
        observe_unload(&runner, unloads);
        let raw = unsafe {
            let make = runner.loaded_ref().image.library().get::<unsafe extern "C" fn(*const AtomicUsize, *const AtomicUsize) -> AttachedFuture>(b"test_make_unmanaged_future").unwrap();
            make(gate, finished)
        };
        // Deliberately bypass FlowRunner's generation lease to model an
        // unregistered spawn. KeepLoaded keeps this DLL's poll/drop code alive.
        let task = unsafe { AttachedFutureTask::new(raw) }.unwrap();
        let local = tokio::task::LocalSet::new();
        let join = local.spawn_local(task);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime.block_on(local.run_until(async {
            tokio::task::yield_now().await;
            assert!(!join.is_finished());
            runner.loaded.as_mut().unwrap().source_mtime = UNIX_EPOCH;
            assert!(runner.reload_if_needed().unwrap());
            drop(runner);
            unsafe {
                collect_retired_libraries();
            }
            assert_eq!(unloads.load(Ordering::Relaxed), 0);
            gate.store(1, Ordering::Release);
            assert_eq!(join.await.unwrap(), RunStatus::Done);
        }));
        assert_eq!(finished.load(Ordering::Acquire), 2);
        assert_eq!(unloads.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn each_execution_uses_current_handle_and_detaches_after_completion_or_unwind() {
        let (_dir, path) = async_fixture("attached_entry");
        let mut runner = unsafe { FlowRunner::new(&path) }.unwrap();
        let mut state = AppState::default();
        let first = Handle::default();
        let second = Handle::default();
        let attached = unsafe {
            *runner
                .loaded_ref()
                .image
                .library()
                .get::<unsafe extern "C" fn() -> bool>(b"test_runtime_attached")
                .unwrap()
        };
        for handle in [&first, &second] {
            assert_eq!(
                runner
                    .run_attached_with_adapter::<Adapter>(&mut state, handle)
                    .unwrap(),
                RunStatus::Done
            );
            assert!(!unsafe { attached() });
        }
        assert_eq!(first.calls.get(), 1);
        assert_eq!(second.calls.get(), 1);
        assert_eq!(first.drops.get(), 1);
        assert_eq!(second.drops.get(), 1);
        let panicking = Handle {
            panic: true,
            ..Handle::default()
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runner.run_attached_with_adapter::<Adapter>(&mut state, &panicking)
        }));
        assert!(result.is_err());
        assert_eq!(panicking.drops.get(), 1);
        assert!(!unsafe { attached() });
    }
}
