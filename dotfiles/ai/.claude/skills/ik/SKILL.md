---
name: ik
description: Explore this repo (or a focused subsystem of it) and produce a single self-contained HTML page diagramming the systems and how they interact, explained generically first with the real repo-specific name given immediately in parentheses. Use when the user runs /ik, or asks to be shown/explained/mapped/diagrammed how the codebase or a part of it is put together and interacts.
---

# ik

Builds a mental model of this repo's systems and their interactions, then renders
it as a single HTML file you can open and read. The explanation is written
generically first (so the shape of the system is understandable on its own
terms) with the real name of each thing given immediately afterward in
parentheses, so you can map straight from the concept to the concrete.

## Scope

- **No argument**: explore the whole repo.
- **With an argument** (e.g. `/ik payments`): explore that subsystem in depth,
  plus a shallow pass on whatever it directly talks to, so the diagram still
  shows its boundaries and doesn't render it as an isolated island.

## Step 1 — Explore

Do this yourself, inline, in the current conversation. Do not dispatch a
subagent for this — building the model requires continuous judgment about what
matters, not a mechanical pass.

Use whatever mix of tools fits the repo: entry points, manifests/config
(package.json, docker-compose, CI configs, Makefiles), directory structure,
top-level modules, READMEs, and enough source reading to see how pieces call
or depend on each other.

Depth is your judgment call, bounded by:
- **Floor**: you must come out understanding what each system is, why it
  exists, and how it interacts with the rest.
- **Ceiling**: never go down to explaining how individual functions compute
  their results. This is an architecture map, not documentation of internals.

## Step 2 — Write the HTML

Produce exactly one HTML file, fully self-contained: inline `<style>` and
`<script>`, inline SVG for any diagram elements, no external CDN or network
dependencies of any kind. It must render correctly opened straight from disk
via `file://` with no server and no internet access.

Make it visually rich, not a wall of text — a real diagram (boxes,
containers, arrows showing call/data flow, groupings for layers or
boundaries), using CSS/SVG/JS as needed to make the interactions explicit and
easy to scan at a glance. Prose-only output is a failure of this skill; the
point is that it reads faster and clearer than documentation would.

**Naming convention (the core trick of this skill):** describe each
component and interaction generically first — what it *is* and what role it
plays — then give the real name immediately afterward in parentheses, inline,
in the same label or sentence. For example: "the request router (`api-gateway`)
forwards jobs to the background worker pool (`worker-service`)". Do not defer
real names to a separate legend, appendix, or mapping table — the parenthetical
must sit right next to the generic description so the two read as one unit.

## Step 3 — Save it

Write the file to `/tmp/ik/`, creating the directory if it doesn't exist.
Name it with a timestamp and a scope slug:

```
/tmp/ik/<YYYY-MM-DD-HHMMSS>-<scope>.html
```

Where `<scope>` is `full` when called with no argument, or a slugified
version of the focus argument otherwise (e.g. `payments`).

## Step 4 — Report back

Reply with one short sentence plus the `file://` path to the file — nothing
else. Do not restate or summarize the explanation in chat; the HTML file is
the deliverable. Do not attempt to auto-open the file in a browser.
