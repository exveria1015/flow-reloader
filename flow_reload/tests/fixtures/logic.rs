#![allow(dead_code)]

mod interface {
    include!(env!("FLOW_RELOAD_INTERFACE_SOURCE"));
}

use interface::{
    AppState, AttachedFuture, AttachedFutureVTable, FuncType, HostRuntimeVTable, HostWaker,
    Metadata, RunStatus, ABI_VERSION,
};
use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

static CONTEXT: AtomicPtr<u64> = AtomicPtr::new(std::ptr::null_mut());
static RUNTIME: AtomicPtr<HostRuntimeVTable> = AtomicPtr::new(std::ptr::null_mut());
static UNLOAD_COUNTER: AtomicPtr<AtomicUsize> = AtomicPtr::new(std::ptr::null_mut());
static RETAINED_WAKER: AtomicPtr<HostWaker> = AtomicPtr::new(std::ptr::null_mut());

#[no_mangle]
pub extern "C" fn hot_type() -> Metadata {
    Metadata {
        abi_version: if cfg!(bad_abi) {
            ABI_VERSION + 1
        } else {
            ABI_VERSION
        },
        func_type: if cfg!(async_entry) || cfg!(attached_entry) || cfg!(retained_waker) {
            FuncType::Async
        } else {
            FuncType::Sync
        },
        state_size: std::mem::size_of::<AppState>() as u32,
        allocator_tag: 1,
    }
}

#[cfg(not(missing_entry))]
#[no_mangle]
pub unsafe extern "C" fn hot_entry(state: *mut AppState) -> RunStatus {
    (*state).counter += 1;
    RunStatus::Done
}

#[no_mangle]
pub unsafe extern "C" fn hot_init(context: *mut c_void) -> RunStatus {
    #[cfg(init_spawns_then_fails)]
    {
        let signals = &*context.cast::<SpawnSignals>();
        test_spawn_unmanaged(signals.started, signals.gate, signals.finished);
        RunStatus::Error
    }
    #[cfg(not(init_spawns_then_fails))]
    {
        let context = context.cast::<u64>();
        if *context == u64::MAX {
            return RunStatus::Error;
        }
        *context += 10;
        CONTEXT.store(context, Ordering::Relaxed);
        RunStatus::Done
    }
}

#[repr(C)]
struct SpawnSignals {
    started: *const AtomicUsize,
    gate: *const AtomicUsize,
    finished: *const AtomicUsize,
}

#[no_mangle]
pub unsafe extern "C" fn hot_reset() {
    let context = CONTEXT.swap(std::ptr::null_mut(), Ordering::Relaxed);
    if !context.is_null() {
        *context += 100;
    }
}

#[no_mangle]
pub extern "C" fn hot_attached_supported() -> bool {
    cfg!(attached_entry) || cfg!(retained_waker)
}

#[no_mangle]
pub unsafe extern "C" fn hot_attach_runtime(table: *const HostRuntimeVTable) -> bool {
    RUNTIME.store(table.cast_mut(), Ordering::Relaxed);
    !table.is_null()
}

#[no_mangle]
pub extern "C" fn test_runtime_attached() -> bool {
    !RUNTIME.load(Ordering::Relaxed).is_null()
}

#[no_mangle]
pub unsafe extern "C" fn hot_make_future(state: *mut AppState) -> AttachedFuture {
    if cfg!(async_entry) {
        return AttachedFuture {
            data: Box::into_raw(Box::new(EntryFuture {
                state,
                started: false,
                done: false,
            }))
            .cast(),
            vtable: &ENTRY_VTABLE,
        };
    }
    if cfg!(retained_waker) {
        return AttachedFuture {
            data: Box::into_raw(Box::new(0u8)).cast(),
            vtable: &RETAINING_VTABLE,
        };
    }
    let table = &*RUNTIME.load(Ordering::Relaxed);
    (table.sleep_ms)(table.ctx, 1)
}

struct EntryFuture {
    state: *mut AppState,
    started: bool,
    done: bool,
}

unsafe extern "C" fn entry_poll(data: *mut c_void, _: *const HostWaker) -> RunStatus {
    let task = &mut *data.cast::<EntryFuture>();
    (*task.state).counter += 1;
    if !task.started {
        task.started = true;
        return RunStatus::Pending;
    }
    task.done = true;
    RunStatus::Done
}

unsafe extern "C" fn entry_drop(data: *mut c_void) {
    let task = Box::from_raw(data.cast::<EntryFuture>());
    if task.started && !task.done {
        (*task.state).counter += 10;
    }
}

static ENTRY_VTABLE: AttachedFutureVTable = AttachedFutureVTable {
    poll: entry_poll,
    drop: entry_drop,
};

unsafe extern "C" fn retain_poll(_: *mut c_void, host: *const HostWaker) -> RunStatus {
    let host = &*host;
    let saved = Box::new(HostWaker {
        data: ((*host.vtable).clone)(host.data),
        vtable: host.vtable,
    });
    let previous = RETAINED_WAKER.swap(Box::into_raw(saved), Ordering::Relaxed);
    if !previous.is_null() {
        release_waker(previous);
    }
    RunStatus::Done
}

unsafe extern "C" fn retain_drop(data: *mut c_void) {
    drop(Box::from_raw(data.cast::<u8>()));
}
static RETAINING_VTABLE: AttachedFutureVTable = AttachedFutureVTable {
    poll: retain_poll,
    drop: retain_drop,
};

unsafe fn release_waker(ptr: *mut HostWaker) {
    let waker = Box::from_raw(ptr);
    ((*waker.vtable).drop)(waker.data);
}

#[no_mangle]
pub unsafe extern "C" fn test_release_waker() {
    let ptr = RETAINED_WAKER.swap(std::ptr::null_mut(), Ordering::Relaxed);
    if !ptr.is_null() {
        release_waker(ptr);
    }
}

#[no_mangle]
pub unsafe extern "C" fn test_wake_retained() {
    let ptr = RETAINED_WAKER.load(Ordering::Relaxed);
    if !ptr.is_null() {
        let waker = &*ptr;
        ((*waker.vtable).wake_by_ref)(waker.data);
    }
}

#[no_mangle]
pub extern "C" fn test_set_unload_counter(counter: *mut AtomicUsize) {
    UNLOAD_COUNTER.store(counter, Ordering::Relaxed);
}

unsafe extern "C" fn on_unload() {
    let counter = UNLOAD_COUNTER.load(Ordering::Relaxed);
    if !counter.is_null() {
        (*counter).fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(target_os = "windows")]
#[no_mangle]
pub unsafe extern "system" fn DllMain(_: *mut c_void, reason: u32, _: *mut c_void) -> i32 {
    if reason == 0 {
        on_unload();
    }
    1
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[used]
#[cfg_attr(target_os = "linux", link_section = ".fini_array")]
#[cfg_attr(target_os = "macos", link_section = "__DATA,__mod_term_func")]
static ON_UNLOAD: unsafe extern "C" fn() = on_unload;

#[no_mangle]
pub unsafe extern "C" fn test_spawn_unmanaged(
    started: *const AtomicUsize,
    gate: *const AtomicUsize,
    finished: *const AtomicUsize,
) {
    let (started, gate, finished) = (&*started, &*gate, &*finished);
    drop(std::thread::spawn(move || {
        started.store(1, Ordering::Release);
        while gate.load(Ordering::Acquire) == 0 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        finished.store(1, Ordering::Release);
    }));
}

struct UnmanagedFuture {
    gate: &'static AtomicUsize,
    finished: &'static AtomicUsize,
}

unsafe extern "C" fn unmanaged_poll(data: *mut c_void, host: *const HostWaker) -> RunStatus {
    let future = &*data.cast::<UnmanagedFuture>();
    if future.gate.load(Ordering::Acquire) == 0 {
        let waker = &*host;
        ((*waker.vtable).wake_by_ref)(waker.data);
        return RunStatus::Pending;
    }
    future.finished.store(1, Ordering::Release);
    RunStatus::Done
}

unsafe extern "C" fn unmanaged_drop(data: *mut c_void) {
    let future = Box::from_raw(data.cast::<UnmanagedFuture>());
    future.finished.store(2, Ordering::Release);
}

static UNMANAGED_VTABLE: AttachedFutureVTable = AttachedFutureVTable {
    poll: unmanaged_poll,
    drop: unmanaged_drop,
};

#[no_mangle]
pub unsafe extern "C" fn test_make_unmanaged_future(
    gate: *const AtomicUsize,
    finished: *const AtomicUsize,
) -> AttachedFuture {
    let future = Box::new(UnmanagedFuture {
        gate: &*gate,
        finished: &*finished,
    });
    AttachedFuture {
        data: Box::into_raw(future).cast(),
        vtable: &UNMANAGED_VTABLE,
    }
}
