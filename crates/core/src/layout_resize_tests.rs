use super::*;

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

fn assert_tiled(layout: &WorkspaceLayout) {
    assert!(is_tiled(layout), "not tiled: {layout:?}");
}

fn resize(kind: WidgetKind, edges: Edges, dx: f32, dy: f32) -> WorkspaceLayout {
    resize_widget(&WorkspaceLayout::default(), kind, edges, dx, dy, MIN, MIN)
}

#[test]
fn the_default_layout_is_tiled() {
    assert_tiled(&WorkspaceLayout::default());
}

#[test]
fn shrinking_a_widget_grows_its_neighbour() {
    let out = resize(WidgetKind::Summary, Edges::RIGHT, -0.1, 0.0);
    assert!(close(frame_of(&out, WidgetKind::Summary).w, 0.2));
    let ext = frame_of(&out, WidgetKind::Extensions);
    assert!(close(ext.x, 0.6) && close(ext.w, 0.4), "{ext:?}");
    assert_tiled(&out);
}

#[test]
fn the_tree_edge_moves_the_whole_column_beside_it() {
    let out = resize(WidgetKind::Tree, Edges::RIGHT, 0.1, 0.0);
    assert!(close(frame_of(&out, WidgetKind::Tree).w, 0.5));
    for kind in [
        WidgetKind::Summary,
        WidgetKind::Treemap,
        WidgetKind::Sunburst,
    ] {
        assert!(close(frame_of(&out, kind).x, 0.5), "{kind:?}");
    }
    assert_tiled(&out);
}

#[test]
fn a_left_edge_drag_takes_from_the_widget_on_the_left() {
    let out = resize(WidgetKind::Treemap, Edges::LEFT, -0.1, 0.0);
    assert!(close(frame_of(&out, WidgetKind::Tree).w, 0.3));
    assert!(close(frame_of(&out, WidgetKind::Summary).x, 0.3));
    assert!(close(frame_of(&out, WidgetKind::Sunburst).x, 0.3));
    assert_tiled(&out);
}

#[test]
fn a_bottom_edge_drag_moves_every_widget_below_it() {
    let out = resize(WidgetKind::Treemap, Edges::BOTTOM, 0.0, -0.1);
    assert!(close(frame_of(&out, WidgetKind::Treemap).h, 0.4));
    for kind in [WidgetKind::Sunburst, WidgetKind::Pie] {
        let f = frame_of(&out, kind);
        assert!(close(f.y, 0.6) && close(f.h, 0.4), "{kind:?} {f:?}");
    }
    assert_tiled(&out);
}

#[test]
fn the_workspace_border_does_not_move() {
    let out = resize(WidgetKind::Tree, Edges::LEFT, -0.1, 0.0);
    assert_eq!(out, WorkspaceLayout::default());
}

#[test]
fn a_neighbour_stops_at_the_minimum_size() {
    let out = resize(WidgetKind::Summary, Edges::RIGHT, 0.5, 0.0);
    assert!(close(frame_of(&out, WidgetKind::Extensions).w, MIN));
    assert_tiled(&out);
}

#[test]
fn moving_swaps_with_the_widget_under_the_pointer() {
    let base = WorkspaceLayout::default();
    let out = move_widget(&base, WidgetKind::Pie, 0.2, 0.5);
    assert_eq!(
        frame_of(&out, WidgetKind::Pie),
        frame_of(&base, WidgetKind::Tree)
    );
    assert_eq!(
        frame_of(&out, WidgetKind::Tree),
        frame_of(&base, WidgetKind::Pie)
    );
    assert_tiled(&out);
    // Back over its own slot: no swap.
    assert_eq!(move_widget(&base, WidgetKind::Pie, 0.85, 0.85), base);
}

#[test]
fn every_drag_keeps_the_layout_tiled() {
    let base = WorkspaceLayout::default();
    let corner = Edges {
        left: false,
        right: true,
        top: false,
        bottom: true,
    };
    for kind in WidgetKind::ALL {
        for edges in [Edges::LEFT, Edges::RIGHT, Edges::TOP, Edges::BOTTOM, corner] {
            for step in -9..=9 {
                let d = step as f32 * 0.1;
                assert_tiled(&resize_widget(&base, kind, edges, d, d, MIN, MIN));
            }
        }
        for gx in 0..10 {
            for gy in 0..10 {
                let (x, y) = (gx as f32 * 0.1 + 0.05, gy as f32 * 0.1 + 0.05);
                assert_tiled(&move_widget(&base, kind, x, y));
            }
        }
    }
}

#[test]
fn closing_a_widget_gives_its_space_to_neighbours() {
    for kind in WidgetKind::ALL {
        let out = remove_widget(&WorkspaceLayout::default(), kind);
        assert!(!out.is_shown(kind));
        assert_tiled(&out);
    }
    let mut out = WorkspaceLayout::default();
    for kind in WidgetKind::ALL {
        out = remove_widget(&out, kind);
        assert_tiled(&out);
    }
    assert!(out.widgets.is_empty());
}

#[test]
fn adding_a_widget_splits_the_largest() {
    let without = remove_widget(&WorkspaceLayout::default(), WidgetKind::Pie);
    let out = add_widget(&without, WidgetKind::Pie);
    assert!(out.is_shown(WidgetKind::Pie));
    assert_tiled(&out);
    let alone = add_widget(
        &WorkspaceLayout {
            widgets: Vec::new(),
        },
        WidgetKind::Tree,
    );
    assert_eq!(
        frame_of(&alone, WidgetKind::Tree),
        Frame::new(0.0, 0.0, 1.0, 1.0)
    );
}

#[test]
fn settle_fills_the_gaps_in_a_saved_layout() {
    // The layout an earlier build saved: Summary narrowed, Extensions shortened, gaps between.
    let saved = layout(&[
        (WidgetKind::Summary, Frame::new(0.0, 0.0, 0.58, 0.27)),
        (WidgetKind::Extensions, Frame::new(0.70, 0.0, 0.30, 0.19)),
        (WidgetKind::Tree, Frame::new(0.0, 0.27, 0.40, 0.73)),
        (WidgetKind::Treemap, Frame::new(0.40, 0.27, 0.60, 0.48)),
        (WidgetKind::Sunburst, Frame::new(0.40, 0.75, 0.30, 0.25)),
        (WidgetKind::Pie, Frame::new(0.70, 0.75, 0.30, 0.25)),
    ]);
    assert_tiled(&settle(&saved).expect("fits"));
}

#[test]
fn settle_pushes_apart_a_saved_overlap() {
    let saved = layout(&[
        (WidgetKind::Tree, Frame::new(0.0, 0.0, 0.4, 1.0)),
        (WidgetKind::Summary, Frame::new(0.057, 0.0, 0.6, 0.5)),
    ]);
    let fixed = settle(&saved).expect("this overlap fits");
    assert!(valid(&slots_of(&fixed)), "{fixed:?}");
}

#[test]
fn settle_leaves_a_tiled_layout_alone() {
    let default = WorkspaceLayout::default();
    assert_eq!(settle(&default), Some(default));
}
