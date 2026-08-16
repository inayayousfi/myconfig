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

7. **Co-author review — after the commit exists, judgment call, not a fixed list.**
   Run `git log -1 --pretty=format:"%B"` on the commit you just made. If it carries a
   `Co-authored-by:` trailer, look at the name and email and decide: does this
   identify one specific real human being? If yes, leave
   the commit as-is. If it instead names a company, a bot account, an AI
   agent/assistant, a generic no-reply address, or anything that isn't a specific
   person, amend the commit to strip that trailer line (`git commit --amend` with the
   trailer removed from the message, keeping everything else identical). Reason about
   this each time rather than pattern-matching a hardcoded blocklist; borderline cases
   (e.g. a human's work email that looks automated) should resolve in favor of keeping
   a trailer only when you're confident it's a real person.

   This is also where the harness's own trailer gets caught. It is appended during
   `git commit`, so it does not exist when you draft at step 3 and it cannot be
   stripped before the fact. Read the finished commit here, and amend out every line
   you did not write, per the "No automatic promotion" rule in the global rules file.
   Then say you did it.

8. **Propose pushing — always the question tool, no exceptions.** After the
   co-author review settles, ask whether to push the commit now via the question
   tool — never a plain-text question, never skipped, never inferred from context.
   This is an absolute rule, same as the message confirmation in step 5: it applies
   every single time this step is reached, regardless of how routine or low-stakes
   the commit seems. If they decline, stop. If they confirm, push with `git push` if
   the branch already tracks an upstream, or `git push -u origin <branch>` if it
   doesn't. Never push without this explicit confirmation, even if a push was
   confirmed earlier in the conversation — ask fresh every time.
