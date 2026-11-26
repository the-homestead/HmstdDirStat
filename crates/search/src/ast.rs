//! The parsed query. Built by `parse`, read by `eval`.

use hmstd_filesystem::EntryKind;

/// A boolean tree over predicates.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Not(Box<Expr>),
    Pred(Pred),
}

/// One condition on one entry.
#[derive(Debug, Clone, PartialEq)]
pub enum Pred {
    /// Name contains this text. Already lowercased.
    Name(String),
    /// Name matches this wildcard (`*`, `?`). Already lowercased.
    Glob(String),
    /// Full path contains this text. Already lowercased.
    Path(String),
    /// Extension is one of these, lowercase, without the leading dot.
    Ext(Vec<String>),
    /// Logical size of the entry (folders: subtree total).
    Size(Cmp<u64>),
    /// On-disk size of the entry (folders: subtree total). Never matches when unknown.
    Allocated(Cmp<u64>),
    /// Age in seconds, `now - modified`. Compared as an age.
    Age(Cmp<i64>),
    /// Modification time as Unix seconds. Compared as a timestamp.
    At(Cmp<i64>),
    Kind(EntryKind),
    Attr(Attr),
}

/// A comparison against one value or a closed range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmp<T> {
    Eq(T),
    Gt(T),
    Ge(T),
    Lt(T),
    Le(T),
    /// Inclusive on both ends.
    Between(T, T),
}

impl<T: PartialOrd + Copy> Cmp<T> {
    pub fn contains(&self, value: T) -> bool {
        match *self {
            Self::Eq(x) => value == x,
            Self::Gt(x) => value > x,
            Self::Ge(x) => value >= x,
            Self::Lt(x) => value < x,
            Self::Le(x) => value <= x,
            Self::Between(lo, hi) => value >= lo && value <= hi,
        }
    }
}

/// Attribute flags an entry can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attr {
    Hidden,
    ReadOnly,
    Reparse,
}
