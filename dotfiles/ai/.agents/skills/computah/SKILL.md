---
name: computah
description: ALWAYS use this skill when a task requires direct desktop control and no purpose-built tool or MCP fits, or after the fitting tool fails and cannot be repaired.
---

# computah

Operate the computer the way a person does. Look at the screen. Move the mouse. Press keys.

Use an existing purpose-built tool or MCP when one fits the task. If that tool fails, diagnose and repair it before changing methods. Do not build a custom parser, API client, accessibility bridge, or other ad hoc automation merely to avoid this skill.

When no natural tool fits, use this skill. Mouse and keyboard work everywhere, including canvas apps, games, remote desktops, and installers.

## Confirmation

Ask through the question tool before the first desktop capture or input. Loading this skill does not require confirmation. A valid global bypass that explicitly names desktop control satisfies this gate.

**Capture is not acting.** Use whatever method the platform reference specifies for taking a screenshot. Operating systems defend against synthetic capture keys, so insisting on a key press there produces silent failures and buys nothing.

## Platform dispatch

After confirmation, read the reference for the target desktop **before issuing any command**. Do not compose commands from memory. The details that matter are ordering constraints and silent-failure modes, and they are not guessable.

| Target desktop | Reference                                |
| -------------- | ---------------------------------------- |
| Windows        | `references/windows.md`                  |
| macOS          | Not yet written. Stop and tell the user. |
| Linux          | Not yet written. Stop and tell the user. |

Dispatch on **the desktop being driven**, not on where the agent is running. A Windows desktop driven from WSL and one driven by a natively-running agent use the same reference; only the invocation wrapper differs, and that file covers both.

## The loop

Every action is: **capture → locate → act → verify.**

1. Capture the full desktop.
2. Read the image, find the target, compute screen coordinates.
3. Act — click, type, or press a key.
4. Capture again and confirm the expected change actually happened.
5. If it did not, stop and diagnose. Do not repeat the same action blindly.

Never chain a second action onto an unverified first. A missed click is invisible. Every step after it compounds the error. That is how automation sequences go badly wrong rather than merely failing.

**Some actions produce no visible change.** Typing a second `1` into `1 × 1` leaves the display reading exactly the same. Verification cannot confirm those. Do not treat an unchanged screen as a miss and retry, or you will enter the input twice. Instead carry the uncertainty forward. Verify at the next step that does produce a distinguishable result, and treat that as confirmation of both.

## Coordinates

Captures are full-desktop, so image pixels map directly to absolute screen coordinates. There is no window-origin arithmetic to get wrong.

Two scale factors must both be handled or every click misses:

**Display scaling.** The capturing process must be made DPI-aware so it works in physical pixels. The platform reference gives the call and, importantly, _when_ it must happen — the ordering is a real constraint, not a style preference.

**Read downscaling.** Image-reading tools clamp large images and report the factor, e.g. `original 2560x1600, displayed at 2000x1250, multiply by 1.28`. Multiply every estimate taken off the displayed image by that reported factor.

```
screen_x = displayed_x × reported_factor
screen_y = displayed_y × reported_factor
```

Parse the factor from the read result every time. Never hardcode it — it depends on the capture's dimensions, and a capture smaller than the clamp arrives unscaled with a factor of 1.

## Targeting

An estimate taken off a downscaled screenshot is accurate to roughly ±15px. Budget that against the size of the target.

**Target comfortably larger than ~40px** — single estimate, click, verify. A full-width link or a normal button is in no danger from ±15px.

**Target smaller than ~40px, or a previous attempt missed** — crop the capture already taken to a tight region around the target, read the crop at native resolution, estimate from that, then add the crop's origin back to get screen coordinates. Cropping removes the downscaling, so the estimate tightens sharply.

Crop the image already captured. Do not take a fresh capture just to zoom in.

The same trick applies to verification: when checking whether a known region changed, crop before reading. Same capture, far fewer tokens.

## Acting

Everything goes through mouse and keyboard — including launching and focusing applications. No process-launch calls, no window-manager APIs.

**Launching.** Press the OS launcher key, type the application name, then **capture and confirm the highlighted result is the intended one before pressing Enter.** This confirmation is not optional. The top hit depends on search history and indexing state, which makes launching the least reliable step in any chain — and it is the first step, so everything downstream inherits the error.

**Focusing.** Alt+Tab, or click the taskbar entry. Verify by capture that the intended window is actually frontmost before acting on it.

**Typing.** Click the target field first, verify by capture that it has focus, then type. Typing into whatever happens to hold focus is how text lands in the wrong window.

## Non-destructive rules

Do what was asked, without asking permission at each step. But never destroy something that was not part of the request.

**Never use replace-semantics.** Type additively. Do not set a control's entire value, and do not select-all before typing.

**Capture any field before typing into it.** If it already holds content, work with that content — append, or position the caret deliberately. An application that restores its previous session has content even when it was just launched and looks empty.

**Stop and ask first** before closing a window with unsaved work, deleting, overwriting a file, submitting a form with real-world consequences, or acting on a password, payment, or account-settings surface — unless that action _is_ the request.

## Privacy

A full-desktop capture takes whatever is on screen, unfiltered: every window, notification, and message, including things the user forgot were open. Say so plainly when it is likely to matter, and delete captures once the task is done.

## When to stop

Stop and report rather than continue when:

- The same target is missed twice.
- An unexpected window or notification appears.
- A modal or dialog blocks the intended path.
- Verification shows no change at all — check the platform reference for causes that produce a _silent_ no-op before assuming the aim was wrong.

Two failed attempts at the same step means the approach is wrong, not that it deserves a third try.
