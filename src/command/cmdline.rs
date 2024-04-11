use crate::buffer::{Buffer, BufferInner};
use crate::debug::log;
use crate::{guile, prelude::*};
use std::fmt::Write;
use std::sync::{mpsc, Arc, OnceLock};

use crate::render::BufId;
use crate::term::TermPos;
use crate::tui::{TermBox, TextSeverity};
use crate::window::{Component, Window};
use crate::{term, window::WindowInner};

use super::{parser, Command};

pub static CMD_TX: OnceLock<mpsc::Sender<CmdMsg>> = OnceLock::new();

pub enum CommandLineInput {
    Append(char),
    Delete,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
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
    buf: Arc<Buffer>,
    typ: CommandType,
    other_ctx: Cursor,
    window: Arc<Window>,
    msg_rx: mpsc::Receiver<CmdMsg>,
    pub output_severity: crate::tui::TextSeverity,
}

impl CommandLine {
    pub fn take_general_input(&mut self, tui: &TermGrid) {
        while let Ok(msg) = self.msg_rx.try_recv() {
            self.set_mode(CommandLineMode::Output);
            let s: &str = match &msg {
                CmdMsg::Str(s) => s,
                CmdMsg::Gmsg(s) => s,
            };
            // log!("{s:?}");
            self.output_severity = crate::tui::TextSeverity::Normal;
            let mut buf = self.buf.get_mut();
            buf.insert_str(s);
        }

        // move this to rendering
        let buf = self.buf.get();
        if buf.linecnt() > 1 {
            let (w, h) = tui.dim();
            let lncnt = buf.linecnt() as u32;
            let top = h - (lncnt + 1).min(h - 1);
            let mut win = self.window.get_mut();
            win.set_bounds_outer(TermBox::from_ranges(0..w, top..h));
        }
    }

    pub fn render(&self, ctx: &Ctx) -> std::fmt::Result {
        let window = self.window.get();
        let buf = self.buf.get();
        match self.mode {
            CommandLineMode::Input => {
                // let lead = match self.typ {
                //     CommandType::Ex => ':',
                //     CommandType::None => ' ',
                //     CommandType::Find => '/',
                // };
                window.draw(ctx);
                let mut tui = ctx.tui.borrow_mut();
                let (_, h) = tui.dim();
                tui.set_cursorpos(TermPos {
                    x: buf.len() as u32 + 1,
                    y: h as u32 - 1,
                });
            }
            CommandLineMode::Output => {
                if buf.len() > 0 {
                    window.draw_colored(
                        ctx,
                        self.output_severity.color(),
                    );
                } else {
                    window.draw(ctx);
                }
            }
        }
        Ok(())
    }

    pub fn draw_cursor(&self, tui: &mut TermGrid) {
        if self.mode == CommandLineMode::Input {
            self.buf.get().cursor.draw(&self.window.get(), tui)
        }
    }

    pub fn input(&mut self, input: CommandLineInput) {
        self.set_mode(CommandLineMode::Input);
        match input {
            CommandLineInput::Append(c) => {
                self.buf.get_mut().push(c);
            }
            CommandLineInput::Delete => {
                self.buf.get_mut().pop();
            }
        };
    }

    fn set_mode(&mut self, mode: CommandLineMode) {
        let prev = self.mode;
        self.mode = mode;
        if mode == CommandLineMode::Input && prev != mode {
            self.buf.get_mut().clear();
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
        assert_eq!(self.mode, CommandLineMode::Input);
        let s = self.buf.get().to_string();
        let out = parser::parse_command(&s, self);
        let mut buf = self.buf.get_mut();
        self.typ = CommandType::None;
        buf.clear();
        self.mode = CommandLineMode::Output;
        out
    }

    pub fn clear_all(&mut self) {
        self.clear_command();
        self.output_severity = TextSeverity::Normal;
    }

    pub fn clear_command(&mut self) {
        self.typ = CommandType::None;
        self.buf.get_mut().clear();
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
        let buf = Buffer::new();
        Self {
            mode: CommandLineMode::Output,
            other_ctx: Cursor::new(),
            typ: CommandType::None,
            window: Window::new_withdim(TermPos { x: 0, y: h - 2 }, w, 2, components, Arc::clone(&buf)),
            buf,
            output_severity: Default::default(),
            msg_rx: rx,
        }
    }

    /// resize to fit window and reset to original size
    pub fn reset_visual(&mut self, tui: &TermGrid) {
        let (w, h) = tui.dim();
        let mut win = self.window.get_mut();
        win.set_bounds_outer(TermBox::from_ranges(0..w, (h-2)..h));
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
        self.buf.get_mut().insert_str(s);
        Ok(())
    }
}
