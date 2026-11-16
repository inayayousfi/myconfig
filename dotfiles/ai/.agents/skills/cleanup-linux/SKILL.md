---
name: cleanup-linux
description: Use when I want to inspect and clean a Linux machine, such as periodic maintenance, optional or distribution-installed packages, caches, build outputs, old installations, or leftovers of removed applications.
---

# cleanup-linux

Help me choose what to remove from this machine. It is occasional maintenance, never a schedule and never unattended deletion. Discover the paths and tools on this machine; do not reuse another machine's paths, package list or commands.

## Look first

Use the environment skill. Find the distribution, package managers, storage layout, configuration source and deployed links. Keep the keep or remove choices I already made.

Inspect without changing anything before proposing. Start with disk usage to find the large areas, then see what they hold. Tell allocated size from apparent size, shared files and space kept by snapshots, and never add a parent's size to its children's. A folder's size is not what removing it frees.

For protected places, use the root method the environment record gives, for one explicit command at a time: a permission error does not end the inspection. Say what stayed out of reach. Do not install a tool when the existing ones suffice.

## Where to look

These are areas to inspect, not things to delete:

- package-manager archives, old package versions, build-helper downloads and build trees. Check for modified recipes and local source first;

- language caches, dependency stores, compiler caches and build outputs. Tell rebuildable outputs from programs something actually runs, and check whether offline builds need the cached downloads;

- application caches and data of removed applications. A folder named cache can hold an application's only local model; profiles, credentials, sessions, histories and models are not caches;

- optional applications and tools, including distribution defaults and packages my configuration installed. An orphan report is a starting point, not a list;

- old manual installs, kept install trees and backups. Resolve links and find the copy in use first;

- downloads, trash, temporary work, logs, reports and container storage when large. Container volumes and databases are data, not cache. A name or a date does not prove something is unwanted.

Do not open credentials or personal documents to estimate size. Use sizes, owners, package records and launch references.

## Know what each removal does

For each candidate, find what it does, what owns it, what calls it, and what stays after removal. Follow dependencies and references through configurations, services, autostart entries, scripts and projects. Take special care with accessibility, input, audio, network, the desktop and boot.

That I never run a command does not mean nothing does: an editor may call it in the background. Removing an installer's download cache does not uninstall what it installed. Explain these with the paths and launch mechanisms you found.

Check running processes and ongoing installs or builds before removing their files. Keep source code, tracked work and deployed configuration out of a cache removal unless I pick them separately.

Prove a distribution default from local install records; otherwise say its origin is unknown. Flag packages my configuration installs, since a redeploy brings them back; removing them here does not mean changing that configuration.

For packages, preview the whole transaction, and separate what I picked from the dependencies a recursive removal adds. Never force past a dependency check.

## Let me pick

Use the question tool with several choices at once, grouped by function or consequence, so I can pick many items without decoding package names. Allow a written answer. Each option says what it is, its size, what I lose, what it costs to get back, and whether my configuration would reinstall it. Unpicked items stay.

Then settle the sub-choices: how many package versions to keep, whether to keep archives of removed packages, modified recipes, or installed programs while deleting their build outputs. Never invent a retention rule or widen a picked category.

Keep uncertain items aside while you find their uses. Say what you checked and what is still unknown: not finding a reference does not prove nothing uses it.

## Confirm the exact list, then remove

Show the exact paths and packages, what is excluded and what follows, and confirm through the question tool. An earlier pick counts only when it already showed the exact targets.

Recheck the targets when the state changed. Use the owning tool's cleanup when it matches the picked scope; otherwise delete only the inspected, confirmed files. Elevate only what needs it. Never run a blanket purge of packages, home, caches or containers instead of the picked list. Restart nothing, change no installer rule, commit or publish nothing unless I picked it.

Stop on an unexpected dependency or a changed target and tell me what changed. Report a partial cleanup as partial.

## Check and report

Check that picked targets are gone, excluded ones remain, and packages still have their dependencies. Check affected services, such as accessibility, without disturbing my session.

Measure free space before and after when it matters, and tell real space freed from file sizes and estimates. Report what was removed, kept, failed or left pending, and what will need rebuilding or downloading later.

Update `~/environment.md` only when what the machine offers changed; a cache cleanup usually changes nothing there.
