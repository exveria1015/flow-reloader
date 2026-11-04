use std::borrow::Cow;
use std::ffi::c_void;
use std::ops::{Deref, DerefMut};

pub mod services;

pub const CARTRIDGE_CAPACITY: usize = 8192;

#[repr(C)]
pub struct AppState {
    pub counter: i32,
    pub message: [u8; 256],
    pub cartridge_len: u32,
    pub cartridge: [u8; CARTRIDGE_CAPACITY],
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            counter: 0,
            message: [0; 256],
            cartridge_len: 0,
            cartridge: [0; CARTRIDGE_CAPACITY],
        }
    }
}

impl AppState {
    pub fn set_message(&mut self, msg: &str) {
        self.message.fill(0);
        let bytes = msg.as_bytes();
        let len = bytes.len().min(self.message.len().saturating_sub(1));
        self.message[..len].copy_from_slice(&bytes[..len]);
    }

    pub fn message_str(&self) -> Cow<'_, str> {
        let end = self
            .message
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(self.message.len());
        String::from_utf8_lossy(&self.message[..end])
    }

    pub fn cartridge_bytes(&self) -> &[u8] {
        let len = (self.cartridge_len as usize).min(self.cartridge.len());
        &self.cartridge[..len]
    }

    pub fn write_cartridge(&mut self, bytes: &[u8]) -> bool {
        if bytes.len() > self.cartridge.len() {
            return false;
        }
        self.cartridge.fill(0);
        self.cartridge[..bytes.len()].copy_from_slice(bytes);
        self.cartridge_len = bytes.len() as u32;
        true
    }

    pub fn load_cartridge<T: CartridgeState>(&self) -> Result<T, String> {
        let mut state = T::default();
        let bytes = self.cartridge_bytes();
        if !bytes.is_empty() {
            state.restore(bytes)?;
        }
        Ok(state)
    }

    pub fn save_cartridge<T: CartridgeState>(&mut self, value: &T) -> Result<(), String> {
        let mut buf = Vec::new();
        value.snapshot(&mut buf)?;
        if !self.write_cartridge(&buf) {
            return Err("cartridge capacity exceeded".to_string());
        }
        Ok(())
    }
}

pub struct State<T: CartridgeState> {
    inner: T,
}

impl<T: CartridgeState> State<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    pub fn load(app: &AppState) -> Result<Self, String> {
        Ok(Self {
            inner: app.load_cartridge()?,
        })
    }

    pub fn save(&self, app: &mut AppState) -> Result<(), String> {
        app.save_cartridge(&self.inner)
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: CartridgeState> Default for State<T> {
    fn default() -> Self {
        Self {
            inner: T::default(),
        }
    }
}

impl<T: CartridgeState> Deref for State<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: CartridgeState> DerefMut for State<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T: CartridgeState> CartridgeState for State<T> {
    fn snapshot(&self, out: &mut Vec<u8>) -> Result<(), String> {
        self.inner.snapshot(out)
    }

    fn restore(&mut self, input: &[u8]) -> Result<(), String> {
        self.inner.restore(input)
    }
}

pub trait CartridgeState: Default {
    fn snapshot(&self, out: &mut Vec<u8>) -> Result<(), String>;
    fn restore(&mut self, input: &[u8]) -> Result<(), String>;
}

#[cfg(feature = "cartridge-serde")]
impl<T> CartridgeState for T
where
    T: Default + serde::Serialize + serde::de::DeserializeOwned,
{
    fn snapshot(&self, out: &mut Vec<u8>) -> Result<(), String> {
        let bytes = bincode::serialize(self).map_err(|err| err.to_string())?;
        out.extend_from_slice(&bytes);
        Ok(())
    }

    fn restore(&mut self, input: &[u8]) -> Result<(), String> {
        if input.is_empty() {
            return Ok(());
        }
        let value: T = bincode::deserialize(input).map_err(|err| err.to_string())?;
        *self = value;
        Ok(())
    }
}

// Async execution uses per-task hot_make_future handles, never a TLS entry slot.
pub const ABI_VERSION: u32 = 4;

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RunStatus {
    Done = 0,
    Pending = 1,
    Error = 2,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FuncType {
    Sync = 0,
    Async = 1,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub abi_version: u32,
    pub func_type: FuncType,
    pub state_size: u32,
    pub allocator_tag: u32,
}

pub type MetadataFn = unsafe extern "C" fn() -> Metadata;
pub type EntryFn = unsafe extern "C" fn(*mut AppState) -> RunStatus;
pub type InitFn = unsafe extern "C" fn(*mut std::ffi::c_void) -> RunStatus;

#[repr(C)]
pub struct HostWakerVTable {
    pub clone: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub wake: unsafe extern "C" fn(*mut c_void),
    pub wake_by_ref: unsafe extern "C" fn(*mut c_void),
    pub drop: unsafe extern "C" fn(*mut c_void),
}

#[repr(C)]
pub struct HostWaker {
    pub data: *mut c_void,
    pub vtable: *const HostWakerVTable,
}

#[repr(C)]
pub struct AttachedFutureVTable {
    pub poll: unsafe extern "C" fn(*mut c_void, *const HostWaker) -> RunStatus,
    pub drop: unsafe extern "C" fn(*mut c_void),
}

#[repr(C)]
/// An owned foreign future handle. Ownership is transferred to one caller,
/// which must invoke `vtable.drop(data)` exactly once while the library is live.
/// A null data or vtable pointer denotes an invalid handle. Borrowed state and
/// callback context must remain valid until the foreign future is destroyed.
pub struct AttachedFuture {
    pub data: *mut c_void,
    pub vtable: *const AttachedFutureVTable,
}

#[repr(C)]
pub struct HostRuntimeVTable {
    pub ctx: *mut c_void,
    pub sleep_ms: unsafe extern "C" fn(*mut c_void, u64) -> AttachedFuture,
    pub tcp_request: unsafe extern "C" fn(
        *mut c_void,
        *const u8,
        usize,
        *const u8,
        usize,
        *mut u8,
        usize,
    ) -> AttachedFuture,
    pub spawn_sleep_ms: unsafe extern "C" fn(*mut c_void, u64),
    pub spawn_tcp_request: unsafe extern "C" fn(*mut c_void, *const u8, usize, *const u8, usize),
}

pub type AttachedSupportedFn = unsafe extern "C" fn() -> bool;
pub type AttachRuntimeFn = unsafe extern "C" fn(*const HostRuntimeVTable) -> bool;
pub type MakeAttachedFutureFn = unsafe extern "C" fn(*mut AppState) -> AttachedFuture;
pub type ResetAttachedFn = unsafe extern "C" fn();
