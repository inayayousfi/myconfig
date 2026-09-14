---
name: commit
description: ALWAYS use this skill when the user asks to create a Git commit or draft a commit message, including implicit requests whose intended result is a commit. Do not use it for other outgoing text.
---

# Commit

Drafts the intended result early when a request includes future work, establishes the actual change set, and commits only after the user confirms the final message.

## Steps

1. **Draft the intent early when work is still pending.** When the user's request combines future task work with a commit, before changing state, draft a provisional commit message from the requested outcome and show it in ordinary chat in a fenced code block tagged `text`. Keep it focused on the intended result and do not invent implementation details. State that it is provisional, do not ask the user to approve it yet, and do not treat it as the final message. Skip this step when the request only commits changes that already exist.

2. **Establish what "this" means.** Inspect `git diff --cached --stat` and `git status --short`. If there are no changes, tell the user there's nothing to commit and stop. Honor any staging scope the user already named. When the current session just produced a task's changes, "commit this" refers to those changes: stage them directly, using `git add -A` when they are the only worktree changes or exact paths or patches when unrelated work also exists. In a fresh session with no task-produced change set, inspect the coherent staging scopes and ask the user to choose. If existing staged work or overlapping edits cannot be safely separated from the intended changes, emit a global deviation instead of committing unrelated work.

3. **Gather context:**
   - Current branch: `git branch --show-current`
   - Recent commit style: `git log -10 --pretty=format:"%h%n%B"`
   - `git diff --cached --stat`
   - Full staged diff: `git diff --cached`

4. **Draft the final message:**
   - The first line must be a concise, descriptive subject line about the change. Never put attribution, disclosure, or other metadata before it.
   - Infer style (tense, capitalization, prefix conventions like `fix:`/`feat:`) from the recent commits above and match it.
   - Include a body only if it adds real information beyond the subject.
   - If another instruction makes attribution or disclosure unavoidable, put it in the body after the subject and a blank line. The subject must remain the first line so short Git history shows the actual change.
   - Reconcile any provisional message with the actual staged changes. Preserve its outcome focus when it remains accurate, but change it when the real change set requires that.
   - No emojis, no filler, no restating the diff line-by-line.

5. **Preview the final message.** Output the commit message directly in the ordinary chat response as plain text, in a fenced code block tagged `text`. Do not put the commit message inside a tool call. In the same response, immediately continue to step 6.

6. **Confirm once.** Call the question tool: Confirm / Request changes / Cancel. Do not put the commit message in this tool call; it was already shown in step 5. Do not run `git commit` until the user confirms the final message. If they ask for changes, redraft, repeat step 5, then ask again. The provisional message from step 1 never counts as this confirmation.

7. Once confirmed, commit with the approved message (e.g. `git commit -F -` fed the final message, or `git commit -m`/`-m` flags as appropriate).

8. **Read back the finished commit.** Run `git log -1 --pretty=format:"%B"`, then apply the global "No automatic promotion" rule. Amend only when that rule requires cleanup, keep the approved message otherwise identical, and report any cleanup.

9. **Push only with authorization.** After the finished-commit cleanup settles, honor an active session premise that explicitly authorizes pushing this commit and push without asking again. Otherwise, ask whether to push now through the globally preferred question mechanism. If they decline, stop. If they confirm, push with `git push` if the branch already tracks an upstream, or `git push -u origin <branch>` if it doesn't.
