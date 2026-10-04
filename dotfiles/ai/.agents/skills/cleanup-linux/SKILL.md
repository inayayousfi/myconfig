---
name: cleanup-linux
description: Use when the user wants to inspect and clean a Linux machine, including periodic maintenance, optional or distribution-provided packages, caches, build outputs, old installations, and application leftovers. Inspect first, offer multiple-selection questions, and confirm exact destructive targets before cleanup.
---

# Clean up a Linux machine

Help the user choose what to remove from their actual machine. This skill supports occasional maintenance, including a review every three or four months. It does not schedule maintenance or authorize unattended deletion.

Discover locations and tools on the current machine. Do not reuse a previous machine's paths, package selection, or deletion commands as a cleanup recipe. Follow the shared agent instructions for choices, confirmations, privilege elevation, and visible execution.

## Establish the current machine

Read the relevant parts of the environment inventory before using host capabilities. Identify the distribution, package managers, storage layout, configuration source and deployed links. Carry forward the user's stated keep/remove decisions; do not ask them to repeat settled choices.

Inspect read-only before proposing deletions. Start with disk usage to find worthwhile areas, then examine what the large directories contain. Distinguish allocated size from apparent size, shared files, and space retained by filesystem snapshots. Do not add overlapping parent and child sizes together. A directory's total size is not automatically its removable size.

Use the user's established elevation method for protected inspection. When pkexec is available and selected, use it for an explicit command rather than treating a permission error as the end of inspection. Report boundaries that remain inaccessible. Do not install another inspection tool when available tools suffice.

## Where to look

Inspect the areas supported by the machine's inventory and initial measurements. These are categories to investigate, not automatic deletion targets.

- Package-manager download archives, old package versions, build-helper downloads and build trees. Check for modified package recipes and locally maintained source before classifying helper directories as disposable.

- Language-tool download caches, dependency stores, compiler caches and project build outputs. Separate rebuildable development outputs from executables actually launched by the user, services, desktop entries or scripts. Check whether offline builds depend on cached downloads.

- Application caches and data left after uninstalling an application. Distinguish temporary downloads from profiles, credentials, browser recovery, saved sessions, histories, models, language servers and installed libraries. A directory named cache can contain an application's only local model.

- Optional applications and command-line tools, including distribution defaults and packages formerly requested by configuration scripts. Inspect explicit installs, dependencies, optional dependencies and orphan reports. An orphan report is a starting point, not a removal list.

- Old manual installations, retained installation trees and backups in system application locations, home directories and configuration state. Resolve symbolic links and find the installation currently used before proposing an older copy for removal.

- Downloads, trash, temporary work areas, generated reports, logs and container storage when present and materially large. Container volumes and databases are persistent data, not interchangeable with image or build caches. A directory name or modification date does not prove that its contents are unwanted.

Do not inspect the contents of credentials or personal documents merely to estimate cleanup potential. Prefer sizes, ownership, package records and launch references.

## Determine use and consequences

For each candidate, establish what it does, what owns it, what invokes it, and what will remain after removal. Follow package dependencies and visible references through configurations, services, autostart entries, scripts and relevant projects. Pay particular attention to accessibility tools, input, audio, networking, the desktop and boot preparation.

The user not launching a command directly does not prove it is unused. An editor or application may call it in the background. Conversely, removing an installer's download cache does not uninstall libraries already installed elsewhere. Explain these distinctions using the paths and launch mechanism actually inspected.

Check running processes and active installation or compilation before removing their files. For a manual installation, distinguish its active payload from previous copies. Keep source code, tracked work and deployed configurations out of a build-cache deletion unless explicitly selected separately.

When presenting distribution defaults, prove origin from local installation records, manifests or other available evidence. Label unproven origin as unknown. Also identify packages requested by the user's configuration scripts, since they may return on redeployment. Removing a package locally does not authorize changing those scripts.

For packages, preview the complete transaction. Separate selected packages from dependencies a recursive removal would add. Check uses outside the package manager before proposing those extra dependencies. Never bypass dependency checks to force a cleanup.

## Ask for selections

Use the question tool. Present independent choices together, grouped by function or consequence so the user can select multiple items without interpreting package names alone. Always offer the shared instructions' explanation option and allow a written answer.

Explain each item's purpose in the question context. Each option states its concrete advantages and disadvantages, including disk size when known, lost functions, regeneration or download cost, and configuration scripts that would reinstall it. State whether selection only prepares a deletion list. Unselected items stay.

Resolve consequential subchoices after selection: how many package versions to retain, whether uninstalled-package archives should remain, whether to retain modified build recipes, and whether to keep installed executables while deleting development outputs. Do not invent a retention policy or silently expand a selected parent category.

Keep uncertain candidates pending while resolving their uses. Report what was checked and what remains unknown; do not describe absence of a found reference as proof of universal non-use. Recommend a path only when the user's stated priorities or a verified requirement order the choices.

## Confirm and execute the exact lot

Once choices are settled, show the exact paths, packages or tool-resolved selection that will be removed, the exclusions and the consequences. Obtain destructive-action confirmation through the question tool. An earlier multiple-selection answer does not substitute for this confirmation unless it already showed the exact destructive targets and effects.

Before acting, give the required intent update. Recheck targets if state has changed. Use the owning tool's cache or package cleanup mechanism when it matches the selected scope; otherwise delete only the inspected, confirmed files. Elevate only the operations that need it. Never run a blanket package, home-directory, cache or container purge as a substitute for the selected lot.

Do not erase local modifications, recovery files or persistent application data under a broader cache label. Do not restart services, change appearance, alter installer rules, create a maintenance schedule, commit or publish unless those actions belong to the user's selected scope.

Stop on an unexpected dependency, changed target or broken premise and use the shared deviation procedure. Report partial completion instead of silently substituting another cleanup.

## Verify and report

Check that selected targets are absent, excluded files and tools remain, and any package transaction preserved dependency integrity. Verify relevant running accessibility or other affected services without launching destructive checks or disturbing the user's session.

Measure free space before and after when meaningful. Distinguish actual filesystem space recovered from removed file sizes and package estimates. Report what was removed, what was preserved, what failed or remains pending, and which future operations need rebuilding or downloading.

Do not append an intervention log to the environment inventory. Update an existing capability entry only if availability, access or a useful limitation changed. Cache cleanup alone normally needs no inventory update. Do not store checksums, process IDs, temporary evidence or test counts there.

If the user requests a cleanup record, keep it in the chosen document and distinguish completed local removals from installer changes still to do. Otherwise finish with the result in the conversation. A guide or filesystem readback is not evidence that an agent has successfully exercised the whole workflow; state verification limits accurately.
