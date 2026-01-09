use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input, spanned::Spanned};

#[proc_macro_derive(DimensifyComponent, attributes(dimensify))]
/// Derive a protocol component mapping.
///
/// Requires `#[dimensify(command = \"...\")]` on the type and supports
/// `#[dimensify(into)]` on fields that should call `Into`.
pub fn derive_dimensify_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let command_ident = match find_command_ident(&input) {
        Ok(ident) => ident,
        Err(err) => return err,
    };

    let fields = match extract_named_fields(&input) {
        Ok(fields) => fields,
        Err(err) => return err,
    };

    let field_inits = fields.iter().map(|field| {
        let ident = field.ident.as_ref().expect("named field");
        if field_has_into(field) {
            quote! { #ident: self.#ident.clone().into() }
        } else {
            quote! { #ident: self.#ident.clone() }
        }
    });

    TokenStream::from(quote! {
        impl ::dimensify_protocol::DimensifyComponent for #name {
            fn to_component(&self) -> ::dimensify_protocol::Component {
                ::dimensify_protocol::Component::#command_ident {
                    #(#field_inits, )*
                }
            }
        }
    })
}

fn find_command_ident(input: &DeriveInput) -> Result<syn::Ident, TokenStream> {
    for attr in &input.attrs {
        if !attr.path().is_ident("dimensify") {
            continue;
        }
        let mut command = None;
        let result = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("command") {
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                command = Some(syn::Ident::new(&lit.value(), lit.span()));
            }
            Ok(())
        });
        if let Err(err) = result {
            return Err(err.to_compile_error().into());
        }
        if let Some(command) = command {
            return Ok(command);
        }
    }

    Err(syn::Error::new(
        input.span(),
        "missing #[dimensify(command = \"...\")] attribute",
    )
    .to_compile_error()
    .into())
}

fn extract_named_fields(input: &DeriveInput) -> Result<Vec<syn::Field>, TokenStream> {
    match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(fields) => Ok(fields.named.iter().cloned().collect()),
            _ => Err(syn::Error::new(
                input.span(),
                "DimensifyComponent only supports structs with named fields",
            )
            .to_compile_error()
            .into()),
        },
        _ => Err(
            syn::Error::new(input.span(), "DimensifyComponent only supports structs")
                .to_compile_error()
                .into(),
        ),
    }
}

fn field_has_into(field: &syn::Field) -> bool {
    for attr in &field.attrs {
        if !attr.path().is_ident("dimensify") {
            continue;
        }
        let mut has_into = false;
        let result = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("into") {
                has_into = true;
            }
            Ok(())
        });
        if result.is_ok() && has_into {
            return true;
        }
    }
    false
}
