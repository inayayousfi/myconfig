---
name: death-sloop
description: Use when the user gives a goal to plan together and then wants an agent that owns it without stopping, tracking everything that affects it and acting on it without being asked, until the goal is reached and verified or, for an ongoing goal, until the user stops it. Examples are a commit whose CI passes, a pull request with every comment addressed, a repository where every issue gets a pull request, or a project kept in step with its upstream.
---

# death-sloop

I give you a goal. We plan it as usual, then it is yours. You own it, and I should never have to talk to you about it again.

I decide the goal. How you get there, track it, store it, and wait for it is your problem, not mine. Never bring me a question about how.

## Plan

We plan the way we always do. The plan also says whether the goal has a finished state, written so that it can be checked against the real world, or whether it is ongoing. The run starts when I say go.

## The go is final

My go is the approval for everything the run needs. From then on, every approval and confirmation gate from my rules is lifted: you push, comment, open pull requests, rerun jobs, and decide on your own. Never ask me anything during the run.

The go holds for the whole run. It survives when the conversation gets summarized: carry the plan and the go into every summary. It covers every helper you start: give each one the plan and the go, so it works under the same terms.

The plan's constraints still hold. A constraint is part of the goal, not a gate.

## Keep state on disk

Your context will not hold the whole run. It gets summarized, it runs out, the machine restarts. Keep the run's state on disk: the plan, the go, everything you track and where it stands, what is done, what is left. Put it where a restart does not erase it and where a new session can find it and pick the run back up. Update it as the run moves, so that it is always the truth and your memory is never the only copy.

When the run ends, delete that state.

## Track everything

This is the heart of it. Anything that has an impact on the goal is yours to watch: the repository, its issues and pull requests, its CI, the upstream projects it depends on, the people and automation that touch it. When something moves, you see it and you act on it without anyone telling you.

A check that ran before your last action says nothing about now. Something you fixed can break again, and someone else can add work while you are on another part. Keep watching all of it.

When nothing needs a reaction, keep moving the goal forward. Look for what would make the result better by the measure of the goal, and work on it.

Do not end your turn while the goal is not reached. When you have to wait, wait with whatever your environment gives you, and come back. When you wait, say what you are waiting for.

Keep making the work visible as my rules ask: before each action, say what it will change; after it, say what happened.

## When it ends

A goal with a finished state ends when you checked that state against the real state just now and it holds. Then report what you did, what you checked, and what the final state is.

An ongoing goal ends when I stop it.

Either kind stops early only when the goal cannot move forward without breaking a constraint from the plan. Then tell me exactly what blocks it and what you tried. Do not bend a constraint to get there.
