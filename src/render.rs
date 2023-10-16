use crate::command::cmdline::CommandLine;
use crate::command::cmdline::CommandLineInput;
use crate::debug::log;
use crate::input::Action;
use crate::input::Operation;
use crate::textobj::Motion;

use crate::term;
use crate::tui::TermGrid;
use crate::tui::TextSeverity;
use crate::window::*;
use crate::{buffer::*, Mode};

use nix::sys::termios;
use nix::sys::termios::{LocalFlags, Termios};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::ops::Range;
use std::os::unix::io::RawFd;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BufId {
    Normal(usize),
    Anon(usize),
}

#[cfg(test)]
impl BufId {
    pub fn new() -> Self {
        BufId::Normal(1)
    }
}

impl BufId {
    pub fn id(&self) -> usize {
        match self {
            BufId::Normal(id) => *id,
            BufId::Anon(id) => *id,
        }
    }

    pub fn new_anon() -> Self {
        static ANON_ID_COUNTER: std::sync::atomic::AtomicUsize =
            std::sync::atomic::AtomicUsize::new(0);
        Self::Anon(ANON_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

pub struct Ctx {
    id_counter: usize,
    buffers: std::collections::BTreeMap<BufId, Buffer>,
    termios: Termios,
    orig_termios: Termios,
    command_line: CommandLine,
    focused: BufId,
    pub tui: RefCell<TermGrid>,
    pub term_fd: RawFd,
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
        let bufid = BufId::Normal(1);
        let tui = TermGrid::new();
        let window = Window::new(&tui);
        Self {
            id_counter: 2,
            buffers: BTreeMap::from([(bufid, buf)]),
            termios: termios.clone(),
            orig_termios: termios,
            term_fd: term,
            command_line: CommandLine::new(&tui),
            tui: tui.into(),
            mode: Mode::Normal,
            window,
            focused: bufid,
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
        termios.local_flags.insert(LocalFlags::ISIG);
        termios::tcsetattr(term, termios::SetArg::TCSANOW, &termios).unwrap();
        let bufid = BufId::Normal(1);
        let tui = TermGrid::new();
        let components = vec![crate::window::Component::RelLineNumbers(
            crate::window::RelLineNumbers,
        )];
        let window = Window::new_withdim(
            term::TermPos { x: 0, y: 0 },
            tui.dim().0,
            tui.dim().1 - 2,
            components,
        );
        Self {
            id_counter: 2,
            buffers: BTreeMap::from([(bufid, buf)]),
            termios,
            orig_termios: orig,
            term_fd: term,
            mode: Mode::Normal,
            window,
            command_line: CommandLine::new(&tui),
            tui: tui.into(),
            focused: bufid,
        }
    }

    pub fn getbuf_mut(&mut self, buf: BufId) -> Option<&mut Buffer> {
        self.buffers.get_mut(&buf)
    }

    pub fn getbuf(&self, buf: BufId) -> Option<&Buffer> {
        self.buffers.get(&buf)
    }

    pub fn cmdtype(&self) -> crate::command::cmdline::CommandType {
        self.command_line.get_type()
    }

    pub fn render(&mut self) {
        {
            let tui = self.tui.get_mut();
            if tui.resize_auto() {
                self.command_line.resize(tui);
                self.window.set_size_padded(tui.dim().0, tui.dim().1);
            }
        }

        let _ = self.command_line.render(self);
        self.window.draw_buf(self, self.focused_buf());

        match self.mode {
            Mode::Normal | Mode::Insert => {
                let tui = self.tui.get_mut();
                self.buffers[&self.focused].cursor.draw(&self.window, tui)
            }
            Mode::Command => {
                let tui = self.tui.get_mut();
                self.command_line.draw_cursor(tui)
            }
        }

        let mut stdout = std::io::stdout().lock();
        self.tui.get_mut().render(&mut stdout).unwrap();
    }

    pub fn focused(&self) -> BufId {
        self.focused
    }

    pub fn focused_buf(&self) -> &Buffer {
        &self.buffers[&self.focused()]
    }

    pub fn open_buffer(&mut self, buf: Buffer) {
        let buf_id = BufId::Normal(self.id_counter);
        self.id_counter += 1;
        self.buffers
            .insert(buf_id, buf)
            .map(|_| panic!("Buf insertion tried to reuse an id"));
        self.tui.borrow_mut().clear();
    }

    pub fn err(&mut self, err: &(impl std::error::Error + ?Sized)) {
        self.command_line.output_severity = TextSeverity::Error;
        self.command_line
            .write_fmt(format_args!("Error: {}", err))
            .unwrap();
    }

    /// get a handle for info dialogue
    pub fn info(&mut self) -> &mut impl std::fmt::Write {
        self.command_line.output_severity = TextSeverity::Normal;
        &mut self.command_line
    }

    /// get a handle for warning dialogue
    pub fn warning(&mut self) -> &mut impl std::fmt::Write {
        self.command_line.output_severity = TextSeverity::Warning;
        &mut self.command_line
    }

    pub fn buffers(&self) -> impl Iterator<Item = (usize, &str)> {
        self.buffers
            .iter()
            .filter(|(k, _)| matches!(k, BufId::Normal(_)))
            .map(|(k, v)| (k.id(), v.name()))
    }

    fn apply_motion(&mut self, motion: Motion) -> DocRange {
        let buf = self.buffers.get_mut(&self.focused).unwrap();
        let start = buf.cursor.pos;
        match motion {
            Motion::ScreenSpace { dy, dx } => {
                self.window.move_cursor(buf, dx, dy);
            }
            Motion::BufferSpace { doff: _ } => todo!(),
            Motion::TextObj(_) => panic!("text objects cannot be move targets"),
            Motion::TextMotion(m) => {
                if let Some(newpos) = m(buf, buf.cursor.pos) {
                    self.window.set_pos(buf, newpos);
                }
            }
        }
        let end = buf.cursor.pos;
        let (start, end) = {
            let mut start = start;
            let mut end = end;
            if start > end {
                std::mem::swap(&mut start, &mut end)
            }
            (start, end)
        };

        // TODO: this is not always correct
        DocRange {
            start_inclusive: true,
            start,
            end,
            end_inclusive: true,
        }
    }

    fn set_mode(&mut self, mode: Mode) {
        if mode == Mode::Command {
            self.command_line
                .set_type(crate::command::cmdline::CommandType::Ex)
        }
        self.mode = mode;
    }

    pub fn process_action(&mut self, action: Action) {
        let motion_range = if let Some(m) = action.motion {
            Some(match m {
                Motion::TextObj(r) => {
                    let buf = self.buffers.get(&self.focused).unwrap();
                    let pos = buf.cursor.pos;
                    r(buf, pos)
                }
                _ => Some(self.apply_motion(m)),
            })
        } else {
            None
        };
        match self.mode {
            Mode::Command => match action.operation {
                Operation::Insert(s) => {
                    let c = s.chars().next().unwrap();
                    if c == '\r' {
                        self.command_line
                            .complete()
                            .map(|x| x.exec(self))
                            .map(|r| r.map_err(|e| self.err(&*e)));
                        self.mode = Mode::Normal;
                    } else {
                        let _ = self.command_line.input(CommandLineInput::Append(c));
                    }
                }
                Operation::DeleteBefore => {
                    let _ = self.command_line.input(CommandLineInput::Delete);
                }
                Operation::DeleteAfter => {
                    panic!("only backspace is implemented for command line")
                    // self.command_line.input(CommandLineInput::Delete)
                }
                Operation::SwitchMode(m) => {
                    if m != Mode::Command {
                        self.command_line.clear_command();
                    }
                    self.mode = m
                }
                Operation::Debug => todo!(),
                Operation::None => (),
                _ => unreachable!(),
            },
            _ => match action.operation {
                Operation::Change => {
                    let range = motion_range.expect("change requires motion");
                    if let Some(range) = range {
                        let buf = self.buffers.get_mut(&self.focused()).unwrap();
                        buf.delete_range(range);
                        self.set_mode(Mode::Insert);
                    }
                }
                Operation::Delete => {
                    let range = motion_range.expect("delete requires motion");
                    if let Some(range) = range {
                        let buf = self.buffers.get_mut(&self.focused()).unwrap();
                        buf.delete_range(range);
                    }
                }
                Operation::Insert(c) => {
                    let buf = self.buffers.get_mut(&self.focused()).unwrap();
                    buf.insert_str(c.replace('\r', "\n").as_str());
                    self.window.fit_ctx_frame(&mut buf.cursor);
                }
                Operation::DeleteBefore => {
                    let buf = self.buffers.get_mut(&self.focused()).unwrap();
                    buf.delete_char_before();
                }
                Operation::DeleteAfter => {
                    let buf = self.buffers.get_mut(&self.focused()).unwrap();
                    buf.delete_char();
                    if buf.cursor.pos.x != 0 {
                        self.window.move_cursor(buf, 1, 0);
                    }
                }
                Operation::SwitchMode(m) => self.set_mode(m),
                Operation::None => (),
                Operation::Replace(_) => todo!(),
                Operation::Debug => {
                    write!(self.warning(), "not yet implemented").unwrap();
                }
                Operation::RecenterView => self
                    .window
                    .center_view(&mut self.buffers.get_mut(&self.focused()).unwrap().cursor),
            },
        };
        if let Some(m) = action.post_motion {
            self.apply_motion(m);
        };
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        termios::tcsetattr(self.term_fd, termios::SetArg::TCSANOW, &self.orig_termios)
            .unwrap_or(());
    }
}
