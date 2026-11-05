mod activity;
mod async_service;
mod attached;
mod cache;
mod environment;
mod execution;
mod request;
mod resources;
mod retention;
mod routing;
mod runner;
mod runtime;
mod thread_service;

pub use activity::{ChildStop, ManagedActivity, MAX_MANAGED_ACTIVITIES};
pub use async_service::{
    AsyncLimits, CancellationToken, HostServiceFuture, ServiceCall, MAX_ASYNC_BUFFER,
};
pub use attached::{
    attach_tokio_runtime, attached_runtime_available, clear_attached_host_waker,
    set_attached_host_waker, spawn_detached_sleep, spawn_detached_tcp_request,
    tokio_runtime_vtable, AttachedFutureTask, AttachedRuntimeGuard, AttachedSleep,
    AttachedTcpRequest,
};
pub use cache::{clean_cache, inspect_cache, CacheEntry, CacheOptions, CacheReport, CacheStatus};
pub use environment::{HandlerEnvironment, InitData, InitPayload};
pub use execution::Execution;
pub use hot_macro::{
    hot_init, hot_reload, hot_swap, reload_direct, reload_handler, reload_init, reload_snapshot,
    reloadable,
};
pub use interface::services::{
    ActivityHandle, ActivityVTable, AsyncOperationId, AsyncPolled, AsyncStarted, HandlerApi,
    HandlerHeader, HandlerRequest, HostServices, HostServicesVTable, ResourceId, ServiceBinding,
    ServiceError, ServiceKey, ServiceRequirement, ServiceResult, ACTIVITY_ABI_VERSION,
    HANDLER_ABI_VERSION, MAX_INIT_BYTES, MAX_SERVICE_REQUIREMENTS, SERVICES_ABI_VERSION,
};
pub use interface::{
    AppState, AttachRuntimeFn, AttachedFuture, AttachedFutureVTable, AttachedSupportedFn,
    CartridgeState, FuncType, HostRuntimeVTable, HostWaker, HostWakerVTable, InitFn,
    MakeAttachedFutureFn, Metadata, ResetAttachedFn, RunStatus, ABI_VERSION, CARTRIDGE_CAPACITY,
};
pub use request::RequestContext;
pub use resources::{HostResource, HostResources, Resource, Services};
pub use retention::{
    retention_snapshot, ArtifactId, CopyRemovalError, ImageOutcome, ImageRecord, ImageState,
    RetentionSnapshot,
};
pub use routing::{
    poll_cancelled_requests, Checkpoint, GenerationBudget, PreparedHandler, RequestRouter,
    RequestTask, Restart, RouteError, RouterOptions, WorkerId,
};
pub use runner::{collect_retired_libraries, FlowRunner, RunnerError, UnloadPolicy};
pub use runtime::{RuntimeBridge, RuntimeHandle, StdRuntime};
pub use thread_service::{ServiceClient, ServiceWorker, ThreadService, ThreadServiceLimits};

pub const ALLOCATOR_TAG_SYSTEM: u32 = 1;

#[doc(hidden)]
pub mod internal;

mod macros;
