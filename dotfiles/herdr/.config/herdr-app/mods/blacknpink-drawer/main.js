// A left-edge drawer of spaces and agents. At rest only the edge column shows:
// each space's avatar and each agent's icon with its status. Hovering slides out the names.
import { View, div, svg, image } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { theme, run, spaces, agents, shared, setAgentOrder, settings } from "herdr";

// Herdr insets a float 12 px from the edge and frames it 5 px above and below.
const fullHeight = () => {
  const vp = globalThis.window?.viewport_size?.();
  return vp ? Math.max(200, vp.height / (settings().zoom ?? 1) - 2 * 12 - 10) : null;
};

const icon = (name) => `mods/workspace-chrome/${name}.svg`;
const EDGE = 42, NAMES = 210;
// The spaces mod's avatar colors.
const MARK = { ansi6: "#22a99a", ansi3: "#e3814f", ansi4: "#5b8def", ansi5: "#8b7cf6", ansi2: "#3fae6a", ansi1: "#d9649a" };
const clean = (s) => (s ?? "").replace(/^[^\p{L}\p{N}\p{Ps}\p{Pi}]+/u, "").trim();
const stop = (_e, cx) => cx.stop_propagation();

export default class Drawer extends View {
  render() {
    const t = theme();
    // The edge cell keeps what must show at rest; the rest only shows when open.
    // At rest Herdr shows only the edge column, cut at its right: the active mark
    // stays a rounded square around the avatar so nothing looks sliced.
    const row = (id, active, action, edge, rest) => h_flex().id(id).h(36).items_center().rounded(8).cursor_pointer()
      .text_color(active ? t.foreground : t.muted).hover(s => s.bg(t.hover))
      .on_mouse_down("left", stop).on_click(action)
      .child(h_flex().w(EDGE).flex_none().justify_center()
        .child(div().size(34).rounded(8).flex().items_center().justify_center().bg(active ? t.selected : "#00000000").child(edge)))
      .child(h_flex().w(NAMES).flex_none().min_w(0).gap(8).pl(14).pr(8).items_center().children(rest));
    const header = (label, extra) => h_flex().h(24).items_center().text_size(11.5).text_color(t.dim)
      .child(h_flex().w(EDGE).flex_none().justify_center().child(div().w(14).h(1).bg(t.border)))
      .child(h_flex().w(NAMES).pl(14).pr(8).items_center().child(div().flex_1().child(label)).when(Boolean(extra), el => el.child(extra)));
    const dot = (status) => status === "blocked" || status === "done" ? div().size(7).rounded_full().bg(t[status]) : null;

    const colors = shared("space.colors") ?? {};
    const spaceRows = spaces().map(s => {
      const c = colors[s.id] ?? "ansi6", fill = c.startsWith("#") ? c : (MARK[c] ?? t[c]);
      const mark = dot(s.status);
      const avatar = div().relative().size(22).rounded(6).bg(fill).flex().items_center().justify_center()
        .text_size(11.5).font_semibold().text_color("#ffffff").child([...s.name][0]?.toUpperCase() ?? "?")
        .when(Boolean(mark), el => el.child(mark.absolute().top(-2).right(-2)));
      return row(`space-${s.id}`, s.active, () => run("space.select", { id: s.id }), avatar,
        [div().flex_1().min_w(0).truncate().child(s.name)]);
    });

    const list = agents();
    setAgentOrder(list.map(a => a.address));
    const agentRows = list.map(a => {
      const p = a.program;
      const glyph = p?.icon ? (p.tint ? svg(p.icon).size(16).text_color(t.muted) : image(p.icon).size(16)) : div().size(16);
      const status = a.status === "working"
        ? svg(icon("spinner")).size(11).text_color(t.working).spin(900)
        : a.status === "blocked" || a.status === "done" ? div().size(7).rounded_full().bg(t[a.status]) : div().size(5).rounded_full().bg(t.idle);
      const edge = div().relative().size(24).flex().items_center().justify_center().child(glyph)
        .child(div().absolute().bottom(-3).right(-3).size(12).flex().items_center().justify_center().child(status));
      return row(`agent-${a.address}`, a.focused, () => run("agent.focus", { terminal: a.address }), edge,
        [div().flex_none().text_color(t.foreground).child(a.space_name), div().flex_1().min_w(0).truncate().text_color(t.dim).child(clean(a.title) || a.tab_name)]);
    });

    const newSpace = div().id("space-new").size(20).flex().items_center().justify_center().rounded(5).cursor_pointer()
      .text_color(t.dim).hover(s => s.bg(t.hover).text_color(t.foreground)).tooltip("Nouvel espace")
      .on_mouse_down("left", stop).on_click(() => run("space.new")).child(svg(icon("plus")).size(13));

    const h = fullHeight();
    return v_flex().w(EDGE + NAMES).py(6).gap(2).text_size(13).when(h !== null, el => el.h(h))
      .child(header("Espaces", newSpace)).children(spaceRows)
      .child(div().h(10))
      .child(header("Agents")).children(agentRows);
  }
}
