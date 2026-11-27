//! Command palette overlay and the mapping from ranked registry rows to real shell actions.

use gpui_kit::component::{
    Disableable,
    command::{Command, CommandItem, CommandState},
    *,
};
use gpui_kit::*;
use timberstat_core::{
    command::{
        CommandCapabilities, CommandId, CommandPaletteModel, CommandRegistry, RankedCommand,
    },
    workspace::WidgetKind,
};

use super::{Phase, Shell, widgets::Palette};

pub struct PaletteUi {
    open: bool,
    state: Option<Entity<CommandState>>,
    model: CommandPaletteModel,
}

impl Default for PaletteUi {
    fn default() -> Self {
        Self {
            open: false,
            state: None,
            model: CommandPaletteModel::new(CommandRegistry::plan(CommandCapabilities::default())),
        }
    }
}

/// Refreshes contextual disabled reasons without resetting the user's query.
pub(super) fn refresh(shell: &mut Shell) {
    if shell.palette.open {
        let registry = CommandRegistry::plan(shell.command_capabilities());
        shell.palette.model.set_registry(registry);
    }
}

pub(super) fn overlay(shell: &Shell, colors: Palette, handle: Entity<Shell>) -> Option<Div> {
    if !shell.palette.open {
        return None;
    }
    let state = shell.palette.state.as_ref()?;
    let query_shell = handle.downgrade();
    let confirm_shell = handle.downgrade();
    let cancel_shell = handle.downgrade();
    let command = Command::new(state)
        .filterable(false)
        .placeholder("Search commands…")
        .max_h(px(420.))
        .items(
            shell
                .palette
                .model
                .visible()
                .iter()
                .map(|entry| command_item(entry, colors)),
        )
        .on_query(move |query, _, cx| {
            let _ = query_shell.update(cx, |shell, cx| {
                shell.palette.model.set_query(query);
                cx.notify();
            });
        })
        .on_confirm(move |index, window, cx| {
            let _ = confirm_shell.update(cx, |shell, cx| {
                shell.execute_palette_row(index.row, window, cx);
            });
        })
        .on_cancel(move |_, cx| {
            let _ = cancel_shell.update(cx, |shell, cx| shell.close_command_palette(cx));
        })
        .footer(move |_, _, _| {
            div()
                .w_full()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(colors.border)
                .text_xs()
                .text_color(colors.muted_foreground)
                .child("↑↓ Navigate  ·  Enter run  ·  Esc clear/close  ·  Ctrl+Shift+P open")
        });

    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                handle.update(cx, |shell, cx| shell.close_command_palette(cx));
            })
            .child(
                div()
                    .absolute()
                    .top(px(72.))
                    .left_0()
                    .right_0()
                    .h_flex()
                    .justify_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(680.))
                            .px_4()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(command),
                    ),
            ),
    )
}

fn command_item(ranked: &RankedCommand, colors: Palette) -> CommandItem {
    let command = &ranked.command;
    let title = command.definition.title.to_string();
    let category = command.definition.category.to_string();
    let reason = command
        .disabled_reason
        .clone()
        .or_else(|| (!command.is_runnable()).then(|| "No shell action is registered.".to_string()));
    CommandItem::new()
        .label(title.clone())
        .disabled(!command.is_runnable())
        .child(move |_, _| {
            let mut text = div()
                .v_flex()
                .min_w_0()
                .flex_1()
                .child(div().text_sm().truncate().child(title.clone()));
            if let Some(reason) = reason.as_ref() {
                text = text.child(
                    div()
                        .text_xs()
                        .text_color(colors.warning)
                        .child(reason.clone()),
                );
            }
            div()
                .h_flex()
                .w_full()
                .items_center()
                .gap_3()
                .child(text)
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(colors.muted_foreground)
                        .child(category.clone()),
                )
        })
}

impl Shell {
    fn command_capabilities(&self) -> CommandCapabilities {
        CommandCapabilities {
            scan_target_available: self.scan_root.is_some() || !self.volumes.is_empty(),
            refresh_target_available: self.scan_root.is_some(),
            operation_busy: self.is_busy(),
            walk_running: matches!(self.phase, Phase::Walking { .. }),
            search_ready: self.search_input.is_some(),
            duplicates_ready: self.tree.is_some() && self.scan_root.is_some(),
            duplicates_running: self.dup.is_running(),
        }
    }

    pub fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_search(window, cx);
        self.menu = None;
        self.palette.model =
            CommandPaletteModel::new(CommandRegistry::plan(self.command_capabilities()));
        let state = cx.new(|cx| CommandState::new(window, cx));
        state.update(cx, |state, cx| state.focus(window, cx));
        self.palette.state = Some(state);
        self.palette.open = true;
        cx.notify();
    }

    pub fn close_command_palette(&mut self, cx: &mut Context<Self>) {
        self.palette.open = false;
        self.palette.state = None;
        cx.notify();
    }

    fn execute_palette_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(command) = self.palette.model.command(row).cloned() else {
            return;
        };
        if !command.is_runnable() {
            return;
        }
        let Some(action) = command.action else {
            return;
        };
        self.close_command_palette(cx);
        match action {
            CommandId::StartScan => {
                let target = self
                    .scan_root
                    .clone()
                    .or_else(|| self.volumes.first().cloned());
                if let Some(target) = target {
                    self.choose_volume(target, cx);
                }
            }
            CommandId::StopScan => self.cancel_walk(cx),
            CommandId::RefreshScan => {
                self.refresh_scan(cx);
            }
            CommandId::ChangeVisualization
            | CommandId::SwitchWorkspace
            | CommandId::TogglePanels => {
                self.open_manage_workspaces(window, cx);
            }
            CommandId::OpenDirectory => {
                if let Some(root) = self.scan_root.as_ref()
                    && let Err(error) = crate::open::open_folder(root)
                {
                    tracing::error!(%error, path = %root.display(), "could not open scan directory");
                }
            }
            CommandId::SearchFiles => {
                self.ensure_search(window, cx);
                if let Some(input) = self.search_input.as_ref() {
                    input.focus_handle(cx).focus(window, cx);
                }
            }
            CommandId::FindDuplicates => {
                if !self.layout.is_shown(WidgetKind::Duplicates) {
                    self.layout.show(WidgetKind::Duplicates);
                    self.save_layout(cx);
                }
                self.start_duplicates(cx);
            }
            CommandId::ChangeTheme => crate::theme::toggle_theme(cx),
            CommandId::OpenSettings => {
                self.open_settings(window, cx);
            }
        }
    }
}
