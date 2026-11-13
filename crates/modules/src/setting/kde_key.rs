//! A key in a KDE configuration file, through `kreadconfig6` and `kwriteconfig6`.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

const MISSING: &str = "__myconfig_missing__";

pub(super) struct KdeKey<'a> {
    pub file: &'a str,
    pub groups: &'a [String],
    pub key: &'a str,
}

impl KdeKey<'_> {
    fn group_arguments(&self) -> Vec<&str> {
        self.groups
            .iter()
            .flat_map(|group| ["--group", group.as_str()])
            .collect()
    }
}

impl Kind for KdeKey<'_> {
    fn describe(&self) -> String {
        format!("{} [{}] {}", self.file, self.groups.join("]["), self.key)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let Self { file, key, .. } = self;
        let groups = self.group_arguments();
        let value = ctx.read(cmd!(
            ctx.shell,
            "kreadconfig6 --file {file} {groups...} --key {key} --default {MISSING}"
        ))?;
        Ok((value != MISSING).then_some(value))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        let Self { file, key, .. } = self;
        let groups = self.group_arguments();
        ctx.run(cmd!(
            ctx.shell,
            "kwriteconfig6 --notify --file {file} {groups...} --key {key} {value}"
        ))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let Self { file, key, .. } = self;
        let groups = self.group_arguments();
        ctx.run(cmd!(
            ctx.shell,
            "kwriteconfig6 --file {file} {groups...} --key {key} --delete"
        ))
    }
}
