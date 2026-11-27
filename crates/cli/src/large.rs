use std::path::Path;

use serde::Serialize;
use timberstat_filesystem::EntryKind;

use crate::{
    cancel::Cancellation,
    engine::{self, Scanned},
    error::CliResult,
    output,
};

#[derive(Debug, Serialize)]
struct LargeDocument {
    schema_version: u32,
    root: String,
    complete: bool,
    issues: u64,
    top: usize,
    files: Vec<LargeFile>,
}

#[derive(Debug, Serialize)]
struct LargeFile {
    path: String,
    logical_size: u64,
    allocated_size: Option<u64>,
}

pub fn run(root: &Path, top: usize, cancel: &Cancellation) -> CliResult<u8> {
    let scanned = engine::scan(root, cancel)?;
    let contents = render(&scanned, top, cancel)?;
    let status = scanned.exit_code();
    cancel.check()?;
    output::stdout(&contents)?;
    Ok(status)
}

pub fn render(scanned: &Scanned, top: usize, cancel: &Cancellation) -> CliResult<String> {
    cancel.check()?;
    let mut files = Vec::new();
    for (visited, id) in scanned.tree.attached_ids().enumerate() {
        if visited.is_multiple_of(65_536) {
            cancel.check()?;
        }
        let Some(node) = scanned.tree.node(id) else {
            continue;
        };
        if node.kind() != EntryKind::File {
            continue;
        }
        let path = scanned
            .tree
            .path_of(id)
            .ok_or_else(|| crate::error::CliError::failure("could not resolve a scanned path"))?;
        files.push(LargeFile {
            path: output::unicode_path(&path)?,
            logical_size: node.logical_size(),
            allocated_size: node.physical_size(),
        });
    }
    files.sort_by(|left, right| {
        right
            .logical_size
            .cmp(&left.logical_size)
            .then_with(|| left.path.cmp(&right.path))
    });
    files.truncate(top);
    cancel.check()?;
    output::json(&LargeDocument {
        schema_version: 1,
        root: output::unicode_path(&scanned.provenance.root)?,
        complete: !scanned.is_partial(),
        issues: scanned.report.issue_count(),
        top,
        files,
    })
}
