use crate::debug::log;
use crate::prelude::*;
use std::fmt::Write;

use crate::render::BufId;
use crate::term::TermPos;
use crate::tui::TextSeverity;
use crate::window::Component;
use crate::{term, window::Window};

use super::{parser, Command};

pub enum CommandLineInput {
    Append(char),
    Delete,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum CommandLineMode {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub output_severity: crate::tui::TextSeverity,
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
    pub fn render(&self, ctx: &Ctx) -> std::fmt::Result {
        match self.mode {
            CommandLineMode::Input => {
                // let lead = match self.typ {
                //     CommandType::Ex => ':',
                //     CommandType::None => ' ',
                //     CommandType::Find => '/',
                // };
                self.window.draw_buf(ctx, &self.input_buf);
                let mut tui = ctx.tui.borrow_mut();
                let (_, h) = tui.dim();
                tui.set_cursorpos(TermPos {
                    x: self.input_buf.len() as u32 + 1,
                    y: h as u32 - 1,
                });
            }
            CommandLineMode::Output => {
                if self.output_buf.len() > 0 {
                    self.window.draw_buf_colored(
                        ctx,
                        &self.output_buf,
                        self.output_severity.color(),
                    );
                } else {
                    self.window.draw_buf(ctx, &self.input_buf);
                }
            }
        }
        Ok(())
    }

    pub fn draw_cursor(&self, tui: &mut TermGrid) {
        if self.mode == CommandLineMode::Input {
            tui.set_cursorpos(self.window.cursorpos());
        }
    }

    pub fn input(&mut self, input: CommandLineInput) {
        self.set_mode(CommandLineMode::Input);
        match input {
            CommandLineInput::Append(c) => {
                self.input_buf.push(&mut self.window.buf_ctx, c);
            }
            CommandLineInput::Delete => {
                self.input_buf.pop(&mut self.window.buf_ctx);
            }
        };
    }

    fn set_mode(&mut self, mode: CommandLineMode) {
        if mode != self.mode {
            std::mem::swap(&mut self.other_ctx, &mut self.window.buf_ctx);
            self.mode = mode;
            if mode == CommandLineMode::Input {
                self.output_buf.clear(out_ctx!(self));
                self.output_severity = TextSeverity::Normal;
            }
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

    pub fn get_type(&self) -> CommandType {
        self.typ
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
        self.output_buf.clear(out_ctx!(self));
        self.output_severity = TextSeverity::Normal;
    }

    pub fn clear_command(&mut self) {
        self.typ = CommandType::None;
        self.input_buf.clear(in_ctx!(self));
    }

    pub fn new(tui: &TermGrid) -> Self {
        let (w, h) = tui.dim();
        let components = vec![
            Component::StatusLine(crate::window::StatusLine),
            Component::CommandPrefix(crate::window::CommandPrefix),
        ];
        Self {
            mode: CommandLineMode::Output,
            input_buf: Buffer::new(),
            other_ctx: BufCtx::new_anon(),
            typ: CommandType::None,
            output_buf: Buffer::new(),
            window: Window::new_withdim(
                BufId::new_anon(),
                TermPos { x: 0, y: h - 2 },
                w,
                2,
                components,
            ),
            output_severity: Default::default(),
        }
    }

    /// resize to fit window
    pub fn resize(&mut self, tui: &TermGrid) {
        self.window.snap_to_bottom(tui);
    }
}

impl std::fmt::Write for CommandLine {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.set_mode(CommandLineMode::Output);
        self.output_buf.insert_str(out_ctx!(self), s);
        Ok(())
    }
}
