#[proc_macro_attribute]
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    expand_test(args, item.clone()).unwrap_or_else(|err| token_stream_with_error(item, err))
}

fn expand_test(args: TokenStream, item: TokenStream) -> Result<TokenStream, syn::Error> {
    let mut input: syn::ItemFn = syn::parse(item)?;
    if let Some(attr) = input.attrs.iter().find(|attr| attr.path.is_ident("test")) {
        let msg = "second test attribute is supplied";
        return Err(syn::Error::new_spanned(attr, msg));
    }
    if !args.is_empty() {
        let msg = "Unknown attribute inside the macro";
        return Err(syn::Error::new_spanned(args, msg));
    }
    if input.sig.asyncness.is_none() {
        let msg = "the `async` keyword is missing from the function declaration";
        return Err(syn::Error::new_spanned(input.sig.fn_token, msg));
    }

    let body = &input.block;
    let brace_token = input.block.brace_token;
    let body = quote! {
        let body = async #body;
        ::tokio_uring::start(body)
    };
    input.block = syn::parse2(body).expect("Parsing failure");
    input.block.brace_token = brace_token;

    let result = quote! {
        #[::core::prelude::v1::test]
        #input
    };

    Ok(result.into())
}

fn token_stream_with_error(mut tokens: TokenStream, error: syn::Error) -> TokenStream {
    tokens.extend(TokenStream::from(error.into_compile_error()));
    tokens
}
