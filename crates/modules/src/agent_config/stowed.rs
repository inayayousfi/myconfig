//! Linux: the `ai` config package through GNU Stow, plus links for each agent.
use std::{fs, path::Path};

use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, ServiceScope,
    deploy::verify_package,
    support::{
        active_group, agent_tool_path, configure_input_access, input_access_settings,
        require_executable, require_programs, user_runtime_directory, verify_input_access,
    },
};

pub struct AgentConfigStowed {
    /// Runs ydotool as a user service for virtual keyboard and pointer input.
    pub ydotool: bool,
}

const INSTRUCTION_LINKS: [&str; 3] = [
    ".config/opencode/AGENTS.md",
    ".fx/AGENTS.md",
    ".pi/agent/AGENTS.md",
];

pub(crate) fn link_agent_config(ctx: &Context) -> ModuleResult {
    let home = ctx.home;
    let skills = home.join(".agents/skills");
    let instructions = home.join(".agents/AGENTS.md");
    if !skills.is_dir() || !instructions.is_file() {
        return Err("agent skills or global AGENTS.md were not stowed".into());
    }
    let claude_skills = home.join(".claude/skills");
    if claude_skills.is_dir() {
        for entry in fs::read_dir(&claude_skills)? {
            let path = entry?.path();
            if let Ok(target) = fs::read_link(&path)
                && target.starts_with("../../.agents/skills")
                && !skills
                    .join(path.file_name().ok_or("skill link has no name")?)
                    .is_dir()
            {
                ctx.delete(&path)?;
            }
        }
    }
    for entry in fs::read_dir(&skills)? {
        let entry = entry?;
        if entry.path().is_dir() {
            ctx.link(
                &claude_skills.join(entry.file_name()),
                &Path::new("../../.agents/skills").join(entry.file_name()),
            )?;
        }
    }
    for relative in INSTRUCTION_LINKS {
        ctx.link(&home.join(relative), &instructions)?;
    }
    Ok(())
}

pub(crate) fn configure_fx_playwright(ctx: &Context) -> ModuleResult {
    let config = ctx.home.join(".fx/mcp.json");
    let contents = match fs::read(&config) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => b"{}".to_vec(),
        Err(error) => return Err(error.into()),
    };
    let program = ctx.home.join(".bun/bin/playwright-mcp");
    let expression = ".mcp = ((.mcpServers // {}) + (.mcp // {})) | .mcp.playwright = {\"type\": \"stdio\", \"command\": [$command, \"--headless\"], \"enabled\": true} | del(.mcpServers)";
    let updated = ctx.read_with_input(
        cmd!(ctx.shell, "jq --arg command {program} {expression}"),
        &contents,
    )?;
    ctx.write_file(&config, format!("{updated}\n").as_bytes(), false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn wait_for_ydotool_socket(ctx: &Context) -> ModuleResult {
    let socket = ctx
        .shell
        .var_os("YDOTOOL_SOCKET")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or(user_runtime_directory(ctx)?.join(".ydotool_socket"));
    for _ in 0..20 {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            if fs::metadata(&socket).is_ok_and(|metadata| metadata.file_type().is_socket()) {
                return Ok(());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(format!("ydotool did not create its socket: {}", socket.display()).into())
}

impl Module for AgentConfigStowed {
    fn name(&self) -> &'static str {
        "agent-config"
    }

    fn footprint(&self, ctx: &Context) -> Footprint {
        let mut footprint = Footprint {
            packages: vec![Package::Jq, Package::Python],
            settings: Vec::new(),
        };
        if self.ydotool {
            footprint.packages.push(Package::Ydotool);
            footprint.settings = input_access_settings(ctx);
        }
        footprint
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        ctx.deploy_config("ai")?;
        let _bun = sh.push_env("BUN_INSTALL", ctx.home.join(".bun"));
        let _path = sh.push_env("PATH", agent_tool_path(ctx)?);
        if ctx.find_program("opencode").is_ok() {
            ctx.read(cmd!(sh, "opencode debug config"))?;
        }
        link_agent_config(ctx)?;
        require_programs(ctx, &["jq"])?;
        configure_fx_playwright(ctx)?;
        let helper = ctx.home.join(".local/bin/claude-config-helper");
        require_executable(&helper)?;
        super::record_mcp_servers(ctx)?;
        ctx.run(cmd!(sh, "{helper} mcp apply"))?;

        if self.ydotool {
            require_programs(ctx, &["ydotool", "systemctl"])?;
            configure_input_access(ctx, "ydotool")?;
            ctx.run(cmd!(sh, "systemctl --user daemon-reload"))?;
            ctx.enable_service("ydotool.service", ServiceScope::User)?;
            if active_group(ctx, "input")? && active_group(ctx, "uinput")? {
                ctx.run(cmd!(sh, "systemctl --user restart ydotool.service"))?;
                ctx.run(cmd!(
                    sh,
                    "systemctl --user --quiet is-active ydotool.service"
                ))?;
                wait_for_ydotool_socket(ctx)?;
            } else {
                ctx.note("Log out and back in before ydotool can access uinput");
            }
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_package(ctx, "ai")?;
        let instructions = ctx.home.join(".agents/AGENTS.md");
        for relative in INSTRUCTION_LINKS {
            let link = ctx.home.join(relative);
            if fs::read_link(&link)? != instructions {
                return Err(format!(
                    "{} does not point to {}",
                    link.display(),
                    instructions.display()
                )
                .into());
            }
        }
        for entry in fs::read_dir(ctx.home.join(".agents/skills"))? {
            let entry = entry?;
            if entry.path().is_dir()
                && !ctx
                    .home
                    .join(".claude/skills")
                    .join(entry.file_name())
                    .is_dir()
            {
                return Err(format!("Claude is missing the skill {:?}", entry.file_name()).into());
            }
        }
        require_executable(&ctx.home.join(".local/bin/claude-config-helper"))?;
        if self.ydotool {
            verify_input_access(ctx, "ydotool")?;
            ctx.run(cmd!(
                ctx.shell,
                "systemctl --user --quiet is-enabled ydotool.service"
            ))?;
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
