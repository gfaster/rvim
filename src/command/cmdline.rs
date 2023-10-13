use std::fmt::Write;
use crate::debug::log;
use crate::prelude::*;

use crate::{screen_write, term, window::TextBox};

use super::{parser, Command};

pub enum CommandLineInput {
    Append(char),
    Delete,
}

pub enum CommandLineMode {
    Input,
    Output,
}

pub enum CommandType {
    Ex,
    Find,
    None,
}

pub struct CommandLine {
    mode: CommandLineMode,
    ctx: BufCtx,
    buf: Buffer,
    typ: CommandType,
    output: Option<TextBox>,
}

impl CommandLine {
    pub fn render(&self) {
        match self.mode {
            CommandLineMode::Input => {
                let (w, h) = terminal_size::terminal_size().unwrap();
                term::goto(term::TermPos {
                    x: 0,
                    y: h.0 as u32 - 1,
                });
                let lead = match self.typ {
                    CommandType::Ex => ':',
                    CommandType::None => ' ',
                    CommandType::Find => '/',
                };
                screen_write!(
                    "\x1b[0m{lead}{: <w$}",
                    self.buf,
                    w = w.0 as usize - 1
                );
                term::goto(term::TermPos {
                    x: self.buf.len() as u32 + 1,
                    y: h.0 as u32 - 1,
                });
            }
            CommandLineMode::Output => {
                let Some(ref text) = self.output else { return };
                text.draw();
            }
        }
    }

    pub fn input(&mut self, input: CommandLineInput) {
        self.mode = CommandLineMode::Input;
        self.output = None;
        match input {
            CommandLineInput::Append(c) => {
                self.buf.push(&mut self.ctx, c);
            }
            CommandLineInput::Delete => {
                self.buf.pop(&mut self.ctx);
            }
        };
        self.render();
        // let (_, h) = terminal_size::terminal_size().unwrap();
        // term::goto(term::TermPos {
        //     x: self.buf.len() as u32 + 1,
        //     y: h.0 as u32 - 1,
        // });
    }

    pub fn set_type(&mut self, typ: CommandType) {
        self.mode = match typ {
            CommandType::Ex => CommandLineMode::Input,
            CommandType::Find => CommandLineMode::Input,
            CommandType::None => CommandLineMode::Output,
        };
        self.typ = typ;
    }

    pub fn complete(&mut self) -> Option<Command> {
        let s = std::mem::take(&mut self.buf);
        let out = parser::parse_command(&s.to_string(), self);
        self.clear_command();
        self.mode = CommandLineMode::Output;
        out
    }

    pub fn clear_all(&mut self) {
        self.clear_command();
        self.output.as_mut().map(|t| t.buf.clear());
        self.output = None;
    }

    pub fn clear_command(&mut self) {
        self.typ = CommandType::None;
        self.buf.clear(&mut self.ctx);
    }

    pub fn new() -> Self {
        Self {
            mode: CommandLineMode::Output,
            buf: Buffer::new(),
            ctx: BufCtx::new_anon(),
            typ: CommandType::None,
            output: None,
        }
    }
}

impl std::fmt::Write for CommandLine {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let (w, h) = terminal_size::terminal_size().unwrap();
        let mut output = self.output.take().unwrap_or_else(|| {
            TextBox::new_withdim(
                term::TermPos {
                    x: 0,
                    y: h.0 as u32 - 1,
                },
                w.0 as u32,
                1,
            )
        });
        output.buf.write_str(s)?;
        let h = output
            .buf
            .lines()
            .count()
            .min((h.0 as usize).saturating_sub(5));
        output.resize(1, h as u32);
        output.clamp_to_screen();
        self.output = Some(output);
        Ok(())
    }
}

impl Default for CommandLine {
    fn default() -> Self {
        Self::new()
    }
}
