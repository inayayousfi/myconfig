# 🖤🌸 Style Guide — Black & Pink Theme

> Unified visual reference for all my configs and projects — web, desktop environments, editors, terminals. Minimalist, high-contrast, soft.

---

## 🎨 Color Palette

### Core Backgrounds

| Name              | Hex       | RGB        | Usage                                |
| ----------------- | --------- | ---------- | ------------------------------------ |
| `black-deep`      | `#000000` | 0, 0, 0    | Main background, root surfaces       |
| `black-surface`   | `#0a0a0a` | 10, 10, 10 | Cards, panels, floating elements     |
| `black-raised`    | `#111111` | 17, 17, 17 | Slightly elevated elements, sidebars |
| `black-border`    | `#1a1a1a` | 26, 26, 26 | Subtle borders, dividers, separators |
| `black-highlight` | `#222222` | 34, 34, 34 | Hover backgrounds, selected lines    |

### Text

| Name             | Hex       | RGB           | Usage                                      |
| ---------------- | --------- | ------------- | ------------------------------------------ |
| `text-heading`   | `#f0f2f7` | 240, 242, 247 | Titles, strong elements, active items      |
| `text-body`      | `#d0d6e0` | 208, 214, 224 | Main body text                             |
| `text-muted`     | `#a0aabe` | 160, 170, 190 | Labels, metadata, comments, line numbers   |
| `text-disabled`  | `#4a4f5e` | 74, 79, 94    | Inactive elements, placeholders            |
| `text-invisible` | `#2a2d36` | 42, 45, 54    | Barely visible — indent guides, ghost text |

### Accent — Pink

| Name           | Hex         | Usage                                              |
| -------------- | ----------- | -------------------------------------------------- |
| `accent`       | `#ff4ead`   | CTAs, links, cursor, highlights, active indicators |
| `accent-hover` | `#c02679`   | Hover / pressed / active state                     |
| `accent-dim`   | `#a0205f`   | Darker variant — visited links, subtle markers     |
| `accent-soft`  | `#ff4ead1a` | Tinted backgrounds, selections, badges             |
| `accent-glow`  | `#ff4ead33` | Focus rings, glow effects                          |

### Semantic Colors

| Name      | Hex       | Usage                                      |
| --------- | --------- | ------------------------------------------ |
| `success` | `#4ade80` | Confirmations, OK status, diffs added      |
| `warning` | `#facc15` | Soft alerts, caution                       |
| `error`   | `#f87171` | Errors, destructive actions, diffs removed |
| `info`    | `#60a5fa` | Neutral info, hints                        |
| `hint`    | `#a78bfa` | Suggestions, LSP hints, subtle cues        |

> 💡 Semantic colors are kept intentionally desaturated so they never compete with the pink accent.

### Terminal / ANSI Colors

> For terminal emulators (Kitty, Alacritty, WezTerm, etc.)

| Slot           | Name                  | Hex       |
| -------------- | --------------------- | --------- |
| Black          | `ansi-black`          | `#000000` |
| Bright Black   | `ansi-bright-black`   | `#2a2d36` |
| Red            | `ansi-red`            | `#f87171` |
| Bright Red     | `ansi-bright-red`     | `#fca5a5` |
| Green          | `ansi-green`          | `#4ade80` |
| Bright Green   | `ansi-bright-green`   | `#86efac` |
| Yellow         | `ansi-yellow`         | `#facc15` |
| Bright Yellow  | `ansi-bright-yellow`  | `#fde047` |
| Blue           | `ansi-blue`           | `#60a5fa` |
| Bright Blue    | `ansi-bright-blue`    | `#93c5fd` |
| Magenta        | `ansi-magenta`        | `#ff4ead` |
| Bright Magenta | `ansi-bright-magenta` | `#ff85c8` |
| Cyan           | `ansi-cyan`           | `#22d3ee` |
| Bright Cyan    | `ansi-bright-cyan`    | `#67e8f9` |
| White          | `ansi-white`          | `#d0d6e0` |
| Bright White   | `ansi-bright-white`   | `#f0f2f7` |

---

## 🔤 Typography

### Font Stack

| Role                      | Font      | Fallback                                           |
| ------------------------- | --------- | -------------------------------------------------- |
| **Monospace / primary**   | `Iosevka` | `ui-monospace`, `Menlo`, `Consolas`, `Courier New` |
| **Sans-serif (optional)** | `Inter`   | `system-ui`, `sans-serif`                          |

> Everything defaults to **Iosevka** — consistent monospace across web, editor and terminal.

### Type Scale

| Level         | Size      | Usage                       |
| ------------- | --------- | --------------------------- |
| `h1`          | `3.052em` | Page title                  |
| `h2`          | `2.441em` | Major section               |
| `h3`          | `1.953em` | Subsection                  |
| `h4`          | `1.563em` | Block title                 |
| `h5`          | `1.25em`  | Strong label                |
| `body`        | `20px`    | Main body text (desktop)    |
| `body-sm`     | `18px`    | Main body text (mobile)     |
| `small`       | `16px`    | Inputs, captions, UI labels |
| `code-inline` | inherited | Inline code snippets        |

### Text Properties

| Property                          | Value     |
| --------------------------------- | --------- |
| `line-height` body                | `1.7`     |
| `line-height` headings            | `1.2`     |
| `font-weight` normal              | `400`     |
| `font-weight` bold                | `700`     |
| `letter-spacing` headings         | `-0.02em` |
| `letter-spacing` body             | `0`       |
| `letter-spacing` uppercase labels | `0.08em`  |

---

## 📐 Spacing & Layout

### Base Units

| Name        | Value  | Usage                         |
| ----------- | ------ | ----------------------------- |
| `space-xs`  | `4px`  | Micro spacing, inline padding |
| `space-sm`  | `8px`  | Tight spacing, icon gaps      |
| `space-md`  | `16px` | Standard spacing              |
| `space-lg`  | `24px` | Section gaps, card padding    |
| `space-xl`  | `48px` | Page padding (desktop)        |
| `space-2xl` | `64px` | Large section separations     |

### Layout

| Name            | Value   | Usage                        |
| --------------- | ------- | ---------------------------- |
| `content-width` | `720px` | Main content column          |
| `sidebar-width` | `240px` | Sidebars, file trees, panels |
| `panel-width`   | `320px` | Wider panels, drawers        |

### Breakpoints

| Name      | Value     |
| --------- | --------- |
| `mobile`  | `≤ 720px` |
| `desktop` | `> 720px` |

---

## 🧱 UI Components

### Buttons

| Variant           | Background  | Text      | Border              |
| ----------------- | ----------- | --------- | ------------------- |
| **Primary**       | `#ff4ead`   | `#000000` | none                |
| **Primary hover** | `#c02679`   | `#000000` | none                |
| **Ghost**         | transparent | `#ff4ead` | `1px solid #ff4ead` |
| **Ghost hover**   | `#ff4ead1a` | `#ff4ead` | `1px solid #ff4ead` |
| **Disabled**      | `#1a1a1a`   | `#4a4f5e` | none                |

### Links

| State   | Color     |
| ------- | --------- |
| Default | `#ff4ead` |
| Hover   | `#c02679` |
| Visited | `#a0205f` |

### Inputs / Textarea

| Property       | Value                 |
| -------------- | --------------------- |
| Background     | `#0a0a0a`             |
| Border         | `1px solid #1a1a1a`   |
| Border (focus) | `1px solid #ff4ead`   |
| Focus ring     | `0 0 0 2px #ff4ead33` |
| Text           | `#d0d6e0`             |
| Placeholder    | `#4a4f5e`             |

### Cards / Panels

| Property      | Value               |
| ------------- | ------------------- |
| Background    | `#0a0a0a`           |
| Border        | `1px solid #1a1a1a` |
| Border-radius | `8px`               |
| Padding       | `24px`              |

### Code

| Variant       | Background | Text      | Radius |
| ------------- | ---------- | --------- | ------ |
| Inline        | `#0a0a0a`  | `#f0f2f7` | `4px`  |
| Block `<pre>` | `#0a0a0a`  | `#f0f2f7` | `8px`  |

### Blockquote

| Property     | Value               |
| ------------ | ------------------- |
| Left border  | `4px solid #ff4ead` |
| Left padding | `20px`              |
| Font-size    | `1.333em`           |
| Text color   | `#a0aabe`           |

### Notifications / Toasts

| Variant | Left border | Background |
| ------- | ----------- | ---------- |
| Success | `#4ade80`   | `#0a0a0a`  |
| Warning | `#facc15`   | `#0a0a0a`  |
| Error   | `#f87171`   | `#0a0a0a`  |
| Info    | `#60a5fa`   | `#0a0a0a`  |

---

## 🖥️ Editor & IDE (Neovim, VS Code…)

### Syntax Highlighting

| Scope                | Color     | Notes                                  |
| -------------------- | --------- | -------------------------------------- |
| Keywords             | `#ff4ead` | `if`, `for`, `return`, `fn`…           |
| Functions / Methods  | `#ff85c8` | Bright magenta — calls and definitions |
| Types / Classes      | `#f0f2f7` | Near-white, strong                     |
| Variables            | `#d0d6e0` | Default body text                      |
| Parameters           | `#c0c8d8` | Slightly dimmer than variables         |
| Strings              | `#4ade80` | Green — easy to spot                   |
| Numbers / Booleans   | `#facc15` | Yellow                                 |
| Comments             | `#4a4f5e` | Muted, never distracting               |
| Operators            | `#a0aabe` | Neutral, recedes                       |
| Punctuation          | `#2a2d36` | Barely visible                         |
| Constants / Enums    | `#22d3ee` | Cyan                                   |
| Macros / Decorators  | `#a78bfa` | Purple hint                            |
| Errors (underline)   | `#f87171` | Red                                    |
| Warnings (underline) | `#facc15` | Yellow                                 |
| Hints (underline)    | `#a78bfa` | Purple                                 |

### Editor Chrome

| Element                | Color       |
| ---------------------- | ----------- |
| Background             | `#000000`   |
| Active line background | `#111111`   |
| Selection background   | `#ff4ead1a` |
| Search match           | `#ff4ead33` |
| Current search match   | `#ff4ead66` |
| Indent guides          | `#1a1a1a`   |
| Active indent guide    | `#ff4ead33` |
| Line numbers           | `#4a4f5e`   |
| Active line number     | `#ff4ead`   |
| Cursor                 | `#ff4ead`   |
| Cursor (insert block)  | `#ff4ead`   |

### UI Panels (file tree, status bar…)

| Element                  | Color       |
| ------------------------ | ----------- |
| Panel background         | `#0a0a0a`   |
| Panel border             | `#1a1a1a`   |
| Active item background   | `#ff4ead1a` |
| Active item text         | `#f0f2f7`   |
| Active item left bar     | `#ff4ead`   |
| Inactive item text       | `#a0aabe`   |
| Status bar background    | `#000000`   |
| Status bar text          | `#a0aabe`   |
| Status bar accent (mode) | `#ff4ead`   |

---

## 🖥️ Desktop Environment

### Window Manager (Hyprland, i3, Sway…)

| Element                | Color     |
| ---------------------- | --------- |
| Active window border   | `#ff4ead` |
| Inactive window border | `#1a1a1a` |
| Urgent window border   | `#f87171` |
| Bar background         | `#000000` |
| Bar text               | `#d0d6e0` |
| Workspace active       | `#ff4ead` |
| Workspace inactive     | `#4a4f5e` |
| Workspace urgent       | `#f87171` |

### Bar / Status Bar

| Element          | Color     |
| ---------------- | --------- |
| Background       | `#000000` |
| Module text      | `#d0d6e0` |
| Module accent    | `#ff4ead` |
| Module icons     | `#ff4ead` |
| Separator        | `#1a1a1a` |
| Alert / critical | `#f87171` |

### App Launcher (Rofi, Wofi…)

| Element             | Color       |
| ------------------- | ----------- |
| Background          | `#000000`   |
| Input background    | `#0a0a0a`   |
| Input border        | `#ff4ead`   |
| Normal text         | `#d0d6e0`   |
| Selected background | `#ff4ead1a` |
| Selected text       | `#f0f2f7`   |
| Selected border     | `#ff4ead`   |

---

## ✨ Visual Effects & Tokens

### Shadows

```text
shadow-sm   : 0 2px 6px rgba(160, 170, 190, 0.25)
shadow-md   : 0 8px 24px rgba(160, 170, 190, 0.20)
shadow-lg   : 0 16px 32px rgba(160, 170, 190, 0.15)
shadow-pink : 0 0 16px rgba(255, 78, 173, 0.25)
shadow-glow : 0 0 32px rgba(255, 78, 173, 0.15)
```

### Border Radius

| Name          | Value    | Usage                         |
| ------------- | -------- | ----------------------------- |
| `radius-sm`   | `4px`    | Inline elements, badges, tags |
| `radius-md`   | `8px`    | Cards, images, blocks         |
| `radius-lg`   | `16px`   | Modals, large panels          |
| `radius-full` | `9999px` | Pills, avatars, round buttons |

### Gradients

```text
bg-gradient    : linear-gradient(rgba(10,10,10,1), #000000)
accent-glow    : radial-gradient(circle, #ff4ead22 0%, transparent 70%)
surface-fade   : linear-gradient(to bottom, #111111, #000000)
```

### Transitions

| Name              | Value        | Usage                            |
| ----------------- | ------------ | -------------------------------- |
| `transition-fast` | `150ms ease` | Hovers, toggles                  |
| `transition-base` | `250ms ease` | Most UI interactions             |
| `transition-slow` | `400ms ease` | Panels, modals, page transitions |

### Opacity Levels

| Name               | Value | Usage             |
| ------------------ | ----- | ----------------- |
| `opacity-disabled` | `0.4` | Disabled elements |
| `opacity-muted`    | `0.6` | Dimmed elements   |
| `opacity-overlay`  | `0.8` | Backdrop overlays |

---

## ♿ Accessibility

- `text-body` / `black-deep` contrast ratio: **≥ 10:1** ✅
- `text-heading` / `black-deep` contrast ratio: **≥ 18:1** ✅
- `accent` / `black-deep` contrast ratio: **≥ 5.5:1** ✅
- All interactive elements must have a visible focus state (`accent-glow` ring)
- Never rely on color alone to convey meaning
- `.sr-only` pattern available for screen-reader-only content

---

## 🗒️ Design Principles

- **Black first** — backgrounds are always as dark as possible, surfaces rise gradually
- **Pink is sparse** — it guides the eye, never decorates for decoration's sake
- **Monospace everywhere** — Iosevka across web, editor and terminal, no font mixing
- **Soft shadows** — shadows use muted warm-gray, never harsh black opaque drops
- **Semantic over aesthetic** — every color choice has a function
- **Consistent elevation** — `#000000` → `#0a0a0a` → `#111111` → `#1a1a1a`, always in that order
