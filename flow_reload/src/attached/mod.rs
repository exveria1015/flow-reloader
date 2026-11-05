mod future;
mod host;
mod waker;

pub use future::AttachedFutureTask;
use future::OwnedAttachedFuture;
#[cfg(any(feature = "async-std", test))]
pub(crate) use host::async_std_runtime_vtable;
#[cfg(feature = "smol")]
pub(crate) use host::smol_runtime_vtable;
pub use host::tokio_runtime_vtable;
pub(crate) use waker::host_waker_from;
pub(crate) use waker::waker_from_host;
pub use waker::{poll_attached_future, poll_attached_result_future};

use crate::RunStatus;
use interface::{HostRuntimeVTable, HostWaker};
use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

thread_local! {
    static ATTACHED_RUNTIME: Cell<*const HostRuntimeVTable> = const { Cell::new(std::ptr::null()) };
}

/// Keeps the host callback table borrowed until the logic runtime is detached.
pub struct AttachedRuntimeGuard<'a> {
    attach: crate::AttachRuntimeFn,
    _table: &'a HostRuntimeVTable,
}

#[cfg(any(feature = "tokio", feature = "async-std", feature = "smol", test))]
impl<'a> AttachedRuntimeGuard<'a> {
    // The runner keeps the library containing `attach` loaded through this guard.
    pub(crate) unsafe fn new(
        attach: crate::AttachRuntimeFn,
        table: &'a HostRuntimeVTable,
    ) -> Option<Self> {
        if !attach(table) {
            // A rejected attachment must not retain the borrowed table either.
            attach(std::ptr::null());
            return None;
        }
        Some(Self {
            attach,
            _table: table,
        })
    }
}

impl Drop for AttachedRuntimeGuard<'_> {
    fn drop(&mut self) {
        unsafe { (self.attach)(std::ptr::null()) };
    }
}

pub struct AttachedTcpRequest<'a> {
    addr: &'a str,
    payload: &'a [u8],
    out: &'a mut [u8],
    inner: Option<OwnedAttachedFuture>,
}

impl<'a> AttachedTcpRequest<'a> {
    pub fn new(addr: &'a str, payload: &'a [u8], out: &'a mut [u8]) -> Self {
        Self {
            addr,
            payload,
            out,
            inner: None,
        }
    }
}

impl Future for AttachedTcpRequest<'_> {
    type Output = Result<usize, RunStatus>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Some(runtime) = attached_runtime() else {
            return Poll::Ready(Err(RunStatus::Error));
        };
        let Some(host_waker) = current_host_waker() else {
            return Poll::Ready(Err(RunStatus::Error));
        };

        if self.inner.is_none() {
            self.out.fill(0);
            let fut = unsafe {
                (runtime.tcp_request)(
                    runtime.ctx,
                    self.addr.as_ptr(),
                    self.addr.len(),
                    self.payload.as_ptr(),
                    self.payload.len(),
                    self.out.as_mut_ptr(),
                    self.out.len(),
                )
            };
            let Some(fut) = (unsafe { OwnedAttachedFuture::new(fut) }) else {
                return Poll::Ready(Err(RunStatus::Error));
            };
            self.inner = Some(fut);
        }

        let fut = self.inner.as_mut().expect("attached tcp future missing");
        let status = fut.poll(&host_waker);
        if status != RunStatus::Pending {
            // End the foreign buffer borrow before reading the result.
            self.inner = None;
        }
        match status {
            RunStatus::Pending => Poll::Pending,
            RunStatus::Done => {
                if self.out.len() < 4 {
                    return Poll::Ready(Ok(0));
                }
                let mut len_bytes = [0u8; 4];
                len_bytes.copy_from_slice(&self.out[..4]);
                let len = u32::from_le_bytes(len_bytes) as usize;
                let max_len = self.out.len().saturating_sub(4);
                Poll::Ready(Ok(len.min(max_len)))
            }
            RunStatus::Error => Poll::Ready(Err(RunStatus::Error)),
        }
    }
}

pub struct AttachedSleep {
    ms: u64,
    inner: Option<OwnedAttachedFuture>,
}

impl AttachedSleep {
    pub fn new(ms: u64) -> Self {
        Self { ms, inner: None }
    }
}

impl Future for AttachedSleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Some(runtime) = attached_runtime() else {
            return Poll::Ready(());
        };
        let Some(host_waker) = current_host_waker() else {
            return Poll::Ready(());
        };

        if self.inner.is_none() {
            let fut = unsafe { (runtime.sleep_ms)(runtime.ctx, self.ms) };
            let Some(fut) = (unsafe { OwnedAttachedFuture::new(fut) }) else {
                return Poll::Ready(());
            };
            self.inner = Some(fut);
        }

        let fut = self.inner.as_mut().unwrap();
        let status = fut.poll(&host_waker);
        match status {
            RunStatus::Pending => Poll::Pending,
            _ => {
                self.inner = None;
                Poll::Ready(())
            }
        }
    }
}

/// Installs the host runtime callbacks used by attached futures.
///
/// # Safety
/// A non-null `vtable`, its context, and all callbacks must remain valid until
/// the runtime is detached and all attached futures have been dropped.
/// Attachment and polling must happen on the same thread. Callbacks must not unwind.
pub unsafe fn attach_tokio_runtime(vtable: *const HostRuntimeVTable) -> bool {
    ATTACHED_RUNTIME.with(|cell| cell.set(vtable));
    !vtable.is_null()
}

fn attached_runtime() -> Option<HostRuntimeVTable> {
    let ptr = ATTACHED_RUNTIME.with(Cell::get);
    if ptr.is_null() {
        return None;
    }
    // Copy the non-owning callbacks; no reference escapes the attachment scope.
    let table = unsafe { &*ptr };
    Some(HostRuntimeVTable {
        ctx: table.ctx,
        sleep_ms: table.sleep_ms,
        tcp_request: table.tcp_request,
        spawn_sleep_ms: table.spawn_sleep_ms,
        spawn_tcp_request: table.spawn_tcp_request,
    })
}

pub fn attached_runtime_available() -> bool {
    attached_runtime().is_some()
}

#[allow(dead_code)]
pub fn spawn_detached_sleep(ms: u64) -> bool {
    let Some(runtime) = attached_runtime() else {
        return false;
    };
    unsafe { (runtime.spawn_sleep_ms)(runtime.ctx, ms) };
    true
}

#[allow(dead_code)]
pub fn spawn_detached_tcp_request(addr: &str, payload: &[u8]) -> bool {
    if addr.is_empty() {
        return false;
    }
    let Some(runtime) = attached_runtime() else {
        return false;
    };
    unsafe {
        (runtime.spawn_tcp_request)(
            runtime.ctx,
            addr.as_ptr(),
            addr.len(),
            payload.as_ptr(),
            payload.len(),
        )
    };
    true
}

thread_local! {
    static CURRENT_HOST_WAKER: Cell<*const HostWaker> = const { Cell::new(std::ptr::null()) };
}

/// Installs a borrowed waker for callbacks on the current thread.
///
/// # Safety
/// The pointer and its callbacks must remain valid until cleared or replaced.
/// Prefer the scoped polling helper used by the generated logic entrypoint.
pub unsafe fn set_attached_host_waker(waker: *const HostWaker) {
    CURRENT_HOST_WAKER.with(|cell| cell.set(waker));
}

pub fn clear_attached_host_waker() {
    CURRENT_HOST_WAKER.with(|cell| cell.set(std::ptr::null()));
}

fn current_host_waker() -> Option<HostWaker> {
    let ptr = CURRENT_HOST_WAKER.with(|cell| cell.get());
    if ptr.is_null() {
        None
    } else {
        let host = unsafe { &*ptr };
        Some(HostWaker {
            data: host.data,
            vtable: host.vtable,
        })
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "smol")]
    use super::host::smol_spawn_tcp_request;
    #[cfg(feature = "smol")]
    use std::io::{ErrorKind, Read};
    #[cfg(feature = "smol")]
    use std::time::{Duration, Instant};

    unsafe extern "C" fn attach(table: *const crate::HostRuntimeVTable) -> bool {
        super::attach_tokio_runtime(table)
    }

    #[test]
    fn runtime_attachment_is_scoped_on_success_and_panic_and_is_thread_local() {
        let table = super::async_std_runtime_vtable();
        {
            let _guard = unsafe { super::AttachedRuntimeGuard::new(attach, &table) }.unwrap();
            assert!(super::attached_runtime_available());
            assert!(!std::thread::spawn(super::attached_runtime_available)
                .join()
                .unwrap());
        }
        assert!(!super::attached_runtime_available());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = unsafe { super::AttachedRuntimeGuard::new(attach, &table) }.unwrap();
            panic!("host runtime panic");
        }));
        assert!(result.is_err());
        assert!(!super::attached_runtime_available());
    }

    #[cfg(feature = "smol")]
    #[test]
    fn smol_detached_request_completes_after_spawn_returns() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let payload = b"detached request";
        unsafe {
            smol_spawn_tcp_request(
                std::ptr::null_mut(),
                address.as_ptr(),
                address.len(),
                payload.as_ptr(),
                payload.len(),
            );
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(err) if err.kind() == ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "detached request did not connect"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("accept failed: {err}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut received = [0; 16];
        stream.read_exact(&mut received).unwrap();
        assert_eq!(&received, payload);
    }
}
