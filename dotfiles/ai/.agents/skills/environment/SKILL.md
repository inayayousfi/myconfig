---
name: environment
description: ALWAYS use this skill before using, searching for, or installing a host program, service, integration, device, or installed capability that the current harness does not expose directly, and after discovering, changing, or removing one. It covers reading and maintaining `~/environment.md`.
---

# Environment inventory

Assume You do not know which host tools and environment capabilities exist. Tools exposed directly by the current harness and basic commands of the active shell are the only exceptions. For every other host program, service, integration, device, or installed capability, `~/environment.md` is the primary source for learning what exists and how the environment can be used.

Read the relevant part of `~/environment.md` before searching the host, trying a capability from memory, installing an alternative, or asking the user. Then verify only the capability that the current work needs. Current evidence outranks stale inventory content.

Keep `~/environment.md` as the closest available record of what this computer currently offers and how to use it. Record anything a future agent could use beyond a single project: a tool, a service, an access, a useful command such as an API reachable with `curl`, and their limits. Leave project-specific details to the project. When You discover a capability, find that a path or command changed, or see that something was removed, correct the existing entry instead of adding a new one. Describe only the present state. Never date entries or record what happened, when, or who did it.

State the exact section and fact You are consulting, adding, or correcting in the transcript.
