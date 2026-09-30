use std::fs;
use std::io;
use std::path::Path;
pub use typed_fs_rs_macros::embed_dir;

pub struct EmbeddedFile {
    pub content: &'static [u8],
    pub path_from_root: &'static str,
    pub executable: bool,
}

impl EmbeddedFile {
    pub fn write(&self, root: &Path) -> io::Result<()> {
        let path = root.join(self.path_from_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.content)
    }

    pub fn verify(&self, root: &Path) -> io::Result<()> {
        let content = fs::read(root.join(self.path_from_root))?;
        if content == self.content {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("content mismatch for {}", self.path_from_root),
            ))
        }
    }
}

pub trait EmbeddedDirectory {
    fn files(&self) -> Vec<&EmbeddedFile>;
}
