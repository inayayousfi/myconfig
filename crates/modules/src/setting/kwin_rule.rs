//! A window rule in KWin's rule list: `[General] rules` in `kwinrulesrc` names the rules
//! in order, and `[General] count` holds how many it names.
use super::{Kind, PRESENT, Setting};
use crate::{Context, ModuleResult};

pub(super) struct KwinRule<'a> {
    pub name: &'a str,
}

fn list() -> Setting {
    Setting::kde("kwinrulesrc", &["General"], "rules")
}

fn count() -> Setting {
    Setting::kde("kwinrulesrc", &["General"], "count")
}

fn names(ctx: &Context) -> ModuleResult<Vec<String>> {
    Ok(list()
        .read(ctx)?
        .unwrap_or_default()
        .split(',')
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect())
}

fn write_names(ctx: &Context, names: &[String]) -> ModuleResult {
    list().write(ctx, &names.join(","))?;
    count().write(ctx, &names.len().to_string())
}

impl Kind for KwinRule<'_> {
    fn describe(&self) -> String {
        format!("KWin rule {} in the rule list", self.name)
    }

    fn presence(&self) -> bool {
        true
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        Ok(names(ctx)?
            .iter()
            .any(|name| name == self.name)
            .then(|| PRESENT.to_owned()))
    }

    fn write(&self, ctx: &Context, _value: &str) -> ModuleResult {
        let mut names = names(ctx)?;
        if names.iter().any(|name| name == self.name) {
            return Ok(());
        }
        names.push(self.name.to_owned());
        write_names(ctx, &names)
    }

    /// Takes the rule out of the list and leaves the other rules in their order.
    fn delete(&self, ctx: &Context) -> ModuleResult {
        let mut names = names(ctx)?;
        let before = names.len();
        names.retain(|name| name != self.name);
        if names.len() == before {
            return Ok(());
        }
        write_names(ctx, &names)
    }
}
