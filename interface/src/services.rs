//! Byte-oriented, allocator-independent host services and message handlers.
use crate::AttachedFuture;
use std::ffi::c_void;

pub const SERVICES_ABI_VERSION: u32 = 1;
pub const HANDLER_ABI_VERSION: u32 = 2;
pub const TASK_CONTROL_ABI_VERSION: u32 = 1;

#[repr(C)]
pub struct HostTaskVTable {
    pub header: HandlerHeader,
    pub retain: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub release: unsafe extern "C" fn(*mut c_void),
    pub stop_requested: unsafe extern "C" fn(*mut c_void) -> bool,
    pub checkpoint: unsafe extern "C" fn(*mut c_void, ServiceKey, *const u8, usize) -> u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostTaskControl {
    pub data: *mut c_void,
    pub vtable: *const HostTaskVTable,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ResourceId {
    pub registry: u64,
    pub slot: u64,
    pub generation: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ServiceKey {
    pub id: u64,
    pub version: u32,
    pub reserved: u32,
}

impl ServiceKey {
    pub const fn new(id: u64, version: u32) -> Self {
        Self {
            id,
            version,
            reserved: 0,
        }
    }
}

/// Open status codes permit unknown future errors without invalid enum values.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceError(pub u32);

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "host service error {}", self.0)
    }
}

impl std::error::Error for ServiceError {}

impl ServiceError {
    pub const REVOKED: Self = Self(1);
    pub const CONTRACT_MISMATCH: Self = Self(2);
    pub const BUSY: Self = Self(3);
    pub const BUFFER_TOO_SMALL: Self = Self(4);
    pub const INVALID_ARGUMENT: Self = Self(5);
    pub const PANICKED: Self = Self(6);
    pub const UNSUPPORTED_OPERATION: Self = Self(7);
    pub const FAILED: Self = Self(8);
}

#[repr(C)]
pub struct ServiceResult {
    /// Zero means success; otherwise a ServiceError code.
    pub status: u32,
    pub written: usize,
}

#[repr(C)]
pub struct HostServicesVTable {
    pub abi_version: u32,
    pub struct_size: u32,
    pub retain: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub release: unsafe extern "C" fn(*mut c_void),
    pub call: unsafe extern "C" fn(
        *mut c_void,
        ResourceId,
        ServiceKey,
        u32,
        *const u8,
        usize,
        *mut u8,
        usize,
    ) -> ServiceResult,
}

/// One owning host reference when retained; descriptor copies alone do not
/// retain it. Calls, retain and release are restricted to the owner thread.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostServices {
    pub data: *mut c_void,
    pub vtable: *const HostServicesVTable,
}

#[repr(C)]
pub struct HandlerRequest {
    pub generation: u64,
    pub resource: ResourceId,
    pub service: ServiceKey,
    pub services: HostServices,
    pub input: *const u8,
    pub input_len: usize,
    pub control: HostTaskControl,
    pub state_schema: ServiceKey,
    pub state: *const u8,
    pub state_len: usize,
}

/// Must copy the input/descriptor and retain host services before returning.
/// The returned task may not borrow HandlerRequest or its input allocation.
/// It must also retain task control and copy seed state. Factories only prepare
/// inert futures: business logic, IO and child-task startup begin at first poll.
/// Destroying an unpolled staged task must not apply application state changes.
pub type MakeRequestFn = unsafe extern "C" fn(*const HandlerRequest) -> AttachedFuture;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HandlerHeader {
    pub abi_version: u32,
    pub struct_size: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HandlerApi {
    pub header: HandlerHeader,
    pub service: ServiceKey,
    pub make_request: MakeRequestFn,
    pub state_schema: ServiceKey,
}

impl HandlerApi {
    pub const fn new(service: ServiceKey, make_request: MakeRequestFn) -> Self {
        Self {
            header: HandlerHeader {
                abi_version: HANDLER_ABI_VERSION,
                struct_size: std::mem::size_of::<Self>() as u32,
            },
            service,
            make_request,
            state_schema: ServiceKey::new(0, 0),
        }
    }

    pub const fn with_state(mut self, schema: ServiceKey) -> Self {
        self.state_schema = schema;
        self
    }
}

pub type HandlerApiFn = unsafe extern "C" fn() -> *const HandlerApi;
