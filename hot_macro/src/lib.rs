use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, Pat, ReturnType, Type};

#[proc_macro_attribute]
pub fn hot_swap(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    if let Err(error) = validate_signature(&input) {
        return error.to_compile_error().into();
    }
    let fn_name = &input.sig.ident;
    let is_async = input.sig.asyncness.is_some();
    let inner_name = syn::Ident::new(&format!("_inner_{}", fn_name), fn_name.span());
    let mut inner_fn = input.clone();
    inner_fn.sig.ident = inner_name.clone();
    inner_fn.vis = syn::Visibility::Inherited;

    let wrapper = if is_async {
        quote! {
            std::thread_local! {
                static FUTURE_SLOT: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>>> =
                    std::cell::RefCell::new(None);
            }

            #[repr(C)]
            struct HostWakerProxy {
                data: *mut ::std::ffi::c_void,
                vtable: *const ::flow_reload::HostWakerVTable,
            }

            unsafe fn host_proxy_new(host: &::flow_reload::HostWaker) -> *mut HostWakerProxy {
                if host.vtable.is_null() {
                    return std::ptr::null_mut();
                }
                let data = ((*host.vtable).clone)(host.data);
                let proxy = HostWakerProxy {
                    data,
                    vtable: host.vtable,
                };
                Box::into_raw(Box::new(proxy))
            }

            unsafe fn host_proxy_clone(data: *const ()) -> ::std::task::RawWaker {
                let proxy = &*(data as *const HostWakerProxy);
                let data = ((*proxy.vtable).clone)(proxy.data);
                let proxy = HostWakerProxy {
                    data,
                    vtable: proxy.vtable,
                };
                ::std::task::RawWaker::new(
                    Box::into_raw(Box::new(proxy)) as *const (),
                    &HOST_WAKER_VTABLE,
                )
            }

            unsafe fn host_proxy_wake(data: *const ()) {
                let proxy = Box::from_raw(data as *mut HostWakerProxy);
                ((*proxy.vtable).wake)(proxy.data);
            }

            unsafe fn host_proxy_wake_by_ref(data: *const ()) {
                let proxy = &*(data as *const HostWakerProxy);
                ((*proxy.vtable).wake_by_ref)(proxy.data);
            }

            unsafe fn host_proxy_drop(data: *const ()) {
                let proxy = Box::from_raw(data as *mut HostWakerProxy);
                ((*proxy.vtable).drop)(proxy.data);
            }

            static HOST_WAKER_VTABLE: ::std::task::RawWakerVTable =
                ::std::task::RawWakerVTable::new(
                    host_proxy_clone,
                    host_proxy_wake,
                    host_proxy_wake_by_ref,
                    host_proxy_drop,
                );

            fn noop_raw() -> ::std::task::RawWaker {
                fn noop_clone(_: *const ()) -> ::std::task::RawWaker {
                    noop_raw()
                }
                fn noop(_: *const ()) {}
                let vtable = &::std::task::RawWakerVTable::new(noop_clone, noop, noop, noop);
                ::std::task::RawWaker::new(::std::ptr::null(), vtable)
            }

            unsafe fn waker_from_host(host: &::flow_reload::HostWaker) -> ::std::task::Waker {
                let proxy = host_proxy_new(host);
                if proxy.is_null() {
                    return ::std::task::Waker::from_raw(noop_raw());
                }
                ::std::task::Waker::from_raw(::std::task::RawWaker::new(
                    proxy as *const (),
                    &HOST_WAKER_VTABLE,
                ))
            }

            struct AttachedDirectFuture {
                future: ::std::pin::Pin<Box<dyn ::std::future::Future<Output = ()>>>,
            }

            unsafe extern "C" fn attached_poll(
                data: *mut ::std::ffi::c_void,
                host_waker: *const ::flow_reload::HostWaker,
            ) -> ::flow_reload::RunStatus {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if data.is_null() || host_waker.is_null() {
                        return ::flow_reload::RunStatus::Error;
                    }
                    ::flow_reload::set_attached_host_waker(host_waker);
                    let fut = &mut *(data as *mut AttachedDirectFuture);
                    let waker = waker_from_host(&*host_waker);
                    let mut cx = ::std::task::Context::from_waker(&waker);
                    let status = match fut.future.as_mut().poll(&mut cx) {
                        ::std::task::Poll::Pending => ::flow_reload::RunStatus::Pending,
                        ::std::task::Poll::Ready(_) => ::flow_reload::RunStatus::Done,
                    };
                    ::flow_reload::clear_attached_host_waker();
                    status
                }));
                match result {
                    Ok(status) => status,
                    Err(_) => ::flow_reload::RunStatus::Error,
                }
            }

            unsafe extern "C" fn attached_drop(data: *mut ::std::ffi::c_void) {
                if data.is_null() {
                    return;
                }
                drop(Box::from_raw(data as *mut AttachedDirectFuture));
            }

            static ATTACHED_VTABLE: ::flow_reload::AttachedFutureVTable =
                ::flow_reload::AttachedFutureVTable {
                    poll: attached_poll,
                    drop: attached_drop,
                };

            #inner_fn

            #[no_mangle]
            pub extern "C" fn hot_type() -> ::flow_reload::Metadata {
                ::flow_reload::Metadata {
                    abi_version: ::flow_reload::ABI_VERSION,
                    func_type: ::flow_reload::FuncType::Async,
                    state_size: std::mem::size_of::<::flow_reload::AppState>() as u32,
                    allocator_tag: {
                        #[cfg(target_os = "windows")]
                        {
                            crate::FLOW_RELOAD_ALLOCATOR_TAG
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            0
                        }
                    },
                }
            }

            #[no_mangle]
            pub unsafe extern "C" fn hot_entry(
                state_ptr: *mut ::flow_reload::AppState,
            ) -> ::flow_reload::RunStatus {
                use std::future::Future;

                let state = &mut *state_ptr;
                FUTURE_SLOT.with(|slot| {
                    let mut slot_ref = slot.borrow_mut();

                    if slot_ref.is_none() {
                        let fut = #inner_name(unsafe { std::mem::transmute(state) });
                        *slot_ref = Some(Box::pin(fut));
                    }

                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        fn noop_clone(_: *const ()) -> std::task::RawWaker {
                            noop_raw()
                        }
                        fn noop(_: *const ()) {}
                        fn noop_raw() -> std::task::RawWaker {
                            let vtable = &std::task::RawWakerVTable::new(
                                noop_clone,
                                noop,
                                noop,
                                noop,
                            );
                            std::task::RawWaker::new(std::ptr::null(), vtable)
                        }

                        let waker = unsafe { std::task::Waker::from_raw(noop_raw()) };
                        let mut cx = std::task::Context::from_waker(&waker);

                        match slot_ref.as_mut().unwrap().as_mut().poll(&mut cx) {
                            std::task::Poll::Ready(_) => {
                                *slot_ref = None;
                                ::flow_reload::RunStatus::Done
                            }
                            std::task::Poll::Pending => ::flow_reload::RunStatus::Pending,
                        }
                    }));

                    match result {
                        Ok(status) => status,
                        Err(_) => {
                            *slot_ref = None;
                            ::flow_reload::RunStatus::Error
                        }
                    }
                })
            }

            #[no_mangle]
            pub extern "C" fn hot_attached_supported() -> bool {
                true
            }

            #[no_mangle]
            pub unsafe extern "C" fn hot_make_future(
                state_ptr: *mut ::flow_reload::AppState,
            ) -> ::flow_reload::AttachedFuture {
                if state_ptr.is_null() {
                    return ::flow_reload::AttachedFuture {
                        data: std::ptr::null_mut(),
                        vtable: std::ptr::null(),
                    };
                }
                if !::flow_reload::attached_runtime_available() {
                    return ::flow_reload::AttachedFuture {
                        data: std::ptr::null_mut(),
                        vtable: std::ptr::null(),
                    };
                }
                let state = &mut *state_ptr;
                let fut = #inner_name(unsafe { std::mem::transmute(state) });
                let task = Box::new(AttachedDirectFuture {
                    future: Box::pin(fut),
                });
                ::flow_reload::AttachedFuture {
                    data: Box::into_raw(task) as *mut ::std::ffi::c_void,
                    vtable: &ATTACHED_VTABLE,
                }
            }

            #[no_mangle]
            pub unsafe extern "C" fn hot_attach_runtime(
                handle: *const ::flow_reload::HostRuntimeVTable,
            ) -> bool {
                ::flow_reload::attach_tokio_runtime(handle)
            }

            #[no_mangle]
            pub extern "C" fn hot_reset() {
                FUTURE_SLOT.with(|slot| {
                    *slot.borrow_mut() = None;
                });
            }
        }
    } else {
        quote! {
            #inner_fn

            #[no_mangle]
            pub extern "C" fn hot_type() -> ::flow_reload::Metadata {
                ::flow_reload::Metadata {
                    abi_version: ::flow_reload::ABI_VERSION,
                    func_type: ::flow_reload::FuncType::Sync,
                    state_size: std::mem::size_of::<::flow_reload::AppState>() as u32,
                    allocator_tag: {
                        #[cfg(target_os = "windows")]
                        {
                            crate::FLOW_RELOAD_ALLOCATOR_TAG
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            0
                        }
                    },
                }
            }

            #[no_mangle]
            pub unsafe extern "C" fn hot_entry(
                state_ptr: *mut ::flow_reload::AppState,
            ) -> ::flow_reload::RunStatus {
                let state = &mut *state_ptr;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    #inner_name(state);
                }));
                match result {
                    Ok(()) => ::flow_reload::RunStatus::Done,
                    Err(_) => ::flow_reload::RunStatus::Error,
                }
            }

            #[no_mangle]
            pub extern "C" fn hot_attached_supported() -> bool {
                false
            }

            #[no_mangle]
            pub unsafe extern "C" fn hot_make_future(
                _state_ptr: *mut ::flow_reload::AppState,
            ) -> ::flow_reload::AttachedFuture {
                ::flow_reload::AttachedFuture {
                    data: std::ptr::null_mut(),
                    vtable: std::ptr::null(),
                }
            }

            #[no_mangle]
            pub unsafe extern "C" fn hot_attach_runtime(
                _handle: *const ::flow_reload::HostRuntimeVTable,
            ) -> bool {
                false
            }

            #[no_mangle]
            pub extern "C" fn hot_reset() {}
        }
    };

    TokenStream::from(wrapper)
}

#[proc_macro_attribute]
pub fn reload_init(attr: TokenStream, item: TokenStream) -> TokenStream {
    hot_init(attr, item)
}

#[proc_macro_attribute]
pub fn reload_snapshot(attr: TokenStream, item: TokenStream) -> TokenStream {
    hot_reload(attr, item)
}

#[proc_macro_attribute]
pub fn reloadable(attr: TokenStream, item: TokenStream) -> TokenStream {
    hot_swap(attr, item)
}

#[proc_macro_attribute]
pub fn reload_direct(attr: TokenStream, item: TokenStream) -> TokenStream {
    hot_swap(attr, item)
}

#[proc_macro_attribute]
pub fn hot_reload(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let state_ty = match validate_reload_signature(&input) {
        Ok(ty) => ty,
        Err(error) => return error.to_compile_error().into(),
    };

    let fn_name = &input.sig.ident;
    let is_async = input.sig.asyncness.is_some();
    let inner_name = syn::Ident::new(&format!("_inner_{}", fn_name), fn_name.span());
    let mut inner_fn = input.clone();
    inner_fn.sig.ident = inner_name.clone();
    inner_fn.vis = syn::Visibility::Inherited;

    let wrapper = if is_async {
        quote! {
            struct CartridgeSlot<T> {
                state: Box<T>,
                future: std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
            }

            std::thread_local! {
                static SLOT: std::cell::RefCell<Option<CartridgeSlot<#state_ty>>> =
                    std::cell::RefCell::new(None);
            }

            #inner_fn

            #[no_mangle]
            pub extern "C" fn hot_type() -> ::flow_reload::Metadata {
                ::flow_reload::Metadata {
                    abi_version: ::flow_reload::ABI_VERSION,
                    func_type: ::flow_reload::FuncType::Async,
                    state_size: std::mem::size_of::<::flow_reload::AppState>() as u32,
                    allocator_tag: {
                        #[cfg(target_os = "windows")]
                        {
                            crate::FLOW_RELOAD_ALLOCATOR_TAG
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            0
                        }
                    },
                }
            }

            #[no_mangle]
            pub unsafe extern "C" fn hot_entry(
                state_ptr: *mut ::flow_reload::AppState,
            ) -> ::flow_reload::RunStatus {
                use std::future::Future;
                use ::flow_reload::CartridgeState;

                let state = &mut *state_ptr;
                SLOT.with(|slot| {
                    let mut slot_ref = slot.borrow_mut();

                    if slot_ref.is_none() {
                        let mut boxed: Box<#state_ty> = Box::new(Default::default());
                        let bytes = state.cartridge_bytes();
                        if !bytes.is_empty() {
                            if boxed.restore(bytes).is_err() {
                                return ::flow_reload::RunStatus::Error;
                            }
                        }
                        let state_ptr: *mut #state_ty = boxed.as_mut();
                        let fut = #inner_name(unsafe { &mut *state_ptr });
                        *slot_ref = Some(CartridgeSlot {
                            state: boxed,
                            future: Box::pin(fut),
                        });
                    }

                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        fn noop_clone(_: *const ()) -> std::task::RawWaker {
                            noop_raw()
                        }
                        fn noop(_: *const ()) {}
                        fn noop_raw() -> std::task::RawWaker {
                            let vtable = &std::task::RawWakerVTable::new(
                                noop_clone,
                                noop,
                                noop,
                                noop,
                            );
                            std::task::RawWaker::new(std::ptr::null(), vtable)
                        }

                        let waker = unsafe { std::task::Waker::from_raw(noop_raw()) };
                        let mut cx = std::task::Context::from_waker(&waker);

                        let poll = {
                            let slot_mut = slot_ref.as_mut().unwrap();
                            slot_mut.future.as_mut().poll(&mut cx)
                        };

                        match poll {
                            std::task::Poll::Ready(_) => {
                                let mut buf = Vec::new();
                                {
                                    let slot_mut = slot_ref.as_mut().unwrap();
                                    slot_mut.state.snapshot(&mut buf);
                                }
                                if !state.write_cartridge(&buf) {
                                    *slot_ref = None;
                                    return ::flow_reload::RunStatus::Error;
                                }
                                *slot_ref = None;
                                ::flow_reload::RunStatus::Done
                            }
                            std::task::Poll::Pending => ::flow_reload::RunStatus::Pending,
                        }
                    }));

                    match result {
                        Ok(status) => status,
                        Err(_) => {
                            *slot_ref = None;
                            ::flow_reload::RunStatus::Error
                        }
                    }
                })
            }
        }
    } else {
        quote! {
            #inner_fn

            #[no_mangle]
            pub extern "C" fn hot_type() -> ::flow_reload::Metadata {
                ::flow_reload::Metadata {
                    abi_version: ::flow_reload::ABI_VERSION,
                    func_type: ::flow_reload::FuncType::Sync,
                    state_size: std::mem::size_of::<::flow_reload::AppState>() as u32,
                    allocator_tag: {
                        #[cfg(target_os = "windows")]
                        {
                            crate::FLOW_RELOAD_ALLOCATOR_TAG
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            0
                        }
                    },
                }
            }

            #[no_mangle]
            pub unsafe extern "C" fn hot_entry(
                state_ptr: *mut ::flow_reload::AppState,
            ) -> ::flow_reload::RunStatus {
                use ::flow_reload::CartridgeState;

                let state = &mut *state_ptr;
                let mut cartridge_state: #state_ty = Default::default();
                let bytes = state.cartridge_bytes();
                if !bytes.is_empty() {
                    if cartridge_state.restore(bytes).is_err() {
                        return ::flow_reload::RunStatus::Error;
                    }
                }
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    #inner_name(&mut cartridge_state);
                }));
                match result {
                    Ok(()) => {
                        let mut buf = Vec::new();
                        cartridge_state.snapshot(&mut buf);
                        if !state.write_cartridge(&buf) {
                            return ::flow_reload::RunStatus::Error;
                        }
                        ::flow_reload::RunStatus::Done
                    }
                    Err(_) => ::flow_reload::RunStatus::Error,
                }
            }
        }
    };

    TokenStream::from(wrapper)
}

#[proc_macro_attribute]
pub fn hot_init(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    if let Err(error) = validate_init_signature(&input) {
        return error.to_compile_error().into();
    }
    let fn_name = &input.sig.ident;
    let inner_name = syn::Ident::new(&format!("_inner_{}", fn_name), fn_name.span());
    let mut inner_fn = input.clone();
    inner_fn.sig.ident = inner_name.clone();
    inner_fn.vis = syn::Visibility::Inherited;

    let ctx_type = extract_mut_ref_arg(&input).expect("validated init arg");

    let wrapper = quote! {
        #inner_fn

        #[no_mangle]
        pub unsafe extern "C" fn hot_init(
            ctx: *mut ::std::ffi::c_void,
        ) -> ::flow_reload::RunStatus {
            if ctx.is_null() {
                return ::flow_reload::RunStatus::Error;
            }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let ctx = &mut *(ctx as *mut #ctx_type);
                #inner_name(ctx);
            }));
            match result {
                Ok(()) => ::flow_reload::RunStatus::Done,
                Err(_) => ::flow_reload::RunStatus::Error,
            }
        }
    };

    TokenStream::from(wrapper)
}

fn validate_signature(input: &ItemFn) -> Result<(), syn::Error> {
    if !input.sig.generics.params.is_empty() || input.sig.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &input.sig.generics,
            "#[reload_direct] does not support generics",
        ));
    }
    if input.sig.variadic.is_some() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "#[reload_direct] does not support variadic arguments",
        ));
    }
    if input.sig.inputs.len() != 1 {
        return Err(syn::Error::new_spanned(
            &input.sig.inputs,
            "#[reload_direct] requires exactly one argument: &mut AppState",
        ));
    }

    let arg = input.sig.inputs.first().unwrap();
    let arg_type = match arg {
        FnArg::Typed(pat_type) => {
            if !matches!(*pat_type.pat, Pat::Ident(_)) {
                return Err(syn::Error::new_spanned(
                    &pat_type.pat,
                    "#[reload_direct] argument must be an identifier pattern",
                ));
            }
            pat_type.ty.as_ref()
        }
        FnArg::Receiver(receiver) => {
            return Err(syn::Error::new_spanned(
                receiver,
                "#[reload_direct] does not support methods",
            ));
        }
    };

    let Type::Reference(reference) = arg_type else {
        return Err(syn::Error::new_spanned(
            arg_type,
            "#[reload_direct] argument must be &mut AppState",
        ));
    };
    if reference.mutability.is_none() {
        return Err(syn::Error::new_spanned(
            reference,
            "#[reload_direct] argument must be &mut AppState",
        ));
    }
    let Type::Path(type_path) = reference.elem.as_ref() else {
        return Err(syn::Error::new_spanned(
            &reference.elem,
            "#[reload_direct] argument must be &mut AppState",
        ));
    };
    let is_app_state = type_path
        .path
        .segments
        .last()
        .map(|segment| segment.ident == "AppState")
        .unwrap_or(false);
    if !is_app_state {
        return Err(syn::Error::new_spanned(
            type_path,
            "#[reload_direct] argument must be &mut AppState",
        ));
    }

    if !is_unit_return(&input.sig.output) {
        return Err(syn::Error::new_spanned(
            &input.sig.output,
            "#[reload_direct] functions must return ()",
        ));
    }

    Ok(())
}

fn validate_reload_signature(input: &ItemFn) -> Result<Type, syn::Error> {
    if !input.sig.generics.params.is_empty() || input.sig.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &input.sig.generics,
            "#[reload_snapshot] does not support generics",
        ));
    }
    if input.sig.variadic.is_some() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "#[reload_snapshot] does not support variadic arguments",
        ));
    }
    if input.sig.inputs.len() != 1 {
        return Err(syn::Error::new_spanned(
            &input.sig.inputs,
            "#[reload_snapshot] requires exactly one argument: &mut State",
        ));
    }

    let arg = input.sig.inputs.first().unwrap();
    let arg_type = match arg {
        FnArg::Typed(pat_type) => {
            if !matches!(*pat_type.pat, Pat::Ident(_)) {
                return Err(syn::Error::new_spanned(
                    &pat_type.pat,
                    "#[reload_snapshot] argument must be an identifier pattern",
                ));
            }
            pat_type.ty.as_ref()
        }
        FnArg::Receiver(receiver) => {
            return Err(syn::Error::new_spanned(
                receiver,
                "#[reload_snapshot] does not support methods",
            ));
        }
    };

    let Type::Reference(reference) = arg_type else {
        return Err(syn::Error::new_spanned(
            arg_type,
            "#[reload_snapshot] argument must be &mut State",
        ));
    };
    if reference.mutability.is_none() {
        return Err(syn::Error::new_spanned(
            reference,
            "#[reload_snapshot] argument must be &mut State",
        ));
    }

    if !is_unit_return(&input.sig.output) {
        return Err(syn::Error::new_spanned(
            &input.sig.output,
            "#[reload_snapshot] functions must return ()",
        ));
    }

    Ok((*reference.elem).clone())
}

fn validate_init_signature(input: &ItemFn) -> Result<(), syn::Error> {
    if !input.sig.generics.params.is_empty() || input.sig.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &input.sig.generics,
            "#[reload_init] does not support generics",
        ));
    }
    if input.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            &input.sig.asyncness,
            "#[reload_init] does not support async functions",
        ));
    }
    if input.sig.variadic.is_some() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "#[reload_init] does not support variadic arguments",
        ));
    }
    if input.sig.inputs.len() != 1 {
        return Err(syn::Error::new_spanned(
            &input.sig.inputs,
            "#[reload_init] requires exactly one argument: &mut Context",
        ));
    }

    let _ = extract_mut_ref_arg(input)?;

    if !is_unit_return(&input.sig.output) {
        return Err(syn::Error::new_spanned(
            &input.sig.output,
            "#[reload_init] functions must return ()",
        ));
    }

    Ok(())
}

fn extract_mut_ref_arg(input: &ItemFn) -> Result<Type, syn::Error> {
    let arg = input.sig.inputs.first().unwrap();
    let arg_type = match arg {
        FnArg::Typed(pat_type) => {
            if !matches!(*pat_type.pat, Pat::Ident(_)) {
                return Err(syn::Error::new_spanned(
                    &pat_type.pat,
                    "#[reload_init] argument must be an identifier pattern",
                ));
            }
            pat_type.ty.as_ref()
        }
        FnArg::Receiver(receiver) => {
            return Err(syn::Error::new_spanned(
                receiver,
                "#[reload_init] does not support methods",
            ));
        }
    };

    let Type::Reference(reference) = arg_type else {
        return Err(syn::Error::new_spanned(
            arg_type,
            "#[reload_init] argument must be &mut Context",
        ));
    };
    if reference.mutability.is_none() {
        return Err(syn::Error::new_spanned(
            reference,
            "#[reload_init] argument must be &mut Context",
        ));
    }

    Ok((*reference.elem).clone())
}

fn is_unit_return(output: &ReturnType) -> bool {
    match output {
        ReturnType::Default => true,
        ReturnType::Type(_, ty) => matches!(**ty, Type::Tuple(ref tuple) if tuple.elems.is_empty()),
    }
}
