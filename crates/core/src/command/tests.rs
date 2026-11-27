use super::{
    CommandCapabilities, CommandDefinition, CommandEntry, CommandId, CommandPaletteModel,
    CommandRegistry,
};

fn definition(id: CommandId, title: &'static str) -> CommandDefinition {
    CommandDefinition {
        id,
        title,
        category: "Test",
        keywords: &[],
    }
}

fn registry_of(titles: &[&'static str]) -> CommandRegistry {
    CommandRegistry::new(
        titles
            .iter()
            .map(|title| {
                CommandEntry::with_action(
                    definition(CommandId::StartScan, title),
                    Some(CommandId::StartScan),
                    None,
                )
            })
            .collect(),
    )
}

#[test]
fn plan_registry_has_titles_and_an_action_or_disabled_reason() {
    let scenarios = [
        CommandCapabilities::default(),
        CommandCapabilities {
            scan_target_available: true,
            refresh_target_available: true,
            operation_busy: false,
            walk_running: true,
            search_ready: true,
            duplicates_ready: true,
            duplicates_running: false,
        },
    ];
    for capabilities in scenarios {
        let registry = CommandRegistry::plan(capabilities);
        assert_eq!(registry.entries().len(), CommandId::ALL.len());
        for command in registry.entries() {
            assert!(!command.definition.title.trim().is_empty());
            assert!(command.action.is_some() || command.disabled_reason.is_some());
            if !command.is_runnable() {
                assert!(
                    command
                        .disabled_reason
                        .as_deref()
                        .is_some_and(|reason| !reason.trim().is_empty()),
                    "{} has no disabled reason",
                    command.definition.title
                );
            }
        }
    }
}

#[test]
fn no_command_without_an_action_is_listed_as_runnable() {
    let unbound = CommandEntry::with_action(
        definition(CommandId::OpenSettings, "Unbound command"),
        None,
        Some("No shell action is registered.".into()),
    );
    let malformed = CommandEntry::with_action(
        definition(CommandId::OpenSettings, "Malformed command"),
        None,
        None,
    );
    let registry = CommandRegistry::new(vec![unbound, malformed]);

    assert!(registry.entries().iter().all(|entry| !entry.is_runnable()));
    let ranked = registry.rank("command");
    assert_eq!(ranked.len(), 2);
    assert!(ranked.iter().all(|entry| !entry.command.is_runnable()));
    assert!(ranked.iter().all(|entry| entry.command.action.is_none()));
}

#[test]
fn exact_prefix_substring_and_fuzzy_matches_rank_in_that_order() {
    let registry = registry_of(&[
        "Start clean analysis",
        "Rescan now",
        "Scan folder",
        "Scan",
        "Open settings",
    ]);

    let titles: Vec<_> = registry
        .rank("scan")
        .into_iter()
        .map(|entry| entry.command.definition.title)
        .collect();
    assert_eq!(
        titles,
        ["Scan", "Scan folder", "Rescan now", "Start clean analysis"]
    );
}

#[test]
fn fuzzy_matching_is_case_insensitive_and_searches_keywords() {
    let mut entry = CommandEntry::runnable(CommandId::OpenSettings);
    entry.definition.keywords = &["preferences"];
    let registry = CommandRegistry::new(vec![entry]);

    assert_eq!(registry.rank("PREF").len(), 1);
    assert_eq!(registry.rank("SeTiNgS").len(), 1);
    assert!(registry.rank("missing").is_empty());
}

#[test]
fn tighter_fuzzy_match_wins_and_registration_order_breaks_ties() {
    let registry = registry_of(&["Open directory", "Open settings", "Stop scan"]);

    let titles: Vec<_> = registry
        .rank("open")
        .into_iter()
        .map(|entry| entry.command.definition.title)
        .collect();
    assert_eq!(titles, ["Open directory", "Open settings"]);
    assert_eq!(registry.rank("").len(), 3);
    assert_eq!(registry.rank("  ").len(), 3);
}

#[test]
fn model_re_filters_the_same_rows_used_by_confirmation() {
    let registry = CommandRegistry::plan(CommandCapabilities::default());
    let mut model = CommandPaletteModel::new(registry);

    model.set_query("settings");
    assert_eq!(model.query(), "settings");
    assert_eq!(model.visible().len(), 1);
    assert_eq!(
        model.command(0).map(|entry| entry.definition.id),
        Some(CommandId::OpenSettings)
    );
    assert!(model.command(0).is_some_and(CommandEntry::is_runnable));
    assert!(model.command(1).is_none());
}

#[test]
fn disabled_commands_remain_discoverable_but_cannot_run() {
    let registry = CommandRegistry::plan(CommandCapabilities::default());
    let ranked = registry.rank("duplicates");
    let entry = ranked
        .first()
        .expect("disabled command should be discoverable");

    assert_eq!(entry.command.definition.id, CommandId::FindDuplicates);
    assert!(entry.command.action.is_some());
    assert!(!entry.command.is_runnable());
    assert!(entry.command.disabled_reason.is_some());
}
