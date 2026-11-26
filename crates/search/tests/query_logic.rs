//! Boolean operators, grouping, blank input, and what a scan records (flags and features).

mod common;

use common::*;
use hmstd_filesystem::{FileTree, NodeId};
use hmstd_search::{compile, features};

#[test]
fn whitespace_is_and() {
    let tree = sample_tree(true);
    check(&tree, "report pdf", &["My Report.pdf"]);
    check(&tree, "report txt", &[]);
    check(&tree, "report AND pdf", &["My Report.pdf"]);
}

#[test]
fn or_joins_alternatives() {
    let tree = sample_tree(true);
    check(&tree, "ext:iso OR ext:zip", &["archive.zip", "setup.iso"]);
    check(&tree, "ext:iso or ext:zip", &["archive.zip", "setup.iso"]);
}

#[test]
fn and_binds_tighter_than_or() {
    let tree = sample_tree(true);
    // Means: zip OR (txt AND readme). Grouping it the other way would drop archive.zip.
    check(
        &tree,
        "ext:zip OR ext:txt readme",
        &["archive.zip", "readme.txt"],
    );
    // Means: (zip OR txt) AND readme.
    check(&tree, "(ext:zip OR ext:txt) readme", &["readme.txt"]);
}

#[test]
fn not_and_bang_negate_a_term() {
    let tree = sample_tree(true);
    let not_iso = names(&tree, "type:file NOT ext:iso");
    assert_eq!(not_iso.len(), 9);
    assert!(!not_iso.contains(&"setup.iso".to_string()));
    check(&tree, "type:file !ext:iso !ext:zip size:>1gb", &[]);
    check(&tree, "!type:dir ext:zip", &["archive.zip"]);
}

#[test]
fn parentheses_group_terms_and_negated_groups() {
    let tree = sample_tree(true);
    check(
        &tree,
        "(ext:iso OR ext:zip) type:file",
        &["archive.zip", "setup.iso"],
    );
    check(
        &tree,
        "!(ext:iso OR ext:zip) type:file report",
        &["My Report.pdf"],
    );
    check(&tree, "((ext:iso))", &["setup.iso"]);
}

#[test]
fn blank_query_means_no_filter() {
    assert!(compile("").unwrap().is_none());
    assert!(compile("   \t ").unwrap().is_none());
}

// ---- what the scan records -------------------------------------------------------------

#[test]
fn evaluation_reports_which_optional_data_the_scan_has() {
    let known = evaluate(&sample_tree(true), "size:>0");
    assert!(known.allocated_known && known.modified_known && known.attributes_known);

    let bare = {
        let mut tree = FileTree::with_root(dir("W", false, None));
        add(&mut tree, NodeId::ROOT, file("x.txt", 10, false, None));
        tree.roll_up();
        tree
    };
    // Filtered to files: the root folder's total (10 bytes) would also pass size:>5.
    let evaluation = evaluate(&bare, "size:>5 type:file");
    assert_eq!(evaluation.ids.len(), 1);
    assert!(!evaluation.allocated_known);
    assert!(!evaluation.modified_known);
    assert!(!evaluation.attributes_known);
    assert!(evaluate(&bare, "modified:<7d").ids.is_empty());
}

#[test]
fn features_lists_the_optional_data_a_query_reads() {
    let found = features(&expr_of("alloc:>1 name:x"));
    assert!(found.allocated && !found.modified && !found.attributes);

    let found = features(&expr_of("(modified:<7d OR NOT attr:hidden) size:>1"));
    assert!(found.modified && found.attributes && !found.allocated);

    let none = features(&expr_of("ext:iso size:>1gb"));
    assert_eq!(none, Default::default());
}
