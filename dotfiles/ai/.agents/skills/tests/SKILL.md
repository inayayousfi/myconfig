---
name: tests
description: ALWAYS use this skill as soon as You consider testing, before deciding whether to write tests or choosing a testing approach. Use it when planning, writing, changing, or evaluating tests. Do not begin test work without loading it. This skill gives equal importance to useful tests and justified decisions not to write tests.
---

# Tests

A test represents an expected behavior defined independently of its implementation. It exercises the path needed to establish that a starting situation produces the expected observable result where that result matters. Good tests matter. Knowing when a test adds nothing matters just as much.

The purpose is to establish that valid inputs produce the right outputs through the necessary sequence of operations. Error handling and meaningful boundary cases are expected behaviors too, but a collection of rejection tests does not establish that ordinary use works.

## Establish the expected behavior

Start from the user's need, an established requirement, or a necessary existing behavior. Inspect the relevant code, callers, and real entry point to understand how that expectation becomes observable. Code explains the current implementation; it does not, by itself, define what the result ought to be.

Identify the starting inputs and state, the operation, and the expected observable result. For stateful behavior, include the sequence of actions and the state that must remain afterward. Derive the expected result from the requirement rather than copying the implementation's calculation or accepting whatever it currently returns.

If the intended behavior is not defined, stop designing tests for it. Explain the missing expectation and resolve it with the user when the available evidence cannot resolve it. Do not invent a requirement to have something to test. A draft test may help express an expectation under discussion, but its success cannot validate that expectation.

## Decide whether a test is useful

Make this decision before writing test code. A code change is not an automatic reason to add a test. Respect explicit user and repository verification requirements without using them as a reason to invent additional tests.

For each proposed test, explain what behavior it establishes, why that behavior matters here, and what verification it adds beyond existing checks. Keep the justification proportionate, usually one sentence. Several tests may share a requirement, but each must represent an expected behavior or a meaningful case of that behavior.

Do not write a new test when:

- The expected result is undefined. Clarify the expectation first.

- It merely repeats the implementation, inspects incidental internal choices, or confirms that a simulated replacement returns the value You supplied.

- An existing check already establishes the same behavior under the relevant conditions, and the new test adds no useful distinction.

- It freezes a presentation detail with no established consequence. A component using a particular font to display a message is not, by itself, a reason to maintain a test. Do not invent a functional requirement to justify a cosmetic assertion.

- It exercises arbitrary extreme inputs without a reason grounded in the behavior, the interface, or observed use. Many unusual cases do not compensate for a missing ordinary-use test.

- A removal leaves no relevant behavior to establish. Do not add a test solely to confirm that a deleted private function no longer exists. If the requirement is that something must no longer happen, or that another behavior must survive the removal, test that observable expectation instead.

Distinguish a useful one-time verification from a test worth keeping. Keep a test when it protects a necessary behavior and its distinct verification justifies maintaining its setup and assertions. Do not add permanent test machinery for every temporary check.

When no new test is justified, say why and stop there. Do not replace the rejected test with another token check to claim that testing happened. Distinguish "no new test needed" from "the behavior has not been verified."

## Choose a path that establishes the expectation

Choose the test boundary from the expected behavior, not from the nearest convenient function. A self-contained calculation can be tested directly. An expectation that depends on several steps needs a check through those steps, from the relevant starting point to the final observable result. Separately passing tests for individual steps do not establish that the steps work together.

Prioritize valid inputs and their expected results. Then include error cases and boundary cases that have a grounded behavioral purpose. Do not treat a successful error return as evidence that valid use succeeds.

Use the actual implementation along the path that the test claims to verify. Simulated dependencies can make a focused rule test useful, but do not replace the behavior being claimed. State what the simulation leaves unverified. If loading, wiring, startup discovery, or deployment is part of the expectation, exercise that mechanism rather than bypassing it in the setup.

Observe the result at the boundary where the expectation matters. Internal calls, intermediate values, and absence of exceptions are not substitutes for that result unless they are themselves the established requirement. Include required effects on saved state or other owned resources when those effects belong to the behavior.

Isolate test state and resources before initialization. Do not let setup alter the user's live configuration or saved work, and do not let cleanup remove resources the test did not create. A test's setup must preserve the conditions relevant to its claim.

## Write from the expectation

Writing a test before the implementation can fix the intended behavior before the code influences the expected answer. Use that order when it helps express a sufficiently defined expectation; do not impose it on every change.

Describe a test's purpose in this form when useful: "Given [inputs and starting state], through [necessary path], expect [observable result], because [established need]." This is a justification, not a demand for repeated boilerplate in every test file.

Assert the expected result and relevant lasting state. Avoid assertions that lock in private structure without protecting a required behavior. Changing an internal implementation while preserving the expectation should not require rewriting its behavioral tests merely to match the new structure.

Deliberately introducing a defect or demonstrating failure against an earlier implementation is not a mandatory gate. Those techniques may help investigate a specific issue, but they do not replace establishing the expected behavior and exercising the path that realizes it.

## Report what was established

Run the chosen checks and report the behaviors actually exercised, the results, and the limits. If a check cannot run or the relevant path is inaccessible, name that boundary and the expectation that remains unverified. Do not silently substitute an easier check and claim the original expectation passed.

A passing test establishes its assertions under its exercised conditions. Do not turn a local rule check into a claim that the whole application works. Test counts, coverage percentages, compilation, and successful checks of simulated behavior are not substitutes for explaining what expected behavior was verified.

Report a justified decision not to add tests as a valid outcome, with the same care as a passing test. Never claim verification that did not happen.
