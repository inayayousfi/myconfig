//! Edits of the initramfs hooks in `/etc/mkinitcpio.conf` and the kernel options in
//! `/boot/refind_linux.conf`, shared by the modules that change the boot.
use std::path::Path;

use crate::{Context, ModuleResult, Setting};

pub(crate) const MKINITCPIO_CONFIG: &str = "/etc/mkinitcpio.conf";
pub(crate) const REFIND_LINUX_CONFIG: &str = "/boot/refind_linux.conf";

fn hook_setting(hook: &str) -> Setting {
    Setting::InitramfsHook {
        hook: hook.to_owned(),
    }
}

/// Records the initramfs rebuild before a hook change, so `remove` rebuilds once the
/// hook is back as it was.
fn record_rebuild(ctx: &Context) -> ModuleResult {
    ctx.record_undo_command(Path::new("sudo"), &["mkinitcpio", "-P"])
}

/// Puts `hook` right after `after` unless the hook list already has it, and reports
/// whether it changed the list. The hook is its own recorded setting, so another
/// module's hook change in the same run stays apart from it.
pub(crate) fn add_initramfs_hook(ctx: &Context, hook: &str, after: &str) -> ModuleResult<bool> {
    let setting = hook_setting(hook);
    if setting.read(ctx)?.is_some() {
        return Ok(false);
    }
    record_rebuild(ctx)?;
    ctx.set(setting, after)?;
    Ok(true)
}

/// Takes `hook` out of the hook list, and reports whether it was there.
pub(crate) fn remove_initramfs_hook(ctx: &Context, hook: &str) -> ModuleResult<bool> {
    let setting = hook_setting(hook);
    if setting.read(ctx)?.is_none() {
        return Ok(false);
    }
    record_rebuild(ctx)?;
    ctx.unset(setting)?;
    Ok(true)
}

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

/// The hook that comes right before `hook`, empty when `hook` is first, or `None` when
/// the list does not have `hook`.
pub(crate) fn hook_before(config: &str, hook: &str) -> Option<String> {
    config.lines().find_map(|line| {
        let (hooks, _) = line.strip_prefix("HOOKS=(")?.split_once(')')?;
        let hooks: Vec<&str> = hooks.split_whitespace().collect();
        let position = hooks.iter().position(|name| *name == hook)?;
        Some(
            position
                .checked_sub(1)
                .map_or("", |before| hooks[before])
                .to_owned(),
        )
    })
}

/// Puts `hook` right after `after`, or first when `after` is empty, moving it when the
/// list already has it elsewhere.
pub(crate) fn with_hook_after(config: &str, hook: &str, after: &str) -> ModuleResult<String> {
    if !after.is_empty() && !has_hook(config, after) {
        return Err(format!("{MKINITCPIO_CONFIG} has no {after} hook to put {hook} after").into());
    }
    Ok(edit_hooks(&without_hook(config, hook), |hooks| {
        let position = hooks
            .iter()
            .position(|name| name == after)
            .map_or(0, |position| position + 1);
        hooks.insert(position, hook.to_owned());
    }))
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

/// Sets the console colors in every boot entry: each of `colors` is a kernel option such
/// as `vt.default_red=0,248,...`, which replaces any earlier value of the same option.
pub(crate) fn with_console_colors(config: &str, colors: &[String]) -> String {
    let names: Vec<&str> = colors
        .iter()
        .filter_map(|option| option.split_once('=').map(|(name, _)| name))
        .collect();
    edit_entries(config, |_, options| {
        options
            .split_whitespace()
            .filter(|option| {
                !names
                    .iter()
                    .any(|name| option.split_once('=').map(|(key, _)| key) == Some(name))
            })
            .chain(colors.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    })
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
    fn a_hook_goes_back_right_after_the_hook_it_followed() {
        assert_eq!(
            hook_before(HOOKS, "plymouth").as_deref(),
            Some("sd-vconsole")
        );
        assert_eq!(hook_before(HOOKS, "base").as_deref(), Some(""));
        assert_eq!(hook_before(HOOKS, "sd-bootvine"), None);
        let without = without_hook(HOOKS, "plymouth");
        assert_eq!(
            with_hook_after(&without, "plymouth", "sd-vconsole").unwrap(),
            HOOKS
        );
        let edited = with_hook_after(&without, "sd-bootvine", "systemd").unwrap();
        assert!(edited.contains("HOOKS=(base systemd sd-bootvine autodetect microcode"));
        assert_eq!(
            with_hook_after(&edited, "sd-bootvine", "systemd").unwrap(),
            edited
        );
        assert!(with_hook_after(&without, "plymouth", "missing").is_err());
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

    #[test]
    fn console_colors_join_every_entry_once_and_replace_earlier_values() {
        let colors = vec![
            "vt.default_red=1,2".to_owned(),
            "vt.default_grn=3,4".to_owned(),
        ];
        let edited = with_console_colors(
            &ENTRIES.replace(
                "rw root=UUID=31f4 resume",
                "rw vt.default_red=9,9 root=UUID=31f4 resume",
            ),
            &colors,
        );
        assert_eq!(with_console_colors(&edited, &colors), edited);
        assert_eq!(
            edited,
            "\"Boot with standard options\"    \"quiet nowatchdog splash rw root=UUID=31f4 resume=UUID=31f4 vt.default_red=1,2 vt.default_grn=3,4\"\n\
             \"Boot to single-user mode\"    \"quiet nowatchdog splash rw root=UUID=31f4 vt.default_red=1,2 vt.default_grn=3,4\" single\n\
             # \"Commented\" \"quiet\"\n\
             \"Boot with minimal options\"   \"ro root=/dev/nvme0n1p2 vt.default_red=1,2 vt.default_grn=3,4\"\n"
        );
    }
}
