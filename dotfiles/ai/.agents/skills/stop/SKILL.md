---
name: stop
description: Use when deciding whether to keep going, wait, come back to me, or end, especially in long or open-ended work. That covers jobs that run for hours or days, watching a pull request, CI or a server ("babysit", "death loop", "until it's merged", "keep going", "don't come back until"), and any loop of improving or reviewing.
---

# stop

Work ends when its done holds, checked against the real state just now. A check from before your last action says nothing about now.

Until then, keep going: end your turn only when the goal is reached or something needs me. When you must wait, say what you are waiting for and roughly how long, then wait with what your environment gives you. Use the waiting time with the explore skill.

When a piece is done, say so in one line and close it: stop watching it, delete its state on disk, and leave it out of later reports.

## Come back to me only when

- the answer will not solve the problem;
- a side effect we did not agree is needed;
- the goal cannot move without breaking one of its limits;
- something only I can provide is missing.

Say exactly what blocks it, what you tried, and what you need.

## Long jobs

When something will run for a long time, say up front how long you expect, and how I can see its progress. Write its state on disk so a restart or a new session picks it up. When you come back to it, check the real state first: what finished, what failed, what is still running. Say when your estimate changes.

## Watching

When the goal is to see something through, such as a pull request to merge, CI to pass or a job to finish, watch everything that touches it and act on each change within the goal and its agreed side effects: fix the failure, answer the comment, rerun the job. If no goal was set, set it first with the goal skill, so the side effects of watching, such as pushes and replies others will read, are named and agreed. Most events need no word to me. Tell me what changed and what you did in one line.

## Loops

A loop of improving or reviewing needs a stopping rule set before it starts. A review loop stops when a round finds nothing that changes the result. An improvement loop stops at the target, or when rounds stop improving. "It could always be better" is not a reason to continue.
