//! The visualization workspace. Each visualization is a widget: a panel with a header to drag,
//! edges and corners to resize, and a close control. The View menu adds closed widgets back.
//! The layout itself (frames, stacking, validation) lives in `hmstd_core::workspace`; this file
//! draws it, turns pointer drags into frame changes, and saves the result off the UI thread.

use gpui_kit::component::*;
use gpui_kit::*;
use hmstd_core::{
    config,
    workspace::{Edges, Frame, WidgetKind, WorkspaceLayout},
};

use super::{Shell, widget_bodies, widgets::*};

/// Smallest widget a drag may make, in pixels.
const MIN_WIDTH_PX: f32 = 180.;
const MIN_HEIGHT_PX: f32 = 120.;
/// Height of a widget's header bar.
const HEADER_PX: f32 = 28.;
/// Thickness of an edge grip, and the side of a corner grip.
const EDGE_PX: f32 = 6.;
const CORNER_PX: f32 = 12.;

/// What a drag on a widget does to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Grip {
    /// Dragging the header moves the widget.
    Move,
    /// Dragging an edge or corner resizes the sides it names.
    Resize(Edges),
}

/// The payload of a widget drag: which widget, and what the drag does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidgetDrag {
    pub kind: WidgetKind,
    pub grip: Grip,
}

/// Where a widget drag began. Each move is measured from here, so a drag never drifts.
#[derive(Clone, Debug)]
pub struct DragStart {
    pub pos: Point<Pixels>,
    /// The whole layout when the drag began. Each move is computed from it, not compounded.
    pub layout: hmstd_core::workspace::WorkspaceLayout,
}

/// The view GPUI shows under the pointer while a drag runs. The widget itself moves, so the
/// preview is empty.
struct DragPreview;

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_0()
    }
}

impl Shell {
    /// Shows a hidden widget, or hides a shown one. Used by the View menu.
    pub fn toggle_widget(&mut self, kind: WidgetKind, cx: &mut Context<Self>) {
        if !self.layout.hide(kind) {
            self.layout.show(kind);
        }
        self.layout_changed(cx);
    }

    /// Hides a widget from its close control.
    pub fn hide_widget(&mut self, kind: WidgetKind, cx: &mut Context<Self>) {
        if self.layout.hide(kind) {
            self.layout_changed(cx);
        }
    }

    /// Every widget back at its default place, all shown.
    pub fn reset_layout(&mut self, cx: &mut Context<Self>) {
        self.layout.reset();
        self.layout_changed(cx);
    }

    /// Brings a widget to the front. Stacking is not saved on its own; the next save keeps it.
    pub fn raise_widget(&mut self, kind: WidgetKind, cx: &mut Context<Self>) {
        self.layout.raise(kind);
        cx.notify();
    }

    /// A press on a widget starts a drag from the frame it has now.
    pub fn begin_widget_drag(&mut self, pos: Point<Pixels>) {
        self.drag_start = Some(DragStart {
            pos,
            layout: self.layout.clone(),
        });
    }

    /// Moves or resizes the dragged widget to follow the pointer.
    pub fn drag_widget(&mut self, drag: WidgetDrag, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let (Some(start), Some(size)) = (self.drag_start.clone(), self.workspace_size.get()) else {
            return;
        };
        let width = f32::from(size.width).max(1.);
        let height = f32::from(size.height).max(1.);
        let dx = f32::from(pos.x - start.pos.x) / width;
        let dy = f32::from(pos.y - start.pos.y) / height;
        let layout = match drag.grip {
            Grip::Move => hmstd_core::layout_resize::move_widget(&start.layout, drag.kind, dx, dy),
            Grip::Resize(edges) => hmstd_core::layout_resize::resize_widget(
                &start.layout,
                drag.kind,
                edges,
                dx,
                dy,
                MIN_WIDTH_PX / width,
                MIN_HEIGHT_PX / height,
            ),
        };
        self.layout = layout;
        self.layout_dirty = true;
        cx.notify();
    }

    /// The pointer was released: the drag is over, so a changed layout is saved.
    pub fn finish_widget_drag(&mut self, cx: &mut Context<Self>) {
        self.drag_start = None;
        if self.layout_dirty {
            self.save_layout(cx);
        }
    }

    pub fn toggle_view_menu(&mut self, cx: &mut Context<Self>) {
        self.view_menu = !self.view_menu;
        cx.notify();
    }

    pub fn close_view_menu(&mut self, cx: &mut Context<Self>) {
        if self.view_menu {
            self.view_menu = false;
            cx.notify();
        }
    }

    fn layout_changed(&mut self, cx: &mut Context<Self>) {
        self.layout_dirty = true;
        self.save_layout(cx);
        cx.notify();
    }

    /// Writes the layout to the config file on the background executor. Replacing the task
    /// drops a write that has not started, so the newest layout always wins.
    fn save_layout(&mut self, cx: &mut Context<Self>) {
        self.layout_dirty = false;
        let path = match config::default_config_path() {
            Ok(path) => path,
            Err(error) => {
                tracing::error!(%error, "layout not saved: no configuration path");
                return;
            }
        };
        let layout: WorkspaceLayout = self.layout.clone();
        self.layout_save = Some(cx.background_executor().spawn(async move {
            if let Err(error) = config::store_workspace_layout(&path, &layout) {
                tracing::error!(%error, "layout not saved");
            }
        }));
    }
}

/// The workspace: every shown widget at its frame, in stacking order, inside one area that
/// measures itself so drags can turn pixels into fractions of the window.
pub(super) fn area(shell: &Shell, p: Palette, handle: Entity<Shell>) -> Stateful<Div> {
    let size_cell = shell.workspace_size.clone();
    let drag_handle = handle.clone();
    let release_handle = handle.clone();
    let mut area = div()
        .id("workspace")
        .relative()
        .size_full()
        .overflow_hidden()
        .child(
            canvas(
                move |bounds, _, _| size_cell.set(Some(bounds.size)),
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        // Fires for every move during a widget drag, wherever the pointer is.
        .on_drag_move::<WidgetDrag>(move |event: &DragMoveEvent<WidgetDrag>, _, cx| {
            let drag = *event.drag(cx);
            let pos = event.event.position;
            drag_handle.update(cx, |shell, cx| shell.drag_widget(drag, pos, cx));
        })
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            release_handle.update(cx, |shell, cx| shell.finish_widget_drag(cx));
        });

    if shell.layout.widgets.is_empty() {
        area = area.child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(message_block(
                    "No widgets shown",
                    "Open View in the toolbar and choose a widget to add it back.",
                    p,
                )),
        );
    }
    for placed in &shell.layout.widgets {
        area = area.child(card(shell, placed.kind, placed.frame, p, handle.clone()));
    }
    area
}

/// One widget: header, body, and the grips around it.
fn card(
    shell: &Shell,
    kind: WidgetKind,
    frame: Frame,
    p: Palette,
    handle: Entity<Shell>,
) -> Stateful<Div> {
    let name = kind.name();
    let press = handle.clone();
    let header = div()
        .id(format!("widget-header-{name}"))
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h_flex()
        .h(px(HEADER_PX))
        .flex_shrink_0()
        .px_2()
        .gap_2()
        .items_center()
        .border_b_1()
        .border_color(p.border)
        .bg(p.background)
        .cursor_move()
        .on_drag(
            WidgetDrag {
                kind,
                grip: Grip::Move,
            },
            |_, _, _, cx| cx.new(|_| DragPreview),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(kind.title()),
        )
        .child(close_control(kind, p, handle.clone()));

    let mut card = div()
        .absolute()
        .top(px(WIDGET_GAP / 2.0))
        .bottom(px(WIDGET_GAP / 2.0))
        .left(px(WIDGET_GAP / 2.0))
        .right(px(WIDGET_GAP / 2.0))
        .overflow_hidden()
        .rounded_md()
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        // A press anywhere on the widget brings it to the front and starts a drag from here.
        .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
            press.update(cx, |shell, cx| {
                shell.raise_widget(kind, cx);
                shell.begin_widget_drag(event.position);
            });
        })
        .child(header)
        .child(
            div()
                .absolute()
                .top(px(HEADER_PX))
                .bottom_0()
                .left_0()
                .right_0()
                .child(widget_bodies::body(shell, kind, p, handle)),
        );
    for grip in grips(kind) {
        card = card.child(grip);
    }
    // The frame is the slot. The card fills it inset by half the gap on each side, so two
    // neighbours sit WIDGET_GAP apart.
    div()
        .id(format!("widget-{name}"))
        .absolute()
        .left(relative(frame.x))
        .top(relative(frame.y))
        .w(relative(frame.w))
        .h(relative(frame.h))
        .child(card)
}

/// The × in a header. It hides the widget; View shows it again.
fn close_control(kind: WidgetKind, p: Palette, handle: Entity<Shell>) -> Stateful<Div> {
    div()
        .id(format!("widget-close-{}", kind.name()))
        .flex_shrink_0()
        .px_1()
        .rounded_sm()
        .text_sm()
        .text_color(p.muted_foreground)
        .cursor_pointer()
        .hover(|style| style.bg(p.border).text_color(p.foreground))
        .child("×")
        .on_click(move |_, _, cx| {
            handle.update(cx, |shell, cx| shell.hide_widget(kind, cx));
        })
}

/// The four edges and four corners of a widget. Each one starts a resize of the sides it names.
fn grips(kind: WidgetKind) -> Vec<Stateful<Div>> {
    let name = kind.name();
    let grip = |part: &str, edges: Edges, cursor: CursorStyle| {
        div()
            .id(format!("widget-grip-{name}-{part}"))
            .absolute()
            .cursor(cursor)
            .on_drag(
                WidgetDrag {
                    kind,
                    grip: Grip::Resize(edges),
                },
                |_, _, _, cx| cx.new(|_| DragPreview),
            )
    };
    let edge = px(EDGE_PX);
    let corner = px(CORNER_PX);
    let side = px(CORNER_PX);
    let horizontal = CursorStyle::ResizeLeftRight;
    let vertical = CursorStyle::ResizeUpDown;
    vec![
        grip("left", Edges::LEFT, horizontal)
            .left(px(0.))
            .top(side)
            .bottom(side)
            .w(edge),
        grip("right", Edges::RIGHT, horizontal)
            .right(px(0.))
            .top(side)
            .bottom(side)
            .w(edge),
        grip("top", Edges::TOP, vertical)
            .top(px(0.))
            .left(side)
            .right(side)
            .h(edge),
        grip("bottom", Edges::BOTTOM, vertical)
            .bottom(px(0.))
            .left(side)
            .right(side)
            .h(edge),
        grip(
            "top-left",
            Edges {
                left: true,
                top: true,
                ..Edges::default()
            },
            CursorStyle::ResizeUpLeftDownRight,
        )
        .left(px(0.))
        .top(px(0.))
        .size(corner),
        grip(
            "top-right",
            Edges {
                right: true,
                top: true,
                ..Edges::default()
            },
            CursorStyle::ResizeUpRightDownLeft,
        )
        .right(px(0.))
        .top(px(0.))
        .size(corner),
        grip(
            "bottom-left",
            Edges {
                left: true,
                bottom: true,
                ..Edges::default()
            },
            CursorStyle::ResizeUpRightDownLeft,
        )
        .left(px(0.))
        .bottom(px(0.))
        .size(corner),
        grip(
            "bottom-right",
            Edges {
                right: true,
                bottom: true,
                ..Edges::default()
            },
            CursorStyle::ResizeUpLeftDownRight,
        )
        .right(px(0.))
        .bottom(px(0.))
        .size(corner),
    ]
}

/// Space between neighbouring widgets, in pixels.
const WIDGET_GAP: f32 = 2.0;
