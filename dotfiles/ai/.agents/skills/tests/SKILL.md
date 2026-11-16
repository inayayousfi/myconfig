---
name: tests
description: Use as soon as testing comes up, before deciding whether to write tests or how: when planning, writing, changing or judging tests. Deciding not to write a test is as valid an outcome as writing a good one.
---

# tests

A test states an expected behavior, defined apart from the code, and shows that a starting situation produces that result where it matters. Good tests matter. Knowing when a test adds nothing matters as much.

## Start from the expected behavior

The expectation comes from my need, a requirement, or a behavior that must keep working. Code shows what happens now, not what should happen. Name the starting inputs and state, the operation, and the observable result; for stateful behavior, the sequence of actions and the state left afterward. Derive the expected result from the requirement, never by copying the code's calculation or accepting what it returns now.

When the behavior is not defined, stop designing tests for it and settle the expectation first, with me when the evidence does not settle it. Do not invent a requirement to have something to test.

## Decide whether a test is worth it

Decide before writing test code. A code change alone is not a reason for a test. For each test, say in one sentence what behavior it establishes and what it adds beyond existing checks.

Do not write a test that:

- checks a behavior nobody defined;

- repeats the implementation, checks internal choices, or checks that a fake returns what you gave it;

- repeats what an existing check already establishes;

- freezes a presentation detail with no established consequence;

- tries extreme inputs with no reason in the behavior, the interface or real use;

- confirms that deleted code is gone. When something must no longer happen, or must survive the removal, test that instead.

A useful one-time check is not a test worth keeping. Keep a test when the behavior it protects justifies maintaining it.

When no test is justified, say why and stop. Do not add a token test so that testing seems to have happened. "No new test needed" is not "verified".

## Test through the real path

Pick the boundary from the behavior, not the nearest function. A self-contained calculation can be tested directly. A behavior that spans several steps needs a check through those steps to the final result: steps passing separately do not show they work together.

Valid inputs and their results come first, then error and boundary cases with a real reason. An error case passing says nothing about valid use.

Use the real code along the path the test claims. A fake can isolate a rule, but say what it leaves unchecked. When loading, wiring, startup or deployment is part of the behavior, go through it instead of around it.

Check the result where it matters: internal calls, intermediate values or the absence of an exception do not replace it. Include the lasting state the behavior must leave.

Isolate test state before setup. A test never changes my live configuration or work, and its cleanup removes only what it created.

## Write it

Writing the test first can fix the expectation before the code shapes it. Use that order when it helps, not by rule. Assert the result and the lasting state, not private structure: changing the insides while keeping the behavior should not require rewriting the test. Breaking the code on purpose to watch a test fail is a tool for investigating, not a required step.

## Report

Say which behaviors the checks exercised, the results, and the limits. When a check cannot run, name the expectation left unchecked instead of reporting an easier check that passed. A passing rule check does not mean the program works, and test counts, coverage or compiling do not say what was verified. Report a justified decision not to test with the same care as a passing test.
