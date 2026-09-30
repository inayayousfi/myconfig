use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use xshell::{Shell, cmd};

pub fn find_program(sh: &Shell, name: &str) -> io::Result<PathBuf> {
    if name.is_empty() || Path::new(name).components().count() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a program name",
        ));
    }
    let path = sh
        .var_os("PATH")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "PATH is unset"))?;
    for directory in env::split_paths(&path) {
        let candidate = directory.join(name);
        #[cfg(windows)]
        let candidates: Vec<_> = std::iter::once(candidate.clone())
            .chain(
                sh.var_os("PATHEXT")
                    .unwrap_or_default()
                    .to_string_lossy()
                    .split(';')
                    .filter(|extension| !extension.is_empty())
                    .map(|extension| directory.join(format!("{name}{extension}"))),
            )
            .collect();
        #[cfg(not(windows))]
        let candidates = [candidate];
        for candidate in candidates {
            let Ok(metadata) = fs::metadata(&candidate) else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o111 == 0 {
                    continue;
                }
            }
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("program not found: {name}"),
    ))
}

pub fn emacs_home(sh: &Shell, emacs: &Path) -> io::Result<PathBuf> {
    let expression = r#"(princ (expand-file-name "~/"))"#;
    let output = cmd!(sh, "{emacs} --batch -Q --eval {expression}")
        .quiet()
        .read()
        .map_err(io::Error::other)?;
    let home = output.trim();
    if home.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Emacs did not return its home directory",
        ));
    }
    Ok(PathBuf::from(home))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn finds_an_executable_and_reads_the_path_reported_by_emacs() {
        let root = env::temp_dir().join(format!("myconfig-emacs-path-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let emacs = root.join("emacs");
        fs::write(&emacs, "#!/bin/sh\nprintf '/emacs/home\\n'\n").unwrap();
        fs::set_permissions(&emacs, fs::Permissions::from_mode(0o755)).unwrap();
        let sh = Shell::new().unwrap();
        sh.set_var("PATH", &root);
        assert_eq!(find_program(&sh, "emacs").unwrap(), emacs);
        assert_eq!(emacs_home(&sh, &emacs).unwrap(), Path::new("/emacs/home"));
        fs::remove_dir_all(root).unwrap();
    }
}
