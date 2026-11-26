//! Splits query text into tokens. Quotes make text literal; `!`, `(`, `)` and `OR`
//! are structural only when unquoted. A `:` typed outside quotes separates a field
//! name from its value, so `path:"My Games"` quotes only the value.

use crate::parse::ParseError;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    LParen,
    RParen,
    Not,
    Or,
    /// A term. `colon` is the byte offset in `text` of the first `:` typed outside
    /// quotes, which splits the field name from its value. `quoted` is true when any
    /// part of the term was inside quotes.
    Word {
        text: String,
        colon: Option<usize>,
        quoted: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    pub tok: Tok,
    /// Byte offset in the query, for error messages.
    pub at: usize,
}

pub fn lex(input: &str) -> Result<Vec<Spanned>, ParseError> {
    let mut out = Vec::new();
    let mut chars = input.char_indices().peekable();
    while let Some(&(at, ch)) = chars.peek() {
        match ch {
            c if c.is_whitespace() => {
                chars.next();
            }
            '(' => {
                chars.next();
                out.push(Spanned {
                    tok: Tok::LParen,
                    at,
                });
            }
            ')' => {
                chars.next();
                out.push(Spanned {
                    tok: Tok::RParen,
                    at,
                });
            }
            '!' => {
                chars.next();
                out.push(Spanned { tok: Tok::Not, at });
            }
            _ => {
                let word = read_word(&mut chars)?;
                let keyword = word.text.to_ascii_lowercase();
                let tok = match (word.quoted, word.colon, keyword.as_str()) {
                    (false, None, "or") => Tok::Or,
                    (false, None, "not") => Tok::Not,
                    (false, None, "and") => continue,
                    _ => Tok::Word {
                        text: word.text,
                        colon: word.colon,
                        quoted: word.quoted,
                    },
                };
                out.push(Spanned { tok, at });
            }
        }
    }
    Ok(out)
}

struct RawWord {
    text: String,
    colon: Option<usize>,
    quoted: bool,
}

fn read_word(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> Result<RawWord, ParseError> {
    let mut text = String::new();
    let mut colon = None;
    let mut quoted = false;
    while let Some(&(at, ch)) = chars.peek() {
        if ch.is_whitespace() || ch == '(' || ch == ')' {
            break;
        }
        chars.next();
        if ch == '"' {
            quoted = true;
            let mut closed = false;
            for (_, inner) in chars.by_ref() {
                if inner == '"' {
                    closed = true;
                    break;
                }
                text.push(inner);
            }
            if !closed {
                return Err(ParseError::new(at, "missing closing quote"));
            }
        } else {
            if ch == ':' && colon.is_none() {
                colon = Some(text.len());
            }
            text.push(ch);
        }
    }
    Ok(RawWord {
        text,
        colon,
        quoted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(input: &str) -> Vec<(String, Option<usize>, bool)> {
        lex(input)
            .unwrap()
            .into_iter()
            .filter_map(|s| match s.tok {
                Tok::Word {
                    text,
                    colon,
                    quoted,
                } => Some((text, colon, quoted)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn colon_outside_quotes_splits_field_and_value() {
        assert_eq!(
            words(r#"path:"My Games""#),
            vec![("path:My Games".to_string(), Some(4), true)]
        );
    }

    #[test]
    fn colon_inside_quotes_is_literal() {
        assert_eq!(
            words(r#""ext:iso""#),
            vec![("ext:iso".to_string(), None, true)]
        );
    }

    #[test]
    fn keywords_are_case_insensitive_and_and_is_dropped() {
        let toks: Vec<Tok> = lex("a AND b or NOT c")
            .unwrap()
            .into_iter()
            .map(|s| s.tok)
            .collect();
        assert_eq!(toks.len(), 5);
        assert_eq!(
            toks[1],
            Tok::Word {
                text: "b".into(),
                colon: None,
                quoted: false
            }
        );
        assert_eq!(toks[2], Tok::Or);
        assert_eq!(toks[3], Tok::Not);
    }

    #[test]
    fn unclosed_quote_points_at_the_quote() {
        let err = lex("name:\"oops").unwrap_err();
        assert_eq!(err.at, 5);
    }
}
