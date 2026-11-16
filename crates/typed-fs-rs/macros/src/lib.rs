//! Procedural macros for `typed-fs-rs`.

pub(crate) mod embed_dir;

use proc_macro::TokenStream;
use syn::{
    Token,
    parse::{Parse, ParseStream},
};

struct StaticInput {
    visibility: syn::Visibility,
    name: syn::Ident,
    path: syn::LitStr,
}

impl Parse for StaticInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let visibility = input.parse()?;
        input.parse::<Token![static]>()?;
        let name = input.parse()?;
        input.parse::<Token![=]>()?;
        let path = input.parse()?;
        Ok(Self {
            visibility,
            name,
            path,
        })
    }
}

/// Embeds a directory tree as a global static: `static NAME = "path"`.
///
/// The path is resolved relative to the folder of the calling crate's `Cargo.toml`.
/// Files become [`::typed_fs_rs::EmbeddedFile`] values, and subdirectories become nested
/// structures whose public fields are named after their entries. A field name replaces
/// each character that a Rust identifier cannot hold with an underscore. Embedded paths
/// keep the original entry names.
///
/// Each embedded file holds its bytes and its path relative to the embedded root, with
/// `/` separators. The directory structures implement
/// [`::typed_fs_rs::EmbeddedDirectory`].
///
/// # Example
///
/// ```ignore
/// typed_fs_rs::embed_dir!(pub static ASSETS = "assets");
/// let bytes: &[u8] = ASSETS.logo_png.content;
/// ```
///
/// # Errors
///
/// Compilation fails if the path is not a readable directory, if an entry name is not
/// valid UTF-8 or cannot become a Rust field, or if two entries of one directory become
/// the same field.
///
/// Cargo does not see which folders the macro lists, so a file added to the tree is not
/// embedded until the calling crate rebuilds. The calling crate needs a build script that
/// prints `cargo:rerun-if-changed` for the embedded folder.
#[proc_macro]
pub fn embed_dir(input: TokenStream) -> TokenStream {
    match syn::parse::<StaticInput>(input) {
        Ok(input) => embed_dir::expand_static(input.visibility, input.name, input.path).into(),
        Err(error) => error.into_compile_error().into(),
    }
}
