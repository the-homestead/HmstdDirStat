//! Sunburst: folders as rings, each item's arc sized by its logical size. Hover an arc
//! for its name and size; click a folder to move into it.

use std::{
    cell::Cell,
    f32::consts::{FRAC_PI_2, TAU},
    rc::Rc,
};

use gpui_kit::component::*;
use gpui_kit::*;
use hmstd_filesystem::{EntryKind, FileTree, NodeId};

use super::{
    Shell,
    views::{Item, breadcrumb, legend, slot_color},
    widgets::*,
};
use crate::{
    explore::{self, Basis},
    layout,
    scan::format_bytes,
};

const SUNBURST_TOP: usize = 16;
const SUNBURST_CHILDREN: usize = 24;
const SUNBURST_RINGS: u32 = 3;
/// Smallest sunburst arc drawn, as a fraction of a full turn (about 0.7 degrees).
const SUNBURST_MIN_SPAN: f64 = 0.002;

/// One sunburst arc with its colour already chosen, ready to paint.
struct DrawnArc {
    start: f32,
    end: f32,
    depth: u32,
    color: Hsla,
}

/// The arcs the sunburst draws for `focus`: one per visible item, ring by ring.
pub fn sunburst_arcs(files: &FileTree, focus: NodeId, basis: Basis) -> Vec<layout::Arc<NodeId>> {
    let top = explore::sized_children(files, focus, SUNBURST_TOP, basis);
    let children_of =
        |node: &NodeId| explore::sized_children(files, *node, SUNBURST_CHILDREN, basis);
    layout::sunburst(&top, SUNBURST_RINGS, SUNBURST_MIN_SPAN, &children_of)
}

/// The item whose arc lies under `position`, on a disc painted in `bounds`.
pub fn sunburst_hit(
    arcs: &[layout::Arc<NodeId>],
    bounds: Bounds<Pixels>,
    position: Point<Pixels>,
) -> Option<NodeId> {
    let (outer, hole, band) = sunburst_geometry(bounds);
    let dx = f32::from(position.x) - f32::from(bounds.center().x);
    let dy = f32::from(position.y) - f32::from(bounds.center().y);
    let radius = dx.hypot(dy);
    if radius < hole || radius > outer {
        return None;
    }
    let depth = (((radius - hole) / band).floor() as u32 + 1).min(SUNBURST_RINGS);
    // Same convention as paint_sunburst: angle from 12 o'clock, clockwise, as a fraction of a turn.
    let fraction = ((dy.atan2(dx) + FRAC_PI_2) / TAU).rem_euclid(1.0);
    arcs.iter()
        .find(|arc| arc.depth == depth && fraction >= arc.start as f32 && fraction < arc.end as f32)
        .map(|arc| arc.item)
}

/// Centre hole radius, the outer radius, and the width of one ring, in pixels.
fn sunburst_geometry(bounds: Bounds<Pixels>) -> (f32, f32, f32) {
    let outer = f32::from(bounds.size.width).min(f32::from(bounds.size.height)) * 0.5;
    let hole = outer * 0.36;
    let band = (outer - hole) / SUNBURST_RINGS as f32;
    (outer, hole, band)
}

/// Sunburst: rings for the focused folder's children, grandchildren, and so on. `hover` is
/// the arc under the pointer and the pointer's place on the disc, which positions its tooltip.
pub fn sunburst_page(
    files: &FileTree,
    focus: NodeId,
    handle: Entity<Shell>,
    p: Palette,
    look: super::treemap::Look<'_>,
    hover: Option<(NodeId, Point<Pixels>)>,
    bounds_cell: Rc<Cell<Option<Bounds<Pixels>>>>,
) -> Div {
    let basis = look.basis;
    let arcs = sunburst_arcs(files, focus, basis);
    let drawn: Vec<DrawnArc> = arcs
        .iter()
        .map(|arc| {
            // Deeper rings fade so the top level reads first.
            let base =
                p.chart[arc.slot % p.chart.len()].opacity(1.0 - 0.15 * (arc.depth - 1) as f32);
            // With a search active, arcs that do not match are faded back.
            let dim = look.query.is_active() && !look.query.hits(arc.item);
            DrawnArc {
                start: arc.start as f32,
                end: arc.end as f32,
                depth: arc.depth,
                color: if dim {
                    base.opacity(super::treemap::DIMMED)
                } else {
                    base
                },
            }
        })
        .collect();
    let focus_total = explore::bytes_of(files, focus, basis);

    // Fits the graph row (340 px card): title, breadcrumb and padding take the rest.
    let side = px(300.);
    let rings = canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            // Remember where the disc is on screen, so pointer positions can be mapped to arcs.
            bounds_cell.set(Some(bounds));
            paint_sunburst(bounds, &drawn, window)
        },
    );
    let (focus_name, focus_bytes) = files
        .node(focus)
        .map(|node| {
            (
                node.name().to_string_lossy().into_owned(),
                explore::bytes_of(files, focus, basis),
            )
        })
        .unwrap_or_default();
    let centre = div()
        .absolute()
        .size_full()
        .v_flex()
        .items_center()
        .justify_center()
        .gap_1()
        .child(div().font_weight(FontWeight::SEMIBOLD).child(focus_name))
        .child(
            div()
                .text_sm()
                .text_color(p.muted_foreground)
                .child(format_bytes(focus_bytes)),
        );
    let mut disc = div()
        .id("sunburst-disc")
        .relative()
        .w(side)
        .h(side)
        .flex_shrink_0()
        .child(rings.absolute().size_full())
        .child(centre)
        .on_mouse_move({
            let handle = handle.clone();
            move |event: &MouseMoveEvent, _, cx| {
                handle.update(cx, |shell, cx| shell.sunburst_pointer(event.position, cx));
            }
        })
        .on_hover({
            let handle = handle.clone();
            move |hovered: &bool, _, cx| {
                if !*hovered {
                    handle.update(cx, |shell, cx| shell.sunburst_leave(cx));
                }
            }
        })
        .on_click({
            let handle = handle.clone();
            move |event: &ClickEvent, _, cx| {
                let (at, count) = (event.position(), event.click_count());
                handle.update(cx, |shell, cx| shell.sunburst_click(at, count, cx));
            }
        })
        .on_mouse_down(MouseButton::Right, {
            let handle = handle.clone();
            move |event: &MouseDownEvent, _, cx| {
                let at = event.position;
                handle.update(cx, |shell, cx| shell.sunburst_menu(at, cx));
            }
        });
    if let Some((node, at)) = hover
        && let Some(tip) = sunburst_tip(files, node, focus_total, basis, at, p)
    {
        disc = disc.child(tip);
    }

    let items: Vec<Item> = explore::children(files, focus, basis)
        .into_iter()
        .filter(|child| child.bytes > 0)
        .take(SUNBURST_TOP)
        .enumerate()
        .map(|(index, child)| Item {
            name: child.name,
            bytes: child.bytes,
            node: child.is_dir.then_some(child.node),
            color: slot_color(index, false, p),
        })
        .collect();
    let total: u64 = items.iter().map(|item| item.bytes).sum();

    div()
        .v_flex()
        .gap_3()
        .child(breadcrumb(files, focus, handle.clone(), p))
        .child(
            div()
                .h_flex()
                .gap_6()
                .items_start()
                .child(disc)
                .child(legend(items, total, p, handle)),
        )
}

/// Hover card for one sunburst arc: name, size, share of the focused folder, and kind.
fn sunburst_tip(
    files: &FileTree,
    node: NodeId,
    focus_total: u64,
    basis: Basis,
    at: Point<Pixels>,
    p: Palette,
) -> Option<Div> {
    let entry = files.node(node)?;
    let is_dir = entry.kind() == EntryKind::Directory;
    let bytes = explore::bytes_of(files, node, basis);
    let share = if focus_total > 0 {
        bytes as f64 * 100.0 / focus_total as f64
    } else {
        0.0
    };
    let name = entry.name().to_string_lossy().into_owned();
    let kind = if is_dir { "Folder" } else { "File" };
    // Flip the card to the left of the pointer near the right edge, so it stays on the disc.
    let left = if at.x > px(260.) {
        at.x - px(200.)
    } else {
        at.x + px(14.)
    };
    Some(
        div()
            .absolute()
            .left(left)
            .top(at.y + px(14.))
            .v_flex()
            .gap_1()
            .p_2()
            .min_w(px(180.))
            .rounded_md()
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(name),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(p.muted_foreground)
                    .child(format!(
                        "{}  ·  {share:.1}% of this folder",
                        format_bytes(bytes)
                    )),
            )
            .child(div().text_xs().text_color(p.muted_foreground).child(kind)),
    )
}

/// Paints the rings. Each arc is an annular sector: outer arc out, inner arc back.
fn paint_sunburst(bounds: Bounds<Pixels>, arcs: &[DrawnArc], window: &mut Window) {
    let cx = f32::from(bounds.center().x);
    let cy = f32::from(bounds.center().y);
    let (_, hole, band) = sunburst_geometry(bounds);
    let at = |radius: f32, angle: f32| {
        point(px(cx + radius * angle.cos()), px(cy + radius * angle.sin()))
    };
    for arc in arcs {
        let r_inner = hole + band * (arc.depth - 1) as f32;
        let r_outer = r_inner + band;
        // Angles start at 12 o'clock and run clockwise, which is increasing angle on screen.
        let sweep = (arc.end - arc.start).min(0.9999);
        let a0 = -FRAC_PI_2 + arc.start * TAU;
        let a1 = a0 + sweep * TAU;
        let large = sweep > 0.5;

        let mut path = PathBuilder::fill();
        path.move_to(at(r_outer, a0));
        path.arc_to(
            point(px(r_outer), px(r_outer)),
            px(0.),
            large,
            true,
            at(r_outer, a1),
        );
        path.line_to(at(r_inner, a1));
        path.arc_to(
            point(px(r_inner), px(r_inner)),
            px(0.),
            large,
            false,
            at(r_inner, a0),
        );
        path.close();
        if let Ok(built) = path.build() {
            window.paint_path(built, arc.color);
        }
    }
}
