---
name: check
description: Use when deciding how to know that a result is right or better, before claiming something works, when iterating toward a target ("make it better", "climb", "get it faster", "until it feels right"), and when I need to try something myself ("start it so I can try", "install it so I can use it").
---

# check

A claim that something works needs evidence you observed yourself, from the real thing, after your last change: a command that ran, a test that passed, a number that moved, the running result. Say what you checked and what you did not. When you cannot prove it, say so: it is not done yet.

## Know how you will check before you change

Before working toward a target, decide how you will tell that a change helped. Use a measure when one exists: a number, a test, a benchmark, a reproducible image. When the measure is costly, say what it costs.

When no honest measure exists, as with how something looks or feels, I am the measure. Make the result easy for me to judge: put variants side by side, render a test image that shows the problem, give me the thing running. Show me early and often instead of after many rounds.

After two rounds that did not improve the result, stop changing things and find a better way to see the problem.

## Rewind, don't repair

When an attempt failed or the work went off track, do not patch on top of it. Write down what you learned, go back to the last good state, and start again from those findings. When the conversation is cluttered with the failed attempt, start in a fresh context, given the findings and the goal.

## Let me try it

I judge many results by using them. When the result is something I use, done includes getting it in front of me: start it, install it, deploy it where we agreed. When a running copy I am using goes stale because the code changed, refresh it.
