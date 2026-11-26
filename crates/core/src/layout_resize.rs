//! Moving and resizing widgets, with the neighbours they run into pushed out of the way.
//!
//! A moved or resized widget takes room from the widgets it overlaps. Each overlapping
//! neighbour is pushed out along the axis of least overlap: it slides if the space beyond
//! is free, and shrinks to the free space if it is not. No neighbour goes below `MIN_SIZE`.
//! When nothing fits, the drag stops at the furthest position that still does. A result
//! never overlaps and never leaves the workspace.

use crate::workspace::{Edges, Frame, MIN_SIZE, WidgetKind, WorkspaceLayout};

/// Float tolerance: frames that touch within this are not overlapping.
const EPS: f32 = 1e-5;
/// Pushes allowed for one step, so a cycle cannot run forever.
const MAX_PUSHES: usize = 512;
/// Bisection steps used when a drag must stop short of the pointer.
const BISECT_STEPS: u32 = 20;
/// Passes allowed when settling an overlapping saved layout.
const MAX_SETTLE_PASSES: usize = 256;

type Slots = Vec<(WidgetKind, Frame)>;

fn overlaps(a: Frame, b: Frame) -> bool {
    a.x < b.x + b.w - EPS && b.x < a.x + a.w - EPS && a.y < b.y + b.h - EPS && b.y < a.y + a.h - EPS
}

fn inside(frame: Frame) -> bool {
    frame.x >= -EPS
        && frame.y >= -EPS
        && frame.x + frame.w <= 1.0 + EPS
        && frame.y + frame.h <= 1.0 + EPS
        && frame.w >= MIN_SIZE - EPS
        && frame.h >= MIN_SIZE - EPS
}

fn with_span(frame: Frame, horizontal: bool, start: f32, len: f32) -> Frame {
    if horizontal {
        Frame {
            x: start,
            w: len,
            ..frame
        }
    } else {
        Frame {
            y: start,
            h: len,
            ..frame
        }
    }
}

/// Pushes `frame` clear of `pinned` on the axis where they overlap least. It slides when the
/// space beyond `pinned` is free, and shrinks to the free space when it is not. `None` when
/// even the shrunk frame would fall below `MIN_SIZE`.
fn push_clear(pinned: Frame, frame: Frame) -> Option<Frame> {
    let ox = (pinned.x + pinned.w).min(frame.x + frame.w) - pinned.x.max(frame.x);
    let oy = (pinned.y + pinned.h).min(frame.y + frame.h) - pinned.y.max(frame.y);
    let horizontal = ox < oy;
    let (p_start, p_len, f_start, f_len) = if horizontal {
        (pinned.x, pinned.w, frame.x, frame.w)
    } else {
        (pinned.y, pinned.h, frame.y, frame.h)
    };
    let high = f_start + f_len / 2.0 >= p_start + p_len / 2.0;
    let edge = if high { p_start + p_len } else { p_start };

    let slid = if high { edge } else { edge - f_len };
    if slid >= -EPS && slid + f_len <= 1.0 + EPS {
        return Some(with_span(frame, horizontal, slid, f_len));
    }
    let (start, len) = if high {
        (edge, 1.0 - edge)
    } else {
        (0.0, edge)
    };
    (len >= MIN_SIZE - EPS).then(|| with_span(frame, horizontal, start, len))
}

/// True when every frame is inside the workspace and no two frames overlap.
fn valid(slots: &[(WidgetKind, Frame)]) -> bool {
    slots.iter().all(|(_, frame)| inside(*frame))
        && slots
            .iter()
            .enumerate()
            .all(|(i, (_, a))| slots[i + 1..].iter().all(|(_, b)| !overlaps(*a, *b)))
}

fn slots_of(layout: &WorkspaceLayout) -> Slots {
    layout
        .widgets
        .iter()
        .map(|placed| (placed.kind, placed.frame))
        .collect()
}

fn layout_from(base: &WorkspaceLayout, slots: &Slots) -> WorkspaceLayout {
    let mut out = base.clone();
    for (placed, (_, frame)) in out.widgets.iter_mut().zip(slots) {
        placed.frame = *frame;
    }
    out
}

/// Pushes every widget that overlaps `anchor` clear of it, then the widgets those pushes
/// run into, and so on. False when a push cannot fit, or the chain comes back to `anchor`.
fn clear_from(slots: &mut Slots, anchor: usize) -> bool {
    let mut queue = vec![anchor];
    let mut pushes = 0;
    while let Some(pinned_ix) = queue.pop() {
        let pinned = slots[pinned_ix].1;
        for (ix, slot) in slots.iter_mut().enumerate() {
            if ix == pinned_ix || !overlaps(pinned, slot.1) {
                continue;
            }
            if ix == anchor {
                return false;
            }
            pushes += 1;
            if pushes > MAX_PUSHES {
                return false;
            }
            match push_clear(pinned, slot.1) {
                Some(frame) => slot.1 = frame,
                None => return false,
            }
            queue.push(ix);
        }
    }
    valid(slots)
}

/// The layout with `kind` at `frame` and its neighbours pushed clear. `None` if that cannot fit.
fn attempt(base: &WorkspaceLayout, kind: WidgetKind, frame: Frame) -> Option<WorkspaceLayout> {
    if !inside(frame) {
        return None;
    }
    let mut slots = slots_of(base);
    let ix = slots.iter().position(|(k, _)| *k == kind)?;
    slots[ix].1 = frame;
    clear_from(&mut slots, ix).then(|| layout_from(base, &slots))
}

/// Applies the furthest step along `target` that fits. Step 1 is the full target. If that
/// does not fit, bisection finds the largest fraction that does. Step 0 is the base layout.
fn place(
    base: &WorkspaceLayout,
    kind: WidgetKind,
    target: impl Fn(f32) -> Frame,
) -> WorkspaceLayout {
    if let Some(layout) = attempt(base, kind, target(1.0)) {
        return layout;
    }
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    let mut best = base.clone();
    for _ in 0..BISECT_STEPS {
        let mid = (lo + hi) / 2.0;
        match attempt(base, kind, target(mid)) {
            Some(layout) => {
                best = layout;
                lo = mid;
            }
            None => hi = mid,
        }
    }
    best
}

/// Moves `kind` by `dx`, `dy` (workspace fractions) from `base`, pushing neighbours out of the way.
pub fn move_widget(base: &WorkspaceLayout, kind: WidgetKind, dx: f32, dy: f32) -> WorkspaceLayout {
    let Some(start) = base.frame(kind) else {
        return base.clone();
    };
    place(base, kind, |t| start.moved(t * dx, t * dy))
}

/// Resizes `kind` from `base` by dragging `edges` by `dx`, `dy`, pushing neighbours out of the way.
/// `min_w` and `min_h` are the smallest sizes, in workspace fractions.
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
    place(base, kind, |t| {
        start.resized(edges, t * dx, t * dy, min_w, min_h)
    })
}

/// Pushes apart any widgets that overlap in a saved layout. `None` when they cannot all fit,
/// so the caller can fall back to the default layout.
pub fn settle(layout: &WorkspaceLayout) -> Option<WorkspaceLayout> {
    let mut slots = slots_of(layout);
    for _ in 0..MAX_SETTLE_PASSES {
        let Some((a, b)) = first_overlap(&slots) else {
            return valid(&slots).then(|| layout_from(layout, &slots));
        };
        let pinned = slots[a].1;
        slots[b].1 = push_clear(pinned, slots[b].1)?;
    }
    None
}

fn first_overlap(slots: &Slots) -> Option<(usize, usize)> {
    for a in 0..slots.len() {
        for b in a + 1..slots.len() {
            if overlaps(slots[a].1, slots[b].1) {
                return Some((a, b));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Placement;

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

    fn side_by_side() -> WorkspaceLayout {
        layout(&[
            (WidgetKind::Tree, Frame::new(0.0, 0.0, 0.4, 1.0)),
            (WidgetKind::Treemap, Frame::new(0.4, 0.0, 0.6, 1.0)),
        ])
    }

    fn assert_valid(layout: &WorkspaceLayout) {
        assert!(valid(&slots_of(layout)), "overlap or outside: {layout:?}");
    }

    #[test]
    fn moving_into_a_neighbour_pushes_it_and_shrinks_it_to_fit() {
        let out = move_widget(&side_by_side(), WidgetKind::Tree, 0.2, 0.0);
        let tree = frame_of(&out, WidgetKind::Tree);
        let map = frame_of(&out, WidgetKind::Treemap);
        assert!(close(tree.x, 0.2), "tree moved: {tree:?}");
        assert!(
            close(map.x, 0.6) && close(map.w, 0.4),
            "treemap pushed: {map:?}"
        );
        assert_valid(&out);
    }

    #[test]
    fn a_neighbour_slides_when_there_is_room() {
        let base = layout(&[
            (WidgetKind::Tree, Frame::new(0.0, 0.0, 0.3, 0.5)),
            (WidgetKind::Summary, Frame::new(0.3, 0.0, 0.3, 0.5)),
            (WidgetKind::Treemap, Frame::new(0.6, 0.0, 0.4, 0.5)),
        ]);
        let out = move_widget(&base, WidgetKind::Tree, 0.2, 0.0);
        // Tree 0.2..0.5 pushes Summary to 0.5..0.8, which pushes Treemap to 0.8..1.0 (shrunk).
        assert!(close(frame_of(&out, WidgetKind::Summary).x, 0.5));
        let map = frame_of(&out, WidgetKind::Treemap);
        assert!(close(map.x, 0.8) && close(map.w, 0.2), "{map:?}");
        assert_valid(&out);
    }

    #[test]
    fn resizing_pushes_the_neighbour_it_runs_into() {
        let out = resize_widget(
            &side_by_side(),
            WidgetKind::Tree,
            Edges::RIGHT,
            0.1,
            0.0,
            0.05,
            0.05,
        );
        assert!(close(frame_of(&out, WidgetKind::Tree).w, 0.5));
        let map = frame_of(&out, WidgetKind::Treemap);
        assert!(close(map.x, 0.5) && close(map.w, 0.5), "{map:?}");
        assert_valid(&out);
    }

    #[test]
    fn every_drag_keeps_the_layout_valid() {
        let base = layout(&[
            (WidgetKind::Tree, Frame::new(0.0, 0.0, 0.4, 1.0)),
            (WidgetKind::Summary, Frame::new(0.4, 0.0, 0.3, 0.2)),
            (WidgetKind::Extensions, Frame::new(0.7, 0.0, 0.3, 0.2)),
            (WidgetKind::Treemap, Frame::new(0.4, 0.2, 0.6, 0.5)),
            (WidgetKind::Pie, Frame::new(0.7, 0.7, 0.3, 0.3)),
        ]);
        for kind in WidgetKind::ALL {
            if base.frame(kind).is_none() {
                continue;
            }
            for step in -9..=9 {
                let d = step as f32 * 0.1;
                assert_valid(&move_widget(&base, kind, d, d));
                assert_valid(&move_widget(&base, kind, d, -d));
                assert_valid(&resize_widget(
                    &base,
                    kind,
                    Edges::RIGHT,
                    d,
                    0.0,
                    0.05,
                    0.05,
                ));
                assert_valid(&resize_widget(&base, kind, Edges::TOP, 0.0, d, 0.05, 0.05));
            }
        }
    }

    #[test]
    fn a_drag_is_computed_from_its_start_layout() {
        let base = side_by_side();
        let _ = move_widget(&base, WidgetKind::Tree, 0.2, 0.0);
        let back = move_widget(&base, WidgetKind::Tree, 0.0, 0.0);
        assert_eq!(back, base);
    }

    #[test]
    fn settle_pushes_apart_a_saved_overlap() {
        let saved = layout(&[
            (WidgetKind::Tree, Frame::new(0.0, 0.0, 0.4, 1.0)),
            (WidgetKind::Summary, Frame::new(0.057, 0.0, 0.6, 0.5)),
        ]);
        let fixed = settle(&saved).expect("this overlap fits");
        assert_valid(&fixed);
    }

    #[test]
    fn settle_leaves_a_valid_layout_alone() {
        let default = WorkspaceLayout::default();
        assert_eq!(settle(&default), Some(default));
    }
}
