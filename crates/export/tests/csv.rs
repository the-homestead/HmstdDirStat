mod common;

use timberstat_duplicates::{DuplicateGroup, DuplicateMember, DuplicateReport};
use timberstat_export::{duplicates_to_csv, scan_to_csv, search_to_csv};
use timberstat_filesystem::{FileIdentity, FileTree};
use timberstat_search::Evaluation;

use common::{directory, file, identified_file, parse_csv, tree_with_files};

#[test]
fn escapes_comma_quote_crlf_and_lf() {
    let names = [
        "comma,name.txt",
        "say \"hello\".txt",
        "line1\r\nline2\nline3.txt",
    ];
    let csv = scan_to_csv(&tree_with_files(&names)).unwrap();
    assert!(csv.contains("\"comma,name.txt\""));
    assert!(csv.contains("\"say \"\"hello\"\".txt\""));
    assert!(csv.contains("\"line1\r\nline2\nline3.txt\""));
    assert!(csv.ends_with("\r\n"));

    let rows = parse_csv(&csv);
    assert_eq!(
        rows[0],
        [
            "path",
            "name",
            "kind",
            "logical_size",
            "allocated_size",
            "modified",
            "depth",
            "hard_link_count"
        ]
    );
    for (index, name) in names.into_iter().enumerate() {
        assert_eq!(rows[index + 2][1], name);
        assert_eq!(rows[index + 2].len(), 8);
    }
}

#[test]
fn unicode_and_windows_long_paths_round_trip() {
    let root_name = format!(r"\\?\C:\{}\root", "目".repeat(100));
    let long_name = format!("{}-naïve-😀.txt", "長".repeat(140));
    let mut tree = FileTree::with_root(directory(&root_name));
    let id = tree.add_child(tree.root(), file(&long_name, 123)).unwrap();
    let expected_path = tree.path_of(id).unwrap();
    let expected_path = expected_path.as_os_str().to_str().unwrap().to_owned();

    let rows = parse_csv(&scan_to_csv(&tree).unwrap());
    assert!(rows[2][0].len() > 260);
    assert_eq!(rows[2][0], expected_path);
    assert_eq!(rows[2][1], long_name);
}

#[test]
fn search_rows_include_sizes_depth_modified_and_hard_link_counts() {
    let identity = FileIdentity {
        volume: 7,
        file_id: 99,
    };
    let mut tree = FileTree::with_root(directory("root"));
    let first = tree
        .add_child(tree.root(), identified_file("first.bin", identity, false))
        .unwrap();
    let second = tree
        .add_child(tree.root(), identified_file("second.bin", identity, true))
        .unwrap();
    let results = Evaluation {
        ids: vec![second, first],
        ..Evaluation::default()
    };

    let rows = parse_csv(&search_to_csv(&tree, &results).unwrap());
    assert_eq!(rows[1][1], "second.bin");
    assert_eq!(rows[2][1], "first.bin");
    for row in &rows[1..] {
        assert_eq!(row[2], "file");
        assert_eq!(row[3], "42");
        assert_eq!(row[4], "4096");
        assert_eq!(row[5], "1699999999");
        assert_eq!(row[6], "1");
        assert_eq!(row[7], "2");
    }
}

#[test]
fn duplicate_csv_preserves_group_and_digest() {
    let mut tree = FileTree::with_root(directory("root"));
    let first = tree.add_child(tree.root(), file("a.bin", 42)).unwrap();
    let second = tree.add_child(tree.root(), file("b.bin", 42)).unwrap();
    let report = DuplicateReport {
        groups: vec![DuplicateGroup {
            size: 42,
            digest: [0xab; 32],
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

    let rows = parse_csv(&duplicates_to_csv(&tree, &report).unwrap());
    assert_eq!(rows[0].len(), 10);
    assert_eq!(rows[0][8], "group_id");
    assert_eq!(rows[0][9], "digest");
    for row in &rows[1..] {
        assert_eq!(row[8], "1");
        assert_eq!(row[9], "ab".repeat(32));
    }
}
