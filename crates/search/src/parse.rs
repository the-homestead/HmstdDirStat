//! Parses tokens into an [`Expr`]. Grammar, lowest precedence first:
//!
//! `expr := and ("OR" and)*`, `and := unary+` (whitespace is AND),
//! `unary := ("NOT" | "!") unary | atom`, `atom := "(" expr ")" | term`.

use timberstat_filesystem::EntryKind;

use crate::ast::{Attr, Expr, Pred};
use crate::lexer::{Spanned, Tok, lex};
use crate::value::{self, Modified};

/// A query that could not be read. `at` is a byte offset into the query text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message} (at position {at})")]
pub struct ParseError {
    pub at: usize,
    pub message: String,
}

impl ParseError {
    pub(crate) fn new(at: usize, message: impl Into<String>) -> Self {
        Self {
            at,
            message: message.into(),
        }
    }
}

/// Compiles a query. Blank input means no filter and returns `Ok(None)`.
pub fn compile(input: &str) -> Result<Option<Expr>, ParseError> {
    let toks = lex(input)?;
    if toks.is_empty() {
        return Ok(None);
    }
    let mut parser = Parser {
        toks,
        pos: 0,
        end: input.len(),
    };
    let expr = parser.or_expr()?;
    if let Some(extra) = parser.peek() {
        return Err(ParseError::new(extra.at, "unexpected ')'"));
    }
    Ok(Some(expr))
}

struct Parser {
    toks: Vec<Spanned>,
    pos: usize,
    end: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Spanned> {
        self.toks.get(self.pos)
    }

    fn bump(&mut self) -> Option<Spanned> {
        let tok = self.toks.get(self.pos).cloned();
        self.pos += 1;
        tok
    }

    fn or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut parts = vec![self.and_expr()?];
        while matches!(self.peek(), Some(Spanned { tok: Tok::Or, .. })) {
            self.bump();
            parts.push(self.and_expr()?);
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Expr::Or(parts)
        })
    }

    fn and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut parts = Vec::new();
        while !matches!(
            self.peek(),
            None | Some(Spanned {
                tok: Tok::Or | Tok::RParen,
                ..
            })
        ) {
            parts.push(self.unary()?);
        }
        if parts.is_empty() {
            return Err(match self.peek() {
                Some(spanned) => ParseError::new(spanned.at, "expected a search term here"),
                None => ParseError::new(self.end, "expected a search term"),
            });
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Expr::And(parts)
        })
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        if matches!(self.peek(), Some(Spanned { tok: Tok::Not, .. })) {
            self.bump();
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, ParseError> {
        let Some(spanned) = self.bump() else {
            return Err(ParseError::new(self.end, "expected a search term"));
        };
        match spanned.tok {
            Tok::LParen => {
                let inner = self.or_expr()?;
                match self.bump() {
                    Some(Spanned {
                        tok: Tok::RParen, ..
                    }) => Ok(inner),
                    _ => Err(ParseError::new(spanned.at, "missing ')' for this '('")),
                }
            }
            Tok::Word {
                text,
                colon,
                quoted,
            } => {
                if quoted && text.is_empty() {
                    return Err(ParseError::new(spanned.at, "empty quoted term"));
                }
                Ok(Expr::Pred(to_pred(&text, colon, quoted, spanned.at)?))
            }
            Tok::RParen => Err(ParseError::new(spanned.at, "unexpected ')'")),
            Tok::Or | Tok::Not => Err(ParseError::new(spanned.at, "expected a search term")),
        }
    }
}

/// `colon` marks the field/value split found by the lexer. Without one, a quoted term
/// is a literal name, and an unquoted term is a name or wildcard.
fn to_pred(text: &str, colon: Option<usize>, quoted: bool, at: usize) -> Result<Pred, ParseError> {
    match colon {
        Some(split) => field_pred(
            &text[..split].to_ascii_lowercase(),
            &text[split + 1..],
            quoted,
            at,
        ),
        None if quoted => Ok(Pred::Name(text.to_lowercase())),
        None => Ok(name_pred(text)),
    }
}

/// A bare name term: a substring, or a wildcard when it contains `*` or `?`.
fn name_pred(text: &str) -> Pred {
    let lower = text.to_lowercase();
    if lower.contains('*') || lower.contains('?') {
        Pred::Glob(lower)
    } else {
        Pred::Name(lower)
    }
}

fn field_pred(field: &str, value: &str, quoted: bool, at: usize) -> Result<Pred, ParseError> {
    if value.is_empty() {
        return Err(ParseError::new(
            at,
            format!("missing value after '{field}:'"),
        ));
    }
    let bad = |message: String| ParseError::new(at, message);
    match field {
        "name" if quoted => Ok(Pred::Name(value.to_lowercase())),
        "name" => Ok(name_pred(value)),
        // Paths are stored with the platform separator, so accept either slash.
        "path" => Ok(Pred::Path(
            value
                .replace('/', std::path::MAIN_SEPARATOR_STR)
                .to_lowercase(),
        )),
        "ext" => {
            let exts: Vec<String> = value
                .split(',')
                .map(|item| item.trim().trim_start_matches('.').to_lowercase())
                .filter(|item| !item.is_empty())
                .collect();
            if exts.is_empty() {
                return Err(bad("ext: needs at least one extension".into()));
            }
            Ok(Pred::Ext(exts))
        }
        "size" => value::size(value).map(Pred::Size).map_err(bad),
        "alloc" => value::size(value).map(Pred::Allocated).map_err(bad),
        "modified" => Ok(match value::modified(value).map_err(bad)? {
            Modified::Age(cmp) => Pred::Age(cmp),
            Modified::At(cmp) => Pred::At(cmp),
        }),
        "type" => match value.to_ascii_lowercase().as_str() {
            "file" => Ok(Pred::Kind(EntryKind::File)),
            "dir" | "folder" => Ok(Pred::Kind(EntryKind::Directory)),
            "symlink" => Ok(Pred::Kind(EntryKind::Symlink)),
            "other" => Ok(Pred::Kind(EntryKind::Other)),
            _ => Err(bad(format!(
                "unknown type '{value}': use file, dir, symlink or other"
            ))),
        },
        "attr" => match value.to_ascii_lowercase().as_str() {
            "hidden" => Ok(Pred::Attr(Attr::Hidden)),
            "readonly" => Ok(Pred::Attr(Attr::ReadOnly)),
            "reparse" => Ok(Pred::Attr(Attr::Reparse)),
            _ => Err(bad(format!(
                "unknown attr '{value}': use hidden, readonly or reparse"
            ))),
        },
        "dup" => match value.to_ascii_lowercase().as_str() {
            "yes" | "true" => Ok(Pred::Dup(true)),
            "no" | "false" => Ok(Pred::Dup(false)),
            _ => Err(bad(format!("unknown dup '{value}': use yes or no"))),
        },
        _ => Err(bad(format!(
            "unknown field '{field}:'. Quote the term to search for it literally"
        ))),
    }
}
