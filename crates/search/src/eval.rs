//! Evaluates an [`Expr`] over a [`FileTree`]. Folder sizes use the subtree totals,
//! so the tree must be rolled up before it is searched.

use std::collections::HashSet;

use hmstd_filesystem::{EntryKind, FileTree, Node, NodeId};

use crate::ast::{Attr, Expr, Pred};

/// Result of a run. The `*_known` flags say whether the scan recorded that data at
/// all. When one is false, terms that need it match nothing, and the caller should
/// say so. Check them with [`features`] to warn only about terms the query uses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Evaluation {
    pub ids: Vec<NodeId>,
    /// Some entry recorded an on-disk size.
    pub allocated_known: bool,
    /// Some entry recorded a modification time.
    pub modified_known: bool,
    /// Some entry recorded platform attribute bits (Windows or Unix mode).
    pub attributes_known: bool,
    /// A duplicate search result was supplied. Without one, `dup:` matches nothing.
    pub duplicates_known: bool,
}

/// Which optional data a query reads, so the caller can warn about missing data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Features {
    pub allocated: bool,
    pub modified: bool,
    pub attributes: bool,
    pub duplicates: bool,
}

/// Every node that matches `expr`, in tree order. `now` is Unix seconds, used for age terms.
///
/// The tree must be rolled up first: folder sizes come from the subtree totals.
/// Duplicate terms match nothing; use [`run_with_duplicates`] to supply a result.
pub fn run(tree: &FileTree, expr: &Expr, now: i64) -> Evaluation {
    run_with_duplicates(tree, expr, now, None)
}

/// Like [`run`], with the node ids of the last duplicate search's groups. `None` means no
/// search has run, so `dup:` terms match nothing and `duplicates_known` is false.
pub fn run_with_duplicates(
    tree: &FileTree,
    expr: &Expr,
    now: i64,
    dups: Option<&HashSet<NodeId>>,
) -> Evaluation {
    let mut known = Evaluation {
        duplicates_known: dups.is_some(),
        ..Evaluation::default()
    };
    for index in 0..tree.len() {
        let Some(node) = NodeId::from_index(index).and_then(|id| tree.node(id)) else {
            continue;
        };
        let metadata = node.metadata();
        known.allocated_known |= node.physical_size().is_some();
        known.modified_known |= metadata.modified_unix_secs.is_some();
        known.attributes_known |=
            metadata.windows_attributes.is_some() || metadata.unix_mode.is_some();
    }
    known.ids = (0..tree.len())
        .filter_map(NodeId::from_index)
        .filter(|&id| {
            tree.node(id)
                .is_some_and(|node| matches(tree, now, dups, expr, id, node))
        })
        .collect();
    known
}

/// The optional data `expr` reads.
pub fn features(expr: &Expr) -> Features {
    let mut found = Features::default();
    collect_features(expr, &mut found);
    found
}

fn collect_features(expr: &Expr, found: &mut Features) {
    match expr {
        Expr::And(parts) | Expr::Or(parts) => {
            for part in parts {
                collect_features(part, found);
            }
        }
        Expr::Not(inner) => collect_features(inner, found),
        Expr::Pred(pred) => match pred {
            Pred::Allocated(_) => found.allocated = true,
            Pred::Age(_) | Pred::At(_) => found.modified = true,
            Pred::Attr(_) => found.attributes = true,
            Pred::Dup(_) => found.duplicates = true,
            _ => {}
        },
    }
}

fn matches(
    tree: &FileTree,
    now: i64,
    dups: Option<&HashSet<NodeId>>,
    expr: &Expr,
    id: NodeId,
    node: &Node,
) -> bool {
    match expr {
        Expr::And(parts) => parts
            .iter()
            .all(|part| matches(tree, now, dups, part, id, node)),
        Expr::Or(parts) => parts
            .iter()
            .any(|part| matches(tree, now, dups, part, id, node)),
        Expr::Not(inner) => !matches(tree, now, dups, inner, id, node),
        Expr::Pred(pred) => pred_matches(tree, now, dups, pred, id, node),
    }
}

fn pred_matches(
    tree: &FileTree,
    now: i64,
    dups: Option<&HashSet<NodeId>>,
    pred: &Pred,
    id: NodeId,
    node: &Node,
) -> bool {
    let name = || node.name().to_string_lossy().to_lowercase();
    match pred {
        Pred::Name(text) => name().contains(text.as_str()),
        Pred::Glob(pattern) => glob_match(pattern, &name()),
        Pred::Path(text) => tree.path_of(id).is_some_and(|path| {
            path.to_string_lossy()
                .to_lowercase()
                .contains(text.as_str())
        }),
        Pred::Ext(list) => {
            node.kind() == EntryKind::File
                && extension(&name()).is_some_and(|ext| list.iter().any(|item| item == ext))
        }
        Pred::Size(cmp) => cmp.contains(logical_size(node)),
        Pred::Allocated(cmp) => allocated_size(node).is_some_and(|size| cmp.contains(size)),
        Pred::Age(cmp) => node
            .metadata()
            .modified_unix_secs
            .is_some_and(|modified| cmp.contains(now.saturating_sub(modified))),
        Pred::At(cmp) => node
            .metadata()
            .modified_unix_secs
            .is_some_and(|modified| cmp.contains(modified)),
        Pred::Kind(kind) => node.kind() == *kind,
        Pred::Attr(attr) => match attr {
            Attr::Hidden => node.metadata().is_hidden_attribute() || name().starts_with('.'),
            Attr::ReadOnly => node.metadata().readonly,
            Attr::Reparse => node.metadata().is_reparse_point(),
        },
        Pred::Dup(want) => dups.is_some_and(|set| set.contains(&id) == *want),
    }
}

fn logical_size(node: &Node) -> u64 {
    match node.kind() {
        EntryKind::Directory => node.totals().logical,
        _ => node.logical_size(),
    }
}

fn allocated_size(node: &Node) -> Option<u64> {
    match node.kind() {
        EntryKind::Directory => {
            let totals = node.totals();
            totals.physical_known.then_some(totals.physical)
        }
        _ => node.physical_size(),
    }
}

/// The extension of a lowercase name, without the dot. A leading dot alone is not an extension.
fn extension(lower_name: &str) -> Option<&str> {
    let (stem, ext) = lower_name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then_some(ext)
}

/// Wildcard match: `*` matches any run, `?` matches one character. Both strings are lowercase.
pub(crate) fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let (mut star, mut resume) = (None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            resume = t;
            p += 1;
        } else if let Some(star_at) = star {
            p = star_at + 1;
            resume += 1;
            t = resume;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    #[test]
    fn glob_handles_stars_and_question_marks() {
        assert!(glob_match("*.iso", "setup.iso"));
        assert!(glob_match("gam?", "game"));
        assert!(glob_match("a*b*c", "axxbyyc"));
        assert!(!glob_match("*.iso", "setup.zip"));
        assert!(!glob_match("gam?", "gam"));
        assert!(glob_match("*", ""));
    }
}
