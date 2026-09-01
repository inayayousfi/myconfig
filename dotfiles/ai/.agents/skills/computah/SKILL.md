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

**Capture is not acting.** Use the selected platform script to take a screenshot. Operating systems defend against synthetic capture keys, so insisting on a key press there produces silent failures and buys nothing.

## Platform scripts

Resolve `scripts/...` relative to this skill's root, never relative to the
working directory. Agent Skills clients may expose the skill root differently;
use the location supplied when this skill was loaded.

Choose the script by the environment issuing commands:

| Command environment | Script                |
| ------------------- | --------------------- |
| Native Windows      | `scripts/windows.ps1` |
| WSL driving Windows | `scripts/wsl.sh`      |
| Native Linux        | `scripts/linux.sh`    |

macOS is not implemented. Stop and tell the user.

Run `capabilities` once before the first capture or action. The scripts detect
their available backends and share this command interface:

```text
capabilities
capture [OUTPUT.png]
crop INPUT OUTPUT X Y WIDTH HEIGHT
move X Y
click X Y [left|right|middle]
drag X1 Y1 X2 Y2 [left|right|middle]
scroll NOTCHES
type TEXT
key CHORD
```

Coordinates are pixels measured from the captured image's top-left corner. The
platform script translates them to native desktop coordinates, including a
Windows virtual desktop whose origin is negative. Positive scroll notches move
up; negative notches move down. Chords use names such as `ENTER`, `CTRL+L`,
`ALT+TAB`, and `META`.

If `capabilities` marks an operation unavailable, stop rather than trying a
different command from memory. The scripts are the platform abstraction; add
new backends there instead of adding platform commands to this file.

## The loop

Every action is: **capture → locate → act → verify.**

1. Capture the full desktop.
2. Read the image, find the target, compute screen coordinates.
3. Act — click, type, or press a key.
4. Capture again and confirm the expected change actually happened.
5. If it did not, stop and diagnose. Do not repeat the same action blindly.

Never chain a second action onto an unverified first. A missed click is invisible. Every step after it compounds the error. That is how automation sequences go badly wrong rather than merely failing.

**Some actions produce no visible change.** Typing a second `1` into `1 × 1` leaves the display reading exactly the same. Verification cannot confirm those. Do not treat an unchanged screen as a miss and retry, or you will enter the input twice. Instead carry the uncertainty forward. Verify at the next step that does produce a distinguishable result, and treat that as confirmation of both.

## Images and coordinates

`capture` writes a PNG and prints its absolute path. Ingest that path with the
current harness's image-reading facility. Script stdout is not vision input.

Actions consume coordinates from the full captured image. If the image-reading
facility downscales it, apply its reported factor to visual estimates before
passing coordinates to the script. Never hardcode a scale factor.

## Targeting

An estimate taken off a downscaled screenshot is accurate to roughly ±15px. Budget that against the size of the target.

**Target comfortably larger than ~40px** — single estimate, click, verify. A full-width link or a normal button is in no danger from ±15px.

**Target smaller than ~40px, or a previous attempt missed** — use `crop` on the capture already taken, read the crop at native resolution, estimate from that, then add the crop's origin back to get screen coordinates. Cropping removes the downscaling, so the estimate tightens sharply.

Crop the image already captured. Do not take a fresh capture just to zoom in.

The same trick applies to verification: when checking whether a known region changed, crop before reading. Same capture, far fewer tokens.

## Acting

Everything goes through mouse and keyboard — including launching and focusing applications. No process-launch calls, no window-manager APIs.

**Launching.** Use `key META`, capture and verify the launcher, use `type`, then **capture and confirm the highlighted result is the intended one before `key ENTER`.** This confirmation is not optional. The top hit depends on search history and indexing state.

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
- Verification shows no change at all — run `capabilities` again and report the backend before assuming the aim was wrong.

Two failed attempts at the same step means the approach is wrong, not that it deserves a third try.
