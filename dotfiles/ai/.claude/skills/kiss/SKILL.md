---
name: kiss
description: ALWAYS use this skill for any code review request — never the built-in code-review or review skill. Triggers on "review this", "review my code", a diff/PR/module/repo review, or any complexity/over-engineering/consistency question. Checks five equal-weight things in one pass — bugs, over-engineering, pattern drift, naive edge cases, dead weight.
---

# KISS review

Premise: the code works, but may be built wrong. The goal is readable,
maintainable code — every finding should trace back to that, not to taste.

Approach every file assuming something is wrong with it. Guilty until
proven innocent, not the reverse: don't extend the code the benefit of the
doubt because it compiles, passes CI, or looks like someone was careful.
Maximum skepticism, every file, every pass — the default read is "what's
wrong here", not "does anything jump out". If a pass turns up nothing,
that's a fact about the code, not a sign to relax scrutiny on the next one.

Go deep, not wide. A surface pass that skims every file and understands none
is worth nothing. This skill is often the last read before code reaches a
stranger, so take the time a careful human reviewer would take.

Read the surrounding code before calling anything a defect. A line that looks
wrong on its own often matches a convention two files away.

Say when you are guessing. A finding you cannot prove is still worth raising,
but it arrives labelled as a guess, never as a fact.

Five peer checks, same weight, same scrutiny, no category outranks
another. This order is presentation only, not priority:

1. **Bugs** — actual correctness bugs.
2. **Over-engineering** — abstraction, indirection, generality, or config
   the problem never asked for.
3. **Pattern drift** — doesn't follow this codebase's own conventions; a
   newcomer couldn't tell which one is "the" pattern.
4. **Naive edge cases** — clunky, roundabout, or overly defensive handling
   where a simpler construct already covers it.
5. **Dead weight** — unused abstractions, speculative flexibility,
   half-finished generalization, code paths that never fire.

## Step 1: Decide what to review

Resolve the default branch: `git symbolic-ref refs/remotes/origin/HEAD`
(strip the `refs/remotes/origin/` prefix); if that fails, first local
branch found among `main`, `master`, `dev`.

**On the default branch → full-review mode.**
- Target named → review only that target's current on-disk state, not a diff.
- No target → review the whole repo in one pass.

**Off the default branch → diff-review mode.**
- Find the real fork point, not a raw diff against default (stacked
  branches would wrongly pull in the parent branch's unmerged changes):
  try `git merge-base --fork-point <default> HEAD`; if empty/unreliable,
  compute `git merge-base` against default and against every other local
  branch, and use whichever gives the most recent common ancestor.
- Target named → `git diff <forkpoint>...HEAD -- <path>`. No target →
  `git diff <forkpoint>...HEAD`.

## Step 2: Confidence + cross-verify

Every finding (all five checks, no exceptions) gets LOW/MEDIUM/HIGH.
Anything below HIGH gets a second opinion: a fresh subagent running the same
model you are, handed
only the file path and line range plus "something may be off around here,
what do you see?" — no category hint, no first-pass reasoning. Agrees →
upgrade. Disagrees or sees nothing → downgrade. Never drop a finding; ship
it at whatever confidence it lands on.

## Step 3: Verify empirically where possible

Wherever a finding's validity can be checked by running something — tests,
a small repro, a snippet — do it, for any of the five checks, not just
bugs. Never leave artifacts in the repo: delete scratch/temp files after,
or work in `/tmp` to begin with. Confirm clean `git status --porcelain`
before reporting.

## Step 4: Report

Plain markdown in the conversation. No file, no artifact, no report tool —
must be copy-pasteable into a fresh context with nothing else attached.
Report generously; tag confidence honestly instead of suppressing.

Bugs section first, then the four other checks in the order above, each
its own section, most-severe/most-confident first within each.

Each finding: `file:line`, what's wrong, why it's wrong *for this
codebase* (point at the actual convention/example it violates), the
directional fix in words (not a diff — the user may change direction),
confidence tag, and cross-verification result if it ran.

## Step 5: Offer to fix, never start unprompted

Never fix anything yourself. Not even a finding that looks critical. Absolute
rule, not a judgment call.

Offer the fix to whoever asked for this review, then stop. That is the same
move every time, whether the asker is a person or another agent. They decide,
you never act on your own offer.
