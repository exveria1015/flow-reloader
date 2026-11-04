#![allow(dead_code)]
mod interface {
    include!(env!("FLOW_RELOAD_INTERFACE_SOURCE"));
}
use interface::services::*;
use interface::{AttachedFuture, AttachedFutureVTable, HostWaker, RunStatus};
use std::ffi::c_void;

const KEY: ServiceKey = ServiceKey::new(7, if cfg!(wrong_service) { 2 } else { 1 });
static API: HandlerApi = HandlerApi {
    header: HandlerHeader {
        abi_version: if cfg!(bad_abi) {
            999
        } else {
            HANDLER_ABI_VERSION
        },
        struct_size: std::mem::size_of::<HandlerApi>() as u32,
    },
    service: KEY,
    make_request: make,
    state_schema: if cfg!(restart_worker) {
        ServiceKey::new(100, if cfg!(version_two) { 2 } else { 1 })
    } else {
        ServiceKey::new(0, 0)
    },
};

#[no_mangle]
pub extern "C" fn hot_handler_api() -> *const HandlerApi {
    &API
}

struct Task {
    host: HostServices,
    resource: ResourceId,
    service: ServiceKey,
    message: Vec<u8>,
    first: bool,
    control: HostTaskControl,
    state: Vec<u8>,
}

unsafe extern "C" fn make(raw: *const HandlerRequest) -> AttachedFuture {
    let request = &*raw;
    let host = HostServices {
        data: ((*request.services.vtable).retain)(request.services.data),
        vtable: request.services.vtable,
    };
    let control = HostTaskControl {
        data: ((*request.control.vtable).retain)(request.control.data),
        vtable: request.control.vtable,
    };
    let state = if request.state_len == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(request.state, request.state_len).to_vec()
    };
    let mut message = if cfg!(version_two) {
        b"v2:".to_vec()
    } else {
        b"v1:".to_vec()
    };
    if request.input_len != 0 {
        message.extend_from_slice(std::slice::from_raw_parts(request.input, request.input_len));
    }
    AttachedFuture {
        data: Box::into_raw(Box::new(Task {
            host,
            resource: request.resource,
            service: request.service,
            message,
            first: true,
            control,
            state,
        }))
        .cast(),
        vtable: &VTABLE,
    }
}

unsafe extern "C" fn poll(data: *mut c_void, waker: *const HostWaker) -> RunStatus {
    let task = &mut *data.cast::<Task>();
    if cfg!(restart_worker) {
        let value = if cfg!(version_two) {
            u16::from_le_bytes(task.state[..2].try_into().unwrap())
        } else {
            task.state[0] as u16
        };
        if std::mem::take(&mut task.first) {
            let reply = format!("v{}:{value}", if cfg!(version_two) { 2 } else { 1 });
            let result = ((*task.host.vtable).call)(
                task.host.data,
                task.resource,
                task.service,
                0,
                reply.as_ptr(),
                reply.len(),
                std::ptr::null_mut(),
                0,
            );
            return if result.status == 0 {
                RunStatus::Pending
            } else {
                RunStatus::Error
            };
        }
        if !((*task.control.vtable).stop_requested)(task.control.data) {
            return RunStatus::Pending;
        }
        let bytes = if cfg!(version_two) {
            (value + 1).to_le_bytes().to_vec()
        } else {
            vec![(value + 1) as u8]
        };
        let schema = API.state_schema;
        return if ((*task.control.vtable).checkpoint)(
            task.control.data,
            schema,
            bytes.as_ptr(),
            bytes.len(),
        ) == 0
        {
            RunStatus::Done
        } else {
            RunStatus::Error
        };
    }
    if std::mem::take(&mut task.first) {
        ((*(*waker).vtable).wake_by_ref)((*waker).data);
        return RunStatus::Pending;
    }
    let result = ((*task.host.vtable).call)(
        task.host.data,
        task.resource,
        task.service,
        0,
        task.message.as_ptr(),
        task.message.len(),
        std::ptr::null_mut(),
        0,
    );
    if result.status == 0 {
        RunStatus::Done
    } else {
        RunStatus::Error
    }
}

unsafe extern "C" fn destroy(data: *mut c_void) {
    let task = Box::from_raw(data.cast::<Task>());
    ((*task.control.vtable).release)(task.control.data);
    ((*task.host.vtable).release)(task.host.data);
}
static VTABLE: AttachedFutureVTable = AttachedFutureVTable {
    poll,
    drop: destroy,
};
