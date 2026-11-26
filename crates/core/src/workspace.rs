//! The visualization workspace: which widgets are shown, and where each one sits.
//!
//! A widget is a movable, resizable panel in the content area. Frames are fractions of the
//! workspace (0.0 to 1.0), so a saved layout keeps its proportions when the window is resized.
//! Minimum sizes in pixels are the app's business; this module only keeps frames inside the
//! workspace and above `MIN_SIZE`, so a hand-edited file cannot produce an empty widget.
//!
//! Widgets tile the workspace: together they cover it with no gaps and no overlaps (see
//! `layout_resize`). Nothing here depends on GPUI.

use serde::{Deserialize, Serialize};

/// Smallest width or height a frame may have, as a fraction of the workspace. The app enforces
/// a larger minimum in pixels while the user drags; this bound only guards loaded files.
pub const MIN_SIZE: f32 = 0.05;

/// The widgets the content area can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    /// The folder tree with its status line and search matches.
    Tree,
    /// Scan progress, or the result of the last scan.
    Summary,
    /// File types under the focused folder: size, share and file count per extension.
    Extensions,
    Treemap,
    Sunburst,
    Pie,
}

impl WidgetKind {
    /// Every kind, in the order the View menu lists them.
    pub const ALL: [WidgetKind; 6] = [
        WidgetKind::Tree,
        WidgetKind::Summary,
        WidgetKind::Extensions,
        WidgetKind::Treemap,
        WidgetKind::Sunburst,
        WidgetKind::Pie,
    ];

    /// The title shown in the widget header and the View menu.
    pub fn title(self) -> &'static str {
        match self {
            Self::Tree => "Folder tree",
            Self::Summary => "Summary",
            Self::Extensions => "Extensions",
            Self::Treemap => "Treemap",
            Self::Sunburst => "Sunburst",
            Self::Pie => "Pie",
        }
    }

    /// The name serde writes for this kind. Stable: it is the saved key, and also an element id.
    pub fn name(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Summary => "summary",
            Self::Extensions => "extensions",
            Self::Treemap => "treemap",
            Self::Sunburst => "sunburst",
            Self::Pie => "pie",
        }
    }

    /// Where the widget goes when it is added back and no layout says otherwise.
    pub fn default_frame(self) -> Frame {
        match self {
            Self::Tree => Frame::new(0.0, 0.0, 0.40, 1.0),
            Self::Summary => Frame::new(0.40, 0.0, 0.30, 0.20),
            Self::Extensions => Frame::new(0.70, 0.0, 0.30, 0.20),
            Self::Treemap => Frame::new(0.40, 0.20, 0.60, 0.50),
            Self::Sunburst => Frame::new(0.40, 0.70, 0.30, 0.30),
            Self::Pie => Frame::new(0.70, 0.70, 0.30, 0.30),
        }
    }
}

/// A rectangle as fractions of the workspace: `x` and `y` are the top-left corner.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Which sides of a frame a resize drag moves. A corner moves two sides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl Edges {
    pub const LEFT: Self = Self {
        left: true,
        right: false,
        top: false,
        bottom: false,
    };
    pub const RIGHT: Self = Self {
        left: false,
        right: true,
        top: false,
        bottom: false,
    };
    pub const TOP: Self = Self {
        left: false,
        right: false,
        top: true,
        bottom: false,
    };
    pub const BOTTOM: Self = Self {
        left: false,
        right: false,
        top: false,
        bottom: true,
    };
}

impl Frame {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Moves the frame by a fraction of the workspace, keeping it inside.
    pub fn moved(self, dx: f32, dy: f32) -> Self {
        let x = (self.x + dx).max(0.0).min(1.0 - self.w);
        let y = (self.y + dy).max(0.0).min(1.0 - self.h);
        Self { x, y, ..self }
    }

    /// Moves the sides named in `edges` by a fraction of the workspace. A side stops at the
    /// workspace border and at `min_w` or `min_h` from the opposite side.
    pub fn resized(self, edges: Edges, dx: f32, dy: f32, min_w: f32, min_h: f32) -> Self {
        let (mut left, mut right) = (self.x, self.x + self.w);
        let (mut top, mut bottom) = (self.y, self.y + self.h);
        if edges.left {
            left = (left + dx).max(0.0).min(right - min_w);
        }
        if edges.right {
            right = (right + dx).max(left + min_w).min(1.0);
        }
        if edges.top {
            top = (top + dy).max(0.0).min(bottom - min_h);
        }
        if edges.bottom {
            bottom = (bottom + dy).max(top + min_h).min(1.0);
        }
        Self {
            x: left,
            y: top,
            w: right - left,
            h: bottom - top,
        }
    }

    /// Returns the frame pulled inside the workspace, with each side at least `MIN_SIZE`.
    /// A non-finite value falls back to `fallback`.
    fn sanitized(self, fallback: Frame) -> Self {
        let finite = [self.x, self.y, self.w, self.h]
            .iter()
            .all(|value| value.is_finite());
        if !finite {
            return fallback;
        }
        let w = self.w.clamp(MIN_SIZE, 1.0);
        let h = self.h.clamp(MIN_SIZE, 1.0);
        Self {
            x: keep_inside(self.x, w),
            y: keep_inside(self.y, h),
            w,
            h,
        }
    }
}

/// Moves `start` back so that `start + size` does not pass the workspace edge. A start that is
/// already inside is returned unchanged, so a value stored exactly stays exact.
fn keep_inside(start: f32, size: f32) -> f32 {
    let limit = 1.0 - size;
    if start > limit + 1e-6 {
        limit.max(0.0)
    } else {
        start.max(0.0)
    }
}

/// One shown widget and its frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub kind: WidgetKind,
    pub frame: Frame,
}

/// The shown widgets, in stacking order (last is in front).
///
/// Deserialization drops entries whose kind this build does not know, keeps only the first
/// entry for each kind, and pulls every frame inside the workspace. A file written by a newer
/// build therefore loads with the widgets this build understands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "RawLayout")]
pub struct WorkspaceLayout {
    pub widgets: Vec<Placement>,
}

#[derive(Deserialize)]
struct RawLayout {
    widgets: Vec<RawPlacement>,
}

#[derive(Deserialize)]
struct RawPlacement {
    kind: String,
    frame: Frame,
}

impl From<RawLayout> for WorkspaceLayout {
    fn from(raw: RawLayout) -> Self {
        let mut widgets: Vec<Placement> = Vec::new();
        for entry in raw.widgets {
            let Some(kind) = kind_from_name(&entry.kind) else {
                continue;
            };
            if widgets.iter().any(|placed| placed.kind == kind) {
                continue;
            }
            widgets.push(Placement {
                kind,
                frame: entry.frame.sanitized(kind.default_frame()),
            });
        }
        // A saved layout may overlap (older builds did not prevent it). Push apart; if that
        // cannot fit, use the default layout.
        crate::layout_resize::settle(&Self { widgets }).unwrap_or_default()
    }
}

fn kind_from_name(name: &str) -> Option<WidgetKind> {
    WidgetKind::ALL.into_iter().find(|kind| kind.name() == name)
}

impl Default for WorkspaceLayout {
    fn default() -> Self {
        Self {
            widgets: vec![
                Placement {
                    kind: WidgetKind::Tree,
                    frame: WidgetKind::Tree.default_frame(),
                },
                Placement {
                    kind: WidgetKind::Summary,
                    frame: WidgetKind::Summary.default_frame(),
                },
                Placement {
                    kind: WidgetKind::Extensions,
                    frame: WidgetKind::Extensions.default_frame(),
                },
                Placement {
                    kind: WidgetKind::Treemap,
                    frame: WidgetKind::Treemap.default_frame(),
                },
                Placement {
                    kind: WidgetKind::Sunburst,
                    frame: WidgetKind::Sunburst.default_frame(),
                },
                Placement {
                    kind: WidgetKind::Pie,
                    frame: WidgetKind::Pie.default_frame(),
                },
            ],
        }
    }
}

impl WorkspaceLayout {
    /// The frame of a shown widget, or `None` if it is hidden.
    pub fn frame(&self, kind: WidgetKind) -> Option<Frame> {
        self.widgets
            .iter()
            .find(|placed| placed.kind == kind)
            .map(|placed| placed.frame)
    }

    pub fn is_shown(&self, kind: WidgetKind) -> bool {
        self.frame(kind).is_some()
    }

    /// Adds a hidden widget by splitting the largest shown widget. Returns false if it was shown.
    pub fn show(&mut self, kind: WidgetKind) -> bool {
        if self.is_shown(kind) {
            return false;
        }
        *self = crate::layout_resize::add_widget(self, kind);
        true
    }

    /// Removes a widget and gives its space to its neighbours. Returns false if it was hidden.
    pub fn hide(&mut self, kind: WidgetKind) -> bool {
        if !self.is_shown(kind) {
            return false;
        }
        *self = crate::layout_resize::remove_widget(self, kind);
        true
    }

    /// Brings a shown widget to the front.
    pub fn raise(&mut self, kind: WidgetKind) {
        if let Some(index) = self.widgets.iter().position(|placed| placed.kind == kind) {
            let placed = self.widgets.remove(index);
            self.widgets.push(placed);
        }
    }

    /// Sets the frame of a shown widget. Hidden widgets are left alone.
    pub fn set_frame(&mut self, kind: WidgetKind, frame: Frame) {
        if let Some(placed) = self.widgets.iter_mut().find(|placed| placed.kind == kind) {
            placed.frame = frame.sanitized(kind.default_frame());
        }
    }

    /// Puts every widget back at its default frame, all shown, in the default order.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn move_stops_at_the_workspace_border() {
        let frame = Frame::new(0.5, 0.5, 0.4, 0.4).moved(0.3, -0.9);
        assert!(close(frame.x, 0.6), "x {}", frame.x);
        assert!(close(frame.y, 0.0), "y {}", frame.y);
        assert!(close(frame.w, 0.4) && close(frame.h, 0.4));
    }

    #[test]
    fn right_and_bottom_edges_grow_and_stop_at_the_border() {
        let frame = Frame::new(0.1, 0.1, 0.3, 0.3);
        let grown = frame.resized(Edges::RIGHT, 0.2, 0.2, MIN_SIZE, MIN_SIZE);
        assert!(close(grown.w, 0.5) && close(grown.h, 0.3), "{grown:?}");
        let clipped = frame.resized(Edges::BOTTOM, 0.0, 5.0, MIN_SIZE, MIN_SIZE);
        assert!(close(clipped.y, 0.1) && close(clipped.h, 0.9));
    }

    #[test]
    fn left_edge_keeps_the_minimum_width_and_the_right_side_in_place() {
        let frame = Frame::new(0.2, 0.0, 0.3, 1.0);
        let shrunk = frame.resized(Edges::LEFT, 0.5, 0.0, 0.1, MIN_SIZE);
        assert!(close(shrunk.x + shrunk.w, 0.5), "right side moved");
        assert!(close(shrunk.w, 0.1), "width {}", shrunk.w);
    }

    #[test]
    fn corner_moves_two_sides_at_once() {
        let corner = Edges {
            left: false,
            right: true,
            top: false,
            bottom: true,
        };
        let frame = Frame::new(0.0, 0.0, 0.5, 0.5).resized(corner, 0.1, -0.2, MIN_SIZE, MIN_SIZE);
        assert!(close(frame.x, 0.0) && close(frame.y, 0.0));
        assert!(close(frame.w, 0.6) && close(frame.h, 0.3));
    }

    #[test]
    fn show_hide_and_raise_keep_one_entry_per_kind() {
        let mut layout = WorkspaceLayout::default();
        assert!(layout.hide(WidgetKind::Pie));
        assert!(!layout.hide(WidgetKind::Pie));
        assert!(!layout.is_shown(WidgetKind::Pie));
        assert!(layout.show(WidgetKind::Pie));
        assert!(!layout.show(WidgetKind::Pie));
        layout.raise(WidgetKind::Tree);
        assert_eq!(
            layout.widgets.last().map(|p| p.kind),
            Some(WidgetKind::Tree)
        );
        assert_eq!(layout.widgets.len(), WidgetKind::ALL.len());
    }

    #[test]
    fn plain_names_match_what_serde_writes() {
        for kind in WidgetKind::ALL {
            let written = serde_json::to_string(&kind).expect("serialize kind");
            assert_eq!(written, format!("\"{}\"", kind.name()));
        }
    }

    #[test]
    fn round_trip_through_json_keeps_the_layout() {
        let layout = WorkspaceLayout::default();
        let json = serde_json::to_string(&layout).expect("serialize");
        let back: WorkspaceLayout = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, layout);
    }

    #[test]
    fn loading_drops_unknown_and_duplicate_kinds_and_fixes_frames() {
        let json = r#"{"widgets":[
            {"kind":"future_panel","frame":{"x":0,"y":0,"w":0.5,"h":0.5}},
            {"kind":"pie","frame":{"x":0.9,"y":0.9,"w":0.5,"h":0.5}},
            {"kind":"pie","frame":{"x":0,"y":0,"w":0.1,"h":0.1}},
            {"kind":"tree","frame":{"x":0,"y":0,"w":0.01,"h":0.2}}
        ]}"#;
        let layout: WorkspaceLayout = serde_json::from_str(json).expect("deserialize");
        assert_eq!(layout.widgets.len(), 2);
        assert!(layout.is_shown(WidgetKind::Pie) && layout.is_shown(WidgetKind::Tree));
        // Frames are clamped inside, then settled: the two widgets tile the workspace.
        assert!(
            crate::layout_resize::is_tiled(&layout),
            "not tiled: {layout:?}"
        );
    }

    #[test]
    fn non_finite_frame_falls_back_to_the_default() {
        let frame = Frame::new(f32::NAN, 0.0, 0.5, 0.5);
        let mut layout = WorkspaceLayout::default();
        layout.set_frame(WidgetKind::Treemap, frame);
        assert_eq!(
            layout.frame(WidgetKind::Treemap),
            Some(WidgetKind::Treemap.default_frame())
        );
    }
}
