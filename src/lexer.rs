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
    /// `~`, which opens the item range of a search scope.
    Tilde,
    /// A quoted search term, written with `"` or `'`. `text` is the content,
    /// without the quotes.
    Text,
    /// A backtick-quoted term, which asks for similarity rather than an exact
    /// match. `text` is the content, without the backticks.
    Similar,
    /// A quoted term prefixed with `?`, which asks for ranked full-text
    /// search. `text` is the content, without the marker or the quotes.
    FullText,
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

        // A quoted search term, in either quote. There are no escapes: the
        // first matching close ends it, which keeps needles literal and the
        // lexer honest. Accepting both means a term containing one quote can
        // always be written with the other — `'Allah\'s'` would end early,
        // `"Allah's"` does not.
        //
        // Both quotes are ASCII and so cannot occur inside a multi-byte
        // sequence, making this byte scan safe for any UTF-8 content.
        // `?` only ever introduces a quoted term, so it is read together with
        // the quote rather than as a token of its own.
        let full_text = b == b'?' && matches!(bytes.get(i + 1), Some(b'"' | b'\'' | b'`'));
        if full_text || b == b'"' || b == b'\'' || b == b'`' {
            let open = i;
            if full_text {
                i += 1;
            }
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            if i == bytes.len() {
                return Err(Error::UnterminatedText { position: open });
            }
            let kind = if full_text {
                Kind::FullText
            } else if quote == b'`' {
                Kind::Similar
            } else {
                Kind::Text
            };
            let from = if full_text { open + 2 } else { open + 1 };
            tokens.push(Token {
                kind,
                text: &input[from..i],
                position: open,
            });
            i += 1;
            continue;
        }

        let kind = match b {
            b':' => Kind::Colon,
            b';' => Kind::Semicolon,
            b',' => Kind::Comma,
            b'-' => Kind::Dash,
            b'~' => Kind::Tilde,
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
    fn quoted_terms_use_either_quote() {
        for query in [r#"q:1:"text""#, "q:1:'text'"] {
            let tokens = tokenize(query).unwrap();
            let term = tokens[tokens.len() - 2];
            assert_eq!(term.kind, Kind::Text, "for {query}");
            assert_eq!(term.text, "text", "for {query}");
            assert_eq!(term.position, 4, "for {query}");
        }
    }

    #[test]
    fn one_quote_can_carry_the_other_verbatim() {
        let tokens = tokenize("q:\"Allah's\"").unwrap();
        assert_eq!(tokens[2].text, "Allah's");

        let tokens = tokenize("q:'say \"this\"'").unwrap();
        assert_eq!(tokens[2].text, "say \"this\"");
    }

    #[test]
    fn an_unclosed_term_reports_its_opening_quote() {
        for (query, position) in [(r#"q:"abc"#, 2), ("q:'abc", 2)] {
            match tokenize(query) {
                Err(Error::UnterminatedText { position: got }) => assert_eq!(got, position),
                other => panic!("expected UnterminatedText for {query}, got {other:?}"),
            }
        }
    }

    #[test]
    fn rejects_multibyte_input_without_panicking() {
        assert!(tokenize("Q:٢").is_err());
    }
}
