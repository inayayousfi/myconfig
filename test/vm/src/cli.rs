//! The command line of `cargo xtask`.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::os::Os;
use crate::run::{self, RunOptions};
use crate::start::Disk;
use crate::{cycle, image, repository, scenario, usb};

#[derive(Parser)]
#[command(bin_name = "cargo xtask", about = "Repository tasks that run by hand")]
struct Arguments {
    #[command(subcommand)]
    task: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Tests the installers in throwaway virtual machines.
    Vm(VmArguments),
}

#[derive(clap::Args)]
struct VmArguments {
    /// Opens a viewer window on the guest display.
    #[arg(long, global = true)]
    watch: bool,
    #[command(subcommand)]
    command: VmCommand,
}

#[derive(Subcommand)]
enum VmCommand {
    /// Installs the fresh installer on a throwaway copy of the clean image, installs it a
    /// second time, removes every module, and fails on any change left behind.
    Cycle { os: Os },
    /// Boots a throwaway copy of the clean image with the fresh installer in it, and keeps
    /// it running until the guest shuts down.
    Run {
        os: Os,
        /// Boots the kept disk of the last failed cycle instead of a new copy.
        #[arg(long, conflicts_with = "resume")]
        failed: bool,
        /// Boots the throwaway disk of the previous run, with what that run left on it,
        /// instead of a new copy.
        #[arg(long)]
        resume: bool,
        /// The number of displays; 2 is available only for a desktop guest.
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=2))]
        displays: u8,
    },
    /// Builds the InayOS USB stick image in a throwaway Arch guest and saves it in the state
    /// folder as usb/inayos.iso.
    Usb {
        #[command(subcommand)]
        command: UsbCommand,
    },
    /// Boots the USB stick image with a blank target disk, installs InayOS from it with an
    /// answers file or through the installer's screens, boots the installed system, and
    /// checks it. broken-kernel starts from the installed disk that blank-disk kept instead.
    Scenario { name: scenario::Name },
    /// Manages the clean images that every throwaway disk starts from.
    Image {
        #[command(subcommand)]
        command: ImageCommand,
    },
}

#[derive(Subcommand)]
enum UsbCommand {
    /// Builds the image from archiso's releng profile, the overlay in test/usb/profile and the
    /// installers built from the working tree, and replaces the previous image when complete.
    Build,
}

#[derive(Subcommand)]
enum ImageCommand {
    /// Downloads the current release, installs it unattended, and replaces the clean image.
    /// The arch image hosts the USB stick build.
    /// The throwaway and kept failed disks of that OS are removed with the old image.
    Build { os: Os },
}

/// Runs the `cargo xtask` command line.
pub fn main() -> ExitCode {
    let Task::Vm(vm) = Arguments::parse().task;
    let result: Result<(), Box<dyn std::error::Error>> = match vm.command {
        VmCommand::Run {
            os,
            failed,
            resume,
            displays,
        } => run::run(
            RunOptions {
                os,
                disk: match (failed, resume) {
                    (true, _) => Disk::Failed,
                    (false, true) => Disk::Work,
                    (false, false) => Disk::Throwaway,
                },
                displays,
                watch: vm.watch,
            },
            &repository(),
        )
        .map_err(Into::into),
        VmCommand::Cycle { os } => cycle::cycle(os, vm.watch, &repository()).map_err(Into::into),
        VmCommand::Usb {
            command: UsbCommand::Build,
        } => usb::build(vm.watch, &repository()).map_err(Into::into),
        VmCommand::Scenario { name } => {
            scenario::run(name, vm.watch, &repository()).map_err(Into::into)
        }
        VmCommand::Image {
            command: ImageCommand::Build { os },
        } => image::build(os, vm.watch).map_err(Into::into),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
