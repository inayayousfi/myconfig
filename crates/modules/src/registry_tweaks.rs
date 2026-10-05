//! Explorer, taskbar and Start menu preferences from `RegistryPreferences.reg`.
use crate::{Context, Footprint, Module, ModuleResult, Setting};
use embedded_dotfiles::DOTFILES;

pub struct RegistryTweaks;

/// One value from the `.reg` file. `None` means the file deletes the value.
struct Entry {
    setting: Setting,
    value: Option<String>,
}

fn entries() -> ModuleResult<Vec<Entry>> {
    let contents = String::from_utf8_lossy(DOTFILES.assets.windows.RegistryPreferences_reg.content);
    let mut key = None;
    let mut entries = Vec::new();
    for line in contents
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
    {
        if line.is_empty() || line.starts_with(';') || line.starts_with("Windows Registry Editor") {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            key = Some(name.to_owned());
            continue;
        }
        let (name, data) = line
            .split_once('=')
            .ok_or_else(|| format!("unexpected registry line: {line}"))?;
        let name = name.trim_matches('"').to_owned();
        let value = match data {
            "-" => None,
            data => {
                let hex = data
                    .strip_prefix("dword:")
                    .ok_or_else(|| format!("unsupported registry value: {line}"))?;
                Some(format!("REG_DWORD:0x{:x}", u32::from_str_radix(hex, 16)?))
            }
        };
        entries.push(Entry {
            setting: Setting::RegistryValue {
                key: key.clone().ok_or("registry value before any key")?,
                name,
            },
            value,
        });
    }
    Ok(entries)
}

impl Module for RegistryTweaks {
    fn name(&self) -> &'static str {
        "registry-tweaks"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        // One value at a time, so each is recorded and only the machine-wide one asks for
        // administrator rights.
        for entry in entries()? {
            match entry.value {
                Some(value) => ctx.set(entry.setting, &value)?,
                None => ctx.unset(entry.setting)?,
            }
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        for entry in entries()? {
            if entry.setting.read(ctx)? != entry.value {
                return Err(
                    format!("{} is not {:?}", entry.setting.describe(), entry.value).into(),
                );
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_file_parses_into_values_and_deletions() {
        let entries = entries().unwrap();
        assert!(entries.iter().any(|entry| entry.value.is_none()));
        let hidden = entries
            .iter()
            .find(|entry| matches!(&entry.setting, Setting::RegistryValue { name, .. } if name == "Hidden"))
            .unwrap();
        assert_eq!(hidden.value.as_deref(), Some("REG_DWORD:0x1"));
    }
}
