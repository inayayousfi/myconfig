//! Windows: copies of the agent instructions, skills and Claude settings.
use std::path::{Path, PathBuf};

use embedded_dotfiles::DOTFILES;
use typed_fs_rs::EmbeddedDirectory;
use xshell::cmd;

use crate::{Context, Footprint, Module, ModuleResult, Package};

pub struct AgentConfigCopied;

/// Where each embedded file goes under the home directory. Skills also go to Claude.
fn destinations(home: &Path, path: &Path) -> Vec<PathBuf> {
    if let Ok(skill) = path.strip_prefix("ai/.agents/skills") {
        return vec![
            home.join(".agents/skills").join(skill),
            home.join(".claude/skills").join(skill),
        ];
    }
    [
        ("ai/.agents/AGENTS.md", ".agents/AGENTS.md"),
        ("ai/.claude/CLAUDE.md", ".claude/CLAUDE.md"),
        ("ai/.claude/settings.json", ".claude/settings.json"),
        (
            "ai/.local/bin/claude-config-helper",
            ".local/bin/claude-config-helper",
        ),
        (
            "ai/.config/claude-config-helper/mcp-servers.json",
            ".config/claude-config-helper/mcp-servers.json",
        ),
    ]
    .into_iter()
    .filter(|(source, _)| path == Path::new(source))
    .map(|(_, destination)| home.join(destination))
    .collect()
}

const REQUIRED: [&str; 5] = [
    ".agents/AGENTS.md",
    ".claude/CLAUDE.md",
    ".claude/settings.json",
    ".local/bin/claude-config-helper",
    ".config/claude-config-helper/mcp-servers.json",
];

impl Module for AgentConfigCopied {
    fn name(&self) -> &'static str {
        "agent-config"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Python],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let files = DOTFILES.ai.files();
        for file in &files {
            for destination in destinations(ctx.home, Path::new(file.path_from_root)) {
                ctx.write_file(&destination, file.content, file.executable)?;
            }
        }
        for required in REQUIRED {
            if !ctx.home.join(required).is_file() {
                return Err(format!("embedded agent configuration is missing {required}").into());
            }
        }
        if ctx.find_program("py").is_ok() && ctx.find_program("claude").is_ok() {
            let helper = ctx.home.join(".local/bin/claude-config-helper");
            super::record_mcp_servers(ctx)?;
            if let Err(error) = ctx.run(cmd!(ctx.shell, "py -3 {helper} mcp apply")) {
                ctx.note(&format!("Claude MCP servers were not applied: {error}"));
            }
        } else {
            ctx.note("Python or Claude Code not found; run claude-config-helper mcp apply later");
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        for file in DOTFILES.ai.files() {
            for destination in destinations(ctx.home, Path::new(file.path_from_root)) {
                if std::fs::read(&destination)? != file.content {
                    return Err(
                        format!("{} differs from the repository", destination.display()).into(),
                    );
                }
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
