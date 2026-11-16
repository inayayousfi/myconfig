pub use typed_fs_rs_macros::embed_dir;

pub struct EmbeddedFile {
    pub content: &'static [u8],
    pub path_from_root: &'static str,
    pub executable: bool,
}

pub trait EmbeddedDirectory {
    fn files(&self) -> Vec<&EmbeddedFile>;
}
