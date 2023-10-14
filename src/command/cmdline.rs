use std::fmt::Write;
use crate::debug::log;
use crate::prelude::*;

use crate::render::BufId;
use crate::term::TermPos;
use crate::window::Component;
use crate::{screen_write, term, window::Window};

use super::{parser, Command};

pub enum CommandLineInput {
    Append(char),
    Delete,
}

#[derive(PartialEq, Eq)]
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
    input_buf: Buffer,
    typ: CommandType,
    other_ctx: BufCtx,
    window: Window,
    output_buf: Buffer,
}

macro_rules! out_ctx {
    ($cmd:expr) => {
        if $cmd.mode == CommandLineMode::Input {
            &mut $cmd.other_ctx
        } else {
            &mut $cmd.window.buf_ctx
        }
    };
}

macro_rules! in_ctx {
    ($cmd:expr) => {
        if $cmd.mode == CommandLineMode::Output {
            &mut $cmd.other_ctx
        } else {
            &mut $cmd.window.buf_ctx
        }
    };
}

impl CommandLine {
    pub fn render(&self, tui: &mut TermGrid) -> std::fmt::Result {
        match self.mode {
            CommandLineMode::Input => {
                let lead = match self.typ {
                    CommandType::Ex => ':',
                    CommandType::None => ' ',
                    CommandType::Find => '/',
                };
                let (_, h) = tui.dim();
                write!(
                    tui.refbox(tui.line_bounds(h - 1)),
                    "{lead}{}",
                    self.input_buf
                )?;
                tui.set_cursorpos(TermPos {x: self.input_buf.len() as u32 - 1, y: h as u32 - 1});
            }
            CommandLineMode::Output => {
                if self.output_buf.len() > 0 {
                    let (_, h) = tui.dim();
                    write!(
                        tui.refbox(tui.line_bounds(h - 1)),
                        "{}",
                        self.output_buf
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn input(&mut self, tui: &mut TermGrid, input: CommandLineInput) -> std::fmt::Result {
        self.set_mode(CommandLineMode::Input);
        match input {
            CommandLineInput::Append(c) => {
                self.input_buf.push(&mut self.window.buf_ctx, c);
            }
            CommandLineInput::Delete => {
                self.input_buf.pop(&mut self.window.buf_ctx);
            }
        };
        self.render(tui)
    }

    fn set_mode(&mut self, mode: CommandLineMode) {
        if mode != self.mode {
            std::mem::swap(&mut self.other_ctx, &mut self.window.buf_ctx);
        }
    }

    pub fn set_type(&mut self, typ: CommandType) {
        self.set_mode(match typ {
            CommandType::Ex => CommandLineMode::Input,
            CommandType::Find => CommandLineMode::Input,
            CommandType::None => CommandLineMode::Output,
        });
        self.typ = typ;
    }

    pub fn complete(&mut self) -> Option<Command> {
        let s = std::mem::take(&mut self.input_buf);
        let out = parser::parse_command(&s.to_string(), self);
        self.clear_command();
        self.mode = CommandLineMode::Output;
        out
    }

    pub fn clear_all(&mut self) {
        self.clear_command();
    }

    pub fn clear_command(&mut self) {
        self.typ = CommandType::None;
        self.input_buf.clear(in_ctx!(self));
    }

    pub fn new(tui: &TermGrid) -> Self {
        let (w, h) = tui.dim();
        let components = vec![
            // Component::StatusLine(crate::window::StatusLine)
        ];
        Self {
            mode: CommandLineMode::Output,
            input_buf: Buffer::new(),
            other_ctx: BufCtx::new_anon(),
            typ: CommandType::None,
            output_buf: Buffer::new(),
            window: Window::new_withdim(BufId::new_anon(), TermPos { x: 0, y: h - 1 }, w, 1, components),
        }
    }
}

impl std::fmt::Write for CommandLine {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.output_buf.insert_str(out_ctx!(self), s);
        Ok(())
    }
}
