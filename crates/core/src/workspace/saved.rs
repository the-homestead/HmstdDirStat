//! Named visualization workspaces and the presets supported by the current widget layer.
//!
//! The document keeps the active layout separately from its saved snapshot. Moving a widget
//! therefore does not silently rewrite a custom workspace; Update does that explicitly, and
//! Restore reapplies the snapshot. The whole document is versioned and persisted by `storage`.

use serde::{Deserialize, Serialize};

use super::{WidgetKind, WorkspaceLayout};

pub const WORKSPACE_SCHEMA_VERSION: u32 = 1;
pub const WORKSPACES_FILE_NAME: &str = "workspaces.json";
const MAX_NAME_CHARS: usize = 64;

mod storage;
pub use storage::{WorkspaceStoreError, default_workspaces_path, load_from, save_to};

/// Built-in layouts whose required widgets exist in this build. Developer storage analysis is
/// deliberately absent until its dedicated widget exists; no preset is a placeholder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspacePreset {
    DiskOverview,
    DeepDirectoryAnalysis,
    DuplicateAnalysis,
    FileTypeAnalysis,
}

pub const WORKSPACE_PRESETS: [WorkspacePreset; 4] = [
    WorkspacePreset::DiskOverview,
    WorkspacePreset::DeepDirectoryAnalysis,
    WorkspacePreset::DuplicateAnalysis,
    WorkspacePreset::FileTypeAnalysis,
];

impl WorkspacePreset {
    pub fn name(self) -> &'static str {
        match self {
            Self::DiskOverview => "Disk overview",
            Self::DeepDirectoryAnalysis => "Deep directory analysis",
            Self::DuplicateAnalysis => "Duplicate analysis",
            Self::FileTypeAnalysis => "File type analysis",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::DiskOverview => "Tree, scan summary, file types and all current charts",
            Self::DeepDirectoryAnalysis => "Folder tree beside treemap and sunburst navigation",
            Self::DuplicateAnalysis => "Duplicate groups with the tree, summary and treemap",
            Self::FileTypeAnalysis => "Extension totals and their pie chart beside the tree",
        }
    }

    pub fn layout(self) -> WorkspaceLayout {
        let mut layout = WorkspaceLayout::default();
        match self {
            Self::DiskOverview => {}
            Self::DeepDirectoryAnalysis => {
                for kind in [WidgetKind::Summary, WidgetKind::Extensions, WidgetKind::Pie] {
                    layout.hide(kind);
                }
            }
            Self::DuplicateAnalysis => {
                for kind in [
                    WidgetKind::Extensions,
                    WidgetKind::Sunburst,
                    WidgetKind::Pie,
                ] {
                    layout.hide(kind);
                }
                layout.show(WidgetKind::Duplicates);
            }
            Self::FileTypeAnalysis => {
                for kind in [
                    WidgetKind::Summary,
                    WidgetKind::Treemap,
                    WidgetKind::Sunburst,
                ] {
                    layout.hide(kind);
                }
            }
        }
        layout
    }
}

/// A named snapshot that can be restored or updated with the active layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedWorkspace {
    pub name: String,
    pub layout: WorkspaceLayout,
}

/// The on-disk workspace document. `layout` is the current layout, including unsaved changes;
/// `custom` holds explicit user snapshots. Unknown JSON fields are tolerated within a schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceDocument {
    pub schema_version: u32,
    pub active: String,
    pub layout: WorkspaceLayout,
    #[serde(default)]
    pub custom: Vec<SavedWorkspace>,
}

impl Default for WorkspaceDocument {
    fn default() -> Self {
        Self::new(WorkspacePreset::DiskOverview.layout())
    }
}

impl WorkspaceDocument {
    /// Starts from the layout migrated from the older config.json storage.
    pub fn new(layout: WorkspaceLayout) -> Self {
        Self {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            active: WorkspacePreset::DiskOverview.name().to_string(),
            layout,
            custom: Vec::new(),
        }
    }

    pub fn is_active_custom(&self) -> bool {
        self.custom_index(&self.active).is_some()
    }

    pub fn is_modified(&self) -> bool {
        self.source_layout()
            .is_some_and(|source| source != self.layout)
    }

    pub fn source_layout(&self) -> Option<WorkspaceLayout> {
        WORKSPACE_PRESETS
            .into_iter()
            .find(|preset| preset.name() == self.active)
            .map(WorkspacePreset::layout)
            .or_else(|| {
                self.custom_index(&self.active)
                    .map(|index| self.custom[index].layout.clone())
            })
    }

    /// Switches to a preset or a custom snapshot and restores its saved layout.
    pub fn switch(&mut self, name: &str) -> Result<(), WorkspaceEditError> {
        let layout = self.layout_named(name).ok_or_else(|| {
            WorkspaceEditError::new(format!("Workspace '{name}' does not exist."))
        })?;
        self.active = name.to_string();
        self.layout = layout;
        Ok(())
    }

    /// Discards unsaved layout changes and restores the active saved snapshot.
    pub fn restore(&mut self) -> Result<(), WorkspaceEditError> {
        self.layout = self.source_layout().ok_or_else(|| {
            WorkspaceEditError::new("The active workspace has no saved snapshot to restore.")
        })?;
        Ok(())
    }

    /// Creates a custom workspace from the active layout and selects it.
    pub fn save_current_as(&mut self, name: &str) -> Result<(), WorkspaceEditError> {
        let name = clean_name(name)?;
        self.ensure_name_free(&name, None)?;
        self.custom.push(SavedWorkspace {
            name: name.clone(),
            layout: self.layout.clone(),
        });
        self.active = name;
        Ok(())
    }

    /// Replaces the active custom snapshot with the current layout. Presets stay immutable.
    pub fn update_active(&mut self) -> Result<(), WorkspaceEditError> {
        let index = self.active_custom_index()?;
        self.custom[index].layout = self.layout.clone();
        Ok(())
    }

    pub fn rename_active(&mut self, new_name: &str) -> Result<(), WorkspaceEditError> {
        let index = self.active_custom_index()?;
        let new_name = clean_name(new_name)?;
        self.ensure_name_free(&new_name, Some(index))?;
        self.custom[index].name = new_name.clone();
        self.active = new_name;
        Ok(())
    }

    /// Deletes the active custom workspace, then restores Disk overview.
    pub fn delete_active(&mut self) -> Result<(), WorkspaceEditError> {
        let index = self.active_custom_index()?;
        self.custom.remove(index);
        self.active = WorkspacePreset::DiskOverview.name().to_string();
        self.layout = WorkspacePreset::DiskOverview.layout();
        Ok(())
    }

    /// Rejects inconsistent documents before a read is accepted or a write replaces a file.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != WORKSPACE_SCHEMA_VERSION {
            return Err(format!(
                "schema version {} is not supported",
                self.schema_version
            ));
        }
        if self.source_layout().is_none() {
            return Err(format!("active workspace '{}' does not exist", self.active));
        }
        for (index, workspace) in self.custom.iter().enumerate() {
            clean_name(&workspace.name).map_err(|error| error.to_string())?;
            self.ensure_name_free(&workspace.name, Some(index))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn layout_named(&self, name: &str) -> Option<WorkspaceLayout> {
        WORKSPACE_PRESETS
            .into_iter()
            .find(|preset| preset.name() == name)
            .map(WorkspacePreset::layout)
            .or_else(|| {
                self.custom_index(name)
                    .map(|index| self.custom[index].layout.clone())
            })
    }

    fn custom_index(&self, name: &str) -> Option<usize> {
        self.custom
            .iter()
            .position(|workspace| name_key(&workspace.name) == name_key(name))
    }

    fn active_custom_index(&self) -> Result<usize, WorkspaceEditError> {
        self.custom_index(&self.active).ok_or_else(|| {
            WorkspaceEditError::new(
                "The active workspace is a built-in preset. Use Save as to create a custom workspace.",
            )
        })
    }

    fn ensure_name_free(
        &self,
        name: &str,
        except: Option<usize>,
    ) -> Result<(), WorkspaceEditError> {
        if WORKSPACE_PRESETS
            .into_iter()
            .any(|preset| name_key(preset.name()) == name_key(name))
            || self.custom.iter().enumerate().any(|(index, workspace)| {
                Some(index) != except && name_key(&workspace.name) == name_key(name)
            })
        {
            return Err(WorkspaceEditError::new(format!(
                "A workspace named '{name}' already exists."
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct WorkspaceEditError {
    message: String,
}

impl WorkspaceEditError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

fn clean_name(name: &str) -> Result<String, WorkspaceEditError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(WorkspaceEditError::new("Enter a workspace name."));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(WorkspaceEditError::new(format!(
            "Workspace names may contain at most {MAX_NAME_CHARS} characters."
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(WorkspaceEditError::new(
            "Workspace names cannot contain control characters.",
        ));
    }
    Ok(name.to_string())
}

fn name_key(name: &str) -> String {
    name.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{WORKSPACE_PRESETS, WidgetKind, WorkspaceDocument, WorkspacePreset};
    use crate::layout_resize;

    #[test]
    fn every_shipped_preset_is_tiled_and_uses_real_widgets() {
        assert_eq!(WORKSPACE_PRESETS.len(), 4);
        for preset in WORKSPACE_PRESETS {
            let layout = preset.layout();
            assert!(
                layout_resize::is_tiled(&layout),
                "{}: {layout:?}",
                preset.name()
            );
            assert!(!layout.widgets.is_empty());
        }
        let duplicates = WorkspacePreset::DuplicateAnalysis.layout();
        assert!(duplicates.is_shown(WidgetKind::Duplicates));
        let file_types = WorkspacePreset::FileTypeAnalysis.layout();
        assert!(file_types.is_shown(WidgetKind::Extensions));
        assert!(file_types.is_shown(WidgetKind::Pie));
    }

    #[test]
    fn custom_workspace_create_update_switch_restore_rename_and_delete() {
        let mut document = WorkspaceDocument::default();
        document.switch("Deep directory analysis").unwrap();
        document.layout.hide(WidgetKind::Tree);
        document.save_current_as("  My analysis  ").unwrap();
        assert_eq!(document.active, "My analysis");
        assert!(!document.is_modified());

        document.layout.show(WidgetKind::Tree);
        assert!(document.is_modified());
        document.restore().unwrap();
        assert!(!document.layout.is_shown(WidgetKind::Tree));

        document.layout.show(WidgetKind::Tree);
        document.update_active().unwrap();
        assert!(!document.is_modified());
        document.rename_active("Renamed analysis").unwrap();
        document.switch("Disk overview").unwrap();
        document.switch("Renamed analysis").unwrap();
        assert!(document.layout.is_shown(WidgetKind::Tree));

        document.delete_active().unwrap();
        assert_eq!(document.active, "Disk overview");
        assert!(document.custom.is_empty());
    }

    #[test]
    fn names_are_trimmed_unique_and_presets_are_not_editable() {
        let mut document = WorkspaceDocument::default();
        assert!(document.save_current_as("   ").is_err());
        assert!(document.save_current_as("disk OVERVIEW").is_err());
        document.save_current_as("Mine").unwrap();
        assert!(document.save_current_as("mINE").is_err());
        document.switch("Disk overview").unwrap();
        assert!(document.update_active().is_err());
        assert!(document.rename_active("Other").is_err());
        assert!(document.delete_active().is_err());
    }

    #[test]
    fn validation_rejects_an_active_name_without_a_snapshot() {
        let document = WorkspaceDocument {
            active: "Missing".to_string(),
            ..WorkspaceDocument::default()
        };
        assert!(document.validate().is_err());
    }
}
