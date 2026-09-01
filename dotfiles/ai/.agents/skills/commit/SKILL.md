---
name: commit
description: ALWAYS use this skill when the user asks to create a Git commit or draft a commit message, including implicit requests whose intended result is a commit. Do not use it for other outgoing text.
---

# Commit

Establishes the intended change set, drafts its commit message, and commits only after the user confirms the message.

## Steps

1. **Establish what "this" means.** Inspect `git diff --cached --stat` and `git status --short`. If there are no changes, tell the user there's nothing to commit and stop. Honor any staging scope the user already named. When the current session just produced a task's changes, "commit this" refers to those changes: stage them directly, using `git add -A` when they are the only worktree changes or exact paths or patches when unrelated work also exists. In a fresh session with no task-produced change set, inspect the coherent staging scopes and ask the user to choose. If existing staged work or overlapping edits cannot be safely separated from the intended changes, emit a global deviation instead of committing unrelated work.

2. **Gather context:**
   - Current branch: `git branch --show-current`
   - Recent commit style: `git log -10 --pretty=format:"%h%n%B"`
   - `git diff --cached --stat`
   - Full staged diff: `git diff --cached`

3. **Draft the message:**
   - The first line must be a concise, descriptive subject line about the change. Never put attribution, disclosure, or other metadata before it.
   - Infer style (tense, capitalization, prefix conventions like `fix:`/`feat:`) from the recent commits above and match it.
   - Include a body only if it adds real information beyond the subject.
   - If another instruction makes attribution or disclosure unavoidable, put it in the body after the subject and a blank line. The subject must remain the first line so short Git history shows the actual change.
   - No emojis, no filler, no restating the diff line-by-line.

4. **Preview.** Output the commit message directly in the ordinary chat response as plain text, in a fenced code block tagged `text`. Do not put the commit message inside a tool call. In the same response, immediately continue to step 5.

5. **Confirm.** Call the question tool: Confirm / Request changes / Cancel. Do not put the commit message in this tool call — it was already shown in step 4. Do not run `git commit` until the user confirms. If they ask for changes, redraft, repeat step 4, then ask again.

6. Once confirmed, commit with the approved message (e.g. `git commit -F -` fed the final message, or `git commit -m`/`-m` flags as appropriate).

7. **Read back the finished commit.** Run `git log -1 --pretty=format:"%B"`, then apply the global "No automatic promotion" rule. Amend only when that rule requires cleanup, keep the approved message otherwise identical, and report any cleanup.

8. **Push only with authorization.** After the finished-commit cleanup settles, honor an active session premise that explicitly authorizes pushing this commit and push without asking again. Otherwise, ask whether to push now through the globally preferred question mechanism. If they decline, stop. If they confirm, push with `git push` if the branch already tracks an upstream, or `git push -u origin <branch>` if it doesn't.
