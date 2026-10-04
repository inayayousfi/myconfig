"""Exercise claude-config-helper and the Zsh shortcuts without starting Claude."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[1]
HELPER = REPO / "dotfiles/ai/.local/bin/claude-config-helper"
PLUGIN = REPO / "dotfiles/zsh/.oh-my-zsh/custom/plugins/inaya/inaya.plugin.zsh"
TRACKED = [
    REPO / "dotfiles/ai/.claude/settings.json",
    REPO / "dotfiles/ai/.config/claude-config-helper/mcp-servers.json",
]


class ClaudeConfigHelper(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="claude-config-helper-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.home = self.root / "home"
        (self.home / ".local/bin").mkdir(parents=True)
        (self.home / ".local/bin/claude-config-helper").symlink_to(HELPER)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        cli = self.bin / "claude"
        # Record each call as one JSON array so arguments with spaces stay intact.
        cli.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "with open(os.path.join(os.environ['HOME'], 'claude-calls'), 'a') as log:\n"
            "    log.write(json.dumps(sys.argv[1:]) + '\\n')\n"
        )
        cli.chmod(0o700)
        # Only system programs and the fake claude, never the user's installed tools.
        self.env = dict(os.environ, HOME=str(self.home), PATH=f"{self.bin}:/usr/bin:/bin")
        self.env.pop("CLAUDE_CONFIG_DIR", None)
        self.project = self.root / "project with spaces"
        self.project.mkdir()
        self.config = self.home / ".claude.json"
        self.backup = self.home / ".claude.json.before-config-helper"
        self.original = {
            "oauthAccount": {"test": "preserve"},
            "projects": {str(self.project): {"allowedTools": ["Read", "Bash(git status)"]}},
        }
        self.config.write_text(json.dumps(self.original))

    def launch(self, command, cwd=None, env=None):
        return subprocess.run(
            ["zsh", "-f", "-c", 'source "$1"; eval "$2"', "test", str(PLUGIN), command],
            cwd=cwd or self.project, env=env or self.env, capture_output=True, text=True,
        )

    def helper(self, *arguments, env=None):
        return subprocess.run(
            [str(self.home / ".local/bin/claude-config-helper"), *arguments],
            cwd=self.project, env=env or self.env, capture_output=True, text=True,
        )

    def calls(self):
        log = self.home / "claude-calls"
        return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []

    def git(self, *arguments, cwd=None):
        return subprocess.run(
            ["git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", *arguments],
            cwd=cwd or self.project, env=self.env, capture_output=True, text=True, check=True,
        )

    def test_shortcuts_trust_project_and_forward_arguments(self):
        for shortcut, expected in [
            ("cco", ["--dangerously-skip-permissions", "--continue", "prompt with spaces"]),
            ("ccor", ["remote-control", "--permission-mode", "bypassPermissions", "--continue", "prompt with spaces"]),
        ]:
            with self.subTest(shortcut=shortcut):
                self.config.write_text(json.dumps(self.original))
                result = self.launch(f'{shortcut} --continue "prompt with spaces"')
                self.assertEqual(result.returncode, 0, result.stderr)
                state = json.loads(self.config.read_text())
                self.assertTrue(state["projects"][str(self.project)]["hasTrustDialogAccepted"])
                del state["projects"][str(self.project)]["hasTrustDialogAccepted"]
                self.assertEqual(state, self.original)
                self.assertEqual(self.calls()[-1], expected)
                self.assertFalse(Path(str(self.config) + ".lock").exists())
        # Each change replaces the single backup of the previous state.
        self.assertEqual(list(self.home.glob(".claude.json.*")), [self.backup])
        self.assertEqual(json.loads(self.backup.read_text()), self.original)

    def test_worktree_launch_trusts_main_checkout(self):
        self.git("init")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "--allow-empty", "-m", "Fixture")
        worktree = self.root / "linked worktree"
        self.git("worktree", "add", "-b", "fixture", str(worktree))
        nested = worktree / "nested"
        nested.mkdir()
        result = self.launch("ccor", cwd=nested)
        self.assertEqual(result.returncode, 0, result.stderr)
        projects = json.loads(self.config.read_text())["projects"]
        self.assertTrue(projects[str(self.project)]["hasTrustDialogAccepted"])
        self.assertNotIn(str(worktree), projects)
        self.assertNotIn(str(nested), projects)

    def test_home_remote_control_stops_without_mutating_state(self):
        before = self.config.read_bytes()
        result = self.launch("ccor", cwd=self.home)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Enter a project directory first", result.stderr)
        self.assertEqual(self.config.read_bytes(), before)
        self.assertEqual(self.calls(), [])

    def test_invalid_state_prevents_launch_and_is_not_overwritten(self):
        self.config.write_text("{broken")
        result = self.launch("cco")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.config.read_text(), "{broken")
        self.assertEqual(self.calls(), [])
        self.assertFalse(Path(str(self.config) + ".lock").exists())

    def test_custom_configuration_directory(self):
        directory = self.root / "custom config"
        directory.mkdir()
        config = directory / ".claude.json"
        config.write_text(json.dumps(self.original))
        before = self.config.read_bytes()
        result = self.launch("ccor", env=dict(self.env, CLAUDE_CONFIG_DIR=str(directory)))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(json.loads(config.read_text())["projects"][str(self.project)]["hasTrustDialogAccepted"])
        self.assertEqual(self.config.read_bytes(), before)

    def test_busy_claude_state_prevents_launch(self):
        lock = Path(str(self.config) + ".lock")
        lock.mkdir()
        before = self.config.read_bytes()
        result = self.launch("ccor")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("state is busy", result.stderr)
        self.assertEqual(self.config.read_bytes(), before)
        self.assertTrue(lock.is_dir())
        self.assertEqual(self.calls(), [])

    def test_approvals_are_listed_and_cleared_for_one_project(self):
        other = str(self.root / "other")
        self.original["projects"][other] = {"allowedTools": ["Edit"]}
        self.config.write_text(json.dumps(self.original))
        listed = self.helper("approvals", "list", str(self.project))
        self.assertEqual(listed.returncode, 0, listed.stderr)
        self.assertEqual(listed.stdout.splitlines(), [str(self.project), "  Read", "  Bash(git status)"])

        cleared = self.helper("approvals", "clear", str(self.project))
        self.assertEqual(cleared.returncode, 0, cleared.stderr)
        self.assertIn("Cleared 2 saved approvals", cleared.stdout)
        state = json.loads(self.config.read_text())
        self.assertEqual(state["projects"][str(self.project)]["allowedTools"], [])
        self.assertEqual(state["projects"][other]["allowedTools"], ["Edit"])
        self.assertEqual(state["oauthAccount"], {"test": "preserve"})
        self.assertEqual(json.loads(self.backup.read_text()), self.original)

    def test_mcp_apply_configures_available_servers_once(self):
        tool = self.home / "tools/present"
        tool.parent.mkdir()
        tool.write_text("#!/bin/sh\n")
        tool.chmod(0o700)
        server_list = self.root / "servers.json"
        listed = {
            "present": {"type": "stdio", "command": "{home}/tools/present", "args": ["--headless"]},
            "missing": {"type": "stdio", "command": "{home}/tools/missing", "args": []},
        }
        server_list.write_text(json.dumps(listed))
        expected = {"type": "stdio", "command": str(tool), "args": ["--headless"]}

        result = self.helper("mcp", "apply", "--list", str(server_list))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Skipped MCP server missing", result.stderr)
        [call] = self.calls()
        self.assertEqual(call[:5], ["mcp", "add-json", "--scope", "user", "present"])
        self.assertEqual(json.loads(call[5]), expected)

        # Claude may store extra fields; an unchanged server is left alone.
        state = json.loads(self.config.read_text())
        state["mcpServers"] = {"present": dict(expected, env={})}
        self.config.write_text(json.dumps(state))
        self.assertEqual(self.helper("mcp", "apply", "--list", str(server_list)).returncode, 0)
        self.assertEqual(len(self.calls()), 1)

        listed["present"]["args"] = []
        server_list.write_text(json.dumps(listed))
        self.assertEqual(self.helper("mcp", "apply", "--list", str(server_list)).returncode, 0)
        self.assertEqual(self.calls()[1], ["mcp", "remove", "--scope", "user", "present"])
        self.assertEqual(json.loads(self.calls()[2][5]), dict(expected, args=[]))

    def test_mcp_apply_without_claude_changes_nothing(self):
        (self.bin / "claude").unlink()
        before = self.config.read_bytes()
        result = self.helper("mcp", "apply", "--list", str(TRACKED[1]))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Claude Code is not installed", result.stderr)
        self.assertEqual(self.config.read_bytes(), before)

    def test_check_accepts_tracked_files_and_reports_personal_data(self):
        clean = self.helper("check", *map(str, TRACKED))
        self.assertEqual(clean.returncode, 0, clean.stdout)
        leaked = self.root / "leaked.json"
        leaked.write_text(json.dumps({
            "env": {"API_TOKEN": "value"},
            "owner": "someone@example.com",
            "command": "/home/someone/bin/tool",
            "header": "Authorization: Bearer abc",
        }))
        result = self.helper("check", str(leaked))
        self.assertEqual(result.returncode, 1)
        for reason in ("secret field", "email address", "home path", "token"):
            self.assertIn(reason, result.stdout)


if __name__ == "__main__":
    unittest.main()
