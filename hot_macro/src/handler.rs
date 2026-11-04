use proc_macro2::TokenStream;
use quote::quote;
use syn::{Expr, FnArg, ItemFn, Type};

pub(crate) struct Options {
    service: Expr,
    state: Option<Expr>,
}
impl syn::parse::Parse for Options {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let service = input.parse()?;
        let state = if input.is_empty() {
            None
        } else {
            input.parse::<syn::Token![,]>()?;
            let name: syn::Ident = input.parse()?;
            if name != "state" {
                return Err(syn::Error::new_spanned(name, "expected state = SCHEMA"));
            }
            input.parse::<syn::Token![=]>()?;
            Some(input.parse()?)
        };
        Ok(Self { service, state })
    }
}

pub(crate) fn expand(mut input: ItemFn, options: Options) -> syn::Result<TokenStream> {
    let service = options.service;
    let state = options.state.map(|schema| quote!(.with_state(#schema)));
    let valid_arg = match input.sig.inputs.first() {
        Some(FnArg::Typed(arg)) => matches!(arg.ty.as_ref(), Type::Path(path)
            if path.path.segments.last().is_some_and(|segment| segment.ident == "RequestContext")),
        _ => false,
    };
    if input.sig.asyncness.is_none()
        || input.sig.unsafety.is_some()
        || input.sig.inputs.len() != 1
        || !valid_arg
        || !input.sig.generics.params.is_empty()
        || input.sig.generics.where_clause.is_some()
        || input.sig.variadic.is_some()
    {
        return Err(syn::Error::new_spanned(&input.sig,
            "reload_handler requires async fn(RequestContext) -> Result<(), ServiceError> without generics or unsafe"));
    }
    let inner = syn::Ident::new(
        &format!("_inner_{}", input.sig.ident),
        input.sig.ident.span(),
    );
    input.sig.ident = inner.clone();
    input.vis = syn::Visibility::Inherited;
    Ok(quote! {
        #input

        unsafe extern "C" fn make_handler_request(
            raw: *const ::flow_reload::HandlerRequest,
        ) -> ::flow_reload::AttachedFuture {
            match ::flow_reload::RequestContext::from_raw(raw) {
                Ok(context) => ::flow_reload::internal::make_handler_future(
                    Box::pin(async move { #inner(context).await })
                ),
                Err(_) => ::flow_reload::AttachedFuture {
                    data: std::ptr::null_mut(), vtable: std::ptr::null(),
                },
            }
        }

        static HANDLER_API: ::flow_reload::HandlerApi =
            ::flow_reload::HandlerApi::new(#service, make_handler_request)#state;

        #[no_mangle]
        pub extern "C" fn hot_handler_api() -> *const ::flow_reload::HandlerApi {
            &HANDLER_API
        }
    })
}
