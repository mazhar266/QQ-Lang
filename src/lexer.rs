// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Tokenizer.
//!
//! The grammar is pure ASCII, so this scans bytes and slices the input at the
//! offsets it finds. Tokens borrow the query; nothing here allocates.

use crate::error::Error;

/// Token kinds produced by the lexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `[A-Za-z][A-Za-z0-9_]*`
    Ident,
    /// `[0-9]+`
    Integer,
    /// `:`
    Colon,
    /// `;`
    Semicolon,
    /// `,`
    Comma,
    /// `-`
    Dash,
    /// End of input.
    Eof,
}

/// A token plus where it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'a> {
    /// What kind of token this is.
    pub kind: Kind,
    /// The exact source text, borrowed from the query.
    pub text: &'a str,
    /// Byte offset of the first character.
    pub position: usize,
}

/// Tokenize a whole query.
///
/// Always ends with a [`Kind::Eof`] token positioned at the end of the input.
pub fn tokenize(input: &str) -> Result<Vec<Token<'_>>, Error> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        let start = i;
        let kind = match b {
            b':' => Kind::Colon,
            b';' => Kind::Semicolon,
            b',' => Kind::Comma,
            b'-' => Kind::Dash,
            b if b.is_ascii_alphabetic() => {
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                push(&mut tokens, Kind::Ident, input, start, i);
                continue;
            }
            b if b.is_ascii_digit() => {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                push(&mut tokens, Kind::Integer, input, start, i);
                continue;
            }
            _ => return Err(Error::InvalidCharacter { position: start }),
        };

        i += 1;
        push(&mut tokens, kind, input, start, i);
    }

    tokens.push(Token {
        kind: Kind::Eof,
        text: "",
        position: input.len(),
    });
    Ok(tokens)
}

/// Both bounds always land on ASCII boundaries, so the slice cannot split a
/// multi-byte character.
fn push<'a>(tokens: &mut Vec<Token<'a>>, kind: Kind, input: &'a str, start: usize, end: usize) {
    tokens.push(Token {
        kind,
        text: &input[start..end],
        position: start,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<Kind> {
        tokenize(input).unwrap().iter().map(|t| t.kind).collect()
    }

    #[test]
    fn tokenizes_a_full_reference() {
        assert_eq!(
            kinds("Q:2:1-5,255;"),
            [
                Kind::Ident,
                Kind::Colon,
                Kind::Integer,
                Kind::Colon,
                Kind::Integer,
                Kind::Dash,
                Kind::Integer,
                Kind::Comma,
                Kind::Integer,
                Kind::Semicolon,
                Kind::Eof,
            ]
        );
    }

    #[test]
    fn whitespace_is_skipped_but_offsets_are_absolute() {
        let tokens = tokenize("  Q : 2").unwrap();
        assert_eq!(tokens[0].position, 2);
        assert_eq!(tokens[1].position, 4);
        assert_eq!(tokens[2].text, "2");
        assert_eq!(tokens[2].position, 6);
    }

    #[test]
    fn identifiers_take_digits_and_underscores_after_the_first_byte() {
        let tokens = tokenize("HM_2:1").unwrap();
        assert_eq!(tokens[0].kind, Kind::Ident);
        assert_eq!(tokens[0].text, "HM_2");
    }

    #[test]
    fn empty_input_is_just_eof() {
        assert_eq!(kinds(""), [Kind::Eof]);
    }

    #[test]
    fn rejects_unknown_bytes_with_a_position() {
        match tokenize("Q:2*3") {
            Err(Error::InvalidCharacter { position }) => assert_eq!(position, 3),
            other => panic!("expected InvalidCharacter, got {other:?}"),
        }
    }

    #[test]
    fn rejects_multibyte_input_without_panicking() {
        assert!(tokenize("Q:٢").is_err());
    }
}
