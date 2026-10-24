---
name: nice-documentation
description: ALWAYS use this skill before writing, changing, or deciding whether to update any documentation, including instruction files such as AGENTS.md, READMEs, module documents, and code comments. Use it after a code change too, before touching any document that describes the changed area.
---

# Nice documentation

A document serves one reader for one purpose. Its scope decides what it may say, and its kind decides how it says it. A fact that is true but sits outside a document's scope or kind does not belong there, however useful it seems.

## Know the kind

Documentation is either for working or for understanding.

Working documentation tells its reader how to do work: where things go, which rules apply, which mechanism to use, how to verify a result. Instruction files for agents and contributors are working documentation. They do not describe the current behavior of the system: the reader can get it from the code or from the understanding documentation, and a behavior description in a working document becomes false at the next behavior change without anyone noticing.

Understanding documentation explains what exists and why: what a system or module is, how its parts relate, what it does. READMEs, module documents, and code comments are understanding documentation.

When a document mixes both kinds, keep each statement in the kind its document serves. Do not add a statement of the other kind because the document already contains some.

## Keep to the scope

A document's scope is the area it covers: a whole repository, one subsystem, one module, one function.

A document whose scope is broad gives the principles shared across its area and a quick overview of each part. Generalize: state the shared principle once instead of describing how each part applies it. It names parts so that the reader can find them. It does not describe the behavior of one part.

A document whose scope is one part says so explicitly in its opening sentence. Only such a document describes that part's behavior.

When a fact belongs to a narrower scope than the document being edited, put it in the document for that scope. If no such document exists and the fact is needed, say so to the user instead of putting it in the broader one.

## Describe the present

Every document, comment included, describes what is true now. It does not mention what was there before, what changed, what was removed, or what is planned, and it does not compare the present with another time. History belongs to version control. Plans belong to the user's issue tracker or notes.

A constraint caused by the outside world, such as a defect in a dependency that the code works around, is present state, not history.

## Leave examples out

State the principle, the rule, or the behavior directly. Do not add examples to illustrate it. An example earns its place only when it demonstrates something really hard to understand from the statement alone, and that is rare. Examples never make up a large part of a document.

## Update after a change

A code change is not, by itself, a reason to add documentation. After a change, check each document whose scope covers the changed area:

1. Remove or correct any statement that the change made false.

2. Add a statement only when it belongs to that document's kind and scope and its reader needs it.

3. When a document's existing text already breaks these rules, report it to the user instead of adding more text in the same pattern.

## Write each statement plainly

Give a new fact its own sentence. Do not extend an existing sentence with another clause or another list item until it becomes hard to follow. When a paragraph covers several topics, split it by topic.

A code comment states what the code at its place cannot show by itself, usually why it is this way or what external constraint it follows. It does not narrate what the code does line by line, and it does not record how the code came to be.
