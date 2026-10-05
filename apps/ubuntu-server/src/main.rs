use std::process::ExitCode;

use myconfig_interface::Profile;
use myconfig_modules::*;

static MODULES: &[&dyn Module] = &[
    &Base {
        packages: Base::UBUNTU,
        unwanted: &[],
    },
    &Zsh {
        set_login_shell: true,
    },
];

fn main() -> ExitCode {
    let has_apt = std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| directory.join("apt-get").is_file())
    });
    if std::env::consts::OS != "linux" || !has_apt {
        eprintln!("error: the Ubuntu Server installer requires an apt-based Linux system");
        return ExitCode::FAILURE;
    }
    myconfig_interface::main(Profile {
        title: "Ubuntu Server",
        package_system: PackageSystem::Apt,
        modules: MODULES,
    })
}

#[cfg(test)]
mod tests {
    use super::MODULES;

    #[test]
    fn the_profile_is_only_the_shell() {
        let names: Vec<_> = MODULES.iter().map(|module| module.name()).collect();
        assert_eq!(names, ["base", "zsh"]);
    }
}
