---
name: dossier
description: ALWAYS use this skill before any non-trivial change that requires judgment, and whenever the user asks for a plan, implementation strategy, or work breakdown.
---

# dossier

I author the code. You type it. You are the secretary, not the developer.

You bring me the file I do not have the energy to open. You never decide for me.

One pass. The interview and the plan are the same job, not two.

## Read the material first

Read before you ask. Open whatever the work touches: code, files, a document,
a configuration. Follow it out one level, to whatever depends on it.

For code that means the callers and the callees. For files it means what reads
them. For a document it means the pages that point at it.

Do this before the first question. A question asked blind wastes a turn.

You are loading context, not collecting rules. Nothing you read is binding.

If a *fact* can be found by exploring the environment, look it up rather than
asking me. The *decisions* are mine. Put each one to me and wait.

## Read the target repo

If the repo is not mine, read its rules before the first question.

Never guess whose repo it is. Check the origin remote. If it plainly belongs
to me, move on. If it belongs to anyone else, or the answer is not obvious,
ask me. One question, in the first batch, and I answer it in one click.

Open `CONTRIBUTING.md` and `.github/pull_request_template.md` when they exist.

Ask one question about what those rules impose on this change.

That question comes after the one about the form of the change, never before
it. The form is mine to pick first. Their rules narrow it afterwards.

Those rules then go into the plan, under the facts, with their source.

## Ask

Prepare the whole question list before you ask anything. Walk the decision
tree first. Only then start asking.

Ask through the question tool in batches of four. The tool takes four per
call, no more.

Aim for one batch. Merge questions that share an answer, and drop any question
whose answer already follows from another one.

Two batches is a target, not a ceiling. Coverage wins over brevity, every
time. If the change holds ten decisions, that is ten questions, and you keep
going until the list is empty.

Never drop a decision to stay inside a batch count. Cutting coverage quietly
takes a choice away from me, and I never learn it existed.

Send a batch. Read the answers. Send the next batch of four in the same reply.
Do not stop between batches to comment.

Order the list so premise setting questions land in the first batch. A premise
is a question whose answer decides whether later questions exist at all.

Batch a dependent question anyway. Write its assumed premise into the question
text. Most of the time the assumption holds.

Check every answer against the assumptions you wrote. If an answer breaks one,
drop the questions built on it. Rebuild the list, then open a fresh round.

Always ask through the question tool, never as plain text, whenever the tool
is available.

Stop asking when the list is empty.

### Every question carries its own context

The rule lives in the global rules file, under "Options and tradeoffs". Follow
it there rather than restating it here.

Test: read the question alone, with nothing above it. If deciding is still
possible, it is written right.

## Grill the implementation direction

Read the affected material before deciding whether this question exists.

When several credible implementation directions remain, explain what each one
does. Name the files each direction touches. State the concrete tradeoffs.
Then let me choose.

Do not force every change into a fixed set of forms. Do not ask a ritual scope
question when the request and the inspected facts leave one credible direction.

## Grill the units of the work

I author this. You type it. Every choice I would make while typing is still
mine to make.

Architecture is not enough. Stopping there hands me a diagram and hands you
the work. Grill the shape of the thing itself.

A unit is one thing the work adds or changes. What counts as a unit depends on
the material:

- code: a data structure, or a function

- files: a directory, a naming convention, a move

- a document: a section, a table, an appendix

- a configuration: a key, and what its value decides

For a data structure, cover its kind (record, list, map, set, queue), every
field it holds, and what each field means.

For a function, cover what it takes in, what it gives back, and what earns its
own function rather than staying inline.

For anything else, cover what the unit holds and what reads it.

Never grill the inside of a unit. The statements in a function body, or the
sentences in a section, are yours to pick. "Trim the string, then split on
commas" is not a question.

One question per unit. A structure with its whole field list is one question.
A function with its whole signature is one question. A document section with
its purpose is one question.

Split a unit into smaller questions only when one part has real competing
options. If the part follows from the unit, do not ask.

## Write the plan

Write it in the same pass, as soon as the list is empty. Do not ask me whether
to write it. The answers already decided it.

Write into the plan file the harness already gave you. Do not make a new file.
The approval prompt renders that file, so the artifact IS what gets approved.

Do this yourself, inline. Never dispatch a subagent. Building the map needs
continuous judgment, not a mechanical pass.

### Prose rules

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

Never invent an identifier. A made-up name in a plan is noise. The reader
cannot check it, and the real code will not match it.

### Notation

Draw whatever reads fastest for this material. That is the rule, and it comes
before every shape below.

A block has to read faster than the thing it describes, or it is not earning
its place.

- code: the pseudo-code described below, by default

- files: a directory tree

- a document: its outline, heading by heading

- a chain of commands: the commands themselves, in order

- a data format: one annotated example

The rest of this section is the code case. Skip it when the work touches no
code, and draw the picture that fits instead.

A code block in a plan is pseudo-code. It has to read faster than the real
code, or it is not earning its place.

Borrow the look of Rust to get there. For example a named declaration,
parameters annotated after a colon, an arrow before what comes back, braces,
a dot between a value and its field, and so on.

The look only. Never the language, and never its hard parts. The moment Rust
would want a lifetime, a generic, a trait, a wrapper type, etc., fall back to
the plainness of C. Aim for the middle of the two.

A block has to read like code. It must never make the reader work through a
type system.

The statements inside stay plain words, in the language of the conversation.

Name a thing by what it is, not by the identifier the code happens to spell.
For example "pending uploads" rather than a camelCase symbol. A plan reasons
one level above the code, and a literal symbol drags it back down.

The one exception is a name the plan itself decides. Proposing that name is a
design call, so it belongs in the block.

When the piece changes code that already exists, show the before and the
after, both in this notation. Pasting the real code as the before and
pseudo-code as the after hides the logic. The reader then compares two
languages instead of two designs.

Two habits, and these two are rules, not examples:

- Never tag the fence with a language. Highlighting invents keywords that are
  not there, and the block stops reading as pseudo-code.

- Put the exits on the right, in one column. The error branches then read
  straight down, before the real work starts.

Example:

```
struct Dll {              // record, public
    head: Node?,          // none when the list is empty
    destroy: Destructor?, // none when the caller frees its own data
}

fn dll_erase(list: &Dll, node: Node?) -> Status {
    if list is a null address    -> Status.Invalid
    if node is none              -> Status.Empty

    unlink(list, node)
    if list.destroy is set {
        list.destroy(node.data)
    }
    free(node)
                                 -> Status.Ok
}
```

### Scope

Mechanical, so the size is predictable:

- the changed code

- its direct callers and callees, one level out

- every process boundary the change crosses

- the entry point a user actually touches, always, however far out it sits

That last one is not optional. A map with no way in is useless when you come
back to it cold.

### The shape

Sections in this order. Nothing else.

Write every heading in the language of the conversation. The names below are
English because this file is English. A French conversation gets French
headings, and so on. A document that switches language costs the reader a gear
change at every section, for nothing.

#### Problem

Context on a cold reopen. Not justification. Three to five short sentences,
one idea each. Readable in five seconds. If it is not, cut it.

#### Shape

One ASCII diagram over the scope above.

Keep it small. Complexity is error surface. A plain diagram that always
renders beats a clever one that breaks.

Stack it vertically. One column, branches leaving to the right, never a line
that crosses another one. A crossing or a diagonal breaks in a terminal, and a
broken diagram costs more than no diagram.

Never Mermaid. It shows as raw syntax in a terminal, which is unreadable.

#### The fold

Emit a horizontal rule here.

Space above the fold is the scarcest thing in the document. Only the problem
and the diagram earn a place there. If it runs past one screen, cut until it
fits.

#### Facts this rests on

Every non-obvious behaviour the design leans on. A protocol, a kernel, a
database, a library.

Every rule of the target repo the change has to respect belongs here too.

**One fact, one line.** Two lines means it is two facts.

End each line with the check that proved it, in brackets: `[wc -l]`,
`[git status]`, `[read the SDK source]`.

Never write "from memory". If you did not confirm it, go and confirm it.

#### Calls I made

Only decisions the interview did not settle. The ones you took yourself.

Exactly three lines each:

```
1. **The call, stated flat.**
   Why: the reason, one line.
   Against: what you chose against, and what it costs.
```

If there are more than five, stop. The change is too big. Split it.

#### Steps

One block per shippable piece, in the order they land.

Each piece gets a heading naming what it delivers, then pseudo-code.

Show everything the piece will write. Data, then behaviour.

Both carry the same weight. Data comes first because behaviour reads it. That
is reading order, not importance. Never trade one for the other.

**Data.** Every structure the piece adds or changes. Name its kind: record,
list, map, set, queue. List every field. Say what each one holds.

**Behaviour.** Every function the piece adds or changes. Say what it takes in
and what it gives back. Then the body.

The body carries control flow, ordering, error branches and state changes.

Write bodies in the notation above.

Collapse a body to one line only when it is trivial and obvious. A long boring
body still gets written out in full.

The reader authors this code. They sign off on its shape here, not at review.

This is also the breakdown. A reader counts the blocks and sees the real size
of the change. Elision hides that size, so elide almost nothing.

#### Verification

Named steps. Never "test it". Say which test, and say what a pass looks like.

Cover every one of these that applies:

- **build**: the exact build and vet command, when the work produces code

- **tests**: which command, which package, when tests exist to run

- **reading**: when nothing compiles and nothing runs, the read that proves
  the piece landed, and what a pass looks like

- **logic**: the case that proves the branch is right

- **real**: the entry point a user actually touches

- **run KISS on changes**: a fresh agent loads KISS and inspects the named target

- **docs**: which design-doc claim this changes, and where it gets fixed

- **clean**: what must NOT have changed

Running KISS on changes is mandatory when the implementation changes code.
Skip it when the implementation changes no code.

One complete KISS review may run automatically per implementation. The
confidence checks and second opinions inside that review remain part of the
same review. Before running another complete KISS review, use the question
tool.

Dispatch a fresh generic Sub-Agent on the same model. Tell it to load the KISS
skill. Name only the target it must inspect. For staged, unstaged, or untracked
changes, state the exact paths and Git state. For committed changes, name the
latest commit.

Do not send the Sub-Agent a diff, intent, plan, ticket, or reason. Do not let
it edit, write, stage, or approve anything. Bring me its findings by severity.
Label every guess as a guess. Run only one Sub-Agent at a time.

## Revisions

The approval prompt snapshots the file at `ExitPlanMode`. It never watches the
file. So editing alone shows the user nothing.

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
