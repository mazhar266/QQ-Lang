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
//! body      := text                      // Q:"..."   whole collection
//!            | ':' selector              // B::100    book-wide numbering
//!            | group (',' group)*
//! group     := primary ':' text          // Q:1:"..."      within a primary
//!            | primary ':' scope ':' text // Q:1:3~5:"..."  within a range
//!            | primary (':' selector)?
//! scope     := integer '~' integer
//! selector  := item (',' item)*
//! item      := integer | integer '-' integer
//! text      := '"' ... '"'
//! ```
//!
//! One rule resolves the only ambiguity: **an integer followed by `:` starts a
//! new group** rather than continuing the current selector. So in
//! `Q:1:2,3,2:3,4-6` the third `2` is a Surah, not an ayah, and the query
//! yields two references. The same rule makes a bare `1,2:255` read as "all of
//! 1, then 2:255".
//!
//! A stated source carries forward to later references in the same query, so
//! `b:1:1;3` is Bukhari twice and `b:1:1;q:3` switches to the Quran. This is
//! still pure syntax — "reuse the previous code" needs no idea what the codes
//! mean. A reference with no code stated anywhere before it stays `None`, and
//! the registry supplies the default.
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
        // The most recent source stated in this query, carried forward.
        let mut inherited: Option<String> = None;

        loop {
            references.extend(self.reference(&mut inherited)?);

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
    ///
    /// `inherited` carries the last stated code forward and is updated when
    /// this reference states one of its own.
    fn reference(&mut self, inherited: &mut Option<String>) -> Result<Vec<Reference>, Error> {
        let token = self.peek();
        // A reference starts with a code, a number, or — for a bare `"text"`
        // search — the term itself.
        if !matches!(token.kind, Kind::Ident | Kind::Integer | Kind::Text) {
            return Err(Error::ExpectedSource {
                position: token.position,
            });
        }

        // A source stated here becomes the one later references inherit. If
        // none was ever stated, this stays `None` — which source that means
        // belongs to the registry, not here.
        let explicit = token.kind == Kind::Ident;
        let source = if explicit {
            let code = self.advance().text.to_ascii_uppercase();
            let token = self.peek();
            if !self.eat(Kind::Colon) {
                return Err(Error::ExpectedColon {
                    position: token.position,
                });
            }
            *inherited = Some(code.clone());
            Some(code)
        } else {
            inherited.clone()
        };

        let mut references = Vec::new();

        // `Q:"text"` — search the whole collection.
        if self.peek().kind == Kind::Text {
            return Ok(vec![self.search(source, None, Vec::new())?]);
        }

        // `B::100` — a second colon skips the primary. Only a source written
        // right here can be followed by that colon, so `Q:` and `Q::` stay the
        // errors they were rather than quietly becoming "the whole collection".
        if explicit && self.eat(Kind::Colon) {
            references.push(Reference {
                source: source.clone(),
                primary: None,
                ranges: self.selector()?,
                text: None,
            });
            if !self.eat(Kind::Comma) {
                return Ok(references);
            }
        }

        loop {
            let primary = self.integer()?;

            let ranges = if self.eat(Kind::Colon) {
                // `Q:1:"text"` — search inside this primary.
                if self.peek().kind == Kind::Text {
                    references.push(self.search(source, Some(primary), Vec::new())?);
                    break;
                }

                // `Q:1:3~5:"text"` — search a range inside it. `~` marks a
                // scope rather than a selection, which is what keeps it apart
                // from the `1-5` of an ordinary selector.
                if self.scope_follows() {
                    let scope = self.scope()?;
                    let token = self.peek();
                    if !self.eat(Kind::Colon) {
                        return Err(Error::ExpectedColon {
                            position: token.position,
                        });
                    }
                    references.push(self.search(source, Some(primary), vec![scope])?);
                    break;
                }

                self.selector()?
            } else {
                Vec::new()
            };

            references.push(Reference {
                source: source.clone(),
                primary: Some(primary),
                ranges,
                text: None,
            });

            if !self.eat(Kind::Comma) {
                break;
            }
        }

        Ok(references)
    }

    /// Consume a quoted term and build the search node.
    fn search(
        &mut self,
        source: Option<String>,
        primary: Option<u32>,
        ranges: Vec<Range>,
    ) -> Result<Reference, Error> {
        let token = self.peek();
        if token.kind != Kind::Text {
            return Err(Error::ExpectedText {
                position: token.position,
            });
        }
        self.advance();

        // An empty needle would match every record, which is never what
        // someone meant to type.
        if token.text.trim().is_empty() {
            return Err(Error::ExpectedText {
                position: token.position,
            });
        }

        Ok(Reference {
            source,
            primary,
            ranges,
            text: Some(token.text.to_string()),
        })
    }

    /// Does `integer '~'` start here?
    fn scope_follows(&self) -> bool {
        self.peek().kind == Kind::Integer && self.peek_at(1).kind == Kind::Tilde
    }

    /// `scope := integer '~' integer`
    fn scope(&mut self) -> Result<Range, Error> {
        let position = self.peek().position;
        let from = self.integer()?;
        self.advance(); // the `~`, already seen by `scope_follows`
        let to = self.integer()?;

        if from > to {
            return Err(Error::InvalidRange { position });
        }
        Ok(Range { from, to })
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
