use timberstat_duplicates::DuplicateReport;
use timberstat_filesystem::FileTree;
use timberstat_search::Evaluation;

use crate::{
    ExportError,
    row::{row_from_tree, tree_hard_link_counts},
};

const BASE_HEADER: [&str; 8] = [
    "path",
    "name",
    "kind",
    "logical_size",
    "allocated_size",
    "modified",
    "depth",
    "hard_link_count",
];
const DUPLICATE_HEADER: [&str; 10] = [
    "path",
    "name",
    "kind",
    "logical_size",
    "allocated_size",
    "modified",
    "depth",
    "hard_link_count",
    "group_id",
    "digest",
];

/// Exports every attached node in scan order, including the root at depth zero.
pub fn scan_to_csv(tree: &FileTree) -> Result<String, ExportError> {
    let hard_links = tree_hard_link_counts(tree);
    let mut rows = Vec::with_capacity(tree.len());
    for id in tree.attached_ids() {
        rows.push(row_from_tree(tree, id, &hard_links)?.csv_fields());
    }
    Ok(encode(&BASE_HEADER, &rows))
}

/// Exports exactly the node ids returned by the P2 search evaluator.
pub fn search_to_csv(tree: &FileTree, results: &Evaluation) -> Result<String, ExportError> {
    let hard_links = tree_hard_link_counts(tree);
    let mut rows = Vec::with_capacity(results.ids.len());
    for &id in &results.ids {
        rows.push(row_from_tree(tree, id, &hard_links)?.csv_fields());
    }
    Ok(encode(&BASE_HEADER, &rows))
}

/// Exports one row per duplicate member. `group_id` is one-based and stable in
/// report order; `digest` is the group's lower-case BLAKE3 digest.
pub fn duplicates_to_csv(tree: &FileTree, report: &DuplicateReport) -> Result<String, ExportError> {
    let hard_links = tree_hard_link_counts(tree);
    let member_count = report.groups.iter().map(|group| group.members.len()).sum();
    let mut rows = Vec::with_capacity(member_count);
    for (group_index, group) in report.groups.iter().enumerate() {
        let group_id = (group_index + 1).to_string();
        let digest = group.digest_hex();
        for member in &group.members {
            let mut row = row_from_tree(tree, member.id, &hard_links)?.csv_fields();
            row.push(group_id.clone());
            row.push(digest.clone());
            rows.push(row);
        }
    }
    Ok(encode(&DUPLICATE_HEADER, &rows))
}

fn encode(header: &[&str], rows: &[Vec<String>]) -> String {
    let estimated: usize = rows
        .iter()
        .map(|row| row.iter().map(String::len).sum::<usize>())
        .sum();
    let mut output = String::with_capacity(estimated + rows.len() * 16 + 128);
    write_record(&mut output, header.iter().copied());
    for row in rows {
        write_record(&mut output, row.iter().map(String::as_str));
    }
    output
}

fn write_record<'a>(output: &mut String, fields: impl IntoIterator<Item = &'a str>) {
    let mut first = true;
    for field in fields {
        if !first {
            output.push(',');
        }
        first = false;
        write_field(output, field);
    }
    output.push_str("\r\n");
}

fn write_field(output: &mut String, value: &str) {
    let quoted = value
        .bytes()
        .any(|byte| matches!(byte, b',' | b'"' | b'\r' | b'\n'));
    if !quoted {
        output.push_str(value);
        return;
    }
    output.push('"');
    for character in value.chars() {
        if character == '"' {
            output.push('"');
        }
        output.push(character);
    }
    output.push('"');
}
