//! GPUI-free command registry, contextual availability and fuzzy-ranked palette model.

mod fuzzy;
#[cfg(test)]
mod tests;

pub use fuzzy::RankedCommand;

/// Every command whose action exists in the desktop shell.
///
/// Features that currently exist only in a headless crate (archive creation, standalone hashing,
/// export and scan comparison) deliberately have no variant here and cannot appear runnable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CommandId {
    StartScan,
    StopScan,
    RefreshScan,
    ChangeVisualization,
    SwitchWorkspace,
    OpenDirectory,
    SearchFiles,
    FindDuplicates,
    ChangeTheme,
    TogglePanels,
    OpenSettings,
}

impl CommandId {
    pub const ALL: [Self; 11] = [
        Self::StartScan,
        Self::StopScan,
        Self::RefreshScan,
        Self::ChangeVisualization,
        Self::SwitchWorkspace,
        Self::OpenDirectory,
        Self::SearchFiles,
        Self::FindDuplicates,
        Self::ChangeTheme,
        Self::TogglePanels,
        Self::OpenSettings,
    ];

    pub const fn definition(self) -> CommandDefinition {
        let (title, category, keywords): (&str, &str, &[&str]) = match self {
            Self::StartScan => ("Start scan", "Scan", &["walk", "analyze"]),
            Self::StopScan => ("Stop scan", "Scan", &["cancel", "abort"]),
            Self::RefreshScan => ("Refresh", "Scan", &["rescan", "reload"]),
            Self::ChangeVisualization => (
                "Change visualization",
                "View",
                &["widgets", "treemap", "sunburst", "pie"],
            ),
            Self::SwitchWorkspace => ("Switch workspace", "View", &["layout", "presets", "custom"]),
            Self::OpenDirectory => (
                "Open directory",
                "Files",
                &["explorer", "folder", "location"],
            ),
            Self::SearchFiles => ("Search files", "Files", &["filter", "query", "find"]),
            Self::FindDuplicates => (
                "Find duplicates",
                "Analysis",
                &["copies", "duplicate files", "dedupe"],
            ),
            Self::ChangeTheme => (
                "Change theme",
                "Preferences",
                &["dark", "light", "appearance"],
            ),
            Self::TogglePanels => ("Toggle panels", "View", &["widgets", "show", "hide"]),
            Self::OpenSettings => (
                "Open settings",
                "Preferences",
                &["options", "preferences", "scale"],
            ),
        };
        CommandDefinition {
            id: self,
            title,
            category,
            keywords,
        }
    }
}

/// Searchable, user-facing metadata for one command.
#[derive(Clone, Copy, Debug)]
pub struct CommandDefinition {
    pub id: CommandId,
    pub title: &'static str,
    pub category: &'static str,
    pub keywords: &'static [&'static str],
}

/// One registry row. Disabled rows retain their action id but can never be confirmed.
#[derive(Clone, Debug)]
pub struct CommandEntry {
    pub definition: CommandDefinition,
    pub action: Option<CommandId>,
    pub disabled_reason: Option<String>,
}

impl CommandEntry {
    pub fn runnable(id: CommandId) -> Self {
        Self::with_action(id.definition(), Some(id), None)
    }

    pub fn disabled(id: CommandId, reason: impl Into<String>) -> Self {
        Self::with_action(id.definition(), Some(id), Some(reason.into()))
    }

    /// General constructor also permits malformed/unbound fixtures so tests can prove those rows
    /// are never treated as runnable.
    pub fn with_action(
        definition: CommandDefinition,
        action: Option<CommandId>,
        disabled_reason: Option<String>,
    ) -> Self {
        Self {
            definition,
            action,
            disabled_reason,
        }
    }

    pub fn is_runnable(&self) -> bool {
        self.action.is_some() && self.disabled_reason.is_none()
    }
}

/// Shell facts needed to decide which existing actions can run now.
#[derive(Clone, Copy, Debug, Default)]
pub struct CommandCapabilities {
    pub scan_target_available: bool,
    pub refresh_target_available: bool,
    pub operation_busy: bool,
    pub walk_running: bool,
    pub search_ready: bool,
    pub duplicates_ready: bool,
    pub duplicates_running: bool,
}

/// The complete discoverable registry. Disabled contextual commands remain visible with a reason.
#[derive(Clone, Debug)]
pub struct CommandRegistry {
    entries: Vec<CommandEntry>,
}

impl CommandRegistry {
    pub fn new(entries: Vec<CommandEntry>) -> Self {
        Self { entries }
    }

    pub fn plan(capabilities: CommandCapabilities) -> Self {
        let entries = CommandId::ALL
            .into_iter()
            .map(|id| match disabled_reason(id, capabilities) {
                Some(reason) => CommandEntry::disabled(id, reason),
                None => CommandEntry::runnable(id),
            })
            .collect();
        Self::new(entries)
    }

    pub fn entries(&self) -> &[CommandEntry] {
        &self.entries
    }

    pub fn rank(&self, query: &str) -> Vec<RankedCommand> {
        fuzzy::rank(self, query)
    }
}

fn disabled_reason(id: CommandId, capabilities: CommandCapabilities) -> Option<&'static str> {
    const BUSY: &str = "Wait for the current operation to finish.";
    match id {
        CommandId::StartScan if capabilities.operation_busy => Some(BUSY),
        CommandId::StartScan if !capabilities.scan_target_available => {
            Some("No scan location is available.")
        }
        CommandId::StopScan if !capabilities.walk_running => {
            Some("No cancellable folder walk is running.")
        }
        CommandId::RefreshScan if capabilities.operation_busy => Some(BUSY),
        CommandId::RefreshScan if !capabilities.refresh_target_available => {
            Some("No scanned location is available to refresh.")
        }
        CommandId::OpenDirectory if !capabilities.refresh_target_available => {
            Some("No scanned directory is available to open.")
        }
        CommandId::SearchFiles if !capabilities.search_ready => {
            Some("The file search field is not ready yet.")
        }
        CommandId::FindDuplicates if capabilities.duplicates_running => {
            Some("A duplicate search is already running.")
        }
        CommandId::FindDuplicates if capabilities.operation_busy => Some(BUSY),
        CommandId::FindDuplicates if !capabilities.duplicates_ready => {
            Some("Finish a folder scan before finding duplicates.")
        }
        _ => None,
    }
}

/// Query and ranked rows shared by the UI and its confirm callback.
#[derive(Clone, Debug)]
pub struct CommandPaletteModel {
    registry: CommandRegistry,
    query: String,
    visible: Vec<RankedCommand>,
}

impl CommandPaletteModel {
    pub fn new(registry: CommandRegistry) -> Self {
        let visible = registry.rank("");
        Self {
            registry,
            query: String::new(),
            visible,
        }
    }

    pub fn set_registry(&mut self, registry: CommandRegistry) {
        self.registry = registry;
        self.refilter();
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.refilter();
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn visible(&self) -> &[RankedCommand] {
        &self.visible
    }

    pub fn command(&self, row: usize) -> Option<&CommandEntry> {
        self.visible.get(row).map(|ranked| &ranked.command)
    }

    fn refilter(&mut self) {
        self.visible = self.registry.rank(&self.query);
    }
}
