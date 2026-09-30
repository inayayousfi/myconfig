use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
enum Node {
    File { name: String, path: PathBuf },
    Directory { name: String, children: Vec<Node> },
}

pub(super) fn expand(path_from_cwd: String) -> TokenStream {
    let (definitions, _, root) = embedded_value(path_from_cwd, false);
    quote! {{
        #definitions
        #root
    }}
}

pub(super) fn expand_static(
    visibility: syn::Visibility,
    name: syn::Ident,
    path_from_cwd: String,
) -> TokenStream {
    let module = format_ident!("__typed_fs_rs_embed_{}", name.to_string().to_lowercase());
    let (definitions, _, root) = embedded_value(path_from_cwd, true);
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

fn embedded_value(
    path_from_cwd: String,
    public_types: bool,
) -> (TokenStream, syn::Ident, TokenStream) {
    let to_embed = std::env::current_dir()
        .expect("Couldn't get the current working directory")
        .join(PathBuf::from(path_from_cwd));
    let file_tree = read_tree(&to_embed).expect("couldn't parse the final tree");

    let mut next_id = 0;
    directory_value(file_tree, &mut next_id, Path::new(""), public_types)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .expect("path has no file name")
        .to_os_string()
        .into_string()
        .unwrap_or_else(|name| panic!("file name is not valid UTF-8: {name:?}"))
}

fn read_tree(path: &Path) -> io::Result<Node> {
    if path.is_file() {
        return Ok(Node::File {
            name: file_name(path),
            path: path.to_path_buf(),
        });
    }

    let mut paths = fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.sort();

    let children = paths
        .iter()
        .map(|path| read_tree(path))
        .collect::<io::Result<Vec<_>>>()?;

    Ok(Node::Directory {
        name: file_name(path),
        children,
    })
}

fn field_ident(name: &str) -> syn::Ident {
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
    syn::parse_str::<syn::Ident>(&normalized).unwrap_or_else(|_| {
        let escaped = format!("{normalized}_");
        syn::parse_str::<syn::Ident>(&escaped)
            .unwrap_or_else(|_| panic!("entry name {name:?} cannot be used as a Rust field"))
    })
}

fn character_name(character: char) -> &'static str {
    match character {
        ' ' => "space",
        '!' => "bang",
        '"' => "quote",
        '#' => "hash",
        '$' => "dollar",
        '%' => "percent",
        '&' => "ampersand",
        '\'' => "apostrophe",
        '(' => "left_paren",
        ')' => "right_paren",
        '*' => "star",
        '+' => "plus",
        ',' => "comma",
        '-' => "dash",
        '.' => "dot",
        '/' => "slash",
        ':' => "colon",
        ';' => "semicolon",
        '<' => "less",
        '=' => "equals",
        '>' => "greater",
        '?' => "question",
        '@' => "at",
        '[' => "left_bracket",
        '\\' => "backslash",
        ']' => "right_bracket",
        '^' => "caret",
        '`' => "backtick",
        '{' => "left_brace",
        '|' => "pipe",
        '}' => "right_brace",
        '~' => "tilde",
        _ => "",
    }
}

fn expanded_field_ident(name: &str) -> syn::Ident {
    let mut expanded = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            expanded.push(character);
        } else {
            let name = character_name(character);
            if name.is_empty() {
                expanded.push_str(&format!("_unicode_{:x}_", character as u32));
            } else {
                expanded.push('_');
                expanded.push_str(name);
                expanded.push('_');
            }
        }
    }
    field_ident(&expanded)
}

fn entry_name(node: &Node) -> &str {
    match node {
        Node::File { name, .. } | Node::Directory { name, .. } => name,
    }
}

fn directory_value(
    node: Node,
    next_id: &mut usize,
    relative_path: &Path,
    public_types: bool,
) -> (TokenStream, syn::Ident, TokenStream) {
    let Node::Directory { children, .. } = node else {
        panic!("The root should be a directory")
    };

    let struct_name = format_ident!("EmbeddedDirectory{}", *next_id);
    *next_id += 1;

    let mut definitions = TokenStream::new();
    let mut fields = Vec::new();
    let mut values = Vec::new();
    let mut file_accessors = Vec::new();
    let mut used_fields = HashSet::new();
    let mut base_counts = HashMap::new();
    for child in &children {
        *base_counts
            .entry(field_ident(entry_name(child)).to_string())
            .or_insert(0usize) += 1;
    }
    let reserved_fields: HashSet<_> = base_counts.keys().cloned().collect();

    for child in children {
        let name = entry_name(&child);
        let base = field_ident(name);
        let base_name = base.to_string();
        let mut field = if base_counts[&base_name] > 1 && name != base_name {
            expanded_field_ident(name)
        } else {
            base
        };
        let candidate = field.to_string();
        if used_fields.contains(&candidate)
            || (candidate != base_name && reserved_fields.contains(&candidate))
        {
            let mut suffix = 2;
            loop {
                let suffixed = field_ident(&format!("{candidate}_{suffix}"));
                let name = suffixed.to_string();
                if !used_fields.contains(&name) && !reserved_fields.contains(&name) {
                    field = suffixed;
                    break;
                }
                suffix += 1;
            }
        }
        used_fields.insert(field.to_string());
        match child {
            Node::File { name, path } => {
                #[cfg(unix)]
                let executable = {
                    use std::os::unix::fs::PermissionsExt;
                    fs::metadata(&path)
                        .expect("couldn't read embedded file permissions")
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
                    public_types,
                );
                definitions.extend(child_definitions);
                fields.push(quote! { pub #field: #child_struct });
                values.push(quote! { #field: #child_value });
                file_accessors.push(quote! {
                    files.extend(::typed_fs_rs::EmbeddedDirectory::files(&self.#field));
                });
            }
        }
    }

    let struct_visibility = if public_types {
        quote! { pub }
    } else {
        TokenStream::new()
    };
    definitions.extend(quote! {
        #struct_visibility struct #struct_name {
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
    (definitions, struct_name, value)
}
