---
name: kiss
description: ALWAYS use this skill for any code review request, including questions about complexity, over-engineering, consistency, edge cases, or dead code. Never use another review skill for those requests.
---

# KISS review

Premise: the code works, but may be built wrong. The primary goal is readable, maintainable code. Correctness matters, but ordinary bugs never substitute for judging the design. Every finding should trace back to evidence, not taste.

Approach every file assuming something is wrong with it. Guilty until proven innocent, not the reverse: don't extend the code the benefit of the doubt because it compiles, passes CI, or looks like someone was careful. Maximum skepticism, every file, every pass — the default read is "what's wrong here", not "does anything jump out". If a pass turns up nothing, that's a fact about the code, not a sign to relax scrutiny on the next one.

Go deep, not wide. A surface pass that skims every file and understands none is worth nothing. This skill is often the last read before code reaches a stranger, so take the time a careful human reviewer would take.

Read the surrounding code before calling anything a defect. A line that looks wrong on its own often matches a convention two files away.

Say when you are guessing. A finding you cannot prove is still worth raising, but it arrives labelled as a guess, never as a fact.

Six checks receive the same scrutiny, but design comes first:

1. **Over-engineering** — abstraction, indirection, generality, config the problem never asked for, poor cohesion, or a unit without one clear purpose.
2. **Pattern drift** — doesn't follow this codebase's conventions for names, vocabulary, control flow, visible data flow, comments or structure; a newcomer couldn't tell which one is "the" pattern.
3. **Failure surface** — design-created invalid states, stale facts, partial updates, ownership ambiguity, ordering constraints, reentrancy hazards, cleanup paths or configuration combinations that a smaller shape makes impossible.
4. **Naive edge cases** — clunky, roundabout, or overly defensive local handling where a simpler construct already covers it.
5. **Dead weight** — unused abstractions, speculative flexibility, half-finished generalization, code paths that never fire.
6. **Bugs** — actual correctness bugs.

A critical bug may interrupt and lead the review only when it threatens security, data integrity or availability. Finish the whole-design pass before verifying ordinary bugs. Never let a concrete bug end structural analysis.

## Step 1: Decide what to review

Honor the target exactly as the caller names it before applying any branch default. A target may describe changes or name current files, modules or the whole repository. Derive the target from the repository state yourself; never require the caller to construct or paste a diff. When the caller limits their changes to named paths, never broaden the review beyond those paths. Do not try to attribute individual lines within a named path to different sessions; any overlap inside that path belongs to the review.

If the caller names staged, unstaged or untracked changes, they must also name the exact paths. Inspect that state regardless of the current branch. If the caller names committed changes, review the latest commit. If the caller names current files or modules, inspect their complete on-disk state. Use the branch defaults below only when the caller gives no more precise target.

Resolve the default branch: `git symbolic-ref refs/remotes/origin/HEAD` (strip the `refs/remotes/origin/` prefix); if that fails, first local branch found among `main`, `master`, `dev`.

**On the default branch → full-review mode.**

- Review the whole repo in one pass.

**Off the default branch → diff-review mode.**

- Find the real fork point, not a raw diff against default (stacked branches would wrongly pull in the parent branch's unmerged changes): try `git merge-base --fork-point <default> HEAD`; if empty or unreliable, compute `git merge-base` against default and against every other local branch, and use whichever gives the most recent common ancestor.
- Review `git diff <forkpoint>...HEAD`.

Before calling anything a defect, discover how this repository expresses its style. Inspect nearby code. When a credible one exists, find a similarly sized module in the same architectural layer that does something different; compare nonblank code lines when choosing it. Use that module only as supplemental calibration; never let its size outweigh relevance or stronger evidence. Read any written repository style guidance. Search the target directory first, then its architectural layer, then the repository.

Use the strongest credible source as evidence. If written guidance conflicts with repeated repository code, report that conflict instead of silently choosing either side. Code outside the named target is evidence only, never an additional source of findings.

## Step 2: Build the simpler counterfactual

Complete this pass before line-level review. Produce four things, even when the current design is sound:

1. **Essential behavior** — what the change must accomplish, stripped of its implementation.
2. **Actual path** — where data, ownership and control enter, move and leave.
3. **Smallest credible shape** — the fewest cohesive units that satisfy the same requirements and repository constraints.
4. **Concrete reduction** — which types, boundaries, states, registrations, branches, failure modes or cleanup paths that shape removes.

Treat elegance as an engineering property, not an aesthetic verdict. An elegant shape has direct visible flow, one authoritative owner for each fact and resource, few representable states, and no machinery without current work. Prefer a design that makes an invalid state or edge case impossible over one that detects, synchronizes or cleans it up correctly.

The counterfactual is evidence, not an automatic verdict. Do not penalize machinery required by observed requirements, failure semantics, performance, compatibility or repository convention. Do not propose a toy that silently drops those constraints.

Inspect the complete shape for these signals:

- an abstraction with one consumer or one implementation
- generic machinery followed by a special-case branch
- duplicated state, facts or representations
- ownership crossing a boundary without buying isolation
- one fact that callers must retrieve and pass back to its owner
- setup, registration or cleanup outweighing the behavior
- test scaffolding outweighing the behavior under test
- names repeating file or type context instead of adding meaning
- error handling created only by an earlier abstraction
- configuration with no current variation
- direct control flow hidden behind dispatch or registration
- a smaller shape that removes states, sequencing rules or failure paths

Measure where possible. Compare nonblank lines, ownership transfers, representations of one fact, public units, implementations per interface, setup paths per behavior, and failure paths before and after. Counts support judgment; they never replace it.

## Step 3: Run every check

Run all five design checks against the counterfactual before the bug check. Do not stop because one category produced findings. A clean category is still a completed internal pass.

Use **Failure surface** only for risks created by the chosen architecture and removed by the smaller shape. Use **Naive edge cases** for roundabout local handling inside an otherwise credible shape. Do not repeat one concern under both headings.

Then inspect correctness. Handle a critical bug immediately. Record ordinary bugs for the final section and continue structural work first.

## Step 4: Confidence + cross-verify

Every finding gets LOW, MEDIUM or HIGH confidence. Anything below HIGH gets a second opinion: a fresh subagent running the same model you are, handed only the file path and line range plus "something may be off around here, what do you see?" — no category hint, no first-pass reasoning. Agrees → upgrade. Disagrees or sees nothing → downgrade. Never drop a finding; ship it at whatever confidence it lands on.

## Step 5: Verify empirically where possible

Wherever a finding's validity can be checked by running something — tests, a small repro, a snippet — do it for any check, not just bugs. Never leave artifacts in the repo: delete scratch or temporary files after, or work in `/tmp` to begin with.

## Step 6: Report

Use plain markdown in the conversation. Do not create a file, artifact or report tool output. The result must be copy-pasteable into a fresh context with nothing else attached. Report generously; tag confidence honestly instead of suppressing.

Critical bugs lead only when present. Always report a **Whole design** section containing the four counterfactual items and a clear verdict. Then report only categories that contain at least one finding, in this order: Over-engineering, Pattern drift, Failure surface, Naive edge cases, Dead weight, ordinary Bugs. Never emit an empty category heading or a `No findings` placeholder. If no category contains a finding, the Whole design verdict is the complete review.

Each finding must include `file:line`, what is wrong, why it is wrong for this codebase, the directional fix in words, a confidence tag, and the cross-verification result if one ran. Point at the actual convention or example it violates.

Each structural finding must name the smaller shape and the concrete machinery it removes. A complaint without that reduction is taste, not a KISS finding. When the reduction removes invalid states, edge cases or failure paths, name those removals explicitly.

For pattern-drift findings, cite the strongest credible repository evidence. Keep uncertain readability concerns and label their confidence honestly.

Do not repeat one finding merely to fill several categories. Put it under the category that best explains the mechanism. Cross-reference another consequence only when it changes the recommended decision.

## Step 7: Offer to fix, never start unprompted

Never fix anything yourself. Not even a finding that looks critical. Absolute rule, not a judgment call.

Offer the fix to whoever asked for this review, then stop. That is the same move every time, whether the asker is a person or another agent. They decide; you never act on your own offer.
