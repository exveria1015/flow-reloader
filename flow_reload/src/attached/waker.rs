use crate::{HostWaker, HostWakerVTable};
use std::ffi::c_void;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Waker;
use std::task::{Context, Poll};

/// Polls a logic future with a host waker scoped to this call.
///
/// # Safety
/// `host` and all its callbacks must be valid during the call. Its clone callback
/// must produce an owned handle that remains valid until its drop callback runs.
pub unsafe fn poll_attached_future(
    future: Pin<&mut dyn Future<Output = ()>>,
    host: *const HostWaker,
) -> crate::RunStatus {
    poll_with_host(future, host, |()| crate::RunStatus::Done)
}

/// Like `poll_attached_future`, for a future that reports its own result.
///
/// # Safety
/// The host Waker must satisfy `poll_attached_future`'s callback contract.
pub unsafe fn poll_attached_result_future(
    future: Pin<&mut dyn Future<Output = crate::RunStatus>>,
    host: *const HostWaker,
) -> crate::RunStatus {
    poll_with_host(future, host, |status| status)
}

unsafe fn poll_with_host<T>(
    mut future: Pin<&mut dyn Future<Output = T>>,
    host: *const HostWaker,
    ready: impl FnOnce(T) -> crate::RunStatus,
) -> crate::RunStatus {
    if host.is_null() {
        return crate::RunStatus::Error;
    }
    struct WakerScope(*const HostWaker);
    impl Drop for WakerScope {
        fn drop(&mut self) {
            super::CURRENT_HOST_WAKER.with(|cell| cell.set(self.0));
        }
    }
    let previous = super::CURRENT_HOST_WAKER.with(|cell| cell.replace(host));
    let _scope = WakerScope(previous);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let waker = waker_from_host(&*host);
        let mut cx = Context::from_waker(&waker);
        match future.as_mut().poll(&mut cx) {
            Poll::Pending => crate::RunStatus::Pending,
            Poll::Ready(value) => ready(value),
        }
    }));
    result.unwrap_or(crate::RunStatus::Error)
}

#[allow(dead_code)]
pub(crate) unsafe fn waker_from_host(host: &HostWaker) -> Waker {
    if host.vtable.is_null() {
        fn noop_clone(_: *const ()) -> std::task::RawWaker {
            noop_raw()
        }
        fn noop(_: *const ()) {}
        fn noop_raw() -> std::task::RawWaker {
            let vtable = &std::task::RawWakerVTable::new(noop_clone, noop, noop, noop);
            std::task::RawWaker::new(std::ptr::null(), vtable)
        }
        return Waker::from_raw(noop_raw());
    }

    #[repr(C)]
    struct HostProxy {
        data: *mut c_void,
        vtable: *const HostWakerVTable,
    }

    unsafe fn proxy_clone(data: *const ()) -> std::task::RawWaker {
        let proxy = &*(data as *const HostProxy);
        let data = ((*proxy.vtable).clone)(proxy.data);
        let proxy = HostProxy {
            data,
            vtable: proxy.vtable,
        };
        std::task::RawWaker::new(Box::into_raw(Box::new(proxy)) as *const (), &PROXY_VTABLE)
    }

    unsafe fn proxy_wake(data: *const ()) {
        let proxy = Box::from_raw(data as *mut HostProxy);
        ((*proxy.vtable).wake)(proxy.data);
    }

    unsafe fn proxy_wake_by_ref(data: *const ()) {
        let proxy = &*(data as *const HostProxy);
        ((*proxy.vtable).wake_by_ref)(proxy.data);
    }

    unsafe fn proxy_drop(data: *const ()) {
        let proxy = Box::from_raw(data as *mut HostProxy);
        ((*proxy.vtable).drop)(proxy.data);
    }

    static PROXY_VTABLE: std::task::RawWakerVTable =
        std::task::RawWakerVTable::new(proxy_clone, proxy_wake, proxy_wake_by_ref, proxy_drop);

    let data = ((*host.vtable).clone)(host.data);
    let proxy = HostProxy {
        data,
        vtable: host.vtable,
    };
    Waker::from_raw(std::task::RawWaker::new(
        Box::into_raw(Box::new(proxy)) as *const (),
        &PROXY_VTABLE,
    ))
}

struct HostWakerState {
    waker: Waker,
    // Must outlive the Waker's own destructor and every clone sent to a plugin.
    keep_alive: Option<Arc<dyn Send + Sync>>,
}

unsafe extern "C" fn host_waker_clone(data: *mut c_void) -> *mut c_void {
    if data.is_null() {
        return std::ptr::null_mut();
    }
    let state = &*(data as *mut HostWakerState);
    let cloned = HostWakerState {
        waker: state.waker.clone(),
        keep_alive: state.keep_alive.clone(),
    };
    Box::into_raw(Box::new(cloned)) as *mut c_void
}

unsafe extern "C" fn host_waker_wake(data: *mut c_void) {
    if data.is_null() {
        return;
    }
    let state = Box::from_raw(data as *mut HostWakerState);
    state.waker.wake();
}

unsafe extern "C" fn host_waker_wake_by_ref(data: *mut c_void) {
    if data.is_null() {
        return;
    }
    let state = &*(data as *mut HostWakerState);
    state.waker.wake_by_ref();
}

unsafe extern "C" fn host_waker_drop(data: *mut c_void) {
    if data.is_null() {
        return;
    }
    drop(Box::from_raw(data as *mut HostWakerState));
}

static HOST_WAKER_VTABLE: HostWakerVTable = HostWakerVTable {
    clone: host_waker_clone,
    wake: host_waker_wake,
    wake_by_ref: host_waker_wake_by_ref,
    drop: host_waker_drop,
};

pub(super) struct HostWakerGuard {
    data: *mut c_void,
}

impl Drop for HostWakerGuard {
    fn drop(&mut self) {
        unsafe { (HOST_WAKER_VTABLE.drop)(self.data) };
    }
}

pub(super) fn host_waker_from(
    waker: &Waker,
    keep_alive: Option<Arc<dyn Send + Sync>>,
) -> (HostWaker, HostWakerGuard) {
    let state = HostWakerState {
        waker: waker.clone(),
        keep_alive,
    };
    let data = Box::into_raw(Box::new(state)) as *mut c_void;
    let host_waker = HostWaker {
        data,
        vtable: &HOST_WAKER_VTABLE,
    };
    (host_waker, HostWakerGuard { data })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::task::Wake;

    struct WakeProbe(AtomicUsize);

    struct CodeLease(Arc<AtomicUsize>);

    impl Drop for CodeLease {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl Wake for WakeProbe {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn cloned_waker_owns_host_handle_after_poll_scope_and_can_wake_on_another_thread() {
        let probe = Arc::new(WakeProbe(AtomicUsize::new(0)));
        let released = Arc::new(AtomicUsize::new(0));
        let lease = Arc::new(CodeLease(released.clone()));
        let mut retained = None;
        {
            let waker = Waker::from(probe.clone());
            let (host, _host_guard) = host_waker_from(&waker, Some(lease.clone()));
            let mut future = Box::pin(std::future::poll_fn(|cx| {
                let owned = cx.waker().clone();
                owned.wake_by_ref();
                retained = Some(owned.clone());
                owned.wake();
                Poll::Ready(())
            }));
            assert_eq!(
                unsafe { poll_attached_future(future.as_mut(), &host) },
                crate::RunStatus::Done
            );
        }
        drop(lease);
        assert_eq!(released.load(Ordering::Relaxed), 0);
        std::thread::spawn(move || retained.unwrap().wake())
            .join()
            .unwrap();
        assert_eq!(probe.0.load(Ordering::Relaxed), 3);
        assert_eq!(Arc::strong_count(&probe), 1);
        assert_eq!(released.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn nested_poll_restores_waker_after_success_and_panic() {
        let (outer, _outer_guard) = host_waker_from(Waker::noop(), None);
        let (inner, _inner_guard) = host_waker_from(Waker::noop(), None);
        let outer_ptr = &outer as *const HostWaker;
        let mut future = Box::pin(std::future::poll_fn(|_| {
            assert_eq!(
                super::super::CURRENT_HOST_WAKER.with(|cell| cell.get()),
                outer_ptr
            );
            let mut nested = Box::pin(async {});
            assert_eq!(
                unsafe { poll_attached_future(nested.as_mut(), &inner) },
                crate::RunStatus::Done
            );
            assert_eq!(
                super::super::CURRENT_HOST_WAKER.with(|cell| cell.get()),
                outer_ptr
            );
            let mut panicking = Box::pin(async { panic!("logic panic") });
            assert_eq!(
                unsafe { poll_attached_future(panicking.as_mut(), &inner) },
                crate::RunStatus::Error
            );
            assert_eq!(
                super::super::CURRENT_HOST_WAKER.with(|cell| cell.get()),
                outer_ptr
            );
            Poll::Ready(())
        }));
        assert_eq!(
            unsafe { poll_attached_future(future.as_mut(), &outer) },
            crate::RunStatus::Done
        );
        assert!(super::super::CURRENT_HOST_WAKER
            .with(|cell| cell.get())
            .is_null());
    }
}
