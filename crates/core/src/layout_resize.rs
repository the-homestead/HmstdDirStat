//! Resizing that shares space. When a widget's edge moves, every widget that touches that edge
//! moves its matching edge by the same amount, so widgets take room from each other and never
//! overlap. A widget stops where a neighbour would fall below its minimum size.

use crate::workspace::{Edges, Frame, WidgetKind, WorkspaceLayout};

/// Two edges closer than this (as workspace fractions) count as the same edge.
const EDGE_EPS: f32 = 1e-4;

/// The axis an edge lies on: `X` for left and right edges, `Y` for top and bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

/// Start and length of a frame along `axis`.
fn span(frame: Frame, axis: Axis) -> (f32, f32) {
    match axis {
        Axis::X => (frame.x, frame.w),
        Axis::Y => (frame.y, frame.h),
    }
}

/// Start and length of a frame across `axis` (the other dimension).
fn across(frame: Frame, axis: Axis) -> (f32, f32) {
    match axis {
        Axis::X => (frame.y, frame.h),
        Axis::Y => (frame.x, frame.w),
    }
}

fn with_span(frame: Frame, axis: Axis, start: f32, len: f32) -> Frame {
    match axis {
        Axis::X => Frame {
            x: start,
            w: len,
            ..frame
        },
        Axis::Y => Frame {
            y: start,
            h: len,
            ..frame
        },
    }
}

/// Moves one edge of `kind` to `target`, and the matching edge of each neighbour that touches
/// it. `moving_low` picks the left or top edge; otherwise the right or bottom edge.
fn shift_edge(
    out: &mut WorkspaceLayout,
    kind: WidgetKind,
    axis: Axis,
    moving_low: bool,
    target: f32,
    min: f32,
) {
    let Some(start) = out.frame(kind) else {
        return;
    };
    let (lo, len) = span(start, axis);
    let hi = lo + len;
    let (a_lo, a_len) = across(start, axis);
    let a_hi = a_lo + a_len;
    let edge = if moving_low { lo } else { hi };

    // Neighbours whose opposite edge sits on this edge and that share some of its length.
    let touching: Vec<(WidgetKind, Frame)> = out
        .widgets
        .iter()
        .filter(|placed| placed.kind != kind)
        .filter(|placed| {
            let (n_lo, n_len) = span(placed.frame, axis);
            let meets = if moving_low {
                (n_lo + n_len - edge).abs() < EDGE_EPS
            } else {
                (n_lo - edge).abs() < EDGE_EPS
            };
            let (o_lo, o_len) = across(placed.frame, axis);
            meets && o_lo < a_hi - EDGE_EPS && o_lo + o_len > a_lo + EDGE_EPS
        })
        .map(|placed| (placed.kind, placed.frame))
        .collect();

    // The edge may move inside [floor, ceil]: no touching neighbour goes below `min`, and the
    // widget itself keeps `min` (and stays in the workspace).
    let (floor, ceil) = if moving_low {
        let floor = touching
            .iter()
            .map(|(_, frame)| span(*frame, axis).0 + min)
            .fold(0.0_f32, f32::max);
        (floor, hi - min)
    } else {
        let ceil = touching
            .iter()
            .map(|(_, frame)| {
                let (n_lo, n_len) = span(*frame, axis);
                n_lo + n_len - min
            })
            .fold(1.0_f32, f32::min);
        (lo + min, ceil)
    };
    if floor > ceil {
        return;
    }
    let edge_new = target.clamp(floor, ceil);

    for (neighbour, frame) in touching {
        let (n_lo, n_len) = span(frame, axis);
        let moved = if moving_low {
            with_span(frame, axis, n_lo, edge_new - n_lo)
        } else {
            with_span(frame, axis, edge_new, n_lo + n_len - edge_new)
        };
        out.set_frame(neighbour, moved);
    }
    let own = if moving_low {
        with_span(start, axis, edge_new, hi - edge_new)
    } else {
        with_span(start, axis, lo, edge_new - lo)
    };
    out.set_frame(kind, own);
}

/// The layout after `kind` is resized from `base` (the layout when the drag began) by `dx`,
/// `dy` in workspace fractions. Shared edges move with it, so no widget overlaps another.
/// `min_w` and `min_h` are the smallest sizes in workspace fractions.
pub fn resize_widget(
    base: &WorkspaceLayout,
    kind: WidgetKind,
    edges: Edges,
    dx: f32,
    dy: f32,
    min_w: f32,
    min_h: f32,
) -> WorkspaceLayout {
    let Some(start) = base.frame(kind) else {
        return base.clone();
    };
    let target = start.resized(edges, dx, dy, min_w, min_h);
    let mut out = base.clone();
    if edges.left {
        shift_edge(&mut out, kind, Axis::X, true, target.x, min_w);
    }
    if edges.right {
        shift_edge(&mut out, kind, Axis::X, false, target.x + target.w, min_w);
    }
    if edges.top {
        shift_edge(&mut out, kind, Axis::Y, true, target.y, min_h);
    }
    if edges.bottom {
        shift_edge(&mut out, kind, Axis::Y, false, target.y + target.h, min_h);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{Placement, WorkspaceLayout};

    const MIN: f32 = 0.1;

    fn layout(placed: &[(WidgetKind, Frame)]) -> WorkspaceLayout {
        WorkspaceLayout {
            widgets: placed
                .iter()
                .map(|(kind, frame)| Placement {
                    kind: *kind,
                    frame: *frame,
                })
                .collect(),
        }
    }

    fn frame_of(layout: &WorkspaceLayout, kind: WidgetKind) -> Frame {
        layout.frame(kind).expect("widget is placed")
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn overlaps(a: Frame, b: Frame) -> bool {
        a.x < b.x + b.w - 1e-4
            && b.x < a.x + a.w - 1e-4
            && a.y < b.y + b.h - 1e-4
            && b.y < a.y + a.h - 1e-4
    }

    fn side_by_side() -> WorkspaceLayout {
        layout(&[
            (WidgetKind::Tree, Frame::new(0.0, 0.0, 0.4, 1.0)),
            (WidgetKind::Treemap, Frame::new(0.4, 0.0, 0.6, 1.0)),
        ])
    }

    #[test]
    fn growing_one_widget_shrinks_its_neighbour() {
        let out = resize_widget(
            &side_by_side(),
            WidgetKind::Tree,
            Edges::RIGHT,
            0.1,
            0.0,
            MIN,
            MIN,
        );
        let tree = frame_of(&out, WidgetKind::Tree);
        let map = frame_of(&out, WidgetKind::Treemap);
        assert!(close(tree.w, 0.5));
        assert!(close(map.x, 0.5) && close(map.w, 0.5));
        assert!(!overlaps(tree, map));
    }

    #[test]
    fn neighbour_stops_at_its_minimum_size() {
        let out = resize_widget(
            &side_by_side(),
            WidgetKind::Tree,
            Edges::RIGHT,
            0.9,
            0.0,
            MIN,
            MIN,
        );
        let tree = frame_of(&out, WidgetKind::Tree);
        let map = frame_of(&out, WidgetKind::Treemap);
        assert!(
            close(map.w, MIN),
            "treemap keeps its minimum, got {}",
            map.w
        );
        assert!(!overlaps(tree, map));
    }

    #[test]
    fn dragging_a_left_edge_pushes_the_widget_on_its_left() {
        let out = resize_widget(
            &side_by_side(),
            WidgetKind::Treemap,
            Edges::LEFT,
            -0.2,
            0.0,
            MIN,
            MIN,
        );
        let tree = frame_of(&out, WidgetKind::Tree);
        let map = frame_of(&out, WidgetKind::Treemap);
        assert!(close(tree.w, 0.2));
        assert!(close(map.x, 0.2) && close(map.w, 0.8));
        assert!(!overlaps(tree, map));
    }

    #[test]
    fn widgets_that_do_not_touch_the_edge_stay_put() {
        let base = layout(&[
            (WidgetKind::Tree, Frame::new(0.0, 0.0, 0.4, 1.0)),
            (WidgetKind::Treemap, Frame::new(0.4, 0.0, 0.6, 0.5)),
            (WidgetKind::Pie, Frame::new(0.4, 0.5, 0.6, 0.5)),
        ]);
        let out = resize_widget(
            &base,
            WidgetKind::Treemap,
            Edges::BOTTOM,
            0.0,
            -0.2,
            MIN,
            MIN,
        );
        assert_eq!(
            frame_of(&out, WidgetKind::Tree),
            frame_of(&base, WidgetKind::Tree)
        );
        assert!(!overlaps(
            frame_of(&out, WidgetKind::Treemap),
            frame_of(&out, WidgetKind::Pie)
        ));
    }

    #[test]
    fn a_drag_is_computed_from_its_start_layout() {
        // Moving back and forth within one drag must not compound.
        let base = side_by_side();
        let there = resize_widget(&base, WidgetKind::Tree, Edges::RIGHT, 0.2, 0.0, MIN, MIN);
        let back = resize_widget(&base, WidgetKind::Tree, Edges::RIGHT, 0.0, 0.0, MIN, MIN);
        assert_eq!(
            frame_of(&back, WidgetKind::Tree),
            frame_of(&base, WidgetKind::Tree)
        );
        assert!(close(frame_of(&there, WidgetKind::Tree).w, 0.6));
    }
}
