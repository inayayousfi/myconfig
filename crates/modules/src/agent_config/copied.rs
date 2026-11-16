//! Windows: copies of the agent instructions, skills and Claude settings.
use std::path::{Path, PathBuf};

use embedded_dotfiles::AI;
use typed_fs_rs::{EmbeddedDirectory, EmbeddedFile};
use xshell::cmd;

use crate::{Context, Footprint, Module, ModuleResult, Need, Package};

pub struct AgentConfigCopied;

/// The files that go to the same path under the home directory.
static FILES: [&EmbeddedFile; 5] = [
    &AI._agents.AGENTS_md,
    &AI._claude.CLAUDE_md,
    &AI._claude.settings_json,
    &AI._local.bin.claude_config_helper,
    &AI._config.claude_config_helper.mcp_servers_json,
];

/// Each copied file with its destination under the home directory. Skills also go to Claude.
fn copies(home: &Path) -> ModuleResult<Vec<(&'static EmbeddedFile, PathBuf)>> {
    let mut copies: Vec<_> = FILES
        .iter()
        .map(|file| (*file, home.join(file.path_from_root)))
        .collect();
    for file in AI._agents.skills.files() {
        let skill = Path::new(file.path_from_root).strip_prefix(".agents/skills")?;
        copies.push((file, home.join(".agents/skills").join(skill)));
        copies.push((file, home.join(".claude/skills").join(skill)));
    }
    Ok(copies)
}

impl Module for AgentConfigCopied {
    fn name(&self) -> &'static str {
        "agent-config"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Python],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        for (file, destination) in copies(ctx.home)? {
            ctx.write_file(&destination, file.content, file.executable)?;
        }
        if ctx.find_program("py").is_ok() && ctx.find_program("claude").is_ok() {
            let helper = ctx.home.join(".local/bin/claude-config-helper");
            super::record_mcp_servers(ctx)?;
            ctx.run(cmd!(ctx.shell, "py -3 {helper} mcp apply"))?;
        } else {
            ctx.note("Python or Claude Code not found; run claude-config-helper mcp apply later");
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        for (file, destination) in copies(ctx.home)? {
            if std::fs::read(&destination)? != file.content {
                return Err(
                    format!("{} differs from the repository", destination.display()).into(),
                );
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
