//! Derives program names for q_proc's typed program registration.

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{DeriveInput, LitStr, parse_macro_input};

/// Implements `q_proc::prelude::ProgramLabel` using `#[program_label("name")]`.
///
/// Exactly one attribute containing a nonempty, whitespace-free string literal
/// is required. Other derives and generic bounds are left to the caller.
#[proc_macro_derive(ProgramLabel, attributes(program_label))]
pub fn derive_program_label(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Generates the name implementation through the caller's q_proc dependency.
fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = program_name(input)?;
    let path = match crate_name("q_proc")
        .map_err(|error| syn::Error::new(input.ident.span(), error.to_string()))?
    {
        FoundCrate::Itself => quote!(::q_proc),
        FoundCrate::Name(name) => {
            let name = format_ident!("{name}");
            quote!(::#name)
        }
    };
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #path::prelude::ProgramLabel for #ident #ty_generics #where_clause {
            fn name(&self) -> #path::prelude::ProgramName {
                #path::prelude::ProgramName::new(#name)
                    .expect("program name must be nonempty and contain no whitespace")
            }
        }
    })
}

/// Validates the explicit command name with the same rules as `ProgramName`.
fn program_name(input: &DeriveInput) -> syn::Result<LitStr> {
    let mut labels = input
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("program_label"));
    let label = labels.next().ok_or_else(|| {
        syn::Error::new(input.ident.span(), "expected #[program_label(\"name\")]")
    })?;
    if let Some(duplicate) = labels.next() {
        return Err(syn::Error::new_spanned(
            duplicate,
            "expected exactly one #[program_label(\"name\")] attribute",
        ));
    }
    let name = label.parse_args::<LitStr>()?;
    let value = name.value();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return Err(syn::Error::new(
            name.span(),
            "program name must be nonempty and contain no whitespace",
        ));
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_command_names_without_changing_them() {
        for name in ["sleep", "my-program", "q", "休眠"] {
            let input: DeriveInput =
                syn::parse_str(&format!("#[program_label({name:?})] struct Program;")).unwrap();
            assert_eq!(program_name(&input).unwrap().value(), name);
        }
    }

    #[test]
    fn rejects_missing_and_duplicate_names() {
        for source in [
            "struct Program;",
            "#[program_label(\"q\")] #[program_label(\"q\")] struct Program;",
        ] {
            let input = syn::parse_str(source).unwrap();
            assert!(program_name(&input).is_err());
        }
    }

    #[test]
    fn rejects_empty_names_and_ascii_or_unicode_whitespace() {
        for literal in [
            "\"\"",
            "\"two words\"",
            "\"a\\tb\"",
            "\"a\\nb\"",
            "\"a\\u{a0}b\"",
        ] {
            let input =
                syn::parse_str(&format!("#[program_label({literal})] struct Program;")).unwrap();
            let error = program_name(&input).unwrap_err();
            assert_eq!(
                error.to_string(),
                "program name must be nonempty and contain no whitespace"
            );
        }
    }

    #[test]
    fn rejects_attributes_that_are_not_one_string_literal() {
        for attribute in [
            "#[program_label]",
            "#[program_label = \"q\"]",
            "#[program_label()]",
            "#[program_label(42)]",
            "#[program_label(NAME)]",
            "#[program_label(\"q\", \"extra\")]",
        ] {
            let input = syn::parse_str(&format!("{attribute} struct Program;")).unwrap();
            assert!(program_name(&input).is_err());
        }
    }
}
