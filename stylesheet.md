# 🪻 Style Guide — Lilas

> Unified visual reference for all my configs and projects: web, desktop, editors, terminals. Pastel, dense, soft, and never neutral.

The palette comes from the desktop video wallpaper (`dotfiles/assets/wallpapers/`) and from the mint and lilac of my general aesthetic. KDE applies it through the `Lilas` color scheme (`dotfiles/plasma/.local/share/color-schemes/Lilas.colors`).

---

## 🗒️ Design Principles

- **Expression, not erasure.** Nothing neutral, nothing that hides. No grey, no beige.
- **Two colors lead.** Lilac and mint lead together; pink, orange and sky blue answer around them. The more colors answer each other, the better, as long as it is clear which ones lead.
- **Neon is one strong element at a time.** The saturated pink marks the one thing that matters on screen: the cursor, the selection, the active item.
- **Flat colors, sharp boundaries.** Colors sit in flat areas and change cleanly. No blended gradients: shapes may wave, colors do not mix.
- **Everything is soft.** Round and organic shapes, generous radii, very few straight angles. The only sharpness is the border between two colors.
- **Dark is a chosen void.** A dark area is a place where I decided there is nothing: the lilac void, or transparency that shows the wallpaper behind. Never a default black.
- **Full but ordered.** Dense and maximalist, but aligned and organized. The overflow is in shapes and colors, never in disorder.
- **Cute is welcome.** Mascots, characters and plush-like icons are part of the pleasure.

---

## 🎨 Color Palette

### Backgrounds

From deepest to highest. Each step is lilac, never grey.

| Name              | Hex       | RGB         | Usage                                                  |
| ----------------- | --------- | ----------- | ------------------------------------------------------ |
| `lilac-void`      | `#1c1528` | 28, 21, 40  | Lists, text areas, editors, terminals, the chosen void |
| `lilac-base`      | `#241c33` | 36, 28, 51  | Window background                                      |
| `lilac-raised`    | `#2e2442` | 46, 36, 66  | Buttons, headers, title bars, bars, tooltips           |
| `lilac-border`    | `#3a2f52` | 58, 47, 82  | Borders, dividers, separators                          |
| `lilac-highlight` | `#4a3a68` | 74, 58, 104 | Hover backgrounds, current line, pressed buttons       |

### Text

| Name             | Hex       | RGB           | Usage                                 |
| ---------------- | --------- | ------------- | ------------------------------------- |
| `text-heading`   | `#fbf6fe` | 251, 246, 254 | Titles, strong elements, active items |
| `text-body`      | `#f1e8f8` | 241, 232, 248 | Main text                             |
| `text-muted`     | `#b8a7cc` | 184, 167, 204 | Labels, metadata, inactive items      |
| `text-faint`     | `#7a6a92` | 122, 106, 146 | Comments, line numbers, placeholders  |
| `text-invisible` | `#4a3a68` | 74, 58, 104   | Indent guides, ghost text             |

### Leading Colors

| Name    | Hex       | RGB           | Usage                                      |
| ------- | --------- | ------------- | ------------------------------------------ |
| `lilac` | `#b296cc` | 178, 150, 204 | Focus, active items, keywords, primary hue |
| `mint`  | `#9ee6c8` | 158, 230, 200 | Primary buttons, success, strings          |

### Answering Colors

| Name         | Hex       | RGB           | Usage                               |
| ------------ | --------- | ------------- | ----------------------------------- |
| `pink-soft`  | `#f1c6dd` | 241, 198, 221 | Functions, soft highlights, badges  |
| `orange`     | `#ffb38a` | 255, 179, 138 | Warnings, numbers                   |
| `sky`        | `#9cc8f0` | 156, 200, 240 | Links, types, information           |
| `butter`     | `#ffe39a` | 255, 227, 154 | Constants, the yellow terminal slot |
| `lilac-pale` | `#c9a8e8` | 201, 168, 232 | Visited links                       |

### Neon Accent

| Name          | Hex         | Usage                                       |
| ------------- | ----------- | ------------------------------------------- |
| `accent`      | `#ff4ead`   | Selection, cursor, the one active indicator |
| `accent-dim`  | `#cc3e8a`   | Pressed or alternate selection              |
| `accent-soft` | `#ff4ead26` | Tinted selection backgrounds, badges        |
| `accent-glow` | `#ff4ead40` | Focus rings                                 |

Text on `accent` uses `lilac-void`, never white.

### Semantic Colors

| Name      | Hex       | Usage                                      |
| --------- | --------- | ------------------------------------------ |
| `success` | `#9ee6c8` | Confirmations, OK status, diffs added      |
| `warning` | `#ffb38a` | Soft alerts, caution                       |
| `error`   | `#ff6b8b` | Errors, destructive actions, diffs removed |
| `info`    | `#9cc8f0` | Neutral information, hints                 |
| `hint`    | `#b296cc` | Suggestions, LSP hints                     |

### Terminal / ANSI Colors

| Slot           | Name                  | Hex       |
| -------------- | --------------------- | --------- |
| Black          | `ansi-black`          | `#1c1528` |
| Bright Black   | `ansi-bright-black`   | `#7a6a92` |
| Red            | `ansi-red`            | `#ff6b8b` |
| Bright Red     | `ansi-bright-red`     | `#ff9ab0` |
| Green          | `ansi-green`          | `#9ee6c8` |
| Bright Green   | `ansi-bright-green`   | `#c4f2de` |
| Yellow         | `ansi-yellow`         | `#ffe39a` |
| Bright Yellow  | `ansi-bright-yellow`  | `#fff0c4` |
| Blue           | `ansi-blue`           | `#9cc8f0` |
| Bright Blue    | `ansi-bright-blue`    | `#c3defa` |
| Magenta        | `ansi-magenta`        | `#ff4ead` |
| Bright Magenta | `ansi-bright-magenta` | `#ff85c8` |
| Cyan           | `ansi-cyan`           | `#8fe3ef` |
| Bright Cyan    | `ansi-bright-cyan`    | `#bff1f7` |
| White          | `ansi-white`          | `#f1e8f8` |
| Bright White   | `ansi-bright-white`   | `#fbf6fe` |

Bright black is readable on the void, because shells use it for suggestions and comments.

---

## 🫧 Transparency & Blur

Transparency is how the void shows the wallpaper. A see-through surface is always one of the levels below, never an arbitrary value.

| Name             | Opacity            | Where                                                        |
| ---------------- | ------------------ | ------------------------------------------------------------ |
| `glass-terminal` | `85%`              | Herdr window, its terminals, gaps and bars; Neovim inside it |
| `glass-menu`     | `80%`              | KDE menus (Breeze `MenuOpacity`)                             |
| `glass-bar`      | Plasma translucent | KDE top bar and dock                                         |
| `solid`          | `100%`             | Drawers, dialogs, anything read for long                     |

- **Blur:** KWin blurs what shows behind see-through surfaces at strength `3` out of `15`, light enough that the wallpaper stays recognizable.
- **No transparent text.** Only backgrounds become see-through; text and icons stay fully opaque.
- **Neovim draws no background of its own** inside Herdr: its floating menus and windows let the glass show through too.

---

## 🎞️ Wallpapers

| Where       | Video                              | Behavior                                                          |
| ----------- | ---------------------------------- | ----------------------------------------------------------------- |
| Desktop     | `anime-cat-girl-lying-down-in-bed` | Muted loop; pauses behind maximized windows and below 30% battery |
| Lock screen | `lofi-pink-town`                   | Muted loop                                                        |

A new wallpaper should be pastel, calm enough behind see-through terminals, and carry lilac or mint. When it changes, the palette is checked against it once; the leading colors stay unless the new wallpaper clearly fights them.

---

## 🔤 Typography

### Font Stack

| Role                     | Font                      | Fallback                                     |
| ------------------------ | ------------------------- | -------------------------------------------- |
| **Sans-serif / primary** | `IBM Plex Sans`           | `system-ui`, `sans-serif`                    |
| **Monospace**            | `BlexMono Nerd Font Mono` | `IBM Plex Mono`, `ui-monospace`, `monospace` |
| **Serif**                | `IBM Plex Serif`          | `Georgia`, `serif`                           |

Interfaces use **IBM Plex Sans**, code and terminals use **BlexMono Nerd Font Mono** (IBM Plex Mono with Nerd Font icons), and long reading text uses **IBM Plex Serif**. fontconfig makes these three the `sans-serif`, `monospace` and `serif` defaults.

### Desktop Sizes

| Use                               | Size   |
| --------------------------------- | ------ |
| Interface, menus, toolbars, title | `12pt` |
| Fixed-width text                  | `12pt` |
| Smallest readable text            | `10pt` |

### Web Type Scale

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
| `font-weight` bold                | `600`     |
| `letter-spacing` headings         | `-0.01em` |
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

| Variant           | Background  | Text      | Border                |
| ----------------- | ----------- | --------- | --------------------- |
| **Primary**       | `#9ee6c8`   | `#1c1528` | none                  |
| **Primary hover** | `#c4f2de`   | `#1c1528` | none                  |
| **Secondary**     | `#2e2442`   | `#f1e8f8` | none                  |
| **Ghost**         | transparent | `#b296cc` | `1.5px solid #b296cc` |
| **Ghost hover**   | `#b296cc26` | `#b296cc` | `1.5px solid #b296cc` |
| **Danger**        | `#ff6b8b`   | `#1c1528` | none                  |
| **Disabled**      | `#2e2442`   | `#7a6a92` | none                  |

Buttons are pills (`radius-full`).

### Links

| State   | Color     |
| ------- | --------- |
| Default | `#9cc8f0` |
| Hover   | `#c3defa` |
| Visited | `#c9a8e8` |

### Inputs / Textarea

| Property       | Value                 |
| -------------- | --------------------- |
| Background     | `#1c1528`             |
| Border         | `1.5px solid #3a2f52` |
| Border (focus) | `1.5px solid #b296cc` |
| Focus ring     | `0 0 0 3px #ff4ead40` |
| Text           | `#f1e8f8`             |
| Placeholder    | `#7a6a92`             |
| Radius         | `radius-md`           |

### Cards / Panels

| Property      | Value                 |
| ------------- | --------------------- |
| Background    | `#241c33`             |
| Border        | `1.5px solid #3a2f52` |
| Border-radius | `radius-lg`           |
| Padding       | `24px`                |

### Code

| Variant       | Background | Text      | Radius      |
| ------------- | ---------- | --------- | ----------- |
| Inline        | `#2e2442`  | `#f1c6dd` | `radius-sm` |
| Block `<pre>` | `#1c1528`  | `#f1e8f8` | `radius-md` |

### Blockquote

| Property     | Value               |
| ------------ | ------------------- |
| Left border  | `4px solid #b296cc` |
| Left padding | `20px`              |
| Font-size    | `1.333em`           |
| Text color   | `#b8a7cc`           |

### Notifications / Toasts

| Variant | Left border | Background |
| ------- | ----------- | ---------- |
| Success | `#9ee6c8`   | `#241c33`  |
| Warning | `#ffb38a`   | `#241c33`  |
| Error   | `#ff6b8b`   | `#241c33`  |
| Info    | `#9cc8f0`   | `#241c33`  |

---

## 🖥️ Editor & IDE (Neovim, VS Code…)

### Syntax Highlighting

| Scope                | Color     | Notes                          |
| -------------------- | --------- | ------------------------------ |
| Keywords             | `#b296cc` | Lilac leads                    |
| Functions / Methods  | `#f1c6dd` | Soft pink                      |
| Types / Classes      | `#9cc8f0` | Sky                            |
| Variables            | `#f1e8f8` | Body text                      |
| Parameters           | `#e2d4ee` | Slightly dimmer than variables |
| Strings              | `#9ee6c8` | Mint leads                     |
| Numbers / Booleans   | `#ffb38a` | Orange                         |
| Constants / Enums    | `#ffe39a` | Butter                         |
| Macros / Decorators  | `#ff85c8` | Bright pink                    |
| Comments             | `#7a6a92` | Faint, never gone              |
| Operators            | `#b8a7cc` | Muted, recedes                 |
| Punctuation          | `#7a6a92` | Faint                          |
| Errors (underline)   | `#ff6b8b` |                                |
| Warnings (underline) | `#ffb38a` |                                |
| Hints (underline)    | `#b296cc` |                                |

### Editor Chrome

| Element              | Color                                             |
| -------------------- | ------------------------------------------------- |
| Background           | transparent over `glass-terminal`, else `#1c1528` |
| Current line         | `#2e2442`                                         |
| Selection            | `#ff4ead26`                                       |
| Search match         | `#b296cc40`                                       |
| Current search match | `#ff4ead66`                                       |
| Indent guides        | `#3a2f52`                                         |
| Active indent guide  | `#b296cc66`                                       |
| Line numbers         | `#7a6a92`                                         |
| Current line number  | `#ff4ead`                                         |
| Cursor               | `#ff4ead`                                         |

### UI Panels (file tree, status bar…)

| Element                | Color                                                |
| ---------------------- | ---------------------------------------------------- |
| Panel background       | `#241c33`                                            |
| Panel border           | `#3a2f52`                                            |
| Active item background | `#b296cc33`                                          |
| Active item text       | `#fbf6fe`                                            |
| Active item marker     | `#ff4ead`                                            |
| Inactive item text     | `#b8a7cc`                                            |
| Status bar background  | `#2e2442`                                            |
| Status bar text        | `#b8a7cc`                                            |
| Mode indicator         | `#9ee6c8` normal, `#ff4ead` insert, `#b296cc` visual |

---

## 🖥️ Desktop (KDE Plasma)

| Element             | Value                                          |
| ------------------- | ---------------------------------------------- |
| Global theme        | Breeze Dark                                    |
| Color scheme        | `Lilas`                                        |
| Accent              | `#ff4ead`                                      |
| Window background   | `#241c33`                                      |
| Title bar, active   | `#2e2442`, text `#f1e8f8`                      |
| Title bar, inactive | `#241c33`, text `#b8a7cc`                      |
| Top bar and dock    | `glass-bar` over the video wallpaper, blur `3` |
| Menus               | `glass-menu`                                   |
| Cursor              | Breeze, size `24`                              |

---

## ✨ Visual Effects & Tokens

### Shadows

Shadows are lilac, never black.

```text
shadow-sm   : 0 2px 6px rgba(28, 21, 40, 0.35)
shadow-md   : 0 8px 24px rgba(28, 21, 40, 0.40)
shadow-lg   : 0 16px 40px rgba(28, 21, 40, 0.45)
shadow-pink : 0 0 16px rgba(255, 78, 173, 0.30)
```

### Border Radius

Everything is round.

| Name          | Value    | Usage                       |
| ------------- | -------- | --------------------------- |
| `radius-sm`   | `6px`    | Inline code, badges, tags   |
| `radius-md`   | `12px`   | Inputs, images, blocks      |
| `radius-lg`   | `20px`   | Cards, modals, large panels |
| `radius-full` | `9999px` | Buttons, pills, avatars     |

### Patterns

Waves and ripples, stars and hearts, distorted checkerboards and stripes. Patterns use the palette's flat colors with sharp boundaries, never gradients.

### Transitions

| Name              | Value        | Usage                            |
| ----------------- | ------------ | -------------------------------- |
| `transition-fast` | `150ms ease` | Hovers, toggles                  |
| `transition-base` | `250ms ease` | Most UI interactions             |
| `transition-slow` | `400ms ease` | Panels, modals, page transitions |

### Opacity Levels

| Name               | Value  | Usage                           |
| ------------------ | ------ | ------------------------------- |
| `opacity-disabled` | `0.4`  | Disabled elements               |
| `opacity-muted`    | `0.6`  | Dimmed elements                 |
| `glass-menu`       | `0.8`  | See-through menus               |
| `glass-terminal`   | `0.85` | See-through terminal and editor |

---

## ♿ Accessibility

| Pair                           | Contrast   |
| ------------------------------ | ---------- |
| `text-body` on `lilac-base`    | **13.7:1** |
| `text-heading` on `lilac-void` | **16.6:1** |
| `text-muted` on `lilac-base`   | **7.3:1**  |
| `lilac` on `lilac-void`        | **6.8:1**  |
| `accent` on `lilac-void`       | **5.8:1**  |
| `lilac-void` on `accent`       | **5.8:1**  |
| `lilac-void` on `mint`         | **12.3:1** |
| `text-faint` on `lilac-void`   | **3.6:1**  |

- `text-faint` is for comments and secondary marks only, never for text that must be read.
- Every interactive element has a visible focus state (`accent-glow` ring).
- Never rely on color alone to convey meaning.
- On see-through surfaces, text contrast is checked against the darkest and lightest parts of the wallpaper behind the glass.
