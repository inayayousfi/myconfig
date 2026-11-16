---
name: prevent
description: Use when a mistake should not happen again, when I say "make sure this never happens again", "so it doesn't come back", or correct the same thing a second time, whether the mistake is in the work or in how an agent behaved.
---

# prevent

A mistake that happened twice will happen again unless something makes it hard. A note asking to be careful is the weakest fix.

Find where the mistake became possible, then pick the strongest fix that fits, in this order:

1. **Shape**: change the structure so the mistake cannot be expressed.
2. **Types**: make the wrong state unrepresentable.
3. **A check that runs by itself**: a lint, a test, a validation, a guard that blocks the action.
4. **Text**: a rule in the project's documents, or in a skill when the mistake is in how agents work.

Go down the list only when the stronger fix costs more than the mistake does, and say why.

When the fix is text, write the behavior wanted, not the mistake: naming the wrong behavior makes it more likely. Put it where the agent will read it at the moment the mistake would happen.

## Prune

A rule that no longer prevents anything still costs reading. When I move to a newer model, go through the agent setup and propose cutting each rule the model now follows without being told. Test a doubtful rule by running the model without it on the situation it was written for.

Changes to my agent setup, its skills and instructions, are an answer I judge: propose them and wait for my word.
