// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Recursive-descent parser.
//!
//! Knows the grammar and nothing else. It has no table of Surah counts and no
//! match on source codes — `Q:500:999` and `XYZ:1:2` both parse cleanly and
//! are rejected later, by the resolver and the registry respectively.

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
            references.push(self.reference()?);

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

    /// `reference := source ':' primary (':' selector)?`
    fn reference(&mut self) -> Result<Reference, Error> {
        let token = self.peek();
        if token.kind != Kind::Ident {
            return Err(Error::ExpectedSource {
                position: token.position,
            });
        }
        let source = self.advance().text.to_ascii_uppercase();

        let token = self.peek();
        if !self.eat(Kind::Colon) {
            return Err(Error::ExpectedColon {
                position: token.position,
            });
        }

        let primary = self.integer()?;

        let ranges = if self.eat(Kind::Colon) {
            self.selector()?
        } else {
            Vec::new()
        };

        Ok(Reference {
            source,
            primary,
            ranges,
        })
    }

    /// `selector := item (',' item)*`
    fn selector(&mut self) -> Result<Vec<Range>, Error> {
        let mut ranges = vec![self.item()?];
        while self.eat(Kind::Comma) {
            ranges.push(self.item()?);
        }
        Ok(ranges)
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
