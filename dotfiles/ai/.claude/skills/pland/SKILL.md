---
name: pland
description: Write the plan for a non-trivial change as a short artifact. Two-sentence problem, one ASCII diagram above the fold, then facts, judgment calls and pseudo-code. Use in Plan Mode after the grilling interview and before any implementation, whenever a plan needs writing.
---

# pland

Plans are long documents that repeat the code. Nobody reads a long plan.
You skim it. Then you approve something you did not read.

`pland` writes the plan as a diagram plus pseudo-code instead.

Write it into the plan file the harness already gave you. Do not make a new
file. The approval prompt renders that file, so the artifact IS what gets
approved.

Do this yourself, inline. Never dispatch a subagent. Building the map needs
continuous judgment, not a mechanical pass.

## Prose rules

Non-negotiable. They apply to every word outside a code block.

- Subject verb object. No subordinate clauses. No rhetoric.
- Target 10 words per sentence. Hard cap 15.
- **One bullet is one line.** If it needs two lines, it is two bullets.
- **Blank line between list items.** Every list. Packed lists are hard to scan.
- The cap is per line on screen, not per sentence.
- Simplest word that works. Technical terms stay.
- No em dash. Ever.
- Test: readable with your brain off. If not, cut it.

Outside code blocks, name the role first and the identifier after it, in
parentheses. Write "the step that unpacks image layers (`extractTar`)", not
"`extractTar`".

Inside code blocks, drop the parentheses and write plain words. Name a thing
by what it is. Write "pending uploads", not a camelCase identifier.

Never invent an identifier. A made-up name in a plan is noise. The reader
cannot check it, and the real code will not match it.

## Scope

Mechanical, so the size is predictable:

- the changed code
- its direct callers and callees, one level out
- every process boundary the change crosses
- the entry point a user actually touches, always, however far out it sits

That last one is not optional. A map with no way in is useless when you come
back to it cold.

## The shape

Sections in this order. Nothing else.

### Problem

Context on a cold reopen. Not justification. Three to five short sentences,
one idea each. Readable in five seconds. If it is not, cut it.

### Shape

One ASCII diagram over the scope above.

Keep it small. Complexity is error surface. A plain diagram that always
renders beats a clever one that breaks.

Never Mermaid. It shows as raw syntax in a terminal, which is unreadable.

### The fold

Emit a horizontal rule here.

Space above the fold is the scarcest thing in the document. Only the problem
and the diagram earn a place there. If it runs past one screen, cut until it
fits.

### Facts this rests on

Every non-obvious behaviour the design leans on. A protocol, a kernel, a
database, a library.

**One fact, one line.** Two lines means it is two facts.

End each line with the check that proved it, in brackets: `[wc -l]`,
`[git status]`, `[read the SDK source]`.

Never write "from memory". If you did not confirm it, go and confirm it.

### Calls I made

Only decisions the interview did not settle. The ones you took yourself.

Exactly three lines each:

```
1. **The call, stated flat.**
   Why: the reason, one line.
   Against: what you chose against, and what it costs.
```

If there are more than five, stop. The change is too big. Split it.

### Steps

One block per shippable piece, in the order they land.

Each piece gets a heading naming what it delivers, then pseudo-code.

Show everything the piece will write. Data, then behaviour.

Both carry the same weight. Data comes first because behaviour reads it.
That is reading order, not importance. Never trade one for the other.

**Data.** Every structure the piece adds or changes. Name its kind: record,
list, map, set, queue. List every field. Say what each one holds.

**Behaviour.** Every function the piece adds or changes. Say what it takes
in and what it gives back. Then the body.

The body carries control flow, ordering, error branches and state changes.

Write bodies in plain words with code structure. Real words, indented like
code. Never target-language syntax.

Collapse a body to one line only when it is trivial and obvious. A long
boring body still gets written out in full.

The reader authors this code. They sign off on its shape here, not at review.

This is also the breakdown. A reader counts the blocks and sees the real
size of the change. Elision hides that size, so elide almost nothing.

### Verification

Named steps. Never "test it". Say which test, and say what a pass looks like.

Cover every one of these that applies:

- **build**: the exact build and vet command
- **tests**: which command, which package
- **logic**: the case that proves the branch is right
- **real**: the entry point a user actually touches
- **docs**: which design-doc claim this changes, and where it gets fixed
- **clean**: what must NOT have changed

## Revisions

The approval prompt snapshots the file at `ExitPlanMode`. It never watches
the file. So editing alone shows the user nothing.

All three steps are required:

1. Re-enter plan mode. Re-entry returns the same path.
2. Edit the existing plan file in place.
3. Call `ExitPlanMode`. This is the only thing the user sees.

Never write a fresh document. Never a new path. Never a second file.

Change only the lines the feedback touches. Every other line stays
byte-identical. A rewrite forces a full re-read to find what moved.

## Deviations

Implementation will meet reality and lose. Report it when it happens.

Four lines, self-contained, so the user never reopens the plan to judge it:

```
DEVIATION
Approved:  "<verbatim quote of the approved line>"
Reality:   <the fact that broke it>
Instead:   <one line>
Cost:      <one line>
```

Emit it to chat. Then append it under a `## Deviations` heading at the end of
the plan file. Leave the approved pseudo-code untouched above it.

Quote verbatim. A paraphrase drifts from what was approved.

## Durable facts

The plan is deleted. Anything worth keeping goes where durable things already
live.

- Describes current behaviour → the design doc (`DESIGN.md`).
- A gap that still needs a decision → the todo file (`TODO.md`).

Never create a new committed file for this. A per-change design record
duplicates the commit message and rots the moment nobody maintains it.
