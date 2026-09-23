---
name: nice-architecture
description: Use when discussing or changing code architecture, including module boundaries, dependencies, composition, and abstractions.
---

# Nice architecture

The aim is to keep the causal structure of a program understandable even when it has many features. Given an observed behavior, the user wants to find the code responsible, see who calls it and why its dependencies exist, and predict which parts a change will touch. Judge simplicity by that traceability, not by line count, number of features, or whether a rebuild is needed.

Start with the actual system and its constraints. Trace the path from the behavior or requested change through its callers, composition, and implementation. Identify which parts the design makes visible and which relationships require reconstructing hidden load order, precedence, or conventions. Do not assume that dynamic wiring is bad or static wiring is good without looking at the resulting path through this particular system.

Prefer explicit composition of cohesive, independent mechanisms with narrow contracts. Let callers set policy where the same mechanism can serve different uses. An abstraction should reduce repetition or complexity already present in the problem, rather than add layers in anticipation of hypothetical uses. A visible registry can be useful; discovery and indirection become costly when they obscure what runs and why.

Let a module's size follow the concept it represents, not an arbitrary line limit. Within the project's conventions, prefer named operations for distinct steps over one long function; it is fine to follow several definitions when each one does real work. Group files by purpose in a directory when that makes their roles clearer than a crowded top level, and keep closely related classes together when separating them would make the behavior harder to follow. Splitting a coherent concept merely to shorten files, or adding layers that only pass calls along, obscures rather than clarifies it. Each unit should have a clear responsibility at its boundary; its internal steps can be followed in depth without having to reconstruct hidden dependencies.

When behavior depends on state, identify who owns the state and keep the rules that change it close to that owner. Keep only the intermediate state the behavior needs; pass values along or derive them where that leaves a clearer path. Make meaningful states and transitions explicit; use a state machine when it makes those transitions easier to follow, not for every stored value. Keep the path from input through calculation and decision to output visible and directional. Give stateful units clear operations so callers can compose them without coordinating their internals. Where work has effects beyond its own result, keep those effects at the smallest coherent boundary that needs them and make that boundary visible to its callers. The right boundary depends on the domain, not on a target number of files or objects.

When a calculation depends only on its inputs, make it a direct operation that returns a result. A caller should be able to find the steps that produce that result without following object construction or configuration first. Use a method on a stateful unit when the calculation belongs with that unit's state; use a factory when constructing something is the actual job, not as a place to hide the calculation.

Keep a conceptually local change structurally local. When comparing designs, account for how far a developer must travel from a behavior to its cause, how many places a change actually touches, and whether they can predict those places before starting. An explicit entry in a central list and a rebuild may be simpler than a plugin system with hidden precedence. The reverse may be true where the existing system gives a clear, short path through its dynamic wiring.

Apply these preferences inside the user's chosen outcome and the target's real constraints. Do not remove required behavior, insist on one implementation style across unrelated systems, or replace an established design only to satisfy an aesthetic rule. When a design choice has lasting consequences, expose those consequences instead of silently choosing for the user.
