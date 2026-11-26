//! Search across the visualisations. The text is compiled with the `timberstat-search` query
//! language (`ext:iso size:>1gb NOT attr:hidden`) and evaluated once over the scanned tree.
//! The Files tree lists only matching rows and the folders that lead to them; the treemap
//! and sunburst dim everything but the matches. A query that does not parse filters nothing,
//! and its error is shown under the search box.

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use timberstat_filesystem::{FileTree, NodeId};
use timberstat_search::{Expr, ParseError, compile, features, run_with_duplicates};

use crate::tiles::Tile;

/// A search, compiled and evaluated. Rebuild it with [`Query::new`] whenever the text or the
/// tree changes, or with [`Query::with_duplicates`] when the duplicate result changes.
#[derive(Debug, Clone, Default)]
pub struct Query {
    text: String,
    /// Why the text does not parse, shown under the search box.
    error: Option<ParseError>,
    /// Match flag per node, indexed by node index. Empty unless `active`.
    hits: Vec<bool>,
    /// Per node: true when some node below it matches, at any depth. Lets the tree and the
    /// graphs follow a match that sits deeper than they draw.
    inside: Vec<bool>,
    /// How many nodes match, across the whole tree.
    matches: usize,
    /// True when the query parsed and was evaluated over a tree.
    active: bool,
    /// Terms the scan cannot answer, such as `alloc:` on a scan with no on-disk sizes.
    warnings: Vec<String>,
}

impl Query {
    /// Parses `text`. When there is a tree, also evaluates it. The tree must be rolled up.
    /// `dup:` terms match nothing here; use [`Query::with_duplicates`] once a search has run.
    pub fn new(text: &str, files: Option<&FileTree>) -> Self {
        Self::with_duplicates(text, files, None)
    }

    /// Like [`Query::new`], with the node ids of the last duplicate search's groups for `dup:`
    /// terms. `None` means no search has run.
    pub fn with_duplicates(
        text: &str,
        files: Option<&FileTree>,
        dups: Option<&HashSet<NodeId>>,
    ) -> Self {
        let text = text.trim().to_string();
        let (expr, error) = match compile(&text) {
            Ok(expr) => (expr, None),
            Err(error) => (None, Some(error)),
        };
        let evaluated = match (expr.as_ref(), files) {
            (Some(expr), Some(files)) => Some(evaluate(expr, files, dups)),
            _ => None,
        };
        let (found, warnings, active) = match evaluated {
            Some((found, warnings)) => (found, warnings, true),
            None => (Found::default(), Vec::new(), false),
        };
        Self {
            text,
            error,
            hits: found.hits,
            inside: found.inside,
            matches: found.matches,
            active,
            warnings,
        }
    }

    /// The text as typed, trimmed.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// True when a parsed query is filtering the tree.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// The parse error, when the text does not read as a query.
    pub fn error(&self) -> Option<&ParseError> {
        self.error.as_ref()
    }

    /// Warnings about terms the scan cannot answer.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// True when the node matches. Always false when inactive.
    pub fn hits(&self, node: NodeId) -> bool {
        self.active && self.hits.get(node.index()).copied().unwrap_or(false)
    }

    /// True when something below the node matches, at any depth. Always false when inactive.
    pub fn contains_hit(&self, node: NodeId) -> bool {
        self.active && self.inside.get(node.index()).copied().unwrap_or(false)
    }

    /// True when the node matches or holds a match, so it stays visible while searching.
    pub fn shows(&self, node: NodeId) -> bool {
        self.hits(node) || self.contains_hit(node)
    }

    /// How many items match across the whole scan. Zero when inactive.
    pub fn match_count(&self) -> usize {
        if self.active { self.matches } else { 0 }
    }

    /// True when the tile matches or anything inside its folder does, drawn or not. A folder
    /// with a match inside is kept bright so the match stays findable.
    pub fn tile_hits(&self, tile: &Tile) -> bool {
        self.shows(tile.node)
    }
}

/// What a query found: the match flag per node, the "holds a match" flag per node, and the
/// number of matches.
#[derive(Debug, Default)]
struct Found {
    hits: Vec<bool>,
    inside: Vec<bool>,
    matches: usize,
}

/// Runs the query over the tree. Returns what it found and the warnings.
fn evaluate(expr: &Expr, files: &FileTree, dups: Option<&HashSet<NodeId>>) -> (Found, Vec<String>) {
    let result = run_with_duplicates(files, expr, now_secs(), dups);
    let mut hits = vec![false; files.len()];
    let mut inside = vec![false; files.len()];
    let mut matches = 0;
    for id in &result.ids {
        if let Some(slot) = hits.get_mut(id.index()) {
            matches += usize::from(!*slot);
            *slot = true;
        }
        // Mark every folder above the match. A folder already marked has its own parents
        // marked too, so the walk stops there and the whole pass stays linear.
        let mut above = files.node(*id).and_then(|node| node.parent());
        while let Some(folder) = above {
            match inside.get_mut(folder.index()) {
                Some(slot) if !*slot => *slot = true,
                _ => break,
            }
            above = files.node(folder).and_then(|node| node.parent());
        }
    }
    let found = Found {
        hits,
        inside,
        matches,
    };
    let used = features(expr);
    let mut warnings = Vec::new();
    if used.allocated && !result.allocated_known {
        warnings.push(
            "alloc: this scan recorded no on-disk sizes, so alloc: matches nothing.".to_string(),
        );
    }
    if used.modified && !result.modified_known {
        warnings.push(
            "modified: this scan recorded no modification times, so it matches nothing."
                .to_string(),
        );
    }
    if used.attributes && !result.attributes_known {
        warnings.push("attr: this scan recorded no attributes, so it matches nothing.".to_string());
    }
    if used.duplicates && !result.duplicates_known {
        warnings.push(
            "dup: no duplicate search has run, so it matches nothing. Run Find duplicates first."
                .to_string(),
        );
    }
    (found, warnings)
}

/// Unix seconds now, for age terms. Zero if the clock is before 1970.
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use timberstat_filesystem::{EntryKind, Metadata, NewEntry};

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

    /// root / GTA / { levels.rpf, readme.txt }, rolled up. Returns the tree and GTA's id.
    fn sample() -> (FileTree, NodeId) {
        let mut tree = FileTree::with_root(NewEntry::directory(
            OsString::from("root"),
            Metadata::default(),
        ));
        let folder = tree
            .add_child(
                NodeId::ROOT,
                NewEntry::directory(OsString::from("GTA"), Metadata::default()),
            )
            .expect("add folder");
        tree.add_child(folder, file("levels.rpf"))
            .expect("add file");
        tree.add_child(folder, file("readme.txt"))
            .expect("add file");
        tree.roll_up();
        (tree, folder)
    }

    #[test]
    fn folders_above_a_match_hold_it_at_any_depth() {
        let (tree, folder) = sample();
        let query = Query::new("ext:rpf", Some(&tree));
        let levels = tree.children(folder).next().expect("first child");
        assert_eq!(query.match_count(), 1);
        assert!(query.contains_hit(NodeId::ROOT) && query.contains_hit(folder));
        assert!(query.shows(folder) && query.shows(levels));
        assert!(!query.contains_hit(levels), "a file holds nothing");
        let readme = tree.children(folder).nth(1).expect("second child");
        assert!(!query.shows(readme));
    }

    #[test]
    fn inactive_query_shows_nothing_and_counts_nothing() {
        let (tree, folder) = sample();
        let query = Query::new("size:>1zz", Some(&tree));
        assert!(!query.is_active());
        assert_eq!(query.match_count(), 0);
        assert!(!query.contains_hit(folder));
    }

    #[test]
    fn blank_query_is_inactive_without_error() {
        for text in ["", "   "] {
            let query = Query::new(text, None);
            assert!(!query.is_active());
            assert!(query.error().is_none());
        }
    }

    #[test]
    fn plain_word_matches_names_case_insensitively() {
        let (tree, folder) = sample();
        let query = Query::new("  LEVELS ", Some(&tree));
        assert!(query.is_active());
        let levels = tree.children(folder).next().expect("first child");
        assert!(query.hits(levels));
        assert!(!query.hits(folder));
    }

    #[test]
    fn field_term_filters_by_extension() {
        let (tree, folder) = sample();
        let query = Query::new("ext:rpf", Some(&tree));
        let mut children = tree.children(folder);
        let levels = children.next().expect("levels");
        let readme = children.next().expect("readme");
        assert!(query.hits(levels));
        assert!(!query.hits(readme));
    }

    #[test]
    fn or_and_not_combine_terms() {
        let (tree, folder) = sample();
        let mut children = tree.children(folder);
        let levels = children.next().expect("levels");
        let readme = children.next().expect("readme");
        let either = Query::new("levels.rpf OR readme.txt", Some(&tree));
        assert!(either.hits(levels) && either.hits(readme));
        let not_rpf = Query::new("NOT ext:rpf", Some(&tree));
        assert!(!not_rpf.hits(levels) && not_rpf.hits(readme));
    }

    #[test]
    fn parse_error_filters_nothing_and_reports_position() {
        let (tree, folder) = sample();
        let query = Query::new("nosuchfield:x", Some(&tree));
        assert!(!query.is_active());
        assert!(!query.hits(folder));
        let error = query.error().expect("parse error");
        assert_eq!(error.at, 0);
    }

    #[test]
    fn alloc_term_warns_when_scan_has_no_on_disk_sizes() {
        let (tree, folder) = sample();
        let query = Query::new("alloc:>1gb", Some(&tree));
        assert!(query.is_active());
        assert!(!query.hits(folder));
        assert_eq!(query.warnings().len(), 1);
        assert!(query.warnings()[0].starts_with("alloc:"));
    }

    #[test]
    fn no_tree_means_parsed_but_inactive() {
        let query = Query::new("levels", None);
        assert!(!query.is_active());
        assert!(query.error().is_none());
        assert!(query.warnings().is_empty());
    }
}
