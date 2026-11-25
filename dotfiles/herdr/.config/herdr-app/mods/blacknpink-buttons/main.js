import { View, div, svg } from "gpui-kit";
import { v_flex } from "gpui-base";
import { theme, run, layout, openOverlay } from "herdr";

function find(node, id) {
  if (node?.id === id) return node;
  for (const child of node?.children ?? []) {
    const found = find(child, id);
    if (found) return found;
  }
  return null;
}

const icon = (name) => `mods/workspace-chrome/${name}.svg`;

// Opens the side panel on one view, or closes it when that view already shows.
function togglePanel(id) {
  const panel = find(layout(), "panel"), body = find(layout(), "panel-body");
  if (panel && !panel.hidden && body?.active === id) run("layout.hide", { id: "panel" });
  else { run("layout.select", { stack: "panel-body", child: id }); run("layout.show", { id: "panel" }); }
}

const panelOn = (id) => {
  const panel = find(layout(), "panel"), body = find(layout(), "panel-body");
  return Boolean(panel && !panel.hidden && body?.active === id);
};

// null draws a separator.
const TOP = [
  ["goto", "search", "Aller à…", () => run("mod.open", { name: "picker" })],
  ["palette", "terminal", "Commandes", () => run("app.palette")],
  ["tab", "plus", "Nouvel onglet", () => openOverlay({ mod: "new-menu", kind: "popup", width: 250, height: 512 })],
  null,
  ["split-right", "panel-right", "Split à droite", () => run("pane.split", { direction: "right" })],
  ["split-down", "panel-bottom", "Split en bas", () => run("pane.split", { direction: "down" })],
  null,
  ["files", "files", "Fichiers", () => togglePanel("files"), () => panelOn("files")],
  ["changes", "git-compare", "Changements", () => togglePanel("changes"), () => panelOn("changes")],
];
const BOTTOM = [
  ["settings", "settings", "Réglages", () => run("app.settings")],
];

export default class Buttons extends View {
  render() {
    const t = theme();
    const button = ([id, name, label, action, on]) => {
      const active = on?.() ?? false;
      return div().id(id).size(28).flex_none().flex().items_center().justify_center().rounded(6).cursor_pointer()
        .bg(active ? t.selected : "#00000000").text_color(active ? t.accent : t.muted)
        .hover(s => s.bg(t.hover).text_color(t.foreground))
        .tooltip(label)
        // The block drags the window; a press on a button must not.
        .on_mouse_down("left", (_e, cx) => cx.stop_propagation())
        .on_click(action)
        .child(svg(icon(name)).size(15));
    };
    const item = (a, i) => a ? button(a) : div().id(`gap-${i}`).w(16).h(1).my(4).bg(t.border);
    // A card like the frame Herdr draws around the drawer beside it.
    return v_flex().size_full().bg(t.popover).border(1).border_color(t.border).rounded(10).items_center().py(8).gap(4)
      .children(TOP.map(item))
      .child(div().flex_1())
      .children(BOTTOM.map(item));
  }
}
