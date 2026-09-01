---
name: outward
description: ALWAYS use this skill whenever You write or edit text whose primary purpose is to communicate with any person other than the user in this session, even when delivery is implicit or happens later. Do not use it for messages shown only to the user in this session.
---

# outward

Everything here leaves the room. Another person reads it, judges it, and remembers it.

The user directs the work and makes the decisions. You write and carry them out. That is true here too, and it has to show.

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
4. Collaboration block on top unless the user removed it.
5. Strip what the harness added.
6. Show it in plain text.
7. Approve wording. Then send when authorized.

Never reorder these. After recipient rules and placement are settled, approved wording and active send authorization are the final gates on sending.

## 1. Read the context

Open what you are answering. The earlier messages, the request, the thread, the document under discussion.

Answering something you did not read is how a reply lands beside the point.

## 2. Know the recipient's rules

Every recipient has expectations about form. A project has a template. A support desk has required fields. An administration has a tone and a reference number.

If a plan already recorded those rules, follow what it says. Do not go looking again.

If no plan recorded them, inspect the available destination, thread, template, and recipient evidence. Ask the user only when that inspection does not establish who the text goes to or which form it requires. Never guess it. Text that ignores the expected form gets ignored back.

## 3. Draft

### Register

This is prose for a stranger. It is not the register the user asks for in chat.

- No bold hook phrases.

- No chopped twenty-word sentences.

- No bullet list where a sentence works.

Write it as a person writes to another person.

You MUST pass every outgoing text through the tone skill (`organic`), always, with no exception. Nothing leaves without that pass. It comes last on the wording, just before the collaboration block goes on.

### Use only delivered context

Whoever opens this has the text and any context the destination delivers with it. They do not have the private agent session.

Write for that person. A reply may rely on the thread they can see. Name anything that neither the text nor its delivered context establishes. Never point back at an exchange they cannot see.

Then keep it short, and here is the rule that decides between the two: every sentence carries a new fact. A sentence that only restates the one before it goes.

### Length

Match what you are answering. A short question gets a short answer. A decision that needs justifying gets the reasoning.

Never pad to look thorough. Whoever reads this reads many of them.

## 4. The collaboration block

One two-line attribution block goes on top of every outgoing text unless an active session premise explicitly removes it. The user does not need to request it.

Write it in the same language as the outgoing text. When the text mixes languages, use its dominant language. Translate the meaning naturally instead of preserving the English words or sentence structure.

```
<handle> directed the work and made every decision.
<model>, running in <harness>, carried <handle>'s decisions out.
```

This example defines the meaning, not fixed wording. Fill in the real handle, model, and harness. They are useful facts for the reader, so name them.

This block is the default collaboration attribution. It does not automatically satisfy an exact disclosure required by the recipient, template, or delivery tool. Include target-required wording in its required form and location. Do not add another generic collaboration credit or paraphrase elsewhere.

This rule does not remove necessary technical discussion. Text about how a model, harness, or agent works stays when that subject is the content rather than a credit or disclosure.

The handle is the user's own name on the platform this text is going to.

The handle appears twice, once per line. That is deliberate, and it is not a legal formality. Repeating the handle makes clear whose decisions the model carried out.

No first-person pronoun stays in these two lines without its referent right there.

Never stretch it past two lines. Never explain why the user does not type. That is private, and it is not the reader's business.

Keep it at the top unless the destination has a functional first-position requirement. A commit subject stays first, with this block at the start of the body. Keep exact target-required content in its required location. If those placement rules are incompatible, return the conflict to the global decision tree.

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

## 7. Approve wording, then send when authorized

If the user requested a draft only, stop after showing it. Do not introduce sending as another choice.

The exact wording must be approved unless an active session premise delegates that choice. When the wording remains unresolved, ask through the globally preferred question mechanism: Approve wording / Request changes / Cancel. Do not put the message body in that question. It was already shown in step 6.

If the user asks for changes, redraft, show it again, then ask again.

Once the wording is approved or delegated, honor any active session premise that authorizes sending. Send directly when that authorization already exists. Otherwise ask through the globally preferred question mechanism: Send / Cancel. Once authorized, send with the right command for the target.
