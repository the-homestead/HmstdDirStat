//! Term tests: name, wildcard, quoted, path, ext, type, attr, size (incl. folder totals),
//! alloc (incl. unknown on-disk sizes) and modified (age, date, date range).

mod common;

use common::*;
use timberstat_filesystem::{FileTree, NodeId};

// ---- name, wildcard, quoted and path terms ---------------------------------------------

#[test]
fn bare_word_is_a_case_insensitive_name_substring() {
    let tree = sample_tree(true);
    check(&tree, "REPORT", &["My Report.pdf"]);
}

#[test]
fn wildcards_match_the_whole_name() {
    let tree = sample_tree(true);
    check(&tree, "*.iso", &["setup.iso"]);
    check(&tree, "read?e.txt", &["readme.txt"]);
    check(&tree, "name:*.iso", &["setup.iso"]);
}

#[test]
fn quoted_text_is_literal_with_spaces_and_no_wildcards() {
    let tree = sample_tree(true);
    check(&tree, "\"my report\"", &["My Report.pdf"]);
    check(&tree, "name:\"my report\"", &["My Report.pdf"]);
    // A literal "*.iso" appears in no name, so quoting disables the wildcard.
    check(&tree, "\"*.iso\"", &[]);
    check(&tree, "name:\"*.iso\"", &[]);
}

#[test]
fn path_term_matches_the_full_path_including_folders() {
    let tree = sample_tree(true);
    check(
        &tree,
        "path:games",
        &[
            "Games",
            "setup.iso",
            "readme.txt",
            "My Report.pdf",
            ".secret",
        ],
    );
}

#[test]
fn path_term_accepts_either_slash_and_quoted_values() {
    let tree = sample_tree(true);
    check(&tree, "path:games/setup", &["setup.iso"]);
    check(&tree, "path:\"games/setup\"", &["setup.iso"]);
}

#[test]
fn quoted_field_value_may_contain_spaces() {
    let tree = sample_tree(true);
    check(&tree, "path:\"games/my report\"", &["My Report.pdf"]);
}

// ---- extension and type ----------------------------------------------------------------

#[test]
fn ext_matches_any_listed_extension_ignoring_case_and_dot() {
    let tree = sample_tree(true);
    check(&tree, "ext:iso,zip", &["archive.zip", "setup.iso"]);
    check(&tree, "ext:.ISO", &["setup.iso"]);
    check(&tree, "ext:txt", &["empty.txt", "readme.txt"]);
}

#[test]
fn type_selects_the_entry_kind() {
    let tree = sample_tree(true);
    check(&tree, "type:dir", &["W", "Games", "Docs", "Split", "Empty"]);
    check(
        &tree,
        "type:folder",
        &["W", "Games", "Docs", "Split", "Empty"],
    );
    check(&tree, "type:symlink", &["link"]);
    check(&tree, "type:other", &[]);
    assert_eq!(names(&tree, "type:file").len(), 10);
}

// ---- attributes ------------------------------------------------------------------------

#[test]
fn attr_hidden_uses_the_hidden_bit_or_a_leading_dot() {
    let tree = sample_tree(true);
    check(&tree, "attr:hidden", &[".secret", "backup.tmp"]);
}

#[test]
fn attr_readonly_and_reparse() {
    let tree = sample_tree(true);
    check(&tree, "attr:readonly", &["setup.iso"]);
    check(&tree, "attr:reparse", &["link"]);
}

// ---- size, including folder totals -----------------------------------------------------

#[test]
fn size_comparison_and_range_on_files() {
    let tree = sample_tree(true);
    check(&tree, "size:>1gb type:file", &["archive.zip", "setup.iso"]);
    // Inclusive at both ends: archive.zip is exactly 2gb.
    check(
        &tree,
        "size:1gb..2gb type:file",
        &["archive.zip", "setup.iso"],
    );
    check(&tree, "size:1.5gb type:file", &["setup.iso"]);
    check(
        &tree,
        "size:<=100 type:file",
        &[
            ".secret",
            "backup.tmp",
            "empty.txt",
            "notes.md",
            "readme.txt",
        ],
    );
}

#[test]
fn folder_size_is_the_subtree_total() {
    let tree = sample_tree(true);
    // No child of Split is above 1gb, but the folder's total is 1.17gb.
    check(
        &tree,
        "size:>1gb type:dir",
        &["W", "Games", "Docs", "Split"],
    );
    check(&tree, "size:1gb..2gb type:dir", &["Games", "Split"]);
    check(&tree, "size:>1gb type:file", &["archive.zip", "setup.iso"]);
}

#[test]
fn size_empty_matches_zero_byte_files_folders_and_links() {
    let tree = sample_tree(true);
    check(&tree, "size:empty", &["Empty", "empty.txt", "link"]);
}

#[test]
fn size_with_huge_unit_saturates_instead_of_panicking() {
    let tree = sample_tree(true);
    check(&tree, "size:>99999999999999999999999tb", &[]);
}

// ---- on-disk (alloc) size --------------------------------------------------------------

#[test]
fn alloc_uses_recorded_on_disk_sizes() {
    let tree = sample_tree(true);
    check(&tree, "alloc:>1gb type:file", &["archive.zip", "setup.iso"]);
    check(
        &tree,
        "alloc:>1gb type:dir",
        &["W", "Games", "Docs", "Split"],
    );
    assert!(evaluate(&tree, "alloc:>1gb").allocated_known);
}

#[test]
fn alloc_matches_nothing_when_the_scan_has_no_on_disk_sizes() {
    let tree = sample_tree(false);
    let evaluation = evaluate(&tree, "alloc:>0");
    assert!(evaluation.ids.is_empty());
    assert!(!evaluation.allocated_known);
    // Logical sizes are still known, so size terms keep working.
    check(&tree, "size:>1gb type:file", &["archive.zip", "setup.iso"]);
}

#[test]
fn folder_alloc_is_unknown_when_any_child_has_no_on_disk_size() {
    let mut tree = FileTree::with_root(dir("W", true, None));
    let split = add(&mut tree, NodeId::ROOT, dir("Split", true, None));
    add(&mut tree, split, file("a.bin", 600 * MB, true, None));
    add(&mut tree, split, file("b.bin", 600 * MB, false, None));
    tree.roll_up();

    // Split and its root have partial totals, so neither matches a folder alloc term.
    check(&tree, "alloc:>0 type:dir", &[]);
    check(&tree, "alloc:>0 type:file", &["a.bin"]);
    assert!(evaluate(&tree, "alloc:>0").allocated_known);
}

// ---- modified: age, date, and date range -----------------------------------------------

#[test]
fn modified_age_within_days() {
    let tree = sample_tree(true);
    let within_a_week = &["W", "Games", "Docs", "readme.txt", ".secret"];
    check(&tree, "modified:<7d", within_a_week);
    // A bare age has the same "within the last N" meaning.
    check(&tree, "modified:7d", within_a_week);
    // Exactly one day old: "less than or equal" includes it, "less than" does not.
    check(&tree, "modified:<=1d type:dir", &["W", "Games"]);
    check(&tree, "modified:<1d", &[]);
}

#[test]
fn modified_older_than_a_year() {
    let tree = sample_tree(true);
    // My Report.pdf is dated 2024, so it is older than a year too.
    check(&tree, "modified:>1y", &["My Report.pdf", "notes.md"]);
    check(
        &tree,
        "modified:>1w type:file",
        &["My Report.pdf", "archive.zip", "notes.md"],
    );
}

#[test]
fn modified_on_a_utc_day_and_after_a_day() {
    let tree = sample_tree(true);
    check(&tree, "modified:2024-01-15", &["My Report.pdf"]);
    check(&tree, "modified:2024-01-01..2024-01-31", &["My Report.pdf"]);
    check(&tree, "modified:<2024-01-15", &[]);
    check(
        &tree,
        "modified:>2024-01-15 type:file",
        &[".secret", "archive.zip", "notes.md", "readme.txt"],
    );
}

#[test]
fn modified_on_a_range_of_days_is_inclusive() {
    let tree = sample_tree(true);
    check(&tree, "modified:2024-01-15..2024-01-15", &["My Report.pdf"]);
    check(&tree, "modified:2024-01-16..2024-01-31", &[]);
}
