//! Query language for HmstdDirStat.
//!
//! A query is a space-separated list of terms. Whitespace means AND, `OR` joins
//! alternatives, `NOT` or a leading `!` negates a term, and parentheses group.
//!
//! Terms:
//!
//! | Term | Matches |
//! |---|---|
//! | `report` | name contains `report` (case-insensitive) |
//! | `*.iso` / `game?` | name matches the wildcard |
//! | `"my report"` | quoted text is literal: no wildcards, no field names (spaces, colons) |
//! | `path:"My Games"` | field value may be quoted; quotes end at the closing quote |
//! | `path:Games` | full path contains the text |
//! | `ext:iso,zip` | file extension is one of the list |
//! | `size:>1gb`, `size:<=10mb`, `size:1gb..2gb`, `size:empty` | file or folder size |
//! | `alloc:>500mb` | on-disk size; false when the scan did not record it |
//! | `modified:<7d` | modified within the last 7 days (age, not date) |
//! | `modified:>1y` | modified more than a year ago |
//! | `modified:2024-01-01` | modified on that UTC day |
//! | `modified:2024-01-01..2024-06-30` | modified in that UTC date range |
//! | `type:file`, `type:dir`, `type:symlink`, `type:other` | entry kind |
//! | `attr:hidden`, `attr:readonly`, `attr:reparse` | Windows or Unix attributes |
//!
//! Units for sizes are binary (1 kb = 1024 bytes). Age units: `d` days, `w` weeks,
//! `m` 30-day months, `y` 365-day years.

mod ast;
mod eval;
mod lexer;
mod parse;
mod value;

pub use ast::{Attr, Cmp, Expr, Pred};
pub use eval::{Evaluation, Features, features, run};
pub use parse::{ParseError, compile};
