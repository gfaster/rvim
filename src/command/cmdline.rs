use crate::buffer::Buffer;
use crate::debug::log;
use crate::{guile, prelude::*};
use std::fmt::Write;
use std::sync::{mpsc, OnceLock};

use crate::render::BufId;
use crate::term::TermPos;
use crate::tui::TextSeverity;
use crate::window::Component;
use crate::{term, window::Window};

use super::{parser, Command};

pub static CMD_TX: OnceLock<mpsc::Sender<CmdMsg>> = OnceLock::new();

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

pub enum CmdMsg {
    Str(String),
    Gmsg(guile::Gmsg)
}

pub struct CommandLine {
    mode: CommandLineMode,
    input_buf: Buffer,
    typ: CommandType,
    other_ctx: Cursor,
    window: Window,
    output_buf: Buffer,
    msg_rx: mpsc::Receiver<CmdMsg>,
    pub output_severity: crate::tui::TextSeverity,
}

impl CommandLine {
    pub fn take_general_input(&mut self) {
        if let Ok(msg) = self.msg_rx.try_recv() {
            self.set_mode(CommandLineMode::Output);
            let s: &str = match &msg {
                CmdMsg::Str(s) => s,
                CmdMsg::Gmsg(s) => s,
            };
            self.output_severity = crate::tui::TextSeverity::Normal;
            self.output_buf.insert_str(s);
        }
    }

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
            self.input_buf.cursor.draw(&self.window, tui)
        }
    }

    pub fn input(&mut self, input: CommandLineInput) {
        self.set_mode(CommandLineMode::Input);
        match input {
            CommandLineInput::Append(c) => {
                self.input_buf.push(c);
            }
            CommandLineInput::Delete => {
                self.input_buf.pop();
            }
        };
    }

    fn set_mode(&mut self, mode: CommandLineMode) {
        self.mode = mode;
        if mode == CommandLineMode::Input {
            self.output_buf.clear();
            self.output_severity = TextSeverity::Normal;
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
        let out = parser::parse_command(&self.input_buf.to_string(), self);
        self.input_buf.clear();
        self.clear_command();
        self.mode = CommandLineMode::Output;
        out
    }

    pub fn clear_all(&mut self) {
        self.clear_command();
        self.output_buf.clear();
        self.output_severity = TextSeverity::Normal;
    }

    pub fn clear_command(&mut self) {
        self.typ = CommandType::None;
        self.input_buf.clear();
    }

    /// initialize command line - can only be done once in program execution
    pub fn new(tui: &TermGrid) -> Self {
        let (w, h) = tui.dim();
        let components = vec![
            Component::StatusLine,
            Component::CommandPrefix,
        ];
        let (tx, rx) = mpsc::channel();
        CMD_TX.set(tx).expect("Command line was initialized multiple times");
        Self {
            mode: CommandLineMode::Output,
            input_buf: Buffer::new(),
            other_ctx: Cursor::new(),
            typ: CommandType::None,
            output_buf: Buffer::new(),
            window: Window::new_withdim(TermPos { x: 0, y: h - 2 }, w, 2, components),
            output_severity: Default::default(),
            msg_rx: rx,
        }
    }

    /// resize to fit window
    pub fn resize(&mut self, tui: &TermGrid) {
        self.window.snap_to_bottom(tui);
    }

    /// sends a message to the command line output, and will be displayed on next render call. This
    /// function will never panic, since it's meant to be used for guile code.
    pub fn send_msg(s: CmdMsg) -> Result<(), ()> {
        let tx = CMD_TX.get().ok_or(())?;
        tx.send(s).map_err(|_| ())
    }
}

impl std::fmt::Write for CommandLine {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.set_mode(CommandLineMode::Output);
        self.output_buf.insert_str(s);
        Ok(())
    }
}
