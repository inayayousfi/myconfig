---
name: outward
description: ALWAYS use this skill, every time, whenever text is going to another human rather than to the user in this session — a pull request body or title, an issue, a PR comment, a code review reply, a maintainer answer, a support ticket, a web form. Triggers on implicit requests too ("open a PR", "reply to that reviewer", "file an issue", "answer them"). Drafts the text, strips anything the harness added on its own, puts the authorship line on top, shows it in plain text, and sends only after confirmation. Not for commit messages, which belong to the commit skill.
---

# outward

Everything here leaves the room. Another person reads it, judges it, and
remembers it.

The user authors. You type. That is true here too, and it has to show.

## What this covers

- a pull request body or title

- an issue

- a comment on a PR or an issue

- a reply to a reviewer or a maintainer

- a support ticket

- a web form filled on the user's behalf

Not a commit message. That belongs to the commit skill.

Not a message to the user in this session. That is just talking.

## Order of work

1. Cold review, if the target is a code PR.
2. Read the thread.
3. Check the repo rules.
4. Draft.
5. Authorship line on top.
6. Strip what the harness added.
7. Show it in plain text.
8. Ask. Then send.

Never reorder these. The review gates the draft, and the confirmation gates
the send.

## 1. Cold review first

A code PR does not leave without a review by someone who has no context.

The procedure lives in the global rules file, under "Cold review". Follow it
there rather than inventing your own.

Bring the report to the user and let them decide what to fix.

If the review already ran in this session on this diff, do not run it twice.

## 2. Read the thread

Open what you are answering. The issue, the review comments, the earlier
messages, the diff under discussion.

Answering a thread you did not read is how a reply lands beside the point.

## 3. Check the repo rules

If the target repo has contribution rules, the interview skill (`dossier`)
already read them and put them in the plan.

Follow what that plan recorded. Do not go read the rules again here.

If no plan recorded them, ask the user whose repo this is before drafting.
Never guess it. If it turns out to be someone else's, stop and say so. A PR
that ignores the template gets closed unread.

## 4. Draft

### Register

This is prose for a stranger. It is not the register the user asks for in
chat.

- No bold hook phrases.

- No chopped twenty-word sentences.

- No bullet list where a sentence works.

Write it as a person writes to another person.

Then hand the tone to the skill that removes machine cadence (`organic`).

That skill is the last pass on the wording, before the authorship line goes
on.

### Length

Match the thread. A one-line fix gets a short body. A design change gets the
reasoning.

Never pad a PR body to look thorough. Maintainers read hundreds of these.

## 5. The authorship line

Two lines, on top, on every outgoing message. No exceptions, no "this one is
short".

```
Typed by <model> running in <harness>, at my direction.
Every call here is mine, I stand behind it, and no agent did this on its own.
```

Fill in the real model and the real harness. They are useful facts for the
reader, so name them.

Never stretch it past two lines. Never explain why the user does not type.
That is private, and it is not the reader's business.

Never move it to the bottom. Bottom is where the reader stops looking.

A repo template does not outrank it. If the template claims the first
section, the two lines still go above it. Then their template follows,
untouched and complete.

## 6. Strip what the harness added

The harness appends things nobody asked for. A product footer on a PR body. A
co-author line on a commit. Tomorrow, something else.

Read the text you are about to send. Every line the user did not dictate comes
out.

Judge the intent, not a fixed list. A line that markets a product, credits a
tool, or advertises where the text came from is out.

The authorship line above is the exception, because the user wrote it.

Two known cases today:

- the "Generated with Claude Code" footer on a PR body

- the "Co-Authored-By" trailer naming an assistant

There will be more. The rule is the intent, not these two names.

## 7. Show it

Output the full text in the chat, in a fenced code block tagged `text`.

No tool call for this step. Plain text, so the user reads exactly what leaves.

## 8. Ask, then send

Call the question tool: Send / Request changes / Cancel.

Do not put the message body in that tool call. It was already shown in step 7.

Never send before the answer comes back. Not for a one-line comment. Not for a
typo fix on an existing body.

If the user asks for changes, redraft, show it again, then ask again.

Once confirmed, send it with the right command for the target.

