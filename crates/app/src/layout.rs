//! Pure geometry for the Treemap and Sunburst views. No GPUI types, so the layouts
//! test without a window.

/// A rectangle with its top-left corner at (`x`, `y`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn area(&self) -> f64 {
        self.w * self.h
    }
}

/// Squarified treemap (Bruls, Huizing and van Wijk). The rectangles come back in
/// input order and tile `bounds` by area, so each tile's area is its share of the
/// total. Non-positive values get an empty rectangle. Pass values largest first for
/// the best aspect ratios.
pub fn squarify(values: &[f64], bounds: Rect) -> Vec<Rect> {
    let mut out = vec![
        Rect {
            x: bounds.x,
            y: bounds.y,
            w: 0.0,
            h: 0.0,
        };
        values.len()
    ];
    let total: f64 = values.iter().filter(|v| **v > 0.0).sum();
    if total <= 0.0 || bounds.area() <= 0.0 {
        return out;
    }
    let scale = bounds.area() / total;
    let areas: Vec<f64> = values
        .iter()
        .map(|value| if *value > 0.0 { value * scale } else { 0.0 })
        .collect();

    let mut rest = bounds;
    let mut i = 0;
    while i < areas.len() {
        if areas[i] <= 0.0 {
            i += 1;
            continue;
        }
        // Grow the row while adding the next item does not make its worst aspect ratio worse.
        let side = rest.w.min(rest.h);
        let mut end = i + 1;
        let mut current = worst(&areas[i..end], side);
        while end < areas.len() && areas[end] > 0.0 {
            let candidate = worst(&areas[i..=end], side);
            if candidate > current {
                break;
            }
            current = candidate;
            end += 1;
        }
        rest = lay_row(&areas[i..end], &mut out[i..end], rest);
        i = end;
    }
    out
}

/// Worst aspect ratio of a row laid along a side of length `side`.
fn worst(row: &[f64], side: f64) -> f64 {
    let sum: f64 = row.iter().sum();
    let max = row.iter().copied().fold(f64::MIN, f64::max);
    let min = row.iter().copied().fold(f64::MAX, f64::min);
    let side_squared = side * side;
    (side_squared * max / (sum * sum)).max(sum * sum / (side_squared * min))
}

/// Places one row along the shorter side of `rest` and returns what is left over.
fn lay_row(row: &[f64], cells: &mut [Rect], rest: Rect) -> Rect {
    const EPSILON: f64 = 1e-12;
    let sum: f64 = row.iter().sum();
    if rest.w <= EPSILON || rest.h <= EPSILON {
        for cell in cells.iter_mut() {
            *cell = Rect {
                x: rest.x,
                y: rest.y,
                w: 0.0,
                h: 0.0,
            };
        }
        return rest;
    }
    if rest.w >= rest.h {
        // A column at the left edge, as wide as the row needs to hold its area.
        let width = sum / rest.h;
        let mut y = rest.y;
        for (area, cell) in row.iter().zip(cells.iter_mut()) {
            let height = area / width;
            *cell = Rect {
                x: rest.x,
                y,
                w: width,
                h: height,
            };
            y += height;
        }
        Rect {
            x: rest.x + width,
            y: rest.y,
            w: (rest.w - width).max(0.0),
            h: rest.h,
        }
    } else {
        // A strip along the top edge, as tall as the row needs to hold its area.
        let height = sum / rest.w;
        let mut x = rest.x;
        for (area, cell) in row.iter().zip(cells.iter_mut()) {
            let width = area / height;
            *cell = Rect {
                x,
                y: rest.y,
                w: width,
                h: height,
            };
            x += width;
        }
        Rect {
            x: rest.x,
            y: rest.y + height,
            w: rest.w,
            h: (rest.h - height).max(0.0),
        }
    }
}

/// One arc of the sunburst. `start` and `end` are fractions of a full turn, clockwise
/// from 12 o'clock. `depth` is 1 for the innermost ring. `slot` is the index of the
/// top-level item the arc descends from, used for colour.
#[derive(Debug, Clone, PartialEq)]
pub struct Arc<T> {
    pub item: T,
    pub depth: u32,
    pub start: f64,
    pub end: f64,
    pub slot: usize,
}

/// Lays `items` (value, largest first) across a full turn, then each item's children
/// across that item's own span, down to `rings` levels. Spans narrower than `min_span`
/// are skipped along with their children.
pub fn sunburst<T: Clone, F: Fn(&T) -> Vec<(T, f64)>>(
    items: &[(T, f64)],
    rings: u32,
    min_span: f64,
    children_of: &F,
) -> Vec<Arc<T>> {
    let mut walk = Walk {
        rings,
        min_span,
        children_of,
        out: Vec::new(),
    };
    walk.place(items, 0.0, 1.0, 1, None);
    walk.out
}

struct Walk<'a, T, F> {
    rings: u32,
    min_span: f64,
    children_of: &'a F,
    out: Vec<Arc<T>>,
}

impl<T: Clone, F: Fn(&T) -> Vec<(T, f64)>> Walk<'_, T, F> {
    fn place(&mut self, items: &[(T, f64)], start: f64, end: f64, depth: u32, slot: Option<usize>) {
        let total: f64 = items.iter().map(|(_, value)| value.max(0.0)).sum();
        if total <= 0.0 || end <= start {
            return;
        }
        let mut cursor = start;
        for (index, (item, value)) in items.iter().enumerate() {
            let span = (end - start) * value.max(0.0) / total;
            if span <= 0.0 {
                continue;
            }
            let arc_end = cursor + span;
            if span >= self.min_span {
                let slot = slot.unwrap_or(index);
                self.out.push(Arc {
                    item: item.clone(),
                    depth,
                    start: cursor,
                    end: arc_end,
                    slot,
                });
                if depth < self.rings {
                    let children = (self.children_of)(item);
                    self.place(&children, cursor, arc_end, depth + 1, Some(slot));
                }
            }
            cursor = arc_end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn treemap_tiles_the_bounds_by_area() {
        let values = [6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0];
        let total: f64 = values.iter().sum();
        let rects = squarify(&values, unit());
        assert_eq!(rects.len(), values.len());
        let mut covered = 0.0;
        for (value, rect) in values.iter().zip(&rects) {
            assert!(rect.x >= -1e-9 && rect.y >= -1e-9);
            assert!(rect.x + rect.w <= 1.0 + 1e-9 && rect.y + rect.h <= 1.0 + 1e-9);
            assert!(
                close(rect.area(), value / total),
                "area {} for {value}",
                rect.area()
            );
            covered += rect.area();
        }
        assert!(close(covered, 1.0));
    }

    #[test]
    fn treemap_gives_zero_values_an_empty_rect() {
        let rects = squarify(&[5.0, 0.0, 5.0], unit());
        assert_eq!(rects[1].area(), 0.0);
        assert!(close(rects[0].area(), 0.5));
        assert!(close(rects[2].area(), 0.5));
    }

    #[test]
    fn treemap_of_nothing_is_empty() {
        assert!(squarify(&[], unit()).is_empty());
        assert_eq!(squarify(&[0.0], unit())[0].area(), 0.0);
    }

    #[test]
    fn sunburst_spans_follow_size_and_nest_inside_their_parent() {
        let items = vec![("a", 3.0), ("b", 1.0)];
        let children = |name: &&str| match *name {
            "a" => vec![("a1", 2.0), ("a2", 1.0)],
            _ => Vec::new(),
        };
        let arcs = sunburst(&items, 2, 0.0, &children);

        let find = |name: &str| arcs.iter().find(|arc| arc.item == name).unwrap().clone();
        let a = find("a");
        assert!(close(a.start, 0.0) && close(a.end, 0.75));
        assert_eq!(a.depth, 1);
        let b = find("b");
        assert!(close(b.start, 0.75) && close(b.end, 1.0));
        let a1 = find("a1");
        assert!(close(a1.start, 0.0) && close(a1.end, 0.5));
        assert_eq!((a1.depth, a1.slot), (2, 0));
        let a2 = find("a2");
        assert!(close(a2.start, 0.5) && close(a2.end, 0.75));
    }

    #[test]
    fn sunburst_skips_spans_below_the_minimum() {
        let items = vec![("big", 99.0), ("tiny", 1.0)];
        let none = |_: &&str| Vec::new();
        let arcs = sunburst(&items, 1, 0.05, &none);
        assert_eq!(arcs.len(), 1);
        assert_eq!(arcs[0].item, "big");
    }
}
