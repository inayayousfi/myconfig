---
name: outward
description: ALWAYS use this skill whenever You write or edit text whose primary purpose is to communicate with any person other than the user in this session, even when delivery is implicit or happens later. Do not use it for messages shown only to the user in this session.
---

# outward

Everything here leaves the room. Another person reads it, judges it, and remembers it.

The user authors. You type. That is true here too, and it has to show.

## What this covers

One rule: someone other than the user in this session will read it.

That is the whole test. The channel does not matter, and the subject does not matter either.

Examples, none of them special:

- an email, or a message to a person

- a support ticket, or a web form filled on the user's behalf

- a reply to an administration, a landlord, a supplier

- an issue, a comment, a reply to a reviewer

- a pull request body or title

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

Open what you are answering. The earlier messages, the request, the thread, the document under discussion.

Answering something you did not read is how a reply lands beside the point.

## 2. Know the recipient's rules

Every recipient has expectations about form. A project has a template. A support desk has required fields. An administration has a tone and a reference number.

If a plan already recorded those rules, follow what it says. Do not go looking again.

If no plan recorded them, ask the user who this text goes to, before drafting. Never guess it. Text that ignores the expected form gets ignored back.

## 3. Draft

### Register

This is prose for a stranger. It is not the register the user asks for in chat.

- No bold hook phrases.

- No chopped twenty-word sentences.

- No bullet list where a sentence works.

Write it as a person writes to another person.

You MUST pass every outgoing text through the tone skill (`organic`), always, with no exception. Nothing leaves without that pass. It comes last on the wording, just before the authorship line goes on.

### The reader knows nothing

Whoever opens this has your text and nothing else. No thread, no history, no idea what you meant.

Write for that person. Name the whole thing the first time it appears. Never point back at an exchange they did not see.

Then keep it short, and here is the rule that decides between the two: every sentence carries a new fact. A sentence that only restates the one before it goes.

### Length

Match what you are answering. A short question gets a short answer. A decision that needs justifying gets the reasoning.

Never pad to look thorough. Whoever reads this reads many of them.

## 4. The authorship line

One two-line attribution block goes on top of every outgoing text. No exceptions, no "this one is short".

Write it in the same language as the outgoing text. When the text mixes languages, use its dominant language. Translate the meaning naturally instead of preserving the English words or sentence structure.

```
Written by <handle>, typed by <model> running in <harness>.
Every call here is <handle>'s, and no agent acted on its own.
```

This example defines the meaning, not fixed wording. Fill in the real model and the real harness. They are useful facts for the reader, so name them.

This block is the only authorship credit or model, harness, and agent disclosure in the outgoing text. A recipient, template, or delivery tool asking for another such credit is already satisfied by this block. Never repeat it, move it, or add a paraphrase later in the text.

This rule does not remove necessary technical discussion. Text about how a model, harness, or agent works stays when that subject is the content rather than a credit or disclosure.

The handle is the user's own name on the platform this text is going to.

The handle appears twice, once per line. That is deliberate, and it is not a legal formality. A cold reader who meets "mine" on the second line has nothing to attach it to, and may well attach it to the model. Repeating the handle costs one word and removes the question.

No first-person pronoun stays in these two lines without its referent right there.

Never stretch it past two lines. Never explain why the user does not type. That is private, and it is not the reader's business.

Never move it to the bottom. Bottom is where the reader stops looking.

A recipient's template does not outrank it. If the template claims the first section, the two lines still go above it. Then their template follows, untouched and complete.

### Finding the handle

Look it up. Do not invent one, and do not reuse one from another platform.

- a git repository: read the remote, then the platform's own CLI

- a signed-in session or an account page: read it there

- nothing gives it: ask the user, once, for that platform

The user is one person with many names. The name that belongs on this text is the one the reader can recognise.

## 5. Strip what the harness added

Apply the global "No automatic promotion" rule before showing the draft. Apply it again after sending, when the delivered text can be read back. That global rule owns generic cleanup; do not duplicate its instructions here.

## 6. Show it

Output the full text in the chat, in a fenced code block tagged `text`.

No tool call for this step. Plain text, so the user reads exactly what leaves.

## 7. Ask, then send

Call the question tool: Send / Request changes / Cancel.

Do not put the message body in that tool call. It was already shown in step 6.

Never send before the answer comes back. Not for a one-line comment. Not for a typo fix on something already sent.

If the user asks for changes, redraft, show it again, then ask again.

Once confirmed, send it with the right command for the target.
