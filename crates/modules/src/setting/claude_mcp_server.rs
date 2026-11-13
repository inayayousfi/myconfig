//! An MCP server in Claude Code's user configuration, whose value is its JSON definition.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct ClaudeMcpServer<'a> {
    pub name: &'a str,
}

impl Kind for ClaudeMcpServer<'_> {
    fn describe(&self) -> String {
        format!("Claude MCP server {}", self.name)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let path = ctx.home.join(".claude.json");
        match std::fs::read(&path) {
            Ok(contents) => {
                let json: serde_json::Value = serde_json::from_slice(&contents)?;
                Ok(json
                    .get("mcpServers")
                    .and_then(|servers| servers.get(self.name))
                    .map(|server| server.to_string()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        if self.read(ctx)?.is_some() {
            self.delete(ctx)?;
        }
        let name = self.name;
        ctx.run(cmd!(
            ctx.shell,
            "claude mcp add-json --scope user {name} {value}"
        ))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let name = self.name;
        ctx.run(cmd!(ctx.shell, "claude mcp remove --scope user {name}"))
    }
}
