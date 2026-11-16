---
name: review
description: Use for any review, of a change or pull request, a module or project where something feels off, an interface, whether a result solves the problem, data, performance, documentation, or security. Also use it after your own work when it is large enough to deserve a fresh look, by handing it to a fresh agent. A small change needs the check skill, not a review.
---

# review

A review looks for what would change the result. There is always something to find, so a list of everything is worthless. What matters is how much each finding changes the outcome.

## Who reviews

Work is reviewed by an agent that did not build it. When you built it, hand the review to a fresh agent with the delegate skill: give it the target and the goal, not your reasoning or what you expect it to find.

## What to look at

Pick what the review covers from what was asked and what the target is. Load the matching preference skill when there is one: it says how I like that kind of thing, and the review checks the work against it.

- **A change or pull request**: does it do what its goal says, without breaking what was there? Check the shape against nice-architecture and the tests against tests.
- **A module or project where something is off**: map its parts, the role of each, who calls whom, who owns which state. Look for the places where that map is hard to draw: a concern with two owners, a hidden effect, a part that cannot be understood alone. Compare with the simplest shape that would meet the same requirements.
- **Problem fit**: does the result solve the problem it was built for, or only the answer as written?
- **An interface**: use it as I would, on the running thing. Look at what is hard to understand, slow to use, or easy to get wrong.

## How much it matters

Rate every finding, and prove the rating:

1. **Breaks the goal**: wrong result, crash, lost data, a security hole. Show the concrete path: a real input or action, what goes wrong, and where.
2. **Changes the result noticeably**: something I would see or feel, now or the next time the area changes. Name what I would notice.
3. **Worth knowing**: real, but small.
4. **Nitpick**: you had to go looking to create the problem.

A finding that cannot meet its level's proof drops a level. Levels 1 and 2 go back to the doing. Level 3 goes in a short list. Level 4 is dropped, not reported.

Prove a finding by running something whenever it can be run: a finding you could have run and did not drops a level. Say when you are guessing. Leave no files behind. Report how many findings you dropped.

## Disputes

When the agent that did the work disputes a finding and two replies do not settle it, bring both sides to me in two or three lines each. I decide. A finding stays open until it is settled: agreeing just to end the loop settles nothing, and a loop that runs out of rounds has approved nothing.

## Reporting

Lead with the verdict in one line. Then the findings by level, each with its location, what is wrong, why it matters here, and the direction of a fix in words. Fix only when the goal you were given includes fixing.
