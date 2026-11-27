mod common;

use serde_json::{Value, json};
use timberstat_duplicates::{DuplicateGroup, DuplicateMember, DuplicateReport};
use timberstat_export::{
    ExportError, duplicates_to_json, import_scan_json, scan_record_to_json, scan_to_json,
    search_to_json,
};
use timberstat_filesystem::{FileIdentity, FileTree};
use timberstat_scan_history::Scan;
use timberstat_search::Evaluation;

use common::{directory, file, identified_file, provenance};

#[test]
fn json_scan_round_trips_through_scan_history_record() {
    let identity = FileIdentity {
        volume: 11,
        file_id: 222,
    };
    let provenance = provenance("C:/scan-root");
    let mut tree = FileTree::with_root(directory("root"));
    tree.add_child(
        tree.root(),
        identified_file("naïve 日本,\"quoted\"\r\n.bin", identity, false),
    )
    .unwrap();
    tree.add_child(tree.root(), identified_file("second.bin", identity, true))
        .unwrap();
    let original = Scan::from_tree(provenance.clone(), &tree).unwrap();

    let json = scan_to_json(&tree, &provenance).unwrap();
    let imported = import_scan_json(&json).unwrap();
    assert_eq!(imported, original);

    let reexported = scan_record_to_json(&imported).unwrap();
    assert_eq!(import_scan_json(&reexported).unwrap(), original);

    let document: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["provenance"]["root"], "C:/scan-root");
    assert_eq!(document["provenance"]["scan_method"], "generic_traversal");
    assert_eq!(document["provenance"]["timestamp_unix_secs"], 1_700_000_123);
    assert_eq!(document["entries"][1]["depth"], 1);
    assert_eq!(document["entries"][1]["hard_link_count"], 2);
}

#[test]
fn json_unicode_and_windows_long_paths_round_trip() {
    let root_name = format!(r"\\?\C:\{}\root", "界".repeat(100));
    let long_name = format!("{}-naïve-😀.txt", "長".repeat(140));
    let provenance = provenance(&root_name);
    let mut tree = FileTree::with_root(directory(&root_name));
    tree.add_child(tree.root(), file(&long_name, 999)).unwrap();
    let original = Scan::from_tree(provenance.clone(), &tree).unwrap();

    let json = scan_to_json(&tree, &provenance).unwrap();
    assert_eq!(import_scan_json(&json).unwrap(), original);
    let document: Value = serde_json::from_str(&json).unwrap();
    assert!(document["entries"][1]["path"].as_str().unwrap().len() > 260);
    assert_eq!(document["entries"][1]["name"], long_name);
}

#[test]
fn search_and_duplicate_json_have_provenance_and_results() {
    let provenance = provenance("C:/scan-root");
    let mut tree = FileTree::with_root(directory("root"));
    let first = tree.add_child(tree.root(), file("a.bin", 42)).unwrap();
    let second = tree.add_child(tree.root(), file("b.bin", 42)).unwrap();
    let results = Evaluation {
        ids: vec![second],
        ..Evaluation::default()
    };

    let search = search_to_json(&tree, &results, "ext:bin", &provenance).unwrap();
    let search: Value = serde_json::from_str(&search).unwrap();
    assert_eq!(search["schema_version"], 1);
    assert_eq!(search["provenance"]["root"], "C:/scan-root");
    assert_eq!(search["query"], "ext:bin");
    assert_eq!(search["results"].as_array().unwrap().len(), 1);
    assert_eq!(search["results"][0]["name"], "b.bin");

    let report = DuplicateReport {
        groups: vec![DuplicateGroup {
            size: 42,
            digest: [0xcd; 32],
            members: vec![
                DuplicateMember {
                    id: first,
                    path: tree.path_of(first).unwrap(),
                },
                DuplicateMember {
                    id: second,
                    path: tree.path_of(second).unwrap(),
                },
            ],
        }],
        ..DuplicateReport::default()
    };
    let duplicates = duplicates_to_json(&tree, &report, &provenance).unwrap();
    let duplicates: Value = serde_json::from_str(&duplicates).unwrap();
    assert_eq!(duplicates["groups"][0]["id"], 1);
    assert_eq!(duplicates["groups"][0]["digest"], "cd".repeat(32));
    assert_eq!(duplicates["groups"][0]["reclaimable_size"], 42);
    assert_eq!(
        duplicates["groups"][0]["members"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn import_rejects_unknown_schema_and_inconsistent_derived_columns() {
    let tree = FileTree::with_root(directory("root"));
    let json = scan_to_json(&tree, &provenance("root")).unwrap();
    let mut document: Value = serde_json::from_str(&json).unwrap();
    document["schema_version"] = json!(99);
    let error = import_scan_json(&document.to_string()).unwrap_err();
    assert!(matches!(
        error,
        ExportError::UnsupportedSchema { found: 99, .. }
    ));

    let mut document: Value = serde_json::from_str(&json).unwrap();
    document["entries"][0]["depth"] = json!(9);
    let error = import_scan_json(&document.to_string()).unwrap_err();
    assert!(matches!(error, ExportError::InvalidScan(_)));
}
