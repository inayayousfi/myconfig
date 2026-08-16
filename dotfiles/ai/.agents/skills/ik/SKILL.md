---
name: ik
description: Use when the user asks for a visual map or explanation of how a repository or focused subsystem is structured and interacts.
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

### Visual theme

Pick a dark, neutral base for every box regardless of category — not a
different hue per category. Reserve actual color for a small, fixed set of
meanings (e.g. normal flow, gated/conditional, warning/destructive), each
color used for that one meaning consistently throughout the file. Convey
category or grouping through layout and labeling instead — a handful of
accent colors plus a neutral base can't distinguish six-plus categories by
hue, so don't try to make color carry that load.

### Layout: one coordinate system, room to breathe

Position every diagram node in one shared coordinate space — don't nest an
absolutely-positioned node inside another absolutely-positioned node. CSS
makes a nested node's coordinates relative to its parent's own box, not the
page, so this silently renders things in the wrong place. For visual
grouping, draw a separate background shape (its own sibling element, not a
wrapper) behind the nodes it's meant to enclose, rather than nesting them
inside it.

Give the whole thing room to breathe — page margins, gaps between sections,
padding inside boxes. Don't default to the tightest spacing that technically
avoids overlap; cramped spacing reads poorly even when nothing is literally
broken. Likewise, size every box a bit more generously than its text
strictly needs — exact wrapping can't be verified without a real browser to
render in, so leave margin for that uncertainty rather than clipping or
hiding overflow.

### Node content vs. detail panel

Diagram boxes carry only a short label — generic role plus the real name —
and at most one short tagline. Never put paragraphs or lists inside a box.
The full explanation for each node lives elsewhere in the document and
surfaces when that node is clicked (a panel, a drawer, whatever mechanism
fits) — not inside the box itself. A diagram with prose crammed into its
boxes has failed at this just as much as a prose-only page has.

### Arrow/edge labels must actually be legible

Check two things for every label that annotates an arrow: paint order, and
available space. A label that shares a layer with the arrows (e.g. SVG
`<text>` alongside SVG `<path>`) can end up painted behind boxes that come
later in the document — make sure labels render on top of anything they
might overlap. And check that the gap you're placing a label in is actually
wider than the label's own text, not just mathematically between two
coordinates — a narrow gap with a long label will overflow into whatever's
on either side regardless of paint order.

### No horizontal scroll, ever

The diagram must never require horizontal scrolling, at any window width.
Scale the whole diagram down to fit the available width rather than
reflowing its layout — a responsive/flex reflow sacrifices the precise
arrow routing that makes this diagram useful in the first place, so prefer
shrinking it uniformly over rebuilding it as a flow layout.

## Step 3 — Save it

Write the file to `/tmp/ik/`, creating the directory if it doesn't exist.
Name it with a timestamp and a scope slug:

```
/tmp/ik/<YYYY-MM-DD-HHMMSS>-<scope>.html
```

Where `<scope>` is `full` when called with no argument, or a slugified
version of the focus argument otherwise (e.g. `payments`).

## Step 4 — Report back

Check whether you're running under WSL (`grep -qi microsoft /proc/version`
or a set `$WSL_DISTRO_NAME`). If so, the browser that opens the link lives on
the Windows side, not in this Linux filesystem, so a plain `file:///tmp/...`
path will not resolve. Convert it first with `wslpath -w <path>` and give
that Windows-style path (e.g. `\\wsl.localhost\archlinux\tmp\ik\...html`) —
it pastes directly into a Windows Explorer or browser address bar. If not
running under WSL, give the plain `file://` path instead.

Reply with one short sentence plus that path — nothing else. Do not restate
or summarize the explanation in chat; the HTML file is the deliverable. Do
not attempt to auto-open the file in a browser.
