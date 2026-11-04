#[macro_export]
macro_rules! allocator {
    () => {
        #[cfg(target_os = "windows")]
        #[global_allocator]
        static FLOW_RELOAD_ALLOCATOR: std::alloc::System = std::alloc::System;

        #[cfg(target_os = "windows")]
        #[allow(dead_code)]
        const FLOW_RELOAD_ALLOCATOR_TAG: u32 = $crate::ALLOCATOR_TAG_SYSTEM;
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __main_impl {
    (@no_context $logic:ident, $state:ty, $init:block, $tick:expr) => {
        $crate::__main_impl!(@no_context_rt $logic, $state, $crate::RuntimeHandle::std(), $init, $tick);
    };
    (@no_context_rt $logic:ident, $state:ty, $runtime:expr, $init:block, $tick:expr) => {
        fn main() {
            let mut state: $state = Default::default();
            let runtime: $crate::RuntimeHandle = $runtime.into();
            $init

            #[cfg(debug_assertions)]
            $crate::internal::run_debug(
                &mut state,
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
                stringify!($logic),
                runtime,
                // This loop builds the named local SDK logic before loading it.
                |path, runtime| unsafe { $crate::FlowRunner::with_runtime(path, runtime) },
                |state| ($tick)(state),
            );

            #[cfg(not(debug_assertions))]
            loop {
                // SDK-generated exports share the blocking path with FlowRunner.
                // The future is destroyed before tick may access state.
                match $logic::hot_run(&mut state, &runtime) {
                    Ok($crate::RunStatus::Error) => eprintln!("Logic returned error status."),
                    Ok(_) => {},
                    Err(err) => {
                        eprintln!("Logic execution error: {err}");
                        return;
                    }
                }
                ($tick)(&state);
                runtime.sleep(std::time::Duration::from_millis(16));
            }
        }
    };

}

#[macro_export]
macro_rules! main {
    ($logic:ident) => {
        $crate::__main_impl!(@no_context $logic, $crate::AppState, {}, |_: &$crate::AppState| {});
    };
    ($logic:ident, runtime: $runtime:expr) => {
        $crate::__main_impl!(@no_context_rt $logic, $crate::AppState, $runtime, {}, |_: &$crate::AppState| {});
    };
    ($logic:ident, $state:ty) => {
        $crate::__main_impl!(@no_context $logic, $state, {}, |_: &$state| {});
    };
    ($logic:ident, $state:ty, runtime: $runtime:expr) => {
        $crate::__main_impl!(@no_context_rt $logic, $state, $runtime, {}, |_: &$state| {});
    };
    ($logic:ident, $state:ty, $init:block) => {
        $crate::__main_impl!(@no_context $logic, $state, $init, |_: &$state| {});
    };
    ($logic:ident, $state:ty, runtime: $runtime:expr, $init:block) => {
        $crate::__main_impl!(@no_context_rt $logic, $state, $runtime, $init, |_: &$state| {});
    };
    ($logic:ident, $state:ty, $init:block, $tick:expr) => {
        $crate::__main_impl!(@no_context $logic, $state, $init, $tick);
    };
    ($logic:ident, $state:ty, runtime: $runtime:expr, $init:block, $tick:expr) => {
        $crate::__main_impl!(@no_context_rt $logic, $state, $runtime, $init, $tick);
    };
    ($logic:ident, $state:ty, runtime: $runtime:expr, $tick:expr) => {
        $crate::__main_impl!(@no_context_rt $logic, $state, $runtime, {}, $tick);
    };
    ($($unsupported:tt)*) => {
        compile_error!("main! no longer accepts a raw context; use an explicit host with unsafe FlowRunner::with_context and uphold its lifetime/ABI contract");
    };
}
