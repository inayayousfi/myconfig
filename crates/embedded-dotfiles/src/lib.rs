use typed_fs_rs::{EmbeddedDirectory, embed_dir};

// The macro currently resolves paths from Cargo's working directory.
// This workspace builds from its root, like the original root binary did.
embed_dir!(pub static DOTFILES = "dotfiles");

pub fn embedded_file_count() -> usize {
    DOTFILES.files().len()
}
