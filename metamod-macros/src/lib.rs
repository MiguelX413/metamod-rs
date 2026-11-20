use proc_macro::TokenStream;
use quote::quote;
use std::ffi::CString;
use syn::{ImplItem, ItemImpl, LitCStr, LitStr, parse_macro_input};

/// Register a concrete `impl Plugin` and supply its static metadata.
///
/// Requires `name`, `version`, and `author`. Optional fields are `date`,
/// `url`, and `log_tag`. Use once per plugin library.
#[proc_macro_attribute]
pub fn metamod_plugin(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut implementation = parse_macro_input!(input as ItemImpl);
    match register(args, &mut implementation) {
        Ok(item) => item,
        Err(error) => error.into_compile_error().into(),
    }
}

fn register(args: TokenStream, implementation: &mut ItemImpl) -> syn::Result<TokenStream> {
    if implementation.trait_.is_none() || !implementation.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &implementation.self_ty,
            "expected a concrete impl Plugin",
        ));
    }
    if let Some(item) = implementation
        .items
        .iter()
        .find(|item| matches!(item, ImplItem::Const(item) if item.ident == "METADATA"))
    {
        return Err(syn::Error::new_spanned(
            item,
            "metamod_plugin supplies METADATA",
        ));
    }

    let mut name = None;
    let mut version = None;
    let mut author = None;
    let mut date = None;
    let mut url = None;
    let mut log_tag = None;
    let parser = syn::meta::parser(|meta| {
        let field = match meta.path.get_ident().map(ToString::to_string).as_deref() {
            Some("name") => &mut name,
            Some("version") => &mut version,
            Some("author") => &mut author,
            Some("date") => &mut date,
            Some("url") => &mut url,
            Some("log_tag") => &mut log_tag,
            _ => return Err(meta.error("unknown plugin metadata field")),
        };
        if field.is_some() {
            return Err(meta.error("duplicate plugin metadata field"));
        }
        let value: LitStr = meta.value()?.parse()?;
        let c_string = CString::new(value.value())
            .map_err(|_| syn::Error::new(value.span(), "metadata cannot contain NUL"))?;
        *field = Some(LitCStr::new(&c_string, value.span()));
        Ok(())
    });
    syn::parse::Parser::parse(parser, args)?;

    let span = implementation.impl_token.span;
    let required = |value: Option<LitCStr>, field| {
        value.ok_or_else(|| syn::Error::new(span, format!("missing plugin metadata field {field}")))
    };
    let name = required(name, "name")?;
    let version = required(version, "version")?;
    let author = required(author, "author")?;
    let date = date.unwrap_or_else(|| LitCStr::new(c"", span));
    let url = url.unwrap_or_else(|| LitCStr::new(c"", span));
    let log_tag = log_tag.unwrap_or_else(|| name.clone());
    implementation.items.push(syn::parse_quote! {
        const METADATA: ::metamod::Metadata = ::metamod::Metadata {
            name: #name,
            version: #version,
            author: #author,
            date: #date,
            url: #url,
            log_tag: #log_tag,
        };
    });
    let plugin = &implementation.self_ty;
    Ok(quote! {
        #implementation
        ::metamod::export_plugin!(#plugin);
    }
    .into())
}
