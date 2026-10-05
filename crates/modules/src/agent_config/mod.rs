//! The shared agent instructions, skills, Claude settings and MCP servers.
mod copied;
mod stowed;

pub use copied::AgentConfigCopied;
pub use stowed::AgentConfigStowed;
#[cfg(test)]
pub(crate) use stowed::{configure_fx_playwright, link_agent_config};

use embedded_dotfiles::DOTFILES;

use crate::{Context, ModuleResult, Setting};

/// The MCP servers that `claude-config-helper mcp apply` adds to Claude.
fn mcp_servers() -> ModuleResult<Vec<Setting>> {
    let list: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(
        DOTFILES
            .ai
            ._config
            .claude_config_helper
            .mcp_servers_json
            .content,
    )?;
    Ok(list
        .keys()
        .map(|name| Setting::ClaudeMcpServer { name: name.clone() })
        .collect())
}

/// Records the MCP servers before the helper changes them, so `remove` puts them back.
fn record_mcp_servers(ctx: &Context) -> ModuleResult {
    for setting in mcp_servers()? {
        ctx.record_setting(setting)?;
    }
    Ok(())
}
