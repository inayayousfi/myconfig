//! Where the xtask keeps its state and its downloads on the host.

use std::env;
use std::path::{Path, PathBuf};

use crate::os::Os;

/// Overrides the state folder, which holds images, throwaway disks, the SSH key and the lock.
pub const STATE_VARIABLE: &str = "MYCONFIG_VM_STATE_DIR";
/// Overrides the download folder, which holds ISOs and cloud images.
pub const CACHE_VARIABLE: &str = "MYCONFIG_VM_CACHE_DIR";

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("HOME is not set, and neither {0} nor its XDG folder is")]
    NoHome(&'static str),
}

/// The host folders of the xtask.
#[derive(Clone, Debug)]
pub struct Paths {
    state: PathBuf,
    cache: PathBuf,
}

impl Paths {
    /// Reads the folders from the environment: the override variable first,
    /// then `XDG_DATA_HOME` or `XDG_CACHE_HOME`, then their default under `HOME`.
    pub fn from_env() -> Result<Paths, PathsError> {
        // An empty variable counts as unset, as `${VARIABLE:-default}` does in a shell.
        let read = |name: &str| env::var_os(name).filter(|value| !value.is_empty());
        let home = read("HOME").map(PathBuf::from);
        let pick = |variable: &'static str, xdg: &str, default: &str| {
            read(variable)
                .map(PathBuf::from)
                .or_else(|| read(xdg).map(|base| PathBuf::from(base).join("myconfig/vm")))
                .or_else(|| home.as_ref().map(|home| home.join(default)))
                .ok_or(PathsError::NoHome(variable))
        };
        Ok(Paths {
            state: pick(STATE_VARIABLE, "XDG_DATA_HOME", ".local/share/myconfig/vm")?,
            cache: pick(CACHE_VARIABLE, "XDG_CACHE_HOME", ".cache/myconfig/vm")?,
        })
    }

    pub fn cache(&self) -> &Path {
        &self.cache
    }

    /// The lock file that allows one VM at a time.
    pub fn lock(&self) -> PathBuf {
        self.state.join("vm.lock")
    }

    /// The SSH private key that every image accepts. The public key is next to it with `.pub`.
    pub fn ssh_key(&self) -> PathBuf {
        self.state.join("ssh/id_ed25519")
    }

    /// The folder for the sockets and the log of the running VM.
    pub fn runtime(&self) -> PathBuf {
        self.state.join("run")
    }

    /// The clean image that every throwaway disk starts from.
    pub fn image(&self, os: Os) -> Layer {
        Layer::new(self.state.join(os.name()).join("image"))
    }

    /// The folder of an image build in progress: the new image layer, the seed disc and the
    /// install logs. A failed build leaves it for inspection until the next build of that OS.
    pub fn image_build(&self, os: Os) -> PathBuf {
        self.state.join(os.name()).join("image-build")
    }

    /// The throwaway disk of the current run or cycle, layered over the clean image.
    pub fn work(&self, os: Os) -> Layer {
        Layer::new(self.state.join(os.name()).join("work"))
    }

    /// The throwaway disk of the last failed cycle, kept until the next cycle of that OS.
    pub fn failed(&self, os: Os) -> Layer {
        Layer::new(self.state.join(os.name()).join("failed"))
    }
}

/// One folder that holds everything a guest boots from: its disk, its UEFI variables,
/// and its TPM state when the guest has a TPM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    dir: PathBuf,
}

impl Layer {
    pub fn new(dir: PathBuf) -> Layer {
        Layer { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn disk(&self) -> PathBuf {
        self.dir.join("disk.qcow2")
    }

    pub fn vars(&self) -> PathBuf {
        self.dir.join("vars.fd")
    }

    pub fn tpm(&self) -> PathBuf {
        self.dir.join("tpm")
    }

    /// The release the clean image was built from. The image builder writes it last,
    /// so its presence also marks a finished image.
    pub fn release(&self) -> PathBuf {
        self.dir.join("release")
    }
}
