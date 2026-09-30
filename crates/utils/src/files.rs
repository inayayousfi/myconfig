use std::{fs, io, path::Path};

use typed_fs_rs::EmbeddedFile;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExistingFilePolicy {
    Refuse,
    Replace,
}

pub fn install_embedded_file(file: &EmbeddedFile, destination: &Path) -> io::Result<()> {
    install_embedded_file_with_policy(file, destination, ExistingFilePolicy::Refuse)
}

pub fn install_embedded_file_with_policy(
    file: &EmbeddedFile,
    destination: &Path,
    policy: ExistingFilePolicy,
) -> io::Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("destination is a symbolic link: {}", destination.display()),
            ));
        }
        Ok(metadata) if metadata.is_file() && fs::read(destination)? == file.content => {
            return Ok(());
        }
        Ok(_) if policy == ExistingFilePolicy::Refuse => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "destination needs a backup before replacement: {}",
                    destination.display()
                ),
            ));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "destination is not a regular file: {}",
                    destination.display()
                ),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(destination, file.content)?;
    #[cfg(unix)]
    if file.executable {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(destination)?.permissions();
        permissions.set_mode(permissions.mode() | 0o111);
        fs::set_permissions(destination, permissions)?;
    }
    if fs::read(destination)? != file.content {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("installed file differs from {}", file.path_from_root),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_to_the_explicit_destination_not_the_embedded_path() {
        let root = std::env::temp_dir().join(format!("myconfig-file-test-{}", std::process::id()));
        let destination = root.join("chosen/target.txt");
        let file = EmbeddedFile {
            content: b"installed\n",
            path_from_root: "original/source.txt",
            executable: false,
        };
        install_embedded_file(&file, &destination).unwrap();
        install_embedded_file(&file, &destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), file.content);
        assert!(!root.join(file.path_from_root).exists());
        fs::write(&destination, b"user content\n").unwrap();
        assert_eq!(
            install_embedded_file(&file, &destination)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(&destination).unwrap(), b"user content\n");
        install_embedded_file_with_policy(&file, &destination, ExistingFilePolicy::Replace)
            .unwrap();
        assert_eq!(fs::read(&destination).unwrap(), file.content);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_follow_an_existing_symlink() {
        let root = std::env::temp_dir().join(format!("myconfig-link-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("target"), b"keep\n").unwrap();
        std::os::unix::fs::symlink("target", root.join("link")).unwrap();
        let file = EmbeddedFile {
            content: b"replace\n",
            path_from_root: "link",
            executable: false,
        };
        assert_eq!(
            install_embedded_file(&file, &root.join("link"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(root.join("target")).unwrap(), b"keep\n");
        fs::remove_dir_all(root).unwrap();
    }
}
