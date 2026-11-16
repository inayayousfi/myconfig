---
name: delegate
description: Use when deciding who carries out a goal (you alone, a sub-agent, several, a workflow, another thread), when choosing models, and when writing a brief for another agent. Also use it when I ask for sub-agents or ask for a prompt I can pass to another agent.
---

# delegate

Match the machinery to the work. A rework may need several agents building, then fresh ones reviewing. Say your plan in one line before starting, so I can redirect it: "I'll do it myself", "one Sonnet sub-agent for this", "workflow: build in a worktree, review, fix".

Delegate when it buys something: work that runs in parallel, a long investigation that would bury the main conversation, or a fresh view that is not anchored on yours. Do yourself what takes a few steps.

Pick the model for the work. Bulk and well-specified work goes to a cheaper, faster model. Design, judgment and taste go to the strongest one.

## Writing a brief

The agent you brief knows nothing of this conversation. Give it:

- the goal and what done means for its piece;
- the decisions that apply to its piece, and the limits it must keep;
- the side effects it may cause, and nothing beyond them;
- where to look, without telling it what to conclude;
- what to bring back: what it did, the small decisions it took, and anything that needs a decision above its level, listed, never guessed.

A brief meant for a fresh view carries the target and the goal only, so the agent forms its own view.

When an agent comes back, check its result before relying on it, and report what it found and how sure it was.
