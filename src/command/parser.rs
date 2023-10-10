use std::ops::Range;
use lazy_regex::regex;

use crate::{prelude::*, debug::log};

use super::Command;

struct Lexer<'a> {
    input: &'a str,
    idx: usize
}

struct Token<'a> {
    data: &'a str,
    span: Range<usize>,
    kind: TokenKind,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum TokenKind {
    Number,
    Ident,
    Path,
}

impl<'a> Lexer<'a> {
    fn new(s: &'a str) -> Self {
        Lexer { input: s, idx: 0 }
    }

    fn next(&mut self) -> Option<Token<'a>> {
        for kind in [TokenKind::Ident, TokenKind::Number] {
            if let Some(tok) = self.try_next_expect(kind) {
                return Some(tok)
            }
        }
        return None
    }

    fn try_next_expect(&mut self, kind: TokenKind) -> Option<Token<'a>> {
        if self.idx >= self.input.len() {
            return None;
        }
        for (i, c) in self.input[self.idx..].char_indices() {
            if !c.is_whitespace() {
                self.idx += i;
            }
        }
        let res = kind.regex().find(&self.input[self.idx..])?;
        let span = self.idx..(self.idx + res.range().end);
        self.idx += res.range().end;

        Some(Token {
            data: res.as_str(),
            span,
            kind
        })
    }

    fn next_expect(&mut self, kind: TokenKind) -> Option<Token<'a>> {
        let Some(tok) = self.try_next_expect(kind) else {
            log!("Expected {kind}");
            return None
        };
        Some(tok)
    }

    fn remainder(&self) -> &str {
        &self.input[self.idx..]
    }

    fn next_expects(&mut self, kinds: &[TokenKind]) -> Option<Token<'a>> {
        let Some(next) = self.next() else {
            log!("Expected {} but found EOL", TokenKindList(kinds));
            return None
        };
        if kinds.contains(&next.kind) {
            return Some(next)
        } else {
            log!("Expected {} but found {}", TokenKindList(kinds), next.kind);
            return None
        }
    }
}

struct TokenKindList<'a>(&'a [TokenKind]);

impl std::fmt::Display for TokenKindList<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, k) in self.0.iter().enumerate() {
            if i == 0 {
                write!(f, "{k}")?;
            } else if i == self.0.len() - 1 {
                write!(f, ", or {k}")?
            } else {
                write!(f, ", {k}")?
            }
        }
        Ok(())
    }
}

impl TokenKind {
    fn regex(&self) -> &lazy_regex::Regex {
        match self {
            TokenKind::Number => regex!(r#"^[1-9][\d]*"#),
            TokenKind::Ident => regex!(r#"^[[:alpha:]][[:alpha:]0-9]*"#),
            TokenKind::Path => regex!(r#"^(?:[^ !$`&*()+]|(?:\\[ !$`&*()+]))+"#),
        }
    }
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TokenKind::Number => "`number`",
            TokenKind::Ident => "`identifier`",
            TokenKind::Path => "`path`",
        };
        f.write_str(s)
    }
}

pub fn parse_command(s: &str) -> Option<Command> {
    let mut args = Lexer::new(s);
    let res = match args.next_expects(&[TokenKind::Ident])?.data {
        "w" | "write" => Command::Write {
            path: args.next_expect(TokenKind::Path).map(|p| p.data.into()),
        },
        "q" | "quit" => Command::Quit,
        "e" | "edit" => Command::Edit {
            path: args.try_next_expect(TokenKind::Path)?.data.to_string(),
        },
        _ => return None,
    };
    Some(res)
}
