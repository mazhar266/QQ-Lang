// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Recursive-descent parser.
//!
//! Knows the grammar and nothing else. It has no table of Surah counts and no
//! match on source codes — `Q:500:999` and `XYZ:1:2` both parse cleanly and
//! are rejected later, by the resolver and the registry respectively. It does
//! not even know which source is the default; it records that the code was
//! omitted and lets the registry decide.
//!
//! ```text
//! query     := reference (';' reference)* ';'?
//! reference := (source ':')? body
//! body      := ':' selector              // B::100, book-wide numbering
//!            | group (',' group)*
//! group     := primary (':' selector)?
//! selector  := item (',' item)*
//! item      := integer | integer '-' integer
//! ```
//!
//! One rule resolves the only ambiguity: **an integer followed by `:` starts a
//! new group** rather than continuing the current selector. So in
//! `Q:1:2,3,2:3,4-6` the third `2` is a Surah, not an ayah, and the query
//! yields two references. The same rule makes a bare `1,2:255` read as "all of
//! 1, then 2:255".
//!
//! A `reference` can therefore produce several [`Reference`] nodes. They are
//! appended in written order, so nothing downstream needs to know that the
//! grouping syntax exists.

use crate::ast::{Query, Range, Reference};
use crate::error::Error;
use crate::lexer::{tokenize, Kind, Token};

/// Parse a query. Touches no files.
pub fn parse(input: &str) -> Result<Query, Error> {
    if input.trim().is_empty() {
        return Err(Error::EmptyQuery);
    }

    let tokens = tokenize(input)?;
    let mut parser = Parser { tokens, index: 0 };
    parser.query()
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    index: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Token<'a> {
        self.tokens[self.index]
    }

    /// Look `offset` tokens ahead, saturating at the trailing `Eof`.
    fn peek_at(&self, offset: usize) -> Token<'a> {
        let last = self.tokens.len() - 1;
        self.tokens[(self.index + offset).min(last)]
    }

    fn advance(&mut self) -> Token<'a> {
        let token = self.tokens[self.index];
        if token.kind != Kind::Eof {
            self.index += 1;
        }
        token
    }

    fn eat(&mut self, kind: Kind) -> bool {
        if self.peek().kind == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    /// `query := reference (';' reference)* ';'?`
    fn query(&mut self) -> Result<Query, Error> {
        let mut references = Vec::new();

        loop {
            references.extend(self.reference()?);

            if !self.eat(Kind::Semicolon) {
                break;
            }
            if self.peek().kind == Kind::Eof {
                break; // trailing ';'
            }
        }

        let token = self.peek();
        if token.kind != Kind::Eof {
            return Err(Error::InvalidCharacter {
                position: token.position,
            });
        }

        Ok(Query { references })
    }

    /// One source's worth of groups. `;` is only needed to change source.
    fn reference(&mut self) -> Result<Vec<Reference>, Error> {
        let token = self.peek();
        if token.kind != Kind::Ident && token.kind != Kind::Integer {
            return Err(Error::ExpectedSource {
                position: token.position,
            });
        }

        // An omitted source is recorded as `None`, not filled in here — which
        // source that means belongs to the registry.
        let source = if token.kind == Kind::Ident {
            let code = self.advance().text.to_ascii_uppercase();
            let token = self.peek();
            if !self.eat(Kind::Colon) {
                return Err(Error::ExpectedColon {
                    position: token.position,
                });
            }
            Some(code)
        } else {
            None
        };

        let mut references = Vec::new();

        // `B::100` — a second colon skips the primary. It needs an explicit
        // source, so `Q:` and `Q::` stay the errors they were rather than
        // quietly becoming "the entire collection".
        if source.is_some() && self.eat(Kind::Colon) {
            references.push(Reference {
                source: source.clone(),
                primary: None,
                ranges: self.selector()?,
            });
            if !self.eat(Kind::Comma) {
                return Ok(references);
            }
        }

        loop {
            let primary = Some(self.integer()?);
            let ranges = if self.eat(Kind::Colon) {
                self.selector()?
            } else {
                Vec::new()
            };

            references.push(Reference {
                source: source.clone(),
                primary,
                ranges,
            });

            if !self.eat(Kind::Comma) {
                break;
            }
        }

        Ok(references)
    }

    /// `selector := item (',' item)*`, stopping before the next group.
    fn selector(&mut self) -> Result<Vec<Range>, Error> {
        let mut ranges = vec![self.item()?];
        while self.peek().kind == Kind::Comma && !self.group_follows() {
            self.advance();
            ranges.push(self.item()?);
        }
        Ok(ranges)
    }

    /// Past the comma at the cursor, does `integer ':'` follow? That integer
    /// is the next group's primary, so the selector ends here.
    fn group_follows(&self) -> bool {
        self.peek_at(1).kind == Kind::Integer && self.peek_at(2).kind == Kind::Colon
    }

    /// `item := integer | integer '-' integer`
    fn item(&mut self) -> Result<Range, Error> {
        let position = self.peek().position;
        let from = self.integer()?;

        let to = if self.eat(Kind::Dash) {
            self.integer()?
        } else {
            from
        };

        if from > to {
            return Err(Error::InvalidRange { position });
        }

        Ok(Range { from, to })
    }

    /// Overflow is a syntax error, never a panic or a wrapping cast.
    fn integer(&mut self) -> Result<u32, Error> {
        let token = self.peek();
        if token.kind != Kind::Integer {
            return Err(Error::ExpectedNumber {
                position: token.position,
            });
        }
        self.advance();
        token
            .text
            .parse::<u32>()
            .map_err(|_| Error::ExpectedNumber {
                position: token.position,
            })
    }
}
