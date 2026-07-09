# Workflow preferences

## Plan mode for real work

Unless a task is genuinely trivial (e.g. changing a CSS color, adding a button
whose behavior is already fully specified, a one-line copy edit), the model
must enter plan mode first when asked to build a new feature or any non-trivial
change. In plan mode:

- Ask clarifying questions about approach, architecture, and how pieces
  (frontend/backend/shared utils) should communicate, before writing a plan.
- Propose the plan (contracts, shared types/utils, file ownership, sub-agent
  breakdown if applicable) and get explicit sign-off before implementing.
- Only skip plan mode for trivial, unambiguous, already-fully-specified asks.

## Model selection

Ratings are 1-10, higher is better. `cost` is inverse spend (10 = cheap,
1 = expensive). When choosing which model to run inline or dispatch as a
sub-agent, prioritize the intelligence/cost ratio, but weight cost slightly
higher than intelligence — never default to the most expensive model for
work a cheaper one can handle. Reserve the top model for planning/architecture
and final synthesis, not for parallelizable implementation slices.

| Model  | Intelligence | Taste | Cost |
|--------|:---:|:---:|:---:|
| Fable  | 10  | 10  | 1   |
| Opus   | 9   | 7   | 4   |
| Sonnet | 6.5 | 7   | 7   |
| Haiku  | 4   | 2   | 9   |

Practical defaults:
- Planning, architecture decisions, cross-cutting synthesis: Fable (sparingly)
  or Opus.
- Independent implementation slices dispatched to sub-agents: Sonnet by
  default.
- Trivial, boilerplate, or high-volume/low-risk tasks: Haiku.
