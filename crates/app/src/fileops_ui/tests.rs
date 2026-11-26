//! Temp-directory-only integration check for the UI's worker/report/view-model path.
//! The fake Recycle Bin removes only the test fixture; the platform bin is never called.

use std::sync::Arc;

use timberstat_fileops::{
    Control, apply_delete, build_delete_plan, execute_delete_with, tree_base,
};
use timberstat_filesystem::NodeId;
use timberstat_scanner::{CancelToken, FileSystemScanner, GenericScanner, ScanOptions};

use super::view_model;

#[test]
fn recycle_worker_result_drives_memory_update_without_a_rescan() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("doomed")).unwrap();
    std::fs::write(temp.path().join("doomed/file.txt"), b"fixture").unwrap();
    std::fs::write(temp.path().join("keep.txt"), b"keep").unwrap();

    let output = GenericScanner
        .scan(
            temp.path(),
            &ScanOptions::default(),
            &CancelToken::new(),
            &mut |_| {},
        )
        .unwrap();
    let mut tree = output.tree;
    let doomed = tree
        .children(NodeId::ROOT)
        .find(|id| {
            tree.node(*id)
                .is_some_and(|node| node.name() == std::ffi::OsStr::new("doomed"))
        })
        .unwrap();
    let plan = build_delete_plan(&tree, &[doomed], &tree_base(temp.path())).unwrap();
    let control = Control::new();
    let report = execute_delete_with(plan.confirm_recycle_bin(), &control, &|path| {
        std::fs::remove_dir_all(path).unwrap();
        Ok(())
    });

    assert!(report.is_complete());
    assert!(!report.cancelled);
    assert_eq!(control.done(), control.total());
    assert!(!temp.path().join("doomed").exists());
    assert!(temp.path().join("keep.txt").exists());
    assert_eq!(apply_delete(&mut tree, &report), 2);
    assert!(!tree.is_attached(doomed));
    assert!(tree.is_attached(NodeId::ROOT));
    assert_eq!(view_model::result_headline(&report), report.summary());
    assert_eq!(
        view_model::progress_fraction(control.done(), control.total()),
        Some(1.0)
    );

    // Keep the report behind the same cheap shared ownership used by the result panel.
    let shared = Arc::new(report);
    assert!(shared.is_complete());
}
