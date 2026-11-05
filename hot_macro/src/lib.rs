//! Attribute macro for the message handler ABI.
use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemFn};

mod handler;

#[proc_macro_attribute]
pub fn reload_handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let options = parse_macro_input!(attr as handler::Options);
    let input = parse_macro_input!(item as ItemFn);
    match handler::expand(input, options) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
