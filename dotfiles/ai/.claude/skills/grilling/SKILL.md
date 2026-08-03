---
name: grilling
description: Grill the user relentlessly about a plan, decision, or idea. Use when the user wants to stress-test their thinking, or uses any 'grill' trigger phrases.
---

Interview me relentlessly about every aspect of this until we reach a shared understanding. Walk down each branch of the decision tree, resolving dependencies between decisions one-by-one. For each question, provide your recommended answer.

Ask the questions one at a time, waiting for feedback on each question before continuing. Asking multiple questions at once is bewildering.

Always ask each question through the question tool (structured, clickable options) rather than typing it out as plain text, whenever the tool is available.

If a *fact* can be found by exploring the environment (filesystem, tools, etc.), look it up rather than asking me. The *decisions*, though, are mine — put each one to me and wait for my answer.

Do not act on it until I confirm we have reached a shared understanding.

## Read the code first

Read before you ask. Open the code the change will touch. Follow it out one
level, to its callers and to what it calls.

Do this before the first question. A question asked blind wastes a turn.

You are loading context, not collecting rules. Nothing you read is binding.

## Grill the code shape too

I author the code. You type it. Every choice I would make while typing is
still mine to make.

Architecture is not enough. Stopping there hands me a diagram and hands you
the code. Grill the shape of the code itself.

Cover every one of these:

- every data structure the change adds or changes

- the kind of each one: record, list, map, set, queue

- every field it holds, and what that field means

- every function the change adds or changes

- what each one takes in, and what it gives back

- what earns its own function, and what stays inline

Never grill a function body. The statements inside are yours to pick. "Trim
the string, then split on commas" is not a question.

One question per unit. A structure with its whole field list is one question.
A function with its whole signature is one question.

Split a unit into per-field questions only when one field has real competing
options. If the field follows from the unit, do not ask.
