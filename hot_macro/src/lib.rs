use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

mod direct;
mod exports;
mod handler;
mod snapshot;
mod validation;

use validation::{extract_mut_ref_arg, validate_init_signature};

#[proc_macro_attribute]
pub fn reload_handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let service = parse_macro_input!(attr as handler::Options);
    let input = parse_macro_input!(item as ItemFn);
    match handler::expand(input, service) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn hot_swap(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    if let Err(error) = validation::validate_signature(&input) {
        return error.to_compile_error().into();
    }
    direct::expand(input).into()
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
    match validation::validate_reload_signature(&input) {
        Ok(state_ty) => snapshot::expand(input, state_ty).into(),
        Err(error) => error.to_compile_error().into(),
    }
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
