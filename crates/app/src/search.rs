//! Search across the visualisations: one case-insensitive substring query, shared by the
//! treemap and sunburst. Matches are highlighted and everything else is dimmed.

use crate::tiles::Tile;

/// The text the user searched for. Empty means search is off and nothing is dimmed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    needle: String,
}

impl Query {
    pub fn new(text: &str) -> Self {
        Self {
            needle: text.trim().to_lowercase(),
        }
    }

    /// True when there is something to search for.
    pub fn is_active(&self) -> bool {
        !self.needle.is_empty()
    }

    /// True when `name` contains the query, ignoring case. Always false when inactive.
    pub fn hits(&self, name: &str) -> bool {
        self.is_active() && name.to_lowercase().contains(&self.needle)
    }

    /// True when the tile or any tile inside it matches. A folder with a matching child is
    /// kept bright so the match inside it stays visible.
    pub fn tile_hits(&self, tile: &Tile) -> bool {
        self.hits(&tile.name) || tile.children.iter().any(|child| self.tile_hits(child))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Rect;
    use hmstd_filesystem::NodeId;

    fn tile(name: &str, children: Vec<Tile>) -> Tile {
        Tile {
            node: NodeId::ROOT,
            name: name.to_string(),
            bytes: 1,
            is_dir: !children.is_empty(),
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            share: 0.0,
            children,
        }
    }

    #[test]
    fn empty_or_blank_query_is_inactive_and_hits_nothing() {
        for text in ["", "   "] {
            let query = Query::new(text);
            assert!(!query.is_active());
            assert!(!query.hits("anything"));
        }
    }

    #[test]
    fn match_is_case_insensitive_substring_and_trims() {
        let query = Query::new("  LEVELS ");
        assert!(query.is_active());
        assert!(query.hits("levels.rpf"));
        assert!(query.hits("Old_Levels_Backup"));
        assert!(!query.hits("models.rpf"));
    }

    #[test]
    fn folder_is_kept_when_a_child_matches() {
        let query = Query::new("data");
        let folder = tile(
            "GTA",
            vec![tile("readme.txt", vec![]), tile("data", vec![])],
        );
        assert!(!query.hits(&folder.name));
        assert!(query.tile_hits(&folder));
        assert!(!query.tile_hits(&tile("other", vec![])));
    }
}
