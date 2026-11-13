//! Edits of the initramfs hooks in `/etc/mkinitcpio.conf` and the kernel options in
//! `/boot/refind_linux.conf`, shared by the modules that change the boot.
use std::path::Path;

use xshell::cmd;

use crate::{Context, ModuleResult};

pub(crate) const MKINITCPIO_CONFIG: &str = "/etc/mkinitcpio.conf";
pub(crate) const REFIND_LINUX_CONFIG: &str = "/boot/refind_linux.conf";

/// Applies `edit` to the hook list of each `HOOKS=(...)` line.
fn edit_hooks(config: &str, edit: impl Fn(&mut Vec<String>)) -> String {
    config
        .split_inclusive('\n')
        .map(|line| {
            let body = line.trim_end_matches('\n');
            let Some(list) = body.strip_prefix("HOOKS=(") else {
                return line.to_owned();
            };
            let Some((hooks, rest)) = list.split_once(')') else {
                return line.to_owned();
            };
            let mut hooks: Vec<String> = hooks.split_whitespace().map(str::to_owned).collect();
            edit(&mut hooks);
            format!("HOOKS=({}){rest}{}", hooks.join(" "), &line[body.len()..])
        })
        .collect()
}

pub(crate) fn has_hook(config: &str, hook: &str) -> bool {
    config.lines().any(|line| {
        line.strip_prefix("HOOKS=(")
            .and_then(|list| list.split_once(')'))
            .is_some_and(|(hooks, _)| hooks.split_whitespace().any(|name| name == hook))
    })
}

pub(crate) fn without_hook(config: &str, hook: &str) -> String {
    edit_hooks(config, |hooks| hooks.retain(|name| name != hook))
}

/// Puts `hook` right after `after`, unless the list already has it.
pub(crate) fn with_hook_after(config: &str, hook: &str, after: &str) -> String {
    edit_hooks(config, |hooks| {
        if hooks.iter().any(|name| name == hook) {
            return;
        }
        if let Some(position) = hooks.iter().position(|name| name == after) {
            hooks.insert(position + 1, hook.to_owned());
        }
    })
}

/// Applies `edit` to the kernel options of each boot entry, with the entry's index.
/// An entry is a line with a quoted title and quoted options; comments stay as they are.
fn edit_entries(config: &str, edit: impl Fn(usize, &str) -> String) -> String {
    let mut index = 0;
    config
        .split_inclusive('\n')
        .map(|line| {
            let body = line.trim_end_matches('\n');
            let trimmed = body.trim_start();
            let mut fields: Vec<&str> = body.split('"').collect();
            if trimmed.is_empty() || trimmed.starts_with('#') || fields.len() < 5 {
                return line.to_owned();
            }
            let options = edit(index, fields[3]);
            index += 1;
            fields[3] = &options;
            format!("{}{}", fields.join("\""), &line[body.len()..])
        })
        .collect()
}

pub(crate) fn without_options(config: &str, removed: &[&str]) -> String {
    edit_entries(config, |_, options| {
        options
            .split_whitespace()
            .filter(|option| !removed.contains(option))
            .collect::<Vec<_>>()
            .join(" ")
    })
}

/// Passes `quiet` on the first boot entry only, so the other entries keep the kernel
/// messages for diagnosis.
pub(crate) fn with_quiet_default(config: &str) -> String {
    edit_entries(config, |index, options| {
        if index > 0 || options.split_whitespace().any(|option| option == "quiet") {
            options.to_owned()
        } else if options.is_empty() {
            "quiet".to_owned()
        } else {
            format!("quiet {options}")
        }
    })
}

/// A root-owned file's contents, or `None` when it does not exist.
pub(crate) fn read_system_file(ctx: &Context, path: &str) -> ModuleResult<Option<String>> {
    if !ctx.succeeds(cmd!(ctx.shell, "sudo test -f {path}"))? {
        return Ok(None);
    }
    let bytes = ctx.read_bytes(cmd!(ctx.shell, "sudo cat {path}"))?;
    Ok(Some(String::from_utf8(bytes)?))
}

/// Replaces a root-owned file when `edit` changes it, and reports whether it did.
pub(crate) fn edit_system_file(
    ctx: &Context,
    path: &str,
    edit: impl Fn(&str) -> String,
) -> ModuleResult<Option<bool>> {
    let Some(current) = read_system_file(ctx, path)? else {
        return Ok(None);
    };
    let edited = edit(&current);
    if edited == current {
        return Ok(Some(false));
    }
    ctx.write_system_file(Path::new(path), edited.as_bytes(), "0644")?;
    Ok(Some(true))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOOKS: &str = "# vim:set ft=sh\nMODULES=()\nHOOKS=(base systemd autodetect microcode kms modconf block keyboard sd-vconsole plymouth filesystems fsck)\n";
    const ENTRIES: &str = "\"Boot with standard options\"    \"quiet nowatchdog splash rw root=UUID=31f4 resume=UUID=31f4\"\n\"Boot to single-user mode\"    \"quiet nowatchdog splash rw root=UUID=31f4\" single\n# \"Commented\" \"quiet\"\n\"Boot with minimal options\"   \"ro root=/dev/nvme0n1p2\"\n";

    #[test]
    fn only_the_named_hook_leaves_the_list() {
        let edited = without_hook(HOOKS, "plymouth");
        assert_eq!(
            edited,
            "# vim:set ft=sh\nMODULES=()\nHOOKS=(base systemd autodetect microcode kms modconf block keyboard sd-vconsole filesystems fsck)\n"
        );
        assert!(!has_hook(&edited, "plymouth"));
        assert_eq!(without_hook(&edited, "plymouth"), edited);
    }

    #[test]
    fn a_hook_goes_right_after_another_once() {
        let edited = with_hook_after(&without_hook(HOOKS, "plymouth"), "sd-bootvine", "systemd");
        assert!(edited.contains("HOOKS=(base systemd sd-bootvine autodetect microcode"));
        assert_eq!(with_hook_after(&edited, "sd-bootvine", "systemd"), edited);
    }

    #[test]
    fn only_the_named_options_leave_each_entry() {
        assert_eq!(
            without_options(ENTRIES, &["quiet", "nowatchdog", "splash"]),
            "\"Boot with standard options\"    \"rw root=UUID=31f4 resume=UUID=31f4\"\n\"Boot to single-user mode\"    \"rw root=UUID=31f4\" single\n# \"Commented\" \"quiet\"\n\"Boot with minimal options\"   \"ro root=/dev/nvme0n1p2\"\n"
        );
    }

    #[test]
    fn only_the_first_entry_becomes_quiet() {
        let clean = without_options(ENTRIES, &["quiet", "nowatchdog", "splash"]);
        let quiet = with_quiet_default(&clean);
        assert_eq!(
            quiet,
            "\"Boot with standard options\"    \"quiet rw root=UUID=31f4 resume=UUID=31f4\"\n\"Boot to single-user mode\"    \"rw root=UUID=31f4\" single\n# \"Commented\" \"quiet\"\n\"Boot with minimal options\"   \"ro root=/dev/nvme0n1p2\"\n"
        );
        assert_eq!(with_quiet_default(&quiet), quiet);
    }
}
