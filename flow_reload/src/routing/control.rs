use crate::{ServiceError, ServiceKey};
use interface::services::{
    HandlerHeader, HostTaskControl, HostTaskVTable, TASK_CONTROL_ABI_VERSION,
};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::rc::Rc;
use std::task::Waker;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub schema: ServiceKey,
    pub bytes: Vec<u8>,
}
impl Checkpoint {
    pub fn new(schema: ServiceKey, bytes: Vec<u8>) -> Self {
        Self { schema, bytes }
    }
}

pub(super) struct Control {
    pub stop: Cell<bool>,
    pub checkpoint: RefCell<Option<Checkpoint>>,
    pub schema: ServiceKey,
    pub max_bytes: usize,
    pub waker: RefCell<Option<Waker>>,
}
impl Control {
    pub fn new(schema: ServiceKey, checkpoint: Option<Checkpoint>, max_bytes: usize) -> Rc<Self> {
        Rc::new(Self {
            stop: Cell::new(false),
            checkpoint: RefCell::new(checkpoint),
            schema,
            max_bytes,
            waker: RefCell::new(None),
        })
    }
    pub fn stop(&self) {
        self.stop.set(true);
        let waker = self.waker.borrow().clone();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
    pub fn raw(self: &Rc<Self>) -> HostTaskControl {
        HostTaskControl {
            data: Rc::as_ptr(self).cast_mut().cast(),
            vtable: &VTABLE,
        }
    }
}
unsafe extern "C" fn retain(data: *mut c_void) -> *mut c_void {
    Rc::increment_strong_count(data.cast::<Control>());
    data
}
unsafe extern "C" fn release(data: *mut c_void) {
    drop(Rc::from_raw(data.cast::<Control>()));
}
unsafe extern "C" fn stopped(data: *mut c_void) -> bool {
    (&*data.cast::<Control>()).stop.get()
}
unsafe extern "C" fn checkpoint(
    data: *mut c_void,
    schema: ServiceKey,
    bytes: *const u8,
    len: usize,
) -> u32 {
    let control = &*data.cast::<Control>();
    if schema != control.schema || schema.id == 0 {
        return ServiceError::CONTRACT_MISMATCH.0;
    }
    if len > control.max_bytes || (len > 0 && bytes.is_null()) {
        return ServiceError::INVALID_ARGUMENT.0;
    }
    let bytes = if len == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(bytes, len).to_vec()
    };
    *control.checkpoint.borrow_mut() = Some(Checkpoint { schema, bytes });
    0
}
static VTABLE: HostTaskVTable = HostTaskVTable {
    header: HandlerHeader {
        abi_version: TASK_CONTROL_ABI_VERSION,
        struct_size: std::mem::size_of::<HostTaskVTable>() as u32,
    },
    retain,
    release,
    stop_requested: stopped,
    checkpoint,
};
