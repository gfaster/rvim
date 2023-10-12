use crate::command::cmdline::CommandLine;
use crate::command::cmdline::CommandLineInput;
use crate::debug::log;
use crate::input::Action;
use crate::screen_write;
use crate::textobj::Motion;

use crate::term;
use crate::window::*;
use crate::{buffer::*, Mode};

use nix::sys::termios;
use nix::sys::termios::{LocalFlags, Termios};
use std::collections::BTreeMap;
use std::os::unix::io::RawFd;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BufId(usize);

#[cfg(test)]
impl BufId {
    pub fn new() -> Self {
        BufId(1)
    }
}

impl BufId {
    pub fn id(&self) -> usize {
        self.0
    }
}

pub struct Ctx {
    id_counter: usize,
    buffers: std::collections::BTreeMap<BufId, Buffer>,
    termios: Termios,
    orig_termios: Termios,
    command_line: CommandLine,
    /// terminal size (w, h)
    termsize: (u32, u32),
    pub term: RawFd,
    pub window: Window,
    pub mode: Mode,
}

fn get_termsize() -> (u32, u32) {
    terminal_size::terminal_size().map_or((80, 40), |(w, h)| (w.0 as u32, h.0 as u32))
}

#[cfg(test)]
impl Ctx {
    pub fn new_testing(buf: Buffer) -> Self {
        let term = libc::STDIN_FILENO;
        let termios = termios::tcgetattr(term).unwrap();
        let bufid = BufId(1);
        let window = Window::new(bufid);
        Self {
            id_counter: 2,
            buffers: BTreeMap::from([(bufid, buf)]),
            termios: termios.clone(),
            orig_termios: termios,
            termsize: (80, 40),
            term,
            mode: Mode::Normal,
            window,
            command_line: Default::default(),
        }
    }
}

impl Ctx {
    pub fn from_file(term: RawFd, file: &Path) -> std::io::Result<Self> {
        let buf = Buffer::open(file)?;
        Ok(Self::from_buffer(term, buf))
    }
    pub fn from_buffer(term: RawFd, buf: Buffer) -> Self {
        term::altbuf_enable();
        term::flush();
        let mut termios = termios::tcgetattr(term).unwrap();
        let orig = termios.clone();
        termios::cfmakeraw(&mut termios);
        termios.local_flags.remove(LocalFlags::ECHO);
        // termios.local_flags.insert(LocalFlags::ISIG);
        termios::tcsetattr(term, termios::SetArg::TCSANOW, &termios).unwrap();
        let bufid = BufId(1);
        let window = Window::new(bufid);
        Self {
            id_counter: 2,
            buffers: BTreeMap::from([(bufid, buf)]),
            termios,
            orig_termios: orig,
            term,
            termsize: get_termsize(),
            mode: Mode::Normal,
            window,
            command_line: Default::default(),
        }
    }

    pub fn getbuf_mut(&mut self, buf: BufId) -> Option<&mut Buffer> {
        self.buffers.get_mut(&buf)
    }

    pub fn getbuf(&self, buf: BufId) -> Option<&Buffer> {
        self.buffers.get(&buf)
    }

    pub fn render(&mut self) {
        let currsize = get_termsize();
        if currsize != self.termsize {
            self.window.clear();
            self.window.set_size_padded(currsize.0, currsize.1);
            self.termsize = currsize;
            term::rst_cur();
            // clear the screen
            screen_write!(
                "{:>w$}",
                "",
                w = (self.termsize.0 * self.termsize.1) as usize
            );
        }
        match self.mode {
            Mode::Command => {
                self.window.draw(self);
                self.command_line.render();
                term::flush();
            }
            _ => {
                self.command_line.render();
                self.window.draw(self);
                term::flush();
            }
        }
    }

    pub fn focused(&self) -> BufId {
        self.window.buf_ctx.buf_id
    }

    pub fn focused_buf(&self) -> &Buffer {
        &self.buffers[&self.focused()]
    }

    pub fn open_buffer(&mut self, buf: Buffer) {
        let buf_id = BufId(self.id_counter);
        self.id_counter += 1;
        self.buffers
            .insert(buf_id, buf)
            .map(|_| panic!("Buf insertion tried to reuse an id"));
        self.window.buf_ctx.buf_id = buf_id;
        self.window.clear();
    }

    pub fn diag(&mut self, args: std::fmt::Arguments) {
        self.command_line.write_diag(args)
    }

    pub fn err(&mut self, err: &(impl std::error::Error + ?Sized)) {
        self.command_line.write_diag(format_args!("Error: {}", err))
    }

    fn apply_motion(&mut self, motion: Motion) {
        match motion {
            Motion::ScreenSpace { dy, dx } => {
                // type system here is kinda sneaky, can't use getbuf because all of self is
                // borrowed
                let buf = &self.buffers[&self.focused()];
                self.window.move_cursor(buf, dx, dy)
            }
            Motion::BufferSpace { doff: _ } => todo!(),
            Motion::TextObj(_) => todo!(),
            Motion::TextMotion(m) => {
                let buf = &self.buffers[&self.focused()];
                let buf_ctx = &mut self.window.buf_ctx;
                if let Some(newpos) = m(buf, buf_ctx.cursorpos) {
                    self.window.set_pos(buf, newpos);
                }
            }
        }
    }

    pub fn process_action(&mut self, action: Action) {
        if let Some(m) = action.motion {
            self.apply_motion(m)
        }
        match self.mode {
            Mode::Command => match action.operation {
                crate::input::Operation::Insert(s) => {
                    let c = s.chars().next().unwrap();
                    if c == '\r' {
                        self.command_line
                            .complete()
                            .map(|x| x.exec(self))
                            .map(|r| r.map_err(|e| self.err(&*e)));
                        self.mode = Mode::Normal;
                    } else {
                        self.command_line.input(CommandLineInput::Append(c));
                    }
                }
                crate::input::Operation::Delete => {
                    self.command_line.input(CommandLineInput::Delete)
                }
                crate::input::Operation::SwitchMode(m) => {
                    if !matches!(m, Mode::Command) {
                        self.command_line.clear();
                    }
                    self.mode = m
                }
                crate::input::Operation::Debug => todo!(),
                crate::input::Operation::None => (),
                _ => unreachable!(),
            },
            _ => match action.operation {
                crate::input::Operation::Change => todo!(),
                crate::input::Operation::Insert(c) => {
                    let buf_ctx = &mut self.window.buf_ctx;
                    let buf = self.buffers.get_mut(&buf_ctx.buf_id).unwrap();
                    buf.insert_str(buf_ctx, c.replace('\r', "\n").as_str());
                    self.window.fit_ctx_frame();
                    self.window.draw(self);
                }
                crate::input::Operation::Delete => {
                    let buf_ctx = &mut self.window.buf_ctx;
                    self.buffers
                        .get_mut(&buf_ctx.buf_id)
                        .unwrap()
                        .delete_char(buf_ctx);
                    self.window.draw(self);
                }
                crate::input::Operation::SwitchMode(m) => {
                    if m == Mode::Command {
                        self.command_line
                            .set_type(crate::command::cmdline::CommandType::Ex)
                    }
                    self.mode = m
                }
                crate::input::Operation::None => (),
                crate::input::Operation::Replace(_) => todo!(),
                crate::input::Operation::Debug => {
                    let buf_ctx = self.window.buf_ctx;
                    let id = buf_ctx.buf_id;
                    let buf = &self.buffers[&id];
                    let lines = buf.get_lines(buf_ctx.cursorpos.y..(buf_ctx.cursorpos.y + 1));
                    log!("line: {:?}", lines);
                    log!("len: {:?}", lines.get(0).unwrap_or(&"".into()).len());
                }
            },
        };
        if let Some(m) = action.post_motion {
            self.apply_motion(m)
        }
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        termios::tcsetattr(self.term, termios::SetArg::TCSANOW, &self.orig_termios).unwrap_or(());
    }
}
