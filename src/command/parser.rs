use lazy_regex::regex;
use std::ops::Range;

use crate::{debug::log, prelude::*};

use super::{cmdline::CommandLine, Command};

struct Lexer<'a> {
    input: &'a str,
    idx: usize,
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

    fn try_next_expect(&mut self, kind: TokenKind) -> Result<Token<'a>, Range<usize>> {
        if self.idx >= self.input.len() {
            return Err(self.idx..self.idx);
        }
        for (i, c) in self.input[self.idx..].char_indices() {
            if !c.is_whitespace() {
                self.idx += i;
                break;
            }
        }
        let end = regex!(r#"\s"#)
            .find(&self.input[self.idx..])
            .map_or(self.input.trim_end().len(), |f| f.start() + self.idx);
        let res = kind
            .regex()
            .find(&self.input[self.idx..end])
            .ok_or(self.idx..end)?;
        let span = self.idx..(self.idx + res.range().end);
        self.idx += res.range().end;

        Ok(Token {
            data: res.as_str(),
            span,
            kind,
        })
    }

    fn remainder(&self) -> &str {
        &self.input[self.idx..]
    }

    fn next_expects(&mut self, diag: &mut CommandLine, kinds: &[TokenKind]) -> Option<Token<'a>> {
        for kind in kinds {
            if let Ok(tok) = self.try_next_expect(*kind) {
                return Some(tok);
            }
        }
        for kind in TokenKindList::difference(kinds) {
            if self.try_next_expect(kind).is_ok() {
                diag.write_diag(format_args!(
                    "Expected {} but found {}",
                    TokenKindList(kinds),
                    kind
                ));
                return None;
            }
        }
        diag.write_diag(format_args!(
            "Expected {} but found EOL",
            TokenKindList(kinds)
        ));
        return None;
    }
}

struct TokenKindList<'a>(&'a [TokenKind]);

impl TokenKindList<'_> {
    fn difference(remove: &[TokenKind]) -> impl IntoIterator<Item = TokenKind> + '_ {
        [TokenKind::Ident, TokenKind::Number, TokenKind::Path]
            .into_iter()
            .filter(|k| !remove.contains(k))
    }
}

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

pub fn parse_command(s: &str, diag: &mut CommandLine) -> Option<Command> {
    let mut args = Lexer::new(s);
    let res = match args.next_expects(diag, &[TokenKind::Ident])?.data {
        "w" | "write" => Command::Write {
            path: args
                .next_expects(diag, &[TokenKind::Path])
                .map(|p| p.data.into()),
        },
        "q" | "quit" => Command::Quit,
        "e" | "edit" => Command::Edit {
            path: args
                .next_expects(diag, &[TokenKind::Path])?
                .data
                .to_string(),
        },
        unknown => {
            diag.write_diag(format_args!("Unknown command: {unknown:?}"));
            return None;
        }
    };
    Some(res)
}
