---
name: commit
description: ALWAYS use this skill when the user asks to create a Git commit or draft a commit message, including implicit requests whose intended result is a commit. Do not use it for other outgoing text.
---

# Commit

Drafts a commit message for the currently staged changes and commits only after the
user confirms it.

## Steps

1. **Check there's something to commit.** Run `git diff --cached --stat`. If it's
   empty, check `git status --short` for unstaged/untracked changes. If there are
   none at all, tell the user there's nothing to commit and stop. Otherwise, ask via
   the question tool whether to stage everything (`git add -A`) — do not stage
   anything without their confirmation. If they decline, stop.

2. **Gather context:**
   - Current branch: `git branch --show-current`
   - Recent commit style: `git log -10 --pretty=format:"%h%n%B"`
   - `git diff --cached --stat`
   - Full staged diff: `git diff --cached`

3. **Draft the message:**
   - Concise, descriptive subject line.
   - Infer style (tense, capitalization, prefix conventions like `fix:`/`feat:`) from
     the recent commits above and match it.
   - Include a body only if it adds real information beyond the subject.
   - No emojis, no filler, no restating the diff line-by-line.

4. **Preview.** Output the commit message as plain text, in the chat, in a fenced code
   block tagged `text`. No tool call. Just plain text.

5. **Confirm.** Call the question tool: Confirm / Request changes / Cancel. Do not put
   the commit message in this tool call — it was already shown in step 4. Do not run
   `git commit` until the user confirms. If they ask for changes, redraft, repeat
   step 4, then ask again.

6. Once confirmed, commit with the approved message (e.g. `git commit -F -` fed the
   final message, or `git commit -m`/`-m` flags as appropriate).

7. **Read back the finished commit.** Run `git log -1 --pretty=format:"%B"`, then
   apply the global "No automatic promotion" rule. Amend only when that rule
   requires cleanup, keep the approved message otherwise identical, and report any
   cleanup.

8. **Propose pushing — always the question tool, no exceptions.** After the
   finished-commit cleanup settles, ask whether to push the commit now via the question
   tool — never a plain-text question, never skipped, never inferred from context.
   This is an absolute rule, same as the message confirmation in step 5: it applies
   every single time this step is reached, regardless of how routine or low-stakes
   the commit seems. If they decline, stop. If they confirm, push with `git push` if
   the branch already tracks an upstream, or `git push -u origin <branch>` if it
   doesn't. Never push without this explicit confirmation, even if a push was
   confirmed earlier in the conversation — ask fresh every time.
