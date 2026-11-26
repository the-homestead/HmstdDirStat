//! Pieces shared by the copy/move and rename dialogs: policy wording and buttons, text
//! inputs bound to shell state, skip wording, and the tree refresh after an edit.
//! Nothing here touches the filesystem.

use gpui_kit::component::{
    button::{Button, ButtonVariants},
    input::{InputEvent, InputState},
    *,
};
use gpui_kit::*;
use hmstd_fileops::{Collision, SkipReason, Skipped};
use hmstd_filesystem::NodeId;

use crate::search::Query;
use crate::shell::{Shell, widgets::Palette};

/// Every collision policy, in the order the buttons show them.
pub(super) const POLICIES: [Collision; 4] = [
    Collision::Skip,
    Collision::Overwrite,
    Collision::Rename,
    Collision::KeepBoth,
];

pub(super) fn policy_label(policy: Collision) -> &'static str {
    match policy {
        Collision::Skip => "Skip",
        Collision::Overwrite => "Overwrite",
        Collision::Rename => "Rename",
        Collision::KeepBoth => "Keep both",
    }
}

pub(super) fn policy_help(policy: Collision) -> &'static str {
    match policy {
        Collision::Skip => {
            "Leave the existing item and the source as they are. The item is reported as skipped."
        }
        Collision::Overwrite => {
            "Replace an existing file with the same name. Folders are merged; a file and a folder never replace each other. You confirm before anything is replaced."
        }
        Collision::Rename => {
            "Use the new name typed below. If that name is taken too, the item fails and nothing is replaced."
        }
        Collision::KeepBoth => {
            "Keep the existing item and write the new one as \"name (2)\", or the next free number."
        }
    }
}

/// Plain wording for a skipped entry in a result list.
pub(super) fn skip_text(skip: &Skipped) -> String {
    match skip.reason {
        SkipReason::CollisionSkipped => "left as it was: the name was taken (Skip policy)".into(),
        SkipReason::SameLocation => "source and destination are the same; nothing done".into(),
        SkipReason::LinkNotCopied => "link not copied, so the copy is incomplete".into(),
        SkipReason::NotRegularFile => "device or special file not copied".into(),
    }
}

/// Four policy buttons with the chosen one filled, plus its help text. `set` runs on click.
pub(super) fn policy_group(
    id: &'static str,
    current: Collision,
    p: Palette,
    handle: Entity<Shell>,
    set: fn(&mut Shell, Collision, &mut Context<Shell>),
) -> Div {
    let mut row = div().h_flex().gap_2().flex_wrap();
    for (index, policy) in POLICIES.into_iter().enumerate() {
        let button = Button::new((id, index)).label(policy_label(policy));
        let button = if policy == current {
            button.primary()
        } else {
            button.outline()
        };
        let handle = handle.clone();
        row = row.child(button.on_click(move |_, _, cx| {
            handle.update(cx, |shell, cx| set(shell, policy, cx));
        }));
    }
    row.child(
        div()
            .text_xs()
            .text_color(p.muted_foreground)
            .child(policy_help(current)),
    )
}

/// A single-line text input. `on_change` receives every edit, so the shell state is the
/// source of truth and the field never holds text the state does not know about.
pub(super) fn text_input(
    window: &mut Window,
    cx: &mut Context<Shell>,
    placeholder: &'static str,
    initial: String,
    on_change: fn(&mut Shell, String),
) -> (Entity<InputState>, Subscription) {
    let input = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .default_value(initial)
    });
    let subscription = cx.subscribe_in(
        &input,
        window,
        move |shell: &mut Shell, input, event: &InputEvent, _window, cx| {
            if matches!(event, InputEvent::Change) {
                on_change(shell, input.read(cx).value().to_string());
                cx.notify();
            }
        },
    );
    (input, subscription)
}

impl Shell {
    /// After an edit to the in-memory tree (never a rescan): keeps focus, selection, the
    /// search filter and the tree and extension views in step with the tree.
    pub(super) fn refresh_after_tree_edit(&mut self, cx: &mut Context<Self>) {
        if self
            .tree
            .as_deref()
            .is_some_and(|files| !files.is_attached(self.focus))
        {
            self.focus = NodeId::ROOT;
        }
        if self.selected.is_some_and(|selected| {
            self.tree
                .as_deref()
                .is_some_and(|files| !files.is_attached(selected))
        }) {
            self.selected = None;
        }
        self.query = Query::new(self.query.text(), self.tree.as_deref());
        self.refresh_tree_state(cx);
        self.refresh_extensions();
        self.sunburst_hover = None;
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{POLICIES, policy_help, policy_label};
    use hmstd_fileops::Collision;

    #[test]
    fn every_policy_has_a_label_and_help_and_is_listed_once() {
        let labels: Vec<&str> = POLICIES.iter().map(|p| policy_label(*p)).collect();
        assert_eq!(labels, ["Skip", "Overwrite", "Rename", "Keep both"]);
        for policy in POLICIES {
            assert!(!policy_help(policy).is_empty());
        }
    }

    #[test]
    fn overwrite_help_says_folders_are_not_replaced() {
        assert!(policy_help(Collision::Overwrite).contains("never replace"));
    }
}
