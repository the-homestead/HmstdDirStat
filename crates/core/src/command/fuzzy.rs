//! Case-insensitive exact, prefix, substring and subsequence matching with deterministic ranking.

use std::cmp::Reverse;

use super::{CommandEntry, CommandRegistry};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MatchKind {
    Subsequence,
    Substring,
    Prefix,
    Exact,
}

/// Larger is more relevant. Within a match kind, earlier and tighter matches win.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct MatchQuality {
    kind: MatchKind,
    gaps: Reverse<usize>,
    start: Reverse<usize>,
    span: Reverse<usize>,
}

/// An owned row so the UI and a deferred confirm callback see the same ordering.
#[derive(Clone, Debug)]
pub struct RankedCommand {
    pub command: CommandEntry,
    quality: Option<MatchQuality>,
    registry_index: usize,
}

pub(super) fn rank(registry: &CommandRegistry, query: &str) -> Vec<RankedCommand> {
    let query = query.trim();
    if query.is_empty() {
        return registry
            .entries()
            .iter()
            .cloned()
            .enumerate()
            .map(|(registry_index, command)| RankedCommand {
                command,
                quality: None,
                registry_index,
            })
            .collect();
    }

    let mut ranked: Vec<_> = registry
        .entries()
        .iter()
        .cloned()
        .enumerate()
        .filter_map(|(registry_index, command)| {
            quality(&command, query).map(|quality| RankedCommand {
                command,
                quality: Some(quality),
                registry_index,
            })
        })
        .collect();
    ranked.sort_by(|left, right| {
        right
            .quality
            .cmp(&left.quality)
            .then_with(|| left.registry_index.cmp(&right.registry_index))
    });
    ranked
}

fn quality(command: &CommandEntry, query: &str) -> Option<MatchQuality> {
    std::iter::once(command.definition.title)
        .chain(std::iter::once(command.definition.category))
        .chain(command.definition.keywords.iter().copied())
        .filter_map(|candidate| score(candidate, query))
        .max()
}

fn score(text: &str, query: &str) -> Option<MatchQuality> {
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let query: Vec<char> = query.to_lowercase().chars().collect();
    if query.is_empty() || query.len() > text.len() {
        return None;
    }
    if text == query {
        return Some(MatchQuality {
            kind: MatchKind::Exact,
            gaps: Reverse(0),
            start: Reverse(0),
            span: Reverse(query.len()),
        });
    }
    if let Some(start) = text.windows(query.len()).position(|window| window == query) {
        let kind = if start == 0 {
            MatchKind::Prefix
        } else {
            MatchKind::Substring
        };
        return Some(MatchQuality {
            kind,
            gaps: Reverse(0),
            start: Reverse(start),
            span: Reverse(query.len()),
        });
    }

    let mut positions = Vec::with_capacity(query.len());
    let mut remaining = query.as_slice();
    for (index, character) in text.iter().enumerate() {
        if remaining.first() == Some(character) {
            positions.push(index);
            remaining = &remaining[1..];
            if remaining.is_empty() {
                break;
            }
        }
    }
    if !remaining.is_empty() {
        return None;
    }
    let start = *positions.first()?;
    let end = *positions.last()?;
    let span = end - start + 1;
    Some(MatchQuality {
        kind: MatchKind::Subsequence,
        gaps: Reverse(span - query.len()),
        start: Reverse(start),
        span: Reverse(span),
    })
}
