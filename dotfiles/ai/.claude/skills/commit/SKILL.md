---
name: commit
description: ALWAYS use this skill, every time, whenever the user asks to commit staged changes, wants a commit made, or asks to generate/write/draft a commit message — including implicit requests like "commit this" or "save this work" — not only on literal /commit. Drafts a commit message from the staged diff, commits it, then reviews the resulting commit's Co-authored-by trailer for whether it names a real human vs. a bot/agent/company and amends it out if not.
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

4. **Preview — mandatory, own message, before the question.** Send a plain-text
   reply containing nothing but the full drafted commit message in a fenced code
   block. This has to be its own conversational message the user can read directly —
   not a tool-call argument, not a question-tool preview/option field, not folded into
   the question in step 5. Do not skip this step or merge it into step 5 for any
   reason, even if it feels redundant with the question that follows.

5. **Confirm.** Only after step 4's message has been sent, ask via the question tool:
   confirm, request changes, or cancel. Do not run `git commit` until the user
   explicitly confirms. If they ask for changes, redraft, repeat step 4 with the
   updated message, then ask again via the question tool.

6. Once confirmed, commit with the approved message (e.g. `git commit -F -` fed the
   final message, or `git commit -m`/`-m` flags as appropriate).

7. **Co-author review — after the commit exists, judgment call, not a fixed list.**
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

8. **Propose pushing.** After the co-author review settles, ask via the question tool
   whether to push the commit now. If they decline, stop. If they confirm, push with
   `git push` if the branch already tracks an upstream, or `git push -u origin <branch>`
   if it doesn't. Never push without this explicit confirmation, even if a push was
   confirmed earlier in the conversation — ask fresh every time.
