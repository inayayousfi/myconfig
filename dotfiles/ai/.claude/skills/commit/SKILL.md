---
name: commit
description: Use when the user asks to commit staged changes, generate/write a commit message, or types /commit. Drafts a commit message from the staged diff, commits it, then reviews the resulting commit's Co-authored-by trailer for whether it names a real human vs. a bot/agent/company and amends it out if not.
---

# Commit

Drafts a commit message for the currently staged changes and commits only after the
user confirms it.

## Steps

1. **Check there's something to commit.** Run `git diff --cached --stat`. If it's
   empty, tell the user to `git add` the files they want to commit first, and stop.

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

4. **Preview and confirm.** Put the full drafted commit message directly into the
   conversation, as plain text the user can actually read (not just a tool-call
   argument or a UI widget they might not be able to see) — e.g. in a fenced code
   block in your reply. Ask them to confirm, request changes, or cancel. Do not run
   `git commit` until the user explicitly confirms the message. If they ask for
   changes, redraft and show the updated message again in the same way.

5. Once confirmed, commit with the approved message (e.g. `git commit -F -` fed the
   final message, or `git commit -m`/`-m` flags as appropriate).

6. **Co-author review — after the commit exists, judgment call, not a fixed list.**
   Run `git log -1 --pretty=format:"%B"` on the commit you just made. If it carries a
   `Co-authored-by:` trailer (including any default trailer this session would
   normally append, e.g. crediting the AI assistant itself), look at the name and
   email and decide: does this identify one specific real human being? If yes, leave
   the commit as-is. If it instead names a company, a bot account, an AI
   agent/assistant, a generic no-reply address, or anything that isn't a specific
   person, amend the commit to strip that trailer line (`git commit --amend` with the
   trailer removed from the message, keeping everything else identical). Reason about
   this each time rather than pattern-matching a hardcoded blocklist; borderline cases
   (e.g. a human's work email that looks automated) should resolve in favor of keeping
   a trailer only when you're confident it's a real person.
