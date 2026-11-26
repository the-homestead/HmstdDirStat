//! The `dup:` term: matches members of the last duplicate search, and reports unknown data when
//! no search result exists.

use std::collections::HashSet;
use std::ffi::OsString;

use timberstat_filesystem::{EntryKind, FileTree, Metadata, NewEntry, NodeId};
use timberstat_search::{compile, features, run, run_with_duplicates};

fn file(name: &str) -> NewEntry {
    NewEntry {
        name: OsString::from(name),
        kind: EntryKind::File,
        logical_size: 10,
        physical_size: None,
        identity: None,
        hardlink_duplicate: false,
        unreadable: false,
        metadata: Metadata::default(),
    }
}

/// Root folder with three files: a.bin, b.bin, c.bin. Returns the tree and the three ids.
fn sample() -> (FileTree, NodeId, NodeId, NodeId) {
    let root = NewEntry::directory(OsString::from("root"), Metadata::default());
    let mut tree = FileTree::with_root(root);
    let a = tree.add_child(NodeId::ROOT, file("a.bin")).expect("a");
    let b = tree.add_child(NodeId::ROOT, file("b.bin")).expect("b");
    let c = tree.add_child(NodeId::ROOT, file("c.bin")).expect("c");
    tree.roll_up();
    (tree, a, b, c)
}

fn expr(text: &str) -> timberstat_search::Expr {
    compile(text).expect("parses").expect("non-empty")
}

#[test]
fn dup_yes_matches_only_the_members_of_the_result() {
    let (tree, a, b, _c) = sample();
    let members: HashSet<NodeId> = [a, b].into_iter().collect();
    let result = run_with_duplicates(&tree, &expr("dup:yes"), 0, Some(&members));
    assert!(result.duplicates_known);
    assert_eq!(result.ids, vec![a, b]);
}

#[test]
fn dup_no_matches_everything_else() {
    let (tree, a, b, c) = sample();
    let members: HashSet<NodeId> = [a, b].into_iter().collect();
    let result = run_with_duplicates(&tree, &expr("dup:no"), 0, Some(&members));
    assert!(result.ids.contains(&c));
    assert!(!result.ids.contains(&a));
    assert!(!result.ids.contains(&b));
}

#[test]
fn dup_without_a_result_matches_nothing_and_reports_unknown() {
    let (tree, _a, _b, _c) = sample();
    let query = expr("dup:yes");
    let result = run(&tree, &query, 0);
    assert!(result.ids.is_empty());
    assert!(!result.duplicates_known);
    assert!(features(&query).duplicates);
    // dup:no also matches nothing without a result, so it is reported the same way.
    let negated = run(&tree, &expr("dup:no"), 0);
    assert!(negated.ids.is_empty());
}

#[test]
fn dup_term_combines_with_other_terms() {
    let (tree, a, b, _c) = sample();
    let members: HashSet<NodeId> = [a, b].into_iter().collect();
    let result = run_with_duplicates(&tree, &expr("dup:yes ext:bin a"), 0, Some(&members));
    assert_eq!(result.ids, vec![a]);
}

#[test]
fn dup_value_must_be_yes_or_no() {
    assert!(compile("dup:maybe").is_err());
    assert!(compile("dup:").is_err());
}
