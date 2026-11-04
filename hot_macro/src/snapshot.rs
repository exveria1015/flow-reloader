use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, Type};

pub(crate) fn expand(mut input: ItemFn, state_ty: Type) -> TokenStream {
    let is_async = input.sig.asyncness.is_some();
    let inner = syn::Ident::new(
        &format!("_inner_{}", input.sig.ident),
        input.sig.ident.span(),
    );
    input.sig.ident = inner.clone();
    input.vis = syn::Visibility::Inherited;
    let call = if is_async {
        quote! { #inner(&mut state).await; }
    } else {
        quote! { #inner(&mut state); }
    };
    // An async outer future owns state; Rust generates its pinned self-borrows.
    // No erased lifetime or manually self-referential state allocation is needed.
    let body = quote! {
        use ::flow_reload::CartridgeState;
        let mut state: #state_ty = Default::default();
        {
            let bytes = (&*state_ptr).cartridge_bytes();
            if !bytes.is_empty() && state.restore(bytes).is_err() {
                return ::flow_reload::RunStatus::Error;
            }
        }
        #call
        let mut bytes = Vec::new();
        if state.snapshot(&mut bytes).is_err() || !(&mut *state_ptr).write_cartridge(&bytes) {
            return ::flow_reload::RunStatus::Error;
        }
        ::flow_reload::RunStatus::Done
    };
    let exports = if is_async {
        crate::exports::asynchronous(body)
    } else {
        crate::exports::synchronous(body)
    };
    quote! { #input #exports }
}
