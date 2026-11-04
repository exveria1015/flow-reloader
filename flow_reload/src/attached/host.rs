use super::waker::waker_from_host;
use crate::{AttachedFuture, AttachedFutureVTable, HostRuntimeVTable, HostWaker, RunStatus};
#[cfg(feature = "smol")]
use smol::io::{AsyncReadExt as SmolReadExt, AsyncWriteExt as SmolWriteExt};
#[cfg(feature = "smol")]
use smol::net::TcpStream as SmolTcpStream;
use std::ffi::c_void;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
#[cfg(feature = "tokio")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
    drop(handle.spawn(async move {
        if let Ok(mut stream) = tokio::net::TcpStream::connect(addr).await {
            let _ = stream.write_all(&payload).await;
            let _ = stream.shutdown().await;
        }
    }));
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

    smol::spawn(async move {
        if let Ok(mut stream) = SmolTcpStream::connect(addr).await {
            let _ = SmolWriteExt::write_all(&mut stream, &payload).await;
        }
    })
    .detach();
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

    drop(async_std::task::spawn(async move {
        if let Ok(mut stream) = async_std::net::TcpStream::connect(addr).await {
            let _ = async_std::io::WriteExt::write_all(&mut stream, &payload).await;
        }
    }));
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
    drop(async_std::task::spawn(async move {
        async_std::task::sleep(std::time::Duration::from_millis(ms)).await;
    }));
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
    smol::spawn(async move {
        smol::Timer::after(std::time::Duration::from_millis(ms)).await;
    })
    .detach();
}

#[cfg(not(feature = "smol"))]
#[allow(dead_code)]
pub unsafe extern "C" fn smol_spawn_sleep_ms(_ctx: *mut c_void, _ms: u64) {}

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
unsafe extern "C" fn host_sleep_poll(data: *mut c_void, host_waker: *const HostWaker) -> RunStatus {
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
unsafe extern "C" fn host_tcp_poll(data: *mut c_void, host_waker: *const HostWaker) -> RunStatus {
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
    drop(handle.spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }));
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
