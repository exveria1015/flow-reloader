mod attached;
mod execution;
mod request;
mod resources;
mod routing;
mod runner;
mod runtime;

pub use attached::{
    attach_tokio_runtime, attached_runtime_available, clear_attached_host_waker,
    set_attached_host_waker, spawn_detached_sleep, spawn_detached_tcp_request,
    tokio_runtime_vtable, AttachedFutureTask, AttachedRuntimeGuard, AttachedSleep,
    AttachedTcpRequest,
};
pub use execution::Execution;
pub use hot_macro::{
    hot_init, hot_reload, hot_swap, reload_direct, reload_handler, reload_init, reload_snapshot,
    reloadable,
};
pub use interface::services::{
    HandlerApi, HandlerHeader, HandlerRequest, HostServices, HostServicesVTable, ResourceId,
    ServiceError, ServiceKey, ServiceResult, HANDLER_ABI_VERSION, SERVICES_ABI_VERSION,
};
pub use interface::{
    AppState, AttachRuntimeFn, AttachedFuture, AttachedFutureVTable, AttachedSupportedFn,
    CartridgeState, FuncType, HostRuntimeVTable, HostWaker, HostWakerVTable, InitFn,
    MakeAttachedFutureFn, Metadata, ResetAttachedFn, RunStatus, ABI_VERSION, CARTRIDGE_CAPACITY,
};
pub use request::RequestContext;
pub use resources::{HostResource, HostResources, Resource, Services};
pub use routing::{
    Checkpoint, GenerationBudget, PreparedHandler, RequestRouter, RequestTask, Restart, RouteError,
    RouterOptions, WorkerId,
};
pub use runner::{collect_retired_libraries, FlowRunner, RunnerError, UnloadPolicy};
pub use runtime::{RuntimeBridge, RuntimeHandle, StdRuntime};

pub const ALLOCATOR_TAG_SYSTEM: u32 = 1;

#[doc(hidden)]
pub mod internal;

mod macros;
