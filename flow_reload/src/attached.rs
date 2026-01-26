use crate::RunStatus;
use interface::{
    AttachedFuture, AttachedFutureVTable, HostRuntimeVTable, HostWaker, HostWakerVTable,
};
use std::ffi::c_void;
use std::future::Future;
use std::pin::Pin;
use std::cell::Cell;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::task::{Context, Poll, Waker};
#[cfg(feature = "tokio")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(feature = "smol")]
use smol::io::{AsyncReadExt as SmolReadExt, AsyncWriteExt as SmolWriteExt};
#[cfg(feature = "smol")]
use smol::net::TcpStream as SmolTcpStream;

static ATTACHED_RUNTIME: AtomicPtr<HostRuntimeVTable> = AtomicPtr::new(std::ptr::null_mut());

pub struct AttachedRuntimeGuard {
    #[cfg(feature = "tokio")]
    #[allow(dead_code)]
    guard: tokio::runtime::EnterGuard<'static>,
}

#[cfg(feature = "async-std")]
#[allow(dead_code)]
pub unsafe extern "C" fn async_std_sleep_ms(_ctx: *mut c_void, ms: u64) -> AttachedFuture {
    let fut = async_std::task::sleep(std::time::Duration::from_millis(ms));
    let boxed = Box::new(HostSleepFuture {
        future: Box::pin(fut),
    });
    AttachedFuture {
        data: Box::into_raw(boxed) as *mut c_void,
        vtable: &HOST_SLEEP_VTABLE,
    }
}

#[cfg(not(feature = "async-std"))]
#[allow(dead_code)]
pub unsafe extern "C" fn async_std_sleep_ms(_ctx: *mut c_void, _ms: u64) -> AttachedFuture {
    AttachedFuture {
        data: std::ptr::null_mut(),
        vtable: std::ptr::null(),
    }
}

#[cfg(feature = "tokio")]
pub unsafe extern "C" fn tokio_spawn_tcp_request(
    ctx: *mut c_void,
    addr: *const u8,
    addr_len: usize,
    payload: *const u8,
    payload_len: usize,
) {
    if ctx.is_null() || addr.is_null() || addr_len == 0 {
        return;
    }
    let addr = std::slice::from_raw_parts(addr, addr_len);
    let addr = match std::str::from_utf8(addr) {
        Ok(value) => value.to_string(),
        Err(_) => return,
    };
    let payload = if payload_len == 0 {
        Vec::new()
    } else {
        if payload.is_null() {
            return;
        }
        std::slice::from_raw_parts(payload, payload_len).to_vec()
    };
    let handle = &*(ctx as *const tokio::runtime::Handle);
    let _guard = handle.enter();
    let _ = handle.spawn(async move {
        if let Ok(mut stream) = tokio::net::TcpStream::connect(addr).await {
            let _ = stream.write_all(&payload).await;
            let _ = stream.shutdown().await;
        }
    });
}

#[cfg(not(feature = "tokio"))]
pub unsafe extern "C" fn tokio_spawn_tcp_request(
    _ctx: *mut c_void,
    _addr: *const u8,
    _addr_len: usize,
    _payload: *const u8,
    _payload_len: usize,
) {
}

#[cfg(feature = "smol")]
#[allow(dead_code)]
pub unsafe extern "C" fn smol_spawn_tcp_request(
    _ctx: *mut c_void,
    addr: *const u8,
    addr_len: usize,
    payload: *const u8,
    payload_len: usize,
) {
    if addr.is_null() || addr_len == 0 {
        return;
    }
    let addr = std::slice::from_raw_parts(addr, addr_len);
    let addr = match std::str::from_utf8(addr) {
        Ok(value) => value.to_string(),
        Err(_) => return,
    };
    let payload = if payload_len == 0 {
        Vec::new()
    } else {
        if payload.is_null() {
            return;
        }
        std::slice::from_raw_parts(payload, payload_len).to_vec()
    };

    let _ = smol::spawn(async move {
        if let Ok(mut stream) = SmolTcpStream::connect(addr).await {
            let _ = SmolWriteExt::write_all(&mut stream, &payload).await;
        }
    });
}

#[cfg(not(feature = "smol"))]
#[allow(dead_code)]
pub unsafe extern "C" fn smol_spawn_tcp_request(
    _ctx: *mut c_void,
    _addr: *const u8,
    _addr_len: usize,
    _payload: *const u8,
    _payload_len: usize,
) {
}

#[cfg(feature = "async-std")]
#[allow(dead_code)]
pub unsafe extern "C" fn async_std_spawn_tcp_request(
    _ctx: *mut c_void,
    addr: *const u8,
    addr_len: usize,
    payload: *const u8,
    payload_len: usize,
) {
    if addr.is_null() || addr_len == 0 {
        return;
    }
    let addr = std::slice::from_raw_parts(addr, addr_len);
    let addr = match std::str::from_utf8(addr) {
        Ok(value) => value.to_string(),
        Err(_) => return,
    };
    let payload = if payload_len == 0 {
        Vec::new()
    } else {
        if payload.is_null() {
            return;
        }
        std::slice::from_raw_parts(payload, payload_len).to_vec()
    };

    let _ = async_std::task::spawn(async move {
        if let Ok(mut stream) = async_std::net::TcpStream::connect(addr).await {
            let _ = async_std::io::WriteExt::write_all(&mut stream, &payload).await;
        }
    });
}

#[cfg(not(feature = "async-std"))]
#[allow(dead_code)]
pub unsafe extern "C" fn async_std_spawn_tcp_request(
    _ctx: *mut c_void,
    _addr: *const u8,
    _addr_len: usize,
    _payload: *const u8,
    _payload_len: usize,
) {
}

#[cfg(feature = "async-std")]
#[allow(dead_code)]
pub unsafe extern "C" fn async_std_spawn_sleep_ms(_ctx: *mut c_void, ms: u64) {
    let _ = async_std::task::spawn(async move {
        async_std::task::sleep(std::time::Duration::from_millis(ms)).await;
    });
}

#[cfg(not(feature = "async-std"))]
#[allow(dead_code)]
pub unsafe extern "C" fn async_std_spawn_sleep_ms(_ctx: *mut c_void, _ms: u64) {}

#[cfg(feature = "smol")]
#[allow(dead_code)]
pub unsafe extern "C" fn smol_sleep_ms(_ctx: *mut c_void, ms: u64) -> AttachedFuture {
    let fut = async move {
        smol::Timer::after(std::time::Duration::from_millis(ms)).await;
    };
    let boxed = Box::new(HostSleepFuture {
        future: Box::pin(fut),
    });
    AttachedFuture {
        data: Box::into_raw(boxed) as *mut c_void,
        vtable: &HOST_SLEEP_VTABLE,
    }
}

#[cfg(not(feature = "smol"))]
#[allow(dead_code)]
pub unsafe extern "C" fn smol_sleep_ms(_ctx: *mut c_void, _ms: u64) -> AttachedFuture {
    AttachedFuture {
        data: std::ptr::null_mut(),
        vtable: std::ptr::null(),
    }
}

#[cfg(feature = "smol")]
#[allow(dead_code)]
pub unsafe extern "C" fn smol_spawn_sleep_ms(_ctx: *mut c_void, ms: u64) {
    let _ = smol::spawn(async move {
        smol::Timer::after(std::time::Duration::from_millis(ms)).await;
    });
}

#[cfg(not(feature = "smol"))]
#[allow(dead_code)]
pub unsafe extern "C" fn smol_spawn_sleep_ms(_ctx: *mut c_void, _ms: u64) {}

pub struct AttachedTcpRequest<'a> {
    addr: &'a str,
    payload: &'a [u8],
    out: &'a mut [u8],
    inner: Option<AttachedFuture>,
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

#[cfg(feature = "async-std")]
#[allow(dead_code)]
pub unsafe extern "C" fn async_std_tcp_request(
    _ctx: *mut c_void,
    addr: *const u8,
    addr_len: usize,
    payload: *const u8,
    payload_len: usize,
    out_buf: *mut u8,
    out_len: usize,
) -> AttachedFuture {
    if addr.is_null() || payload.is_null() || out_buf.is_null() || out_len < 4 {
        return AttachedFuture {
            data: std::ptr::null_mut(),
            vtable: std::ptr::null(),
        };
    }

    let addr = std::slice::from_raw_parts(addr, addr_len);
    let payload = std::slice::from_raw_parts(payload, payload_len);
    let addr = match std::str::from_utf8(addr) {
        Ok(value) => value.to_string(),
        Err(_) => {
            return AttachedFuture {
                data: std::ptr::null_mut(),
                vtable: std::ptr::null(),
            }
        }
    };
    let payload = payload.to_vec();

    let fut = async move {
        let mut stream = async_std::net::TcpStream::connect(addr).await?;
        async_std::io::WriteExt::write_all(&mut stream, &payload).await?;
        let mut buf = vec![0u8; out_len.saturating_sub(4)];
        let n = async_std::io::ReadExt::read(&mut stream, &mut buf).await?;
        buf.truncate(n);
        Ok::<_, std::io::Error>(buf)
    };

    let boxed = Box::new(HostTcpFuture {
        future: Box::pin(fut),
        out_ptr: out_buf,
        out_len,
    });
    AttachedFuture {
        data: Box::into_raw(boxed) as *mut c_void,
        vtable: &HOST_TCP_VTABLE,
    }
}

#[cfg(not(feature = "async-std"))]
#[allow(dead_code)]
pub unsafe extern "C" fn async_std_tcp_request(
    _ctx: *mut c_void,
    _addr: *const u8,
    _addr_len: usize,
    _payload: *const u8,
    _payload_len: usize,
    _out_buf: *mut u8,
    _out_len: usize,
) -> AttachedFuture {
    AttachedFuture {
        data: std::ptr::null_mut(),
        vtable: std::ptr::null(),
    }
}

#[cfg(feature = "smol")]
#[allow(dead_code)]
pub unsafe extern "C" fn smol_tcp_request(
    _ctx: *mut c_void,
    addr: *const u8,
    addr_len: usize,
    payload: *const u8,
    payload_len: usize,
    out_buf: *mut u8,
    out_len: usize,
) -> AttachedFuture {
    if addr.is_null() || payload.is_null() || out_buf.is_null() || out_len < 4 {
        return AttachedFuture {
            data: std::ptr::null_mut(),
            vtable: std::ptr::null(),
        };
    }

    let addr = std::slice::from_raw_parts(addr, addr_len);
    let payload = std::slice::from_raw_parts(payload, payload_len);
    let addr = match std::str::from_utf8(addr) {
        Ok(value) => value.to_string(),
        Err(_) => {
            return AttachedFuture {
                data: std::ptr::null_mut(),
                vtable: std::ptr::null(),
            }
        }
    };
    let payload = payload.to_vec();

    let fut = async move {
        let mut stream = SmolTcpStream::connect(addr).await?;
        SmolWriteExt::write_all(&mut stream, &payload).await?;
        let mut buf = vec![0u8; out_len.saturating_sub(4)];
        let n = SmolReadExt::read(&mut stream, &mut buf).await?;
        buf.truncate(n);
        Ok::<_, std::io::Error>(buf)
    };

    let boxed = Box::new(HostTcpFuture {
        future: Box::pin(fut),
        out_ptr: out_buf,
        out_len,
    });
    AttachedFuture {
        data: Box::into_raw(boxed) as *mut c_void,
        vtable: &HOST_TCP_VTABLE,
    }
}

#[cfg(not(feature = "smol"))]
#[allow(dead_code)]
pub unsafe extern "C" fn smol_tcp_request(
    _ctx: *mut c_void,
    _addr: *const u8,
    _addr_len: usize,
    _payload: *const u8,
    _payload_len: usize,
    _out_buf: *mut u8,
    _out_len: usize,
) -> AttachedFuture {
    AttachedFuture {
        data: std::ptr::null_mut(),
        vtable: std::ptr::null(),
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
            if fut.vtable.is_null() {
                return Poll::Ready(Err(RunStatus::Error));
            }
            self.inner = Some(fut);
        }

        let fut = self.inner.as_mut().expect("attached tcp future missing");
        let status = unsafe { ((*fut.vtable).poll)(fut.data, host_waker as *const _) };
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

#[cfg(feature = "tokio")]
pub unsafe extern "C" fn tokio_tcp_request(
    ctx: *mut c_void,
    addr: *const u8,
    addr_len: usize,
    payload: *const u8,
    payload_len: usize,
    out_buf: *mut u8,
    out_len: usize,
) -> AttachedFuture {
    if ctx.is_null() || addr.is_null() || payload.is_null() || out_buf.is_null() || out_len < 4 {
        return AttachedFuture {
            data: std::ptr::null_mut(),
            vtable: std::ptr::null(),
        };
    }

    let addr = std::slice::from_raw_parts(addr, addr_len);
    let payload = std::slice::from_raw_parts(payload, payload_len);
    let addr = match std::str::from_utf8(addr) {
        Ok(value) => value.to_string(),
        Err(_) => {
            return AttachedFuture {
                data: std::ptr::null_mut(),
                vtable: std::ptr::null(),
            }
        }
    };
    let payload = payload.to_vec();
    let handle = &*(ctx as *const tokio::runtime::Handle);
    let _guard = handle.enter();

    let fut = async move {
        let mut stream = tokio::net::TcpStream::connect(addr).await?;
        stream.write_all(&payload).await?;
        stream.shutdown().await?;
        let mut buf = vec![0u8; out_len.saturating_sub(4)];
        let n = stream.read(&mut buf).await?;
        buf.truncate(n);
        Ok::<_, std::io::Error>(buf)
    };

    let boxed = Box::new(HostTcpFuture {
        future: Box::pin(fut),
        out_ptr: out_buf,
        out_len,
    });
    AttachedFuture {
        data: Box::into_raw(boxed) as *mut c_void,
        vtable: &HOST_TCP_VTABLE,
    }
}

#[cfg(not(feature = "tokio"))]
pub unsafe extern "C" fn tokio_tcp_request(
    _ctx: *mut c_void,
    _addr: *const u8,
    _addr_len: usize,
    _payload: *const u8,
    _payload_len: usize,
    _out_buf: *mut u8,
    _out_len: usize,
) -> AttachedFuture {
    AttachedFuture {
        data: std::ptr::null_mut(),
        vtable: std::ptr::null(),
    }
}

#[allow(dead_code)]
unsafe fn waker_from_host(host: &HostWaker) -> Waker {
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

    static PROXY_VTABLE: std::task::RawWakerVTable = std::task::RawWakerVTable::new(
        proxy_clone,
        proxy_wake,
        proxy_wake_by_ref,
        proxy_drop,
    );

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

#[allow(dead_code)]
struct HostSleepFuture {
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
}

#[allow(dead_code)]
struct HostTcpFuture {
    future: Pin<Box<dyn Future<Output = Result<Vec<u8>, std::io::Error>> + Send>>,
    out_ptr: *mut u8,
    out_len: usize,
}

#[allow(dead_code)]
unsafe extern "C" fn host_sleep_poll(
    data: *mut c_void,
    host_waker: *const HostWaker,
) -> RunStatus {
    if data.is_null() || host_waker.is_null() {
        return RunStatus::Error;
    }
    let host_waker = &*host_waker;
    let waker = waker_from_host(host_waker);
    let mut cx = Context::from_waker(&waker);
    let fut = &mut *(data as *mut HostSleepFuture);
    match fut.future.as_mut().poll(&mut cx) {
        Poll::Pending => RunStatus::Pending,
        Poll::Ready(_) => RunStatus::Done,
    }
}

#[allow(dead_code)]
unsafe extern "C" fn host_sleep_drop(data: *mut c_void) {
    if data.is_null() {
        return;
    }
    drop(Box::from_raw(data as *mut HostSleepFuture));
}

#[allow(dead_code)]
static HOST_SLEEP_VTABLE: AttachedFutureVTable = AttachedFutureVTable {
    poll: host_sleep_poll,
    drop: host_sleep_drop,
};

#[allow(dead_code)]
unsafe extern "C" fn host_tcp_poll(
    data: *mut c_void,
    host_waker: *const HostWaker,
) -> RunStatus {
    if data.is_null() || host_waker.is_null() {
        return RunStatus::Error;
    }
    let host_waker = &*host_waker;
    let waker = waker_from_host(host_waker);
    let mut cx = Context::from_waker(&waker);
    let fut = &mut *(data as *mut HostTcpFuture);
    match fut.future.as_mut().poll(&mut cx) {
        Poll::Pending => RunStatus::Pending,
        Poll::Ready(result) => {
            let buf = std::slice::from_raw_parts_mut(fut.out_ptr, fut.out_len);
            buf.fill(0);
            match result {
                Ok(bytes) => {
                    if fut.out_len >= 4 {
                        let cap = fut.out_len.saturating_sub(4);
                        let len = bytes.len().min(cap);
                        buf[..4].copy_from_slice(&(len as u32).to_le_bytes());
                        buf[4..4 + len].copy_from_slice(&bytes[..len]);
                        RunStatus::Done
                    } else {
                        RunStatus::Error
                    }
                }
                Err(_) => RunStatus::Error,
            }
        }
    }
}

#[allow(dead_code)]
unsafe extern "C" fn host_tcp_drop(data: *mut c_void) {
    if data.is_null() {
        return;
    }
    drop(Box::from_raw(data as *mut HostTcpFuture));
}

#[allow(dead_code)]
static HOST_TCP_VTABLE: AttachedFutureVTable = AttachedFutureVTable {
    poll: host_tcp_poll,
    drop: host_tcp_drop,
};

#[cfg(feature = "tokio")]
pub unsafe extern "C" fn tokio_sleep_ms(ctx: *mut c_void, ms: u64) -> AttachedFuture {
    let handle = &*(ctx as *const tokio::runtime::Handle);
    let _guard = handle.enter();
    let fut = tokio::time::sleep(std::time::Duration::from_millis(ms));
    let boxed = Box::new(HostSleepFuture {
        future: Box::pin(fut),
    });
    AttachedFuture {
        data: Box::into_raw(boxed) as *mut c_void,
        vtable: &HOST_SLEEP_VTABLE,
    }
}

#[cfg(not(feature = "tokio"))]
pub unsafe extern "C" fn tokio_sleep_ms(_ctx: *mut c_void, _ms: u64) -> AttachedFuture {
    AttachedFuture {
        data: std::ptr::null_mut(),
        vtable: std::ptr::null(),
    }
}

#[cfg(feature = "tokio")]
pub unsafe extern "C" fn tokio_spawn_sleep_ms(ctx: *mut c_void, ms: u64) {
    if ctx.is_null() {
        return;
    }
    let handle = &*(ctx as *const tokio::runtime::Handle);
    let _guard = handle.enter();
    let _ = handle.spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    });
}

#[cfg(not(feature = "tokio"))]
pub unsafe extern "C" fn tokio_spawn_sleep_ms(_ctx: *mut c_void, _ms: u64) {}

#[cfg(feature = "tokio")]
pub fn tokio_runtime_vtable(handle: &tokio::runtime::Handle) -> HostRuntimeVTable {
    HostRuntimeVTable {
        ctx: handle as *const _ as *mut c_void,
        sleep_ms: tokio_sleep_ms,
        tcp_request: tokio_tcp_request,
        spawn_sleep_ms: tokio_spawn_sleep_ms,
        spawn_tcp_request: tokio_spawn_tcp_request,
    }
}

#[cfg(not(feature = "tokio"))]
pub fn tokio_runtime_vtable(_handle: &()) -> HostRuntimeVTable {
    HostRuntimeVTable {
        ctx: std::ptr::null_mut(),
        sleep_ms: tokio_sleep_ms,
        tcp_request: tokio_tcp_request,
        spawn_sleep_ms: tokio_spawn_sleep_ms,
        spawn_tcp_request: tokio_spawn_tcp_request,
    }
}

#[cfg(feature = "async-std")]
#[allow(dead_code)]
pub fn async_std_runtime_vtable() -> HostRuntimeVTable {
    HostRuntimeVTable {
        ctx: std::ptr::null_mut(),
        sleep_ms: async_std_sleep_ms,
        tcp_request: async_std_tcp_request,
        spawn_sleep_ms: async_std_spawn_sleep_ms,
        spawn_tcp_request: async_std_spawn_tcp_request,
    }
}

#[cfg(not(feature = "async-std"))]
#[allow(dead_code)]
pub fn async_std_runtime_vtable() -> HostRuntimeVTable {
    HostRuntimeVTable {
        ctx: std::ptr::null_mut(),
        sleep_ms: async_std_sleep_ms,
        tcp_request: async_std_tcp_request,
        spawn_sleep_ms: async_std_spawn_sleep_ms,
        spawn_tcp_request: async_std_spawn_tcp_request,
    }
}

#[cfg(feature = "smol")]
#[allow(dead_code)]
pub fn smol_runtime_vtable() -> HostRuntimeVTable {
    HostRuntimeVTable {
        ctx: std::ptr::null_mut(),
        sleep_ms: smol_sleep_ms,
        tcp_request: smol_tcp_request,
        spawn_sleep_ms: smol_spawn_sleep_ms,
        spawn_tcp_request: smol_spawn_tcp_request,
    }
}

#[cfg(not(feature = "smol"))]
#[allow(dead_code)]
pub fn smol_runtime_vtable() -> HostRuntimeVTable {
    HostRuntimeVTable {
        ctx: std::ptr::null_mut(),
        sleep_ms: smol_sleep_ms,
        tcp_request: smol_tcp_request,
        spawn_sleep_ms: smol_spawn_sleep_ms,
        spawn_tcp_request: smol_spawn_tcp_request,
    }
}

pub struct AttachedSleep {
    ms: u64,
    inner: Option<AttachedFuture>,
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
            if fut.vtable.is_null() {
                return Poll::Ready(());
            }
            self.inner = Some(fut);
        }

        let fut = self.inner.as_mut().unwrap();
        let status = unsafe { ((*fut.vtable).poll)(fut.data, host_waker as *const _) };
        match status {
            RunStatus::Pending => Poll::Pending,
            _ => Poll::Ready(()),
        }
    }
}

pub unsafe fn attach_tokio_runtime(vtable: *const HostRuntimeVTable) -> bool {
    if vtable.is_null() {
        ATTACHED_RUNTIME.store(std::ptr::null_mut(), Ordering::Release);
        return false;
    }
    ATTACHED_RUNTIME.store(vtable as *mut HostRuntimeVTable, Ordering::Release);
    true
}

pub fn attached_runtime() -> Option<&'static HostRuntimeVTable> {
    let ptr = ATTACHED_RUNTIME.load(Ordering::Acquire);
    if ptr.is_null() {
        return None;
    }
    Some(unsafe { &*ptr })
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
    static CURRENT_HOST_WAKER: Cell<*const HostWaker> = Cell::new(std::ptr::null());
}

pub fn set_attached_host_waker(waker: *const HostWaker) {
    CURRENT_HOST_WAKER.with(|cell| cell.set(waker));
}

pub fn clear_attached_host_waker() {
    CURRENT_HOST_WAKER.with(|cell| cell.set(std::ptr::null()));
}

fn current_host_waker() -> Option<&'static HostWaker> {
    let ptr = CURRENT_HOST_WAKER.with(|cell| cell.get());
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &*ptr })
    }
}

struct HostWakerState {
    waker: Waker,
}

unsafe extern "C" fn host_waker_clone(data: *mut c_void) -> *mut c_void {
    if data.is_null() {
        return std::ptr::null_mut();
    }
    let state = &*(data as *mut HostWakerState);
    let cloned = HostWakerState {
        waker: state.waker.clone(),
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

struct HostWakerGuard {
    data: *mut c_void,
}

impl Drop for HostWakerGuard {
    fn drop(&mut self) {
        unsafe { (HOST_WAKER_VTABLE.drop)(self.data) };
    }
}

fn host_waker_from(waker: &Waker) -> (HostWaker, HostWakerGuard) {
    let state = HostWakerState {
        waker: waker.clone(),
    };
    let data = Box::into_raw(Box::new(state)) as *mut c_void;
    let host_waker = HostWaker {
        data,
        vtable: &HOST_WAKER_VTABLE,
    };
    (host_waker, HostWakerGuard { data })
}

pub struct AttachedFutureTask {
    future: AttachedFuture,
}

impl AttachedFutureTask {
    pub fn new(future: AttachedFuture) -> Option<Self> {
        if future.vtable.is_null() {
            return None;
        }
        Some(Self { future })
    }
}

impl Future for AttachedFutureTask {
    type Output = RunStatus;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (host_waker, _guard) = host_waker_from(cx.waker());
        let vtable = unsafe { &*self.future.vtable };
        let status = unsafe { (vtable.poll)(self.future.data, &host_waker as *const _) };
        match status {
            RunStatus::Pending => Poll::Pending,
            _ => Poll::Ready(status),
        }
    }
}

impl Drop for AttachedFutureTask {
    fn drop(&mut self) {
        if self.future.vtable.is_null() {
            return;
        }
        let vtable = unsafe { &*self.future.vtable };
        unsafe { (vtable.drop)(self.future.data) };
        self.future.data = std::ptr::null_mut();
    }
}
