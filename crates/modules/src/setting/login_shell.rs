//! The login shell of a user.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct LoginShell<'a> {
    pub user: &'a str,
}

impl Kind for LoginShell<'_> {
    fn describe(&self) -> String {
        format!("login shell of {}", self.user)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let user = self.user;
        let account = ctx.read(cmd!(ctx.shell, "getent passwd {user}"))?;
        Ok(Some(
            account
                .rsplit(':')
                .next()
                .ok_or("getent returned no shell")?
                .to_owned(),
        ))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        let user = self.user;
        ctx.run(cmd!(ctx.shell, "sudo chsh -s {value} {user}"))?;
        if self.read(ctx)?.as_deref() != Some(value) {
            return Err(format!("could not verify the login shell for {user}").into());
        }
        Ok(())
    }

    fn delete(&self, _ctx: &Context) -> ModuleResult {
        Err(format!("the login shell of {} cannot be deleted", self.user).into())
    }
}
