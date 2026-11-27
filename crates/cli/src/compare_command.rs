use std::path::Path;

use serde_json::{Value, json};
use timberstat_export::import_scan_json_file;
use timberstat_filesystem::EntryKind;
use timberstat_scan_comparison::{
    ChangeCategory, EntryChange, EntrySnapshot, ScanComparison, SizeDelta, SizeTotals, compare,
};
use timberstat_scan_history::{Scan, ScanStore};

use crate::{
    cancel::Cancellation,
    error::{CliError, CliResult, EXIT_SUCCESS},
    output,
};

pub fn run(before: &Path, after: &Path, cancel: &Cancellation) -> CliResult<u8> {
    cancel.check()?;
    eprintln!("compare: loading {}", before.display());
    let before = load_scan(before)?;
    eprintln!("compare: loading {}", after.display());
    let after = load_scan(after)?;
    let comparison =
        compare(&before, &after).map_err(|error| CliError::failure(error.to_string()))?;
    cancel.check()?;
    let contents = render(&comparison, cancel)?;
    cancel.check()?;
    output::stdout(&contents)?;
    eprintln!("compare: {} changes", comparison.changes.len());
    Ok(EXIT_SUCCESS)
}

fn load_scan(path: &Path) -> CliResult<Scan> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => {
            import_scan_json_file(path).map_err(|error| CliError::failure(error.to_string()))
        }
        Some("tscan") => load_binary_scan(path),
        _ => Err(CliError::usage(format!(
            "scan input {} must end in .json or .tscan",
            path.display()
        ))),
    }
}

fn load_binary_scan(path: &Path) -> CliResult<Scan> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let id = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".tscan"))
        .ok_or_else(|| CliError::usage(format!("invalid scan path {}", path.display())))?;
    ScanStore::new(directory)
        .load(id)
        .map_err(|error| CliError::failure(error.to_string()))
}

fn render(comparison: &ScanComparison, cancel: &Cancellation) -> CliResult<String> {
    let mut changes = Vec::with_capacity(comparison.changes.len());
    for (index, change) in comparison.changes.iter().enumerate() {
        if index.is_multiple_of(4_096) {
            cancel.check()?;
        }
        changes.push(change_json(change)?);
    }
    let mut directories = Vec::with_capacity(comparison.directories.len());
    for directory in &comparison.directories {
        directories.push(json!({
            "path": output::unicode_path(&directory.path)?,
            "delta": delta_json(&directory.delta),
        }));
    }
    output::json(&json!({
        "schema_version": 1,
        "root": output::unicode_path(&comparison.root)?,
        "before_timestamp_unix_secs": comparison.before_timestamp_unix_secs,
        "after_timestamp_unix_secs": comparison.after_timestamp_unix_secs,
        "summary": {
            "new": comparison.new_entries().count(),
            "deleted": comparison.deleted_entries().count(),
            "modified": comparison.modified_entries().count(),
        },
        "total": delta_json(&comparison.total),
        "changes": changes,
        "directories": directories,
    }))
}

fn change_json(change: &EntryChange) -> CliResult<Value> {
    Ok(json!({
        "path": output::unicode_path(&change.path)?,
        "category": match change.category {
            ChangeCategory::New => "new",
            ChangeCategory::Deleted => "deleted",
            ChangeCategory::Modified => "modified",
        },
        "logical_delta": change.logical_delta().to_string(),
        "physical_delta": change.physical_delta().map(|value| value.to_string()),
        "before": snapshot_json(change.before.as_ref()),
        "after": snapshot_json(change.after.as_ref()),
        "reasons": {
            "logical_size": change.reasons.logical_size,
            "physical_size": change.reasons.physical_size,
            "modified_time": change.reasons.modified_time,
            "kind": change.reasons.kind,
            "identity": change.reasons.identity,
        },
    }))
}

fn snapshot_json(snapshot: Option<&EntrySnapshot>) -> Value {
    let Some(snapshot) = snapshot else {
        return Value::Null;
    };
    json!({
        "kind": match snapshot.kind {
            EntryKind::File => "file",
            EntryKind::Directory => "directory",
            EntryKind::Symlink => "symlink",
            EntryKind::Other => "other",
        },
        "logical_size": snapshot.logical_size,
        "allocated_size": snapshot.physical_size,
        "modified_unix_secs": snapshot.modified_unix_secs,
        "identity": snapshot.identity.map(|identity| json!({
            "volume": identity.volume,
            "file_id": identity.file_id,
        })),
        "hardlink_duplicate": snapshot.hardlink_duplicate,
    })
}

fn delta_json(delta: &SizeDelta) -> Value {
    json!({
        "before": totals_json(&delta.before),
        "after": totals_json(&delta.after),
        "logical_delta": delta.logical.to_string(),
        "physical_delta": delta.physical.map(|value| value.to_string()),
    })
}

fn totals_json(totals: &SizeTotals) -> Value {
    json!({
        "logical": totals.logical.to_string(),
        "physical": totals.physical.map(|value| value.to_string()),
    })
}
