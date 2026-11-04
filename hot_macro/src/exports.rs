use proc_macro2::TokenStream;
use quote::quote;

fn metadata(is_async: bool) -> TokenStream {
    let kind = if is_async {
        quote!(Async)
    } else {
        quote!(Sync)
    };
    quote! {
        #[no_mangle]
        pub extern "C" fn hot_type() -> ::flow_reload::Metadata {
            ::flow_reload::Metadata {
                abi_version: ::flow_reload::ABI_VERSION,
                func_type: ::flow_reload::FuncType::#kind,
                state_size: std::mem::size_of::<::flow_reload::AppState>() as u32,
                allocator_tag: {
                    #[cfg(target_os = "windows")]
                    { crate::FLOW_RELOAD_ALLOCATOR_TAG }
                    #[cfg(not(target_os = "windows"))]
                    { 0 }
                },
            }
        }

        /// Type-checked Rust wrapper for statically linked SDK logic.
        pub fn hot_run(
            state: &mut ::flow_reload::AppState,
            runtime: &::flow_reload::RuntimeHandle,
        ) -> Result<::flow_reload::RunStatus, ::flow_reload::RunnerError> {
            // These functions are generated together by this SDK.
            unsafe {
                ::flow_reload::internal::run_static(
                    state, runtime, hot_type, hot_entry, hot_make_future,
                )
            }
        }

        // Kept for instance-level cleanup in the current ABI. No pending task
        // is stored globally; each handle has its own destructor.
        #[no_mangle]
        pub extern "C" fn hot_reset() {}
    }
}

pub(crate) fn asynchronous(body: TokenStream) -> TokenStream {
    let metadata = metadata(true);
    quote! {
        #metadata

        struct LogicFuture {
            future: std::pin::Pin<Box<dyn std::future::Future<Output = ::flow_reload::RunStatus>>>,
        }

        unsafe extern "C" fn logic_poll(
            data: *mut std::ffi::c_void,
            waker: *const ::flow_reload::HostWaker,
        ) -> ::flow_reload::RunStatus {
            let task = &mut *data.cast::<LogicFuture>();
            ::flow_reload::internal::poll_attached_result_future(task.future.as_mut(), waker)
        }

        unsafe extern "C" fn logic_drop(data: *mut std::ffi::c_void) {
            drop(Box::from_raw(data.cast::<LogicFuture>()));
        }

        static LOGIC_FUTURE_VTABLE: ::flow_reload::AttachedFutureVTable =
            ::flow_reload::AttachedFutureVTable { poll: logic_poll, drop: logic_drop };

        /// Async execution requires an owned handle, rather than hot_entry.
        #[no_mangle]
        pub unsafe extern "C" fn hot_entry(
            _: *mut ::flow_reload::AppState,
        ) -> ::flow_reload::RunStatus {
            ::flow_reload::RunStatus::Error
        }

        #[no_mangle]
        pub unsafe extern "C" fn hot_make_future(
            state_ptr: *mut ::flow_reload::AppState,
        ) -> ::flow_reload::AttachedFuture {
            if state_ptr.is_null() {
                return ::flow_reload::AttachedFuture {
                    data: std::ptr::null_mut(), vtable: std::ptr::null(),
                };
            }
            // Construction does not borrow state or run user code yet. The
            // pinned outer future owns every value needed across await points.
            let task = LogicFuture { future: Box::pin(async move { #body }) };
            ::flow_reload::AttachedFuture {
                data: Box::into_raw(Box::new(task)).cast(),
                vtable: &LOGIC_FUTURE_VTABLE,
            }
        }

        #[no_mangle]
        pub extern "C" fn hot_attached_supported() -> bool { true }

        #[no_mangle]
        pub unsafe extern "C" fn hot_attach_runtime(
            table: *const ::flow_reload::HostRuntimeVTable,
        ) -> bool {
            ::flow_reload::attach_tokio_runtime(table)
        }
    }
}

pub(crate) fn synchronous(body: TokenStream) -> TokenStream {
    let metadata = metadata(false);
    quote! {
        #metadata

        #[no_mangle]
        pub unsafe extern "C" fn hot_entry(
            state_ptr: *mut ::flow_reload::AppState,
        ) -> ::flow_reload::RunStatus {
            if state_ptr.is_null() { return ::flow_reload::RunStatus::Error; }
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { #body }))
                .unwrap_or(::flow_reload::RunStatus::Error)
        }

        #[no_mangle]
        pub unsafe extern "C" fn hot_make_future(
            _: *mut ::flow_reload::AppState,
        ) -> ::flow_reload::AttachedFuture {
            ::flow_reload::AttachedFuture {
                data: std::ptr::null_mut(), vtable: std::ptr::null(),
            }
        }

        #[no_mangle]
        pub extern "C" fn hot_attached_supported() -> bool { false }

        #[no_mangle]
        pub unsafe extern "C" fn hot_attach_runtime(
            _: *const ::flow_reload::HostRuntimeVTable,
        ) -> bool { false }
    }
}
