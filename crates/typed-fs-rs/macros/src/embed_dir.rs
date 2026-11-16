use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug)]
enum Node {
    File { name: String, path: PathBuf },
    Directory { name: String, children: Vec<Node> },
}

pub(super) fn expand_static(
    visibility: syn::Visibility,
    name: syn::Ident,
    path: syn::LitStr,
) -> TokenStream {
    let module = format_ident!("__typed_fs_rs_embed_{}", name.to_string().to_lowercase());
    let (definitions, root) = match embedded_value(&path.value()) {
        Ok(embedded) => embedded,
        Err(message) => return syn::Error::new(path.span(), message).into_compile_error(),
    };
    quote! {
        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #module {
            #definitions
            pub static #name: EmbeddedDirectory0 = #root;
        }
        #visibility use #module::#name;
    }
}

fn embedded_value(path_from_manifest: &str) -> Result<(TokenStream, TokenStream), String> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .ok_or("CARGO_MANIFEST_DIR is not set; build with Cargo")?;
    let to_embed = PathBuf::from(manifest).join(path_from_manifest);
    if !to_embed.is_dir() {
        return Err(format!("{} is not a directory", to_embed.display()));
    }
    let file_tree = read_tree(&to_embed)?;

    let mut next_id = 0;
    let (definitions, _, root) = directory_value(file_tree, &mut next_id, Path::new(""))?;
    Ok((definitions, root))
}

fn file_name(path: &Path) -> Result<String, String> {
    path.file_name()
        .ok_or_else(|| format!("{} has no file name", path.display()))?
        .to_os_string()
        .into_string()
        .map_err(|name| format!("file name is not valid UTF-8: {name:?}"))
}

fn read_tree(path: &Path) -> Result<Node, String> {
    let unreadable = |error: std::io::Error| format!("cannot read {}: {error}", path.display());
    if path.is_file() {
        return Ok(Node::File {
            name: file_name(path)?,
            path: path.to_path_buf(),
        });
    }

    let mut paths = fs::read_dir(path)
        .map_err(unreadable)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(unreadable)?;
    paths.sort();

    let children = paths
        .iter()
        .map(|path| read_tree(path))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Node::Directory {
        name: file_name(path)?,
        children,
    })
}

fn field_ident(name: &str) -> Result<syn::Ident, String> {
    let normalized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let normalized = if normalized
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        format!("_{normalized}")
    } else {
        normalized
    };
    let normalized = if normalized.is_empty() {
        "_entry".to_owned()
    } else {
        normalized
    };
    syn::parse_str::<syn::Ident>(&normalized).or_else(|_| {
        let escaped = format!("{normalized}_");
        syn::parse_str::<syn::Ident>(&escaped)
            .map_err(|_| format!("entry name {name:?} cannot be used as a Rust field"))
    })
}

fn entry_name(node: &Node) -> &str {
    match node {
        Node::File { name, .. } | Node::Directory { name, .. } => name,
    }
}

/// Gives each entry its field, or fails when two entries would get the same one.
fn entry_fields(children: &[Node], relative_path: &Path) -> Result<Vec<syn::Ident>, String> {
    let mut owners = HashMap::new();
    let mut fields = Vec::new();
    for child in children {
        let name = entry_name(child);
        let field = field_ident(name)?;
        if let Some(other) = owners.insert(field.to_string(), name) {
            return Err(format!(
                "{:?} and {:?} would both become the field `{field}`; rename one of them",
                relative_path.join(other),
                relative_path.join(name),
            ));
        }
        fields.push(field);
    }
    Ok(fields)
}

fn directory_value(
    node: Node,
    next_id: &mut usize,
    relative_path: &Path,
) -> Result<(TokenStream, syn::Ident, TokenStream), String> {
    let Node::Directory { children, .. } = node else {
        return Err("the embedded root must be a directory".to_owned());
    };

    let struct_name = format_ident!("EmbeddedDirectory{}", *next_id);
    *next_id += 1;

    let mut definitions = TokenStream::new();
    let mut fields = Vec::new();
    let mut values = Vec::new();
    let mut file_accessors = Vec::new();
    let entry_fields = entry_fields(&children, relative_path)?;

    for (child, field) in children.into_iter().zip(entry_fields) {
        match child {
            Node::File { name, path } => {
                #[cfg(unix)]
                let executable = {
                    use std::os::unix::fs::PermissionsExt;
                    fs::metadata(&path)
                        .map_err(|error| format!("cannot read {}: {error}", path.display()))?
                        .permissions()
                        .mode()
                        & 0o111
                        != 0
                };
                #[cfg(not(unix))]
                let executable = false;
                let relative_file = relative_path
                    .join(&name)
                    .to_string_lossy()
                    .replace('\\', "/");
                let path = path.to_string_lossy();
                fields.push(quote! { pub #field: ::typed_fs_rs::EmbeddedFile });
                values.push(quote! {
                    #field: ::typed_fs_rs::EmbeddedFile {
                        content: ::core::include_bytes!(#path),
                        path_from_root: #relative_file,
                        executable: #executable,
                    }
                });
                file_accessors.push(quote! { files.push(&self.#field); });
            }
            Node::Directory {
                name: child_name,
                children: child_children,
            } => {
                let child_path = relative_path.join(&child_name);
                let (child_definitions, child_struct, child_value) = directory_value(
                    Node::Directory {
                        name: child_name,
                        children: child_children,
                    },
                    next_id,
                    &child_path,
                )?;
                definitions.extend(child_definitions);
                fields.push(quote! { pub #field: #child_struct });
                values.push(quote! { #field: #child_value });
                file_accessors.push(quote! {
                    files.extend(::typed_fs_rs::EmbeddedDirectory::files(&self.#field));
                });
            }
        }
    }

    definitions.extend(quote! {
        pub struct #struct_name {
            #(#fields,)*
        }
        impl ::typed_fs_rs::EmbeddedDirectory for #struct_name {
            fn files(&self) -> ::std::vec::Vec<&::typed_fs_rs::EmbeddedFile> {
                let mut files = ::std::vec::Vec::new();
                #(#file_accessors)*
                files
            }
        }
    });

    let value = quote! { #struct_name { #(#values,)* } };
    Ok((definitions, struct_name, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> Node {
        Node::File {
            name: name.to_owned(),
            path: PathBuf::from(name),
        }
    }

    #[test]
    fn two_entries_with_the_same_field_name_are_refused_by_name() {
        let tree = Node::Directory {
            name: "root".to_owned(),
            children: vec![Node::Directory {
                name: "themes".to_owned(),
                children: vec![file("size-bdiag"), file("size_bdiag")],
            }],
        };

        let error = directory_value(tree, &mut 0, Path::new("")).unwrap_err();

        assert_eq!(
            error,
            "\"themes/size-bdiag\" and \"themes/size_bdiag\" would both become the field \
             `size_bdiag`; rename one of them"
        );
    }
}
