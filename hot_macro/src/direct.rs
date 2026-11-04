use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemFn;

pub(crate) fn expand(mut input: ItemFn) -> TokenStream {
    let is_async = input.sig.asyncness.is_some();
    let inner = syn::Ident::new(
        &format!("_inner_{}", input.sig.ident),
        input.sig.ident.span(),
    );
    input.sig.ident = inner.clone();
    input.vis = syn::Visibility::Inherited;
    let exports = if is_async {
        crate::exports::asynchronous(quote! {
            #inner(&mut *state_ptr).await;
            ::flow_reload::RunStatus::Done
        })
    } else {
        crate::exports::synchronous(quote! {
            #inner(&mut *state_ptr);
            ::flow_reload::RunStatus::Done
        })
    };
    quote! { #input #exports }
}
