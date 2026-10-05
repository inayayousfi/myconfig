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

/// Embeds a directory tree as a typed value or a global static.
///
/// Pass a string literal containing the directory path to produce a value, or
/// use `static NAME = "path"` to declare a global static. The path is resolved
/// relative to the compiler process's current working directory. Files become
/// [`::typed_fs_rs::EmbeddedFile`] values, and subdirectories become nested
/// structures with public fields named from their entries. Non-alphanumeric
/// Entry names normally use underscores for punctuation in generated Rust fields.
/// If two entries would get the same field name, the colliding names instead
/// spell out punctuation, such as `_dash_`, `_dot_`, or `_tilde_`. Other
/// characters use their Unicode code point. A numeric suffix resolves any
/// remaining collision. Embedded paths always keep the original entry names.
///
/// Each embedded file contains its bytes and its path relative to the embedded
/// root, using `/` separators. The returned directory structures implement
/// [`::typed_fs_rs::EmbeddedDirectory`].
///
/// # Example
///
/// ```ignore
/// let assets = typed_fs_rs::embed_dir!("assets");
/// let bytes: &[u8] = assets.logo_png.content;
/// typed_fs_rs::embed_dir!(pub static ASSETS = "assets");
/// ```
///
/// The generated field for `logo.png` is `logo_png`.
///
/// # Panics
///
/// Compilation fails if the path cannot be read, an entry name is not valid
/// UTF-8.
#[proc_macro]
pub fn embed_dir(input: TokenStream) -> TokenStream {
    let input = proc_macro2::TokenStream::from(input);
    if let Ok(input) = syn::parse2::<StaticInput>(input.clone()) {
        embed_dir::expand_static(input.visibility, input.name, input.path.value()).into()
    } else {
        match syn::parse2::<syn::LitStr>(input) {
            Ok(path) => embed_dir::expand(path.value()).into(),
            Err(error) => error.into_compile_error().into(),
        }
    }
}
