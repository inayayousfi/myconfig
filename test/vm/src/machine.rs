//! Describes the virtual hardware of each guest and turns it into QEMU arguments.
//! It decides only; `vm` starts the processes.

use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use crate::firmware::Firmware;
use crate::os::{Boot, Os};
use crate::paths::Layer;

/// The host port that QEMU forwards to the guest's SSH port.
pub const SSH_PORT: u16 = 2222;
const CPUS: u32 = 4;

/// The unix sockets of the running VM, all inside one runtime folder.
#[derive(Clone, Debug)]
pub struct Sockets {
    pub qmp: PathBuf,
    pub spice: PathBuf,
    pub tpm: PathBuf,
}

impl Sockets {
    pub fn in_dir(dir: &Path) -> Sockets {
        Sockets {
            qmp: dir.join("qmp.sock"),
            spice: dir.join("spice.sock"),
            tpm: dir.join("swtpm.sock"),
        }
    }
}

/// Read-only disc images attached to the guest, such as an installer ISO or a seed ISO.
#[derive(Clone, Debug, Default)]
pub struct Media {
    /// The disc the firmware starts first, before the guest disk.
    pub boot: Option<PathBuf>,
    /// Discs that are attached without being started.
    pub attached: Vec<PathBuf>,
}

/// Everything that decides the QEMU command line of one VM.
#[derive(Clone, Debug)]
pub struct Machine {
    pub os: Os,
    pub layer: Layer,
    pub sockets: Sockets,
    pub media: Media,
    /// 1, or 2 for a CachyOS guest tested with a second display.
    pub displays: u8,
    /// The host file that receives the guest's first serial port, which image installers
    /// use to report their progress.
    pub serial: Option<PathBuf>,
}

impl Machine {
    pub fn qemu_args(&self) -> Vec<OsString> {
        let mut args = Args::default();
        let secure = self.os.boot() == Boot::SecureUefiWithTpm;
        let windows = self.os == Os::Windows;

        // -nodefaults leaves out QEMU's own display, network card and empty CD drive, so the
        // guest has exactly the devices below.
        args.push(["-nodefaults", "-name", &format!("myconfig-{}", self.os)]);
        if secure {
            args.push(["-machine", "q35,accel=kvm,smm=on"]);
            args.push(["-global", "driver=cfi.pflash01,property=secure,value=on"]);
        } else {
            args.push(["-machine", "q35,accel=kvm"]);
        }
        // The host CPU model carries the AMD-V or VT-x flag, which WSL in Windows needs.
        args.push(["-cpu", "host", "-smp", &CPUS.to_string()]);
        args.push(["-m", &self.os.memory_mib().to_string()]);
        args.push_path(
            "-drive",
            "if=pflash,format=raw,unit=0,readonly=on,file=",
            &Firmware::for_boot(self.os.boot()).code,
        );
        args.push_path(
            "-drive",
            "if=pflash,format=raw,unit=1,file=",
            &self.layer.vars(),
        );

        // Disks use the q35 built-in SATA controller (buses ide.0 to ide.5) when the guest has
        // no virtio driver, which is the case for Windows.
        let disk_boot_index = if self.media.boot.is_some() { 1 } else { 0 };
        args.push_path(
            "-drive",
            "if=none,id=disk,format=qcow2,cache=writeback,discard=unmap,file=",
            &self.layer.disk(),
        );
        if windows {
            args.push([
                "-device",
                &format!("ide-hd,drive=disk,bus=ide.0,bootindex={disk_boot_index}"),
            ]);
        } else {
            args.push([
                "-device",
                &format!("virtio-blk-pci,drive=disk,bootindex={disk_boot_index}"),
            ]);
        }
        let discs = self.media.boot.iter().map(|path| (path, true));
        let discs = discs.chain(self.media.attached.iter().map(|path| (path, false)));
        for (index, (path, boot)) in discs.enumerate() {
            let id = format!("media{index}");
            args.push_path(
                "-drive",
                &format!("if=none,id={id},media=cdrom,readonly=on,format=raw,file="),
                path,
            );
            let boot_index = if boot { ",bootindex=0" } else { "" };
            args.push([
                "-device",
                &format!("ide-cd,drive={id},bus=ide.{}{boot_index}", index + 1),
            ]);
        }

        let network = if windows { "e1000e" } else { "virtio-net-pci" };
        args.push([
            "-netdev",
            &format!("user,id=network,hostfwd=tcp:127.0.0.1:{SSH_PORT}-:22"),
            "-device",
            &format!("{network},netdev=network"),
        ]);

        args.push(["-display", "none"]);
        args.push_path(
            "-spice",
            "unix=on,disable-ticketing=on,addr=",
            &self.sockets.spice,
        );
        if windows {
            // Windows drives the standard VGA device with its built-in display driver.
            args.push(["-device", "VGA"]);
            args.push(["-rtc", "base=localtime"]);
        } else {
            args.push([
                "-device",
                &format!("qxl-vga,xres=1920,yres=1080,max_outputs={}", self.displays),
            ]);
            args.push(["-device", "virtio-serial-pci"]);
            args.push(["-chardev", "spicevmc,id=vdagent,name=vdagent"]);
            args.push([
                "-device",
                "virtserialport,chardev=vdagent,name=com.redhat.spice.0",
            ]);
            args.push(["-device", "virtio-rng-pci"]);
        }
        args.push(["-device", "qemu-xhci", "-device", "usb-tablet"]);

        if let Some(serial) = &self.serial {
            args.push_path("-serial", "file:", serial);
        }

        args.push_path_then("-qmp", "unix:", &self.sockets.qmp, ",server=on,wait=off");

        if secure {
            args.push_path("-chardev", "socket,id=tpm,path=", &self.sockets.tpm);
            args.push([
                "-tpmdev",
                "emulator,id=tpm0,chardev=tpm",
                "-device",
                "tpm-crb,tpmdev=tpm0",
            ]);
        }
        args.0
    }
}

#[derive(Default)]
struct Args(Vec<OsString>);

impl Args {
    fn push<const N: usize>(&mut self, values: [&str; N]) {
        self.0.extend(values.into_iter().map(OsString::from));
    }

    /// Pushes `flag` and a value made of `prefix` and a path.
    fn push_path(&mut self, flag: &str, prefix: &str, path: &Path) {
        self.push_path_then(flag, prefix, path, "");
    }

    /// Pushes `flag` and a value made of `prefix`, a path and `suffix`. QEMU splits option
    /// values at commas and reads a doubled comma as one comma, so commas in the path are doubled.
    fn push_path_then(&mut self, flag: &str, prefix: &str, path: &Path, suffix: &str) {
        let bytes = path.as_os_str().as_bytes();
        let mut escaped = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            escaped.push(byte);
            if byte == b',' {
                escaped.push(b',');
            }
        }
        let mut value = OsString::from(prefix);
        value.push(OsString::from_vec(escaped));
        value.push(suffix);
        self.0.push(OsString::from(flag));
        self.0.push(value);
    }
}
