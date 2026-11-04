use syn::{FnArg, ItemFn, Pat, ReturnType, Type};

pub(crate) fn validate_signature(input: &ItemFn) -> Result<(), syn::Error> {
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

pub(crate) fn validate_reload_signature(input: &ItemFn) -> Result<Type, syn::Error> {
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

pub(crate) fn validate_init_signature(input: &ItemFn) -> Result<(), syn::Error> {
    if !input.sig.generics.params.is_empty() || input.sig.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &input.sig.generics,
            "#[reload_init] does not support generics",
        ));
    }
    if input.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            input.sig.asyncness,
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

pub(crate) fn extract_mut_ref_arg(input: &ItemFn) -> Result<Type, syn::Error> {
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
