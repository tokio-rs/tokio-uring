use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

/// Marks async function to be executed by the tokio-uring runtime.
#[cfg(not(test))] // Work around for rust-lang/rust#62127
#[proc_macro_attribute]
pub fn main(args: TokenStream, item: TokenStream) -> TokenStream {
    expand_main(args, item.clone()).unwrap_or_else(|err| token_stream_with_error(item, err))
}

#[allow(dead_code)]
fn expand_main(args: TokenStream, item: TokenStream) -> Result<TokenStream, syn::Error> {
    let input: syn::ItemFn = syn::parse(item)?;

    if !args.is_empty() {
        let msg = "Unknown attribute inside the macro";
        return Err(syn::Error::new_spanned(TokenStream2::from(args), msg));
    }
    if input.sig.ident == "main" && !input.sig.inputs.is_empty() {
        let msg = "The main function cannot accept arguments";
        return Err(syn::Error::new_spanned(&input.sig.ident, msg));
    }

    expand(input, false)
}

/// Marks async function to be executed by tokio-uring runtime,
/// suitable to test environment.
///
/// ## Usage
///
/// ```no_run
/// #[tokio_uring::test]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
#[proc_macro_attribute]
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    expand_test(args, item.clone()).unwrap_or_else(|err| token_stream_with_error(item, err))
}

fn expand_test(args: TokenStream, item: TokenStream) -> Result<TokenStream, syn::Error> {
    let input: syn::ItemFn = syn::parse(item)?;

    if !args.is_empty() {
        let msg = "Unknown attribute inside the macro";
        return Err(syn::Error::new_spanned(TokenStream2::from(args), msg));
    }
    if let Some(attr) = input.attrs.iter().find(|attr| attr.path.is_ident("test")) {
        let msg = "Second test attribute is supplied";
        return Err(syn::Error::new_spanned(attr, msg));
    }

    expand(input, true)
}

fn expand(mut input: syn::ItemFn, is_test: bool) -> Result<TokenStream, syn::Error> {
    if input.sig.asyncness.is_none() {
        let msg = "The `async` keyword is missing from the function declaration";
        return Err(syn::Error::new_spanned(input.sig.fn_token, msg));
    }

    input.sig.asyncness = None;

    let body = &input.block;
    let brace_token = input.block.brace_token;
    let body = quote! {
        {
            let body = async #body;
            tokio_uring::start(body)
        }
    };
    input.block = syn::parse2(body).expect("Parsing failure");
    input.block.brace_token = brace_token;

    let header = is_test.then(|| {
        quote! {
            #[::core::prelude::v1::test]
        }
    });

    let result = quote! {
        #header
        #input
    };

    Ok(result.into())
}

/// Append the error to an existing TokenStream.
///
/// If any of the macros fail, we still want to expand to an item that is as close
/// to the expected output as possible. This helps out IDEs such that completions and other
/// related features keep working.
fn token_stream_with_error(mut tokens: TokenStream, error: syn::Error) -> TokenStream {
    tokens.extend(TokenStream::from(error.into_compile_error()));
    tokens
}
