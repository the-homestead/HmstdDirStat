//! Nested treemap layout (WizTree style): each folder is a box that holds its children,
//! and every child is sized by its logical size. Pure geometry over the scanned tree, so
//! it tests without a window.

use hmstd_filesystem::{FileTree, NodeId};

use crate::{
    explore::{self, Child},
    layout::{self, Rect},
};

/// One folder or file in the nested treemap. `rect` is in the unit square of the box this
/// tile sits in. `share` is the tile's fraction of the whole focused folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub node: NodeId,
    pub name: String,
    pub bytes: u64,
    pub is_dir: bool,
    pub rect: Rect,
    pub share: f64,
    pub children: Vec<Tile>,
}

/// How far the nesting goes and how much is drawn.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Levels of folders drawn, the focused folder's children being level one.
    pub depth: u32,
    /// Children drawn per folder; the rest are left out.
    pub children: usize,
    /// Tiles covering less than this share of the focused folder are left out.
    pub min_share: f64,
}

/// Lays out the children of `focus` as nested tiles inside a box `aspect` wide and 1 high.
pub fn nested(files: &FileTree, focus: NodeId, aspect: f64, limits: Limits) -> Vec<Tile> {
    place(files, focus, aspect, limits, limits.depth, 1.0)
}

fn place(
    files: &FileTree,
    parent: NodeId,
    aspect: f64,
    limits: Limits,
    depth_left: u32,
    parent_share: f64,
) -> Vec<Tile> {
    let kids: Vec<Child> = explore::children(files, parent)
        .into_iter()
        .filter(|child| child.bytes > 0)
        .take(limits.children)
        .collect();
    if kids.is_empty() || aspect <= 0.0 {
        return Vec::new();
    }
    let values: Vec<f64> = kids.iter().map(|child| child.bytes as f64).collect();
    let boxes = layout::squarify(
        &values,
        Rect {
            x: 0.0,
            y: 0.0,
            w: aspect,
            h: 1.0,
        },
    );

    let mut out = Vec::new();
    for (child, placed) in kids.into_iter().zip(boxes) {
        // Squarify works in a box `aspect` wide; bring the tile back into the unit square.
        let rect = Rect {
            x: placed.x / aspect,
            y: placed.y,
            w: placed.w / aspect,
            h: placed.h,
        };
        let share = parent_share * rect.area();
        if rect.area() <= 0.0 || share < limits.min_share {
            continue;
        }
        let children = if child.is_dir && depth_left > 1 {
            // The tile's physical shape: its unit width times the box aspect, over its height.
            let child_aspect = rect.w * aspect / rect.h;
            place(
                files,
                child.node,
                child_aspect,
                limits,
                depth_left - 1,
                share,
            )
        } else {
            Vec::new()
        };
        out.push(Tile {
            node: child.node,
            name: child.name,
            bytes: child.bytes,
            is_dir: child.is_dir,
            rect,
            share,
            children,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmstd_scanner::{CancelToken, FileSystemScanner, GenericScanner, ScanOptions};
    use std::path::Path;

    fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; bytes]).unwrap();
    }

    fn sample() -> (tempfile::TempDir, FileTree) {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("big/inner/x.bin"), 5000);
        write(&dir.path().join("big/y.bin"), 1000);
        write(&dir.path().join("mid.bin"), 900);
        write(&dir.path().join("small.bin"), 100);
        let tree = GenericScanner
            .scan(
                dir.path(),
                &ScanOptions::default(),
                &CancelToken::new(),
                &mut |_| {},
            )
            .unwrap()
            .tree;
        (dir, tree)
    }

    const ALL: Limits = Limits {
        depth: 3,
        children: 100,
        min_share: 0.0,
    };

    #[test]
    fn top_level_tiles_cover_the_box_by_size() {
        let (_dir, tree) = sample();
        let tiles = nested(&tree, NodeId::ROOT, 2.0, ALL);
        let total: f64 = tiles.iter().map(|tile| tile.share).sum();
        assert!((total - 1.0).abs() < 1e-9, "shares sum to {total}");
        let big = tiles.iter().find(|tile| tile.name == "big").unwrap();
        assert!((big.share - 6000.0 / 7000.0).abs() < 1e-9);
        for tile in &tiles {
            assert!(tile.rect.x >= -1e-9 && tile.rect.x + tile.rect.w <= 1.0 + 1e-9);
            assert!(tile.rect.y >= -1e-9 && tile.rect.y + tile.rect.h <= 1.0 + 1e-9);
        }
    }

    #[test]
    fn folders_hold_their_children_inside_their_box() {
        let (_dir, tree) = sample();
        let tiles = nested(&tree, NodeId::ROOT, 2.0, ALL);
        let big = tiles.iter().find(|tile| tile.name == "big").unwrap();
        assert!(big.is_dir);
        let child_share: f64 = big.children.iter().map(|tile| tile.share).sum();
        assert!(
            (child_share - big.share).abs() < 1e-9,
            "children fill their folder: {child_share} vs {}",
            big.share
        );
        for child in &big.children {
            assert!(child.rect.x >= -1e-9 && child.rect.x + child.rect.w <= 1.0 + 1e-9);
            assert!(child.rect.y >= -1e-9 && child.rect.y + child.rect.h <= 1.0 + 1e-9);
        }
        let inner = big
            .children
            .iter()
            .find(|tile| tile.name == "inner")
            .unwrap();
        assert_eq!(
            inner.children.len(),
            1,
            "the nesting reaches the file inside"
        );
        assert!(inner.children[0].children.is_empty());
    }

    #[test]
    fn depth_limit_stops_the_nesting() {
        let (_dir, tree) = sample();
        let shallow = Limits { depth: 1, ..ALL };
        let tiles = nested(&tree, NodeId::ROOT, 2.0, shallow);
        assert!(tiles.iter().all(|tile| tile.children.is_empty()));
    }

    #[test]
    fn small_tiles_are_left_out() {
        let (_dir, tree) = sample();
        let cut = Limits {
            min_share: 0.1,
            ..ALL
        };
        let tiles = nested(&tree, NodeId::ROOT, 2.0, cut);
        assert!(tiles.iter().all(|tile| tile.share >= 0.1));
        assert!(tiles.iter().all(|tile| tile.name != "small.bin"));
    }
}
