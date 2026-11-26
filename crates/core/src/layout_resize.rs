//! The workspace is tiled: shown widgets cover the whole area with no gaps and no overlaps.
//!
//! - Resizing moves a shared edge line. Every widget that ends on the line and every widget that
//!   starts on it (within the run of widgets that touch across it) move together, so one side
//!   grows exactly as much as the other shrinks. No widget goes below the minimum size.
//! - Moving swaps the dragged widget with the widget under the pointer.
//! - Removing a widget gives its space to the neighbours on one side; adding one splits the
//!   largest widget in two.
//! - A saved layout that overlaps or has gaps is settled: overlaps are pushed apart, then
//!   widgets grow into the gaps.

use crate::workspace::{Edges, Frame, MIN_SIZE, Placement, WidgetKind, WorkspaceLayout};

/// Edges closer than this (as workspace fractions) are the same edge.
const EPS: f32 = 1e-4;
/// Passes allowed when settling a saved layout, so a cycle cannot run forever.
const MAX_PASSES: usize = 256;

type Slots = Vec<(WidgetKind, Frame)>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

fn lo(frame: Frame, axis: Axis) -> f32 {
    match axis {
        Axis::X => frame.x,
        Axis::Y => frame.y,
    }
}

fn hi(frame: Frame, axis: Axis) -> f32 {
    match axis {
        Axis::X => frame.x + frame.w,
        Axis::Y => frame.y + frame.h,
    }
}

/// Start and end of the frame across `axis` (the other dimension).
fn cross(frame: Frame, axis: Axis) -> (f32, f32) {
    match axis {
        Axis::X => (frame.y, frame.y + frame.h),
        Axis::Y => (frame.x, frame.x + frame.w),
    }
}

/// The frame with its span along `axis` set to `start..end`.
fn with_span(frame: Frame, axis: Axis, start: f32, end: f32) -> Frame {
    match axis {
        Axis::X => Frame {
            x: start,
            w: end - start,
            ..frame
        },
        Axis::Y => Frame {
            y: start,
            h: end - start,
            ..frame
        },
    }
}

fn spans_overlap(a: (f32, f32), b: (f32, f32)) -> bool {
    a.0 < b.1 - EPS && b.0 < a.1 - EPS
}

fn overlaps(a: Frame, b: Frame) -> bool {
    spans_overlap((a.x, a.x + a.w), (b.x, b.x + b.w))
        && spans_overlap((a.y, a.y + a.h), (b.y, b.y + b.h))
}

fn inside(frame: Frame) -> bool {
    frame.x >= -EPS
        && frame.y >= -EPS
        && frame.x + frame.w <= 1.0 + EPS
        && frame.y + frame.h <= 1.0 + EPS
        && frame.w > EPS
        && frame.h > EPS
}

/// True when every frame is inside the workspace and no two frames overlap.
fn valid(slots: &[(WidgetKind, Frame)]) -> bool {
    slots.iter().all(|(_, frame)| inside(*frame))
        && slots
            .iter()
            .enumerate()
            .all(|(i, (_, a))| slots[i + 1..].iter().all(|(_, b)| !overlaps(*a, *b)))
}

/// Share of the workspace the frames cover. 1.0 for a tiled layout without overlaps.
pub fn coverage(layout: &WorkspaceLayout) -> f32 {
    layout
        .widgets
        .iter()
        .map(|placed| placed.frame.w * placed.frame.h)
        .sum()
}

/// True when the layout is tiled: inside, no overlaps, nothing left uncovered.
pub fn is_tiled(layout: &WorkspaceLayout) -> bool {
    let slots = slots_of(layout);
    valid(&slots) && (slots.is_empty() || (coverage(layout) - 1.0).abs() < 1e-3)
}

fn slots_of(layout: &WorkspaceLayout) -> Slots {
    layout
        .widgets
        .iter()
        .map(|placed| (placed.kind, placed.frame))
        .collect()
}

fn layout_from(base: &WorkspaceLayout, slots: &[(WidgetKind, Frame)]) -> WorkspaceLayout {
    let mut out = base.clone();
    for (placed, (_, frame)) in out.widgets.iter_mut().zip(slots) {
        placed.frame = *frame;
    }
    out
}

/// Moves the edge line of widget `ix` (its high edge when `high`, else its low edge) by
/// `delta`, together with every widget on either side of that line that touches the run.
fn shift_line(slots: &mut Slots, ix: usize, axis: Axis, high: bool, delta: f32, min: f32) {
    let frame = slots[ix].1;
    let line = if high {
        hi(frame, axis)
    } else {
        lo(frame, axis)
    };
    if line <= EPS || line >= 1.0 - EPS {
        return; // The workspace border does not move.
    }
    // `before` ends on the line, `after` starts on it. Grow both sets until every widget that
    // touches a member across the line is in.
    let mut before = vec![false; slots.len()];
    let mut after = vec![false; slots.len()];
    if high {
        before[ix] = true;
    } else {
        after[ix] = true;
    }
    let touches = |set: &[bool], span: (f32, f32)| {
        set.iter()
            .zip(slots.iter())
            .any(|(on, (_, other))| *on && spans_overlap(span, cross(*other, axis)))
    };
    loop {
        let mut grew = false;
        for (j, (_, other)) in slots.iter().enumerate() {
            let span = cross(*other, axis);
            if !before[j] && (hi(*other, axis) - line).abs() < EPS && touches(&after, span) {
                before[j] = true;
                grew = true;
            }
            if !after[j] && (lo(*other, axis) - line).abs() < EPS && touches(&before, span) {
                after[j] = true;
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    if !before.contains(&true) || !after.contains(&true) {
        return;
    }
    let pick = |set: &[bool]| -> Vec<Frame> {
        set.iter()
            .zip(slots.iter())
            .filter(|(on, _)| **on)
            .map(|(_, (_, frame))| *frame)
            .collect()
    };
    let floor = pick(&before)
        .iter()
        .map(|frame| lo(*frame, axis) + min)
        .fold(0.0_f32, f32::max);
    let ceil = pick(&after)
        .iter()
        .map(|frame| hi(*frame, axis) - min)
        .fold(1.0_f32, f32::min);
    if floor > ceil {
        return;
    }
    let edge = (line + delta).clamp(floor, ceil);
    for (j, (_, frame)) in slots.iter_mut().enumerate() {
        if before[j] {
            *frame = with_span(*frame, axis, lo(*frame, axis), edge);
        } else if after[j] {
            *frame = with_span(*frame, axis, edge, hi(*frame, axis));
        }
    }
}

/// Resizes `kind` from `base` (the layout when the drag began) by dragging `edges` by `dx`,
/// `dy` in workspace fractions. Neighbours across each dragged edge give or take the space.
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
    let Some(ix) = base.widgets.iter().position(|placed| placed.kind == kind) else {
        return base.clone();
    };
    let mut slots = slots_of(base);
    if edges.left {
        shift_line(&mut slots, ix, Axis::X, false, dx, min_w);
    }
    if edges.right {
        shift_line(&mut slots, ix, Axis::X, true, dx, min_w);
    }
    if edges.top {
        shift_line(&mut slots, ix, Axis::Y, false, dy, min_h);
    }
    if edges.bottom {
        shift_line(&mut slots, ix, Axis::Y, true, dy, min_h);
    }
    layout_from(base, &slots)
}

/// Swaps `kind` with the widget under the pointer at (`x`, `y`) in workspace fractions.
/// Computed from `base`, so moving back over the widget's own slot undoes the swap.
pub fn move_widget(base: &WorkspaceLayout, kind: WidgetKind, x: f32, y: f32) -> WorkspaceLayout {
    let Some(own) = base.frame(kind) else {
        return base.clone();
    };
    let target = base.widgets.iter().find(|placed| {
        let f = placed.frame;
        placed.kind != kind && x >= f.x && x < f.x + f.w && y >= f.y && y < f.y + f.h
    });
    let Some(target) = target.copied() else {
        return base.clone();
    };
    let mut out = base.clone();
    for placed in &mut out.widgets {
        if placed.kind == kind {
            placed.frame = target.frame;
        } else if placed.kind == target.kind {
            placed.frame = own;
        }
    }
    out
}

/// Removes `kind` and gives its space to its neighbours.
pub fn remove_widget(layout: &WorkspaceLayout, kind: WidgetKind) -> WorkspaceLayout {
    let Some(hole) = layout.frame(kind) else {
        return layout.clone();
    };
    let mut out = layout.clone();
    out.widgets.retain(|placed| placed.kind != kind);
    let mut slots = slots_of(&out);
    if !cover_hole(&mut slots, hole) {
        fill_gaps(&mut slots);
    }
    layout_from(&out, &slots)
}

/// Grows the neighbours on one side of `hole` across it, when their edges line up with the
/// hole exactly. Picks the side with the fewest neighbours. False if no side lines up.
fn cover_hole(slots: &mut Slots, hole: Frame) -> bool {
    let mut best: Option<(Axis, bool, Vec<usize>)> = None;
    for axis in [Axis::X, Axis::Y] {
        let span = cross(hole, axis);
        for from_low in [true, false] {
            // from_low: neighbours that end where the hole starts, and grow forward.
            let line = if from_low {
                lo(hole, axis)
            } else {
                hi(hole, axis)
            };
            let side: Vec<usize> = slots
                .iter()
                .enumerate()
                .filter(|(_, (_, f))| {
                    let edge = if from_low { hi(*f, axis) } else { lo(*f, axis) };
                    let (c0, c1) = cross(*f, axis);
                    (edge - line).abs() < EPS && c0 >= span.0 - EPS && c1 <= span.1 + EPS
                })
                .map(|(ix, _)| ix)
                .collect();
            let covered: f32 = side
                .iter()
                .map(|ix| {
                    let (c0, c1) = cross(slots[*ix].1, axis);
                    c1 - c0
                })
                .sum();
            let fits = !side.is_empty() && (covered - (span.1 - span.0)).abs() < 1e-3;
            if fits && best.as_ref().is_none_or(|(_, _, b)| side.len() < b.len()) {
                best = Some((axis, from_low, side));
            }
        }
    }
    let Some((axis, from_low, side)) = best else {
        return false;
    };
    for ix in side {
        let frame = slots[ix].1;
        slots[ix].1 = if from_low {
            with_span(frame, axis, lo(frame, axis), hi(hole, axis))
        } else {
            with_span(frame, axis, lo(hole, axis), hi(frame, axis))
        };
    }
    true
}

/// Grows each widget into free space on every side until nothing can grow.
fn fill_gaps(slots: &mut Slots) {
    for _ in 0..MAX_PASSES {
        let mut changed = false;
        for ix in 0..slots.len() {
            for axis in [Axis::X, Axis::Y] {
                let frame = slots[ix].1;
                let span = cross(frame, axis);
                let blockers = || {
                    slots
                        .iter()
                        .enumerate()
                        .filter(move |(j, (_, other))| {
                            *j != ix && spans_overlap(span, cross(*other, axis))
                        })
                        .map(|(_, (_, other))| *other)
                };
                let low = blockers()
                    .map(|other| hi(other, axis))
                    .filter(|edge| *edge <= lo(frame, axis) + EPS)
                    .fold(0.0_f32, f32::max);
                let high = blockers()
                    .map(|other| lo(other, axis))
                    .filter(|edge| *edge >= hi(frame, axis) - EPS)
                    .fold(1.0_f32, f32::min);
                if low < lo(frame, axis) - EPS || high > hi(frame, axis) + EPS {
                    slots[ix].1 = with_span(frame, axis, low, high);
                    changed = true;
                }
            }
        }
        if !changed {
            return;
        }
    }
}

/// Adds `kind` by splitting the largest widget along its longer side. The new widget takes
/// the right or bottom half. An empty workspace gives it the whole area.
pub fn add_widget(layout: &WorkspaceLayout, kind: WidgetKind) -> WorkspaceLayout {
    if layout.is_shown(kind) {
        return layout.clone();
    }
    let mut out = layout.clone();
    let largest = out
        .widgets
        .iter_mut()
        .max_by(|a, b| (a.frame.w * a.frame.h).total_cmp(&(b.frame.w * b.frame.h)));
    let frame = match largest {
        None => Frame::new(0.0, 0.0, 1.0, 1.0),
        Some(placed) => {
            let f = placed.frame;
            // The workspace is wider than tall, so a fraction of width is worth more pixels.
            if f.w * 1.6 >= f.h {
                placed.frame = Frame { w: f.w / 2.0, ..f };
                Frame::new(f.x + f.w / 2.0, f.y, f.w / 2.0, f.h)
            } else {
                placed.frame = Frame { h: f.h / 2.0, ..f };
                Frame::new(f.x, f.y + f.h / 2.0, f.w, f.h / 2.0)
            }
        }
    };
    out.widgets.push(Placement { kind, frame });
    out
}

/// Pushes `frame` clear of `pinned` on the axis where they overlap least, shrinking it to the
/// free space when it cannot slide. `None` when it would fall below `MIN_SIZE`.
fn push_clear(pinned: Frame, frame: Frame) -> Option<Frame> {
    let ox = hi(pinned, Axis::X).min(hi(frame, Axis::X)) - pinned.x.max(frame.x);
    let oy = hi(pinned, Axis::Y).min(hi(frame, Axis::Y)) - pinned.y.max(frame.y);
    let axis = if ox < oy { Axis::X } else { Axis::Y };
    let (p0, p1, f0, f1) = (
        lo(pinned, axis),
        hi(pinned, axis),
        lo(frame, axis),
        hi(frame, axis),
    );
    let len = f1 - f0;
    let forward = (f0 + f1) / 2.0 >= (p0 + p1) / 2.0;
    let (start, end) = if forward {
        (p1, (p1 + len).min(1.0))
    } else {
        ((p0 - len).max(0.0), p0)
    };
    (end - start >= MIN_SIZE - EPS).then(|| with_span(frame, axis, start, end))
}

/// Repairs a saved layout: pushes overlapping widgets apart, then grows widgets into gaps.
/// `None` when it cannot be made valid, so the caller can use the default layout.
pub fn settle(layout: &WorkspaceLayout) -> Option<WorkspaceLayout> {
    let mut slots = slots_of(layout);
    for _ in 0..MAX_PASSES {
        let Some((a, b)) = first_overlap(&slots) else {
            fill_gaps(&mut slots);
            return valid(&slots).then(|| layout_from(layout, &slots));
        };
        slots[b].1 = push_clear(slots[a].1, slots[b].1)?;
    }
    None
}

fn first_overlap(slots: &[(WidgetKind, Frame)]) -> Option<(usize, usize)> {
    (0..slots.len()).find_map(|a| {
        (a + 1..slots.len())
            .find(|b| overlaps(slots[a].1, slots[*b].1))
            .map(|b| (a, b))
    })
}

#[cfg(test)]
#[path = "layout_resize_tests.rs"]
mod tests;
