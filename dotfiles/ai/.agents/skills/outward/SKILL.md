---
name: outward
description: ALWAYS use this skill, every time, whenever text will be read by someone other than the user in this session. Any channel, any subject, technical or not. Triggers on implicit requests too ("reply to them", "file that", "answer this", "fill this in", "open a PR"). Examples: an email, a support ticket, a web form, a message to a person, an issue, a comment, a reply to a reviewer, a pull request body. Drafts the text, strips anything the harness added on its own, puts the authorship line on top, shows it in plain text, and sends only after confirmation. Not for commit messages, which belong to the commit skill.
---

# outward

Everything here leaves the room. Another person reads it, judges it, and
remembers it.

The user authors. You type. That is true here too, and it has to show.

## What this covers

One rule: someone other than the user in this session will read it.

That is the whole test. The channel does not matter, and the subject does not
matter either.

Examples, none of them special:

- an email, or a message to a person

- a support ticket, or a web form filled on the user's behalf

- a reply to an administration, a landlord, a supplier

- an issue, a comment, a reply to a reviewer

- a pull request body or title

Not a commit message. That belongs to the commit skill.

Not a message to the user in this session. That is just talking.

## Order of work

1. Read the context.
2. Know the recipient's rules.
3. Draft.
4. Authorship line on top.
5. Strip what the harness added.
6. Show it in plain text.
7. Ask. Then send.

Never reorder these. The confirmation gates the send, and nothing else does.

## 1. Read the context

Open what you are answering. The earlier messages, the request, the thread,
the document under discussion.

Answering something you did not read is how a reply lands beside the point.

## 2. Know the recipient's rules

Every recipient has expectations about form. A project has a template. A
support desk has required fields. An administration has a tone and a reference
number.

If a plan already recorded those rules, follow what it says. Do not go looking
again.

If no plan recorded them, ask the user who this text goes to, before drafting.
Never guess it. Text that ignores the expected form gets ignored back.

## 3. Draft

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

Match what you are answering. A short question gets a short answer. A decision
that needs justifying gets the reasoning.

Never pad to look thorough. Whoever reads this reads many of them.

## 4. The authorship line

Two lines, on top, on every outgoing text. No exceptions, no "this one is
short".

```
Typed by <model> running in <harness>, under the direction of <handle>.
Every call here is mine, I stand behind it, and no agent did this on its own.
```

Fill in the real model and the real harness. They are useful facts for the
reader, so name them.

The handle is the user's own name on the platform this text is going to.

Never stretch it past two lines. Never explain why the user does not type.
That is private, and it is not the reader's business.

Never move it to the bottom. Bottom is where the reader stops looking.

A recipient's template does not outrank it. If the template claims the first
section, the two lines still go above it. Then their template follows,
untouched and complete.

### Finding the handle

Look it up. Do not invent one, and do not reuse one from another platform.

- a git repository: read the remote, then the platform's own CLI

- a signed-in session or an account page: read it there

- nothing gives it: ask the user, once, for that platform

The user is one person with many names. The name that belongs on this text is
the one the reader can recognise.

## 5. Strip what the harness added

The harness appends things nobody asked for. A product footer at the end. A
credit line. Tomorrow, something else.

Read the text you are about to send. Every line the user did not dictate comes
out.

Judge the intent, not a fixed list. A line that markets a product, credits a
tool, or advertises where the text came from is out.

The authorship line above is the exception, because the user wrote it.

Two known cases today:

- the "Generated with Claude Code" footer

- the "Co-Authored-By" trailer naming an assistant

There will be more. The rule is the intent, not these two names.

## 6. Show it

Output the full text in the chat, in a fenced code block tagged `text`.

No tool call for this step. Plain text, so the user reads exactly what leaves.

## 7. Ask, then send

Call the question tool: Send / Request changes / Cancel.

Do not put the message body in that tool call. It was already shown in step 6.

Never send before the answer comes back. Not for a one-line comment. Not for a
typo fix on something already sent.

If the user asks for changes, redraft, show it again, then ask again.

Once confirmed, send it with the right command for the target.
