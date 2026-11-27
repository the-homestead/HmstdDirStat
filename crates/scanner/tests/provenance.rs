use timberstat_filesystem::ScanMethod;
use timberstat_scanner::{CancelToken, FileSystemScanner, GenericScanner, ScanOptions};

#[test]
fn generic_scan_attaches_provenance_to_tree() {
    let dir = tempfile::tempdir().unwrap();
    let output = GenericScanner
        .scan(
            dir.path(),
            &ScanOptions::default(),
            &CancelToken::new(),
            &mut |_| {},
        )
        .unwrap();

    let provenance = output.tree.provenance().expect("scan provenance");
    assert_eq!(provenance.method, ScanMethod::GenericTraversal);
    assert_eq!(provenance.root, dir.path());
    assert!(provenance.timestamp_unix_secs > 0);
}
