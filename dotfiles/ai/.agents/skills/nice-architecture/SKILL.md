---
name: nice-architecture
description: Use when discussing or changing code architecture, including module boundaries, dependencies, composition, abstractions, components and their roles, contracts, side effects, and failure handling.
---

# Nice architecture

The aim is a program made of small, self-contained components, each with one clear role. Each component can be understood, replaced, and removed on its own, and the causal structure of the program stays understandable even when it has many features. Given an observed behavior, the user wants to find the component responsible, see who asks it and why its dependencies exist, and predict which parts a change will touch. Judge simplicity by that traceability and by the independence of the components, not by line count, number of features, or whether a rebuild is needed.

Start with the actual system and its constraints. Trace the path from the behavior or requested change through the components involved, the code that assembles them, and their implementation. Identify which relationships the design makes visible and which require reconstructing hidden load order, precedence, conventions, or shared state.

## Roles

Every component has exactly one role: one job that can be stated in one sentence. A role declares four things:

- the state it owns, which no other component changes;
- the effects it causes outside its own result, such as writing files, running commands, changing settings, or talking to the network;
- the operations it offers to its callers;
- the ways it can fail, each with its own identity, and the state each failure leaves behind.

Those four parts form the component's contract. A caller should be able to use the component, and handle its failures, from the contract alone.

Components that play the same role share one shape, so the code that uses them can treat them alike. A component with a different role has the shape its own job needs; do not force every component into one shape. Roles come from the problem. Coordinating other components, holding shared program state, queueing work, or running one step of a larger job are possible roles, and none of them is required in every program.

## No side effects

A side effect is any change a component makes that its declared role does not include. Side effects must not exist. Each change either belongs to the component's declared role or moves to the component whose role is that kind of change. To check a component, ask whether its declared role alone lists every change it makes outside its own result. If it does not, something is hidden.

By default, divide a program so that most components cause no outside effects. They take inputs, decide, and return results. A few components at the edge of the program each own one kind of outside effect, and the deciding components hand them requests. The deciding components are then easy to test and cannot leave anything behind, and every effect has one visible owner.

## Four properties

Judge each component by four properties. Each one has its own tests, and no test is shared between them, so a flaw belongs to exactly one property.

- **Self-contained.** The component owns its whole concern: its state, its checks, and its cleanup. No shared component handles one slice of every other component's concern. Tests: delete it, and only its explicit callers break, with nothing left behind elsewhere; and run and check it alone, without the rest of the program.
- **Small.** Its surface is small and it represents one concept. Any amount of work can sit behind that surface; the size of its inside follows the concept, not a size limit. Test: its job fits in one sentence without "and", and a caller needs to know only its contract.
- **Composable.** Whoever assembles it sets its options. It never checks where or for whom it runs, and it shares no hidden state with other components. Test: it can be combined with other components in a new way without adapters and without knowledge of its insides.
- **Modular.** Its implementation can change behind its contract. Test: replace the implementation, and no caller changes.

A component receives only the pieces it uses, and those pieces are visible at its boundary. When one central part grows to serve every component, split it by role, so that each component depends only on the roles it actually uses.

## Assembly and coordination

A component that coordinates others has its own role: it asks the right components, in the right order, and never does their work or handles their internal logic. Its order is visible in one place, and the few order rules whose breakage would matter are checked.

Keep each fact in one place. A choice, a list, or an option belongs to one component, never repeated where the copies can disagree.

An abstraction earns its place only when something uses components without knowing which concrete component it has. An abstraction with no such user, or a layer that only passes calls along, adds names without hiding anything. Indirection becomes costly when it obscures what runs and why.

## Scales

Components nest, and they grow from small to big. The need comes from the top: the required behavior says which small components must exist. Building and checking go from the bottom: each small component is built and checked on its own first, then assembled into larger ones. The smaller a component, the easier it is to understand, run, and debug alone, and a failure in a larger component can be narrowed down to the smallest one that fails.

A larger component exists only when several smaller ones together form a role of their own, with its own state, effects, operations, and failures, and a shape that fits that role rather than a copy of its members' shape. Components do not need to be the same size, and they can live at different scales. From outside, a larger component shows only its own contract; its inner components are visible when zooming in and never required when zooming out. Test: stated from outside, its role does not need a list of its parts. If the only honest description is "contains A, B and C", it is only a grouping, not a component. The four properties apply at every scale.

## Failure

Whoever owns a responsibility decides what failure means for that responsibility, and no component decides it for another level. A component decides what counts as its own failure; the component that asked it decides what that failure means for its own job.

Prefer, in this order:

1. Fail at zero by construction: shape the work so that a failure leaves nothing changed.
2. Otherwise, undo what was changed before reporting the failure.
3. Otherwise, report a failure that names exactly what was left, such as leftover data or changed state.

Put the step that cannot be undone last, so that everything before it can still be undone. A failed undo is a failure with its own identity.

Give every failure an identity, not only a description. A caller tells failures apart by checking that identity exactly, never by reading or parsing a message. A component's failures form a closed, known set, so a caller can see every failure it may receive and confirm that it handles each one. Details, such as the cause or the item involved, travel with the identity and never replace it. A failure deserves its own identity when it leaves a distinct state. At each failure point, the caller must know what state it is in without knowing the component's insides. A larger component turns the failures of its inner components into failures with its own identities, each stating what is left at its own level.

## State

When behavior depends on state, identify who owns the state and keep the rules that change it with that owner. Keep only the state the behavior needs. Make meaningful states and transitions explicit. Keep the path from input through decision to output visible and directional.

## Local change

Keep a conceptually local change structurally local. When comparing designs, account for how far a developer must travel from a behavior to its cause, how many places a change actually touches, and whether they can predict those places before starting.

## Limits

Apply these preferences inside the user's chosen outcome and the target's real constraints. Do not remove required behavior, insist on one implementation style across unrelated systems, or replace an established design only to satisfy these rules. In existing code, point out where the design departs from them instead of rewriting it unasked. When a design choice has lasting consequences, expose those consequences instead of silently choosing for the user.
