use crate::command::cmdline::CommandLine;
use crate::command::cmdline::CommandLineInput;
use crate::debug::log;
use crate::input::Action;
use crate::input::Operation;
use crate::textobj::Motion;

use crate::term;
use crate::tui::TermBox;
use crate::tui::TermGrid;
use crate::tui::TextSeverity;
use crate::window::*;
use crate::Color;
use crate::{buffer::*, Mode};

use nix::sys::termios;
use nix::sys::termios::{LocalFlags, Termios};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Display;
use std::fmt::Write;
use std::ops::Range;
use std::os::unix::io::RawFd;
use std::path::Path;
use std::sync::Arc;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BufId {
    id: u64,
}

impl BufId {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn new() -> Self {
        static ANON_ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = ANON_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        BufId { id }
    }
}

pub struct Ctx {
    id_counter: usize,
    first_buffer: Arc<Buffer>,
    last_buffer: Arc<Buffer>,
    termios: Termios,
    orig_termios: Termios,
    command_line: CommandLine,
    focused_buf: Arc<Buffer>,
    focused_win: Arc<Window>,
    root: crate::window::org::Node,
    pub tui: RefCell<TermGrid>,
    pub term_fd: RawFd,
    pub mode: Mode,
}

fn get_termsize() -> (u32, u32) {
    terminal_size::terminal_size().map_or((80, 40), |(w, h)| (w.0 as u32, h.0 as u32))
}

#[cfg(test)]
impl Ctx {
    pub fn new_testing(buf: Arc<Buffer>) -> Self {
        let term = libc::STDIN_FILENO;
        let termios = termios::tcgetattr(term).unwrap();
        let tui = TermGrid::new();
        let window = Window::new(tui.bounds(), Arc::clone(&buf));
        Self {
            id_counter: 2,
            first_buffer: Arc::clone(&buf),
            last_buffer: Arc::clone(&buf),
            termios: termios.clone(),
            orig_termios: termios,
            term_fd: term,
            command_line: CommandLine::new(&tui),
            tui: tui.into(),
            mode: Mode::Normal,
            focused_buf: buf,
            focused_win: Arc::clone(&window),
            root: window.into(),
        }
    }
}

impl Ctx {
    pub fn from_file(term: RawFd, file: &Path) -> std::io::Result<Self> {
        let buf = Buffer::open(file)?;
        Ok(Self::from_buffer(term, buf))
    }

    pub fn from_buffer(term: RawFd, buf: Arc<Buffer>) -> Self {
        term::altbuf_enable();
        term::flush();
        let mut termios = termios::tcgetattr(term).unwrap();
        let orig = termios.clone();
        termios::cfmakeraw(&mut termios);
        termios.local_flags.remove(LocalFlags::ECHO);
        termios.local_flags.insert(LocalFlags::ISIG);
        termios::tcsetattr(term, termios::SetArg::TCSANOW, &termios).unwrap();
        let tui = TermGrid::new();
        let components = vec![crate::window::Component::RelLineNumbers];
        let window = Window::new_withdim(
            term::TermPos { x: 0, y: 0 },
            tui.dim().0,
            tui.dim().1 - 2,
            components,
            Arc::clone(&buf),
        );
        Self {
            id_counter: 2,
            first_buffer: Arc::clone(&buf),
            last_buffer: Arc::clone(&buf),
            termios,
            orig_termios: orig,
            term_fd: term,
            mode: Mode::Normal,
            command_line: CommandLine::new(&tui),
            tui: tui.into(),
            focused_win: Arc::clone(&window),
            focused_buf: buf,
            root: window.into(),
        }
    }

    pub fn cmdtype(&self) -> crate::command::cmdline::CommandType {
        self.command_line.get_type()
    }

    pub fn render(&mut self) {
        {
            let tui = self.tui.get_mut();
            if tui.resize_auto() {
                self.command_line.reset_visual(tui);
                self.root.fit(tui.bounds());
            }
        }
        self.command_line.take_general_input(&self.tui.get_mut());
        let _ = self.command_line.render(self);
        self.root.draw(self);

        match self.mode {
            Mode::Normal | Mode::Insert => {
                let tui = self.tui.get_mut();
                self.focused_win.get().draw_cursor(tui);
            }
            Mode::Command => {
                let tui = self.tui.get_mut();
                self.command_line.draw_cursor(tui)
            }
        }

        let mut stdout = std::io::stdout().lock();
        self.tui.get_mut().render(&mut stdout).unwrap();
    }

    pub fn focused_buf(&self) -> RwLockReadGuard<BufferInner> {
        self.focused_buf.get()
    }

    pub fn open_buffer(&mut self, buf: Arc<Buffer>) {
        self.id_counter += 1;
        if std::ptr::eq(&*self.first_buffer, &*self.last_buffer) {
            self.first_buffer = Arc::clone(&buf);
        }
        self.last_buffer = Arc::clone(&buf);
        self.focused_buf = Arc::clone(&buf);
        self.focused_win.get_mut().buffer = buf;
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

    fn apply_motion(&mut self, motion: Motion) -> Option<Range<usize>> {
        let start = self.focused_buf().cursor.pos;
        match motion {
            Motion::ScreenSpace { dy, dx } => {
                self.focused_win.get_mut().move_cursor(dx, dy);
            }
            Motion::BufferSpace { doff: _ } => todo!(),
            Motion::TextObj(_) => panic!("text objects cannot be move targets"),
            Motion::TextMotion(m) => {
                let buf = self.focused_buf.get();
                let newoff = m(&buf, buf.coff())?;
                let pos = buf.offset_to_pos(newoff);
                drop(buf);
                self.focused_win.get_mut().set_pos(pos);
            }
        }
        let buf = self.focused_buf.get_mut();
        let end = buf.cursor.pos;
        let (start, end) = {
            let mut start = start;
            let mut end = end;
            if start > end {
                std::mem::swap(&mut start, &mut end)
            }
            (start, end)
        };
        Some(buf.pos_to_offset(start)..buf.pos_to_offset(end))
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
                    let buf = self.focused_buf();
                    let pos = buf.coff();
                    r(&buf, pos)
                }
                _ => self.apply_motion(m),
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
                        self.command_line.reset_visual(self.tui.get_mut());
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
                        self.focused_buf.get_mut().delete_range(range);
                        self.set_mode(Mode::Insert);
                    }
                }
                Operation::Delete => {
                    let range = motion_range.expect("delete requires motion");
                    if let Some(range) = range {
                        self.focused_buf.get_mut().delete_range(range);
                    }
                }
                Operation::Insert(c) => {
                    let mut buf = self.focused_buf.get_mut();
                    buf.insert_str(c.replace('\r', "\n").as_str());
                    self.focused_win.get().fit_ctx_frame(&mut buf.cursor);
                    if let Some(pos) = c.bytes().rev().position(|b| b == b'\r') {
                        buf.cursor.virtcol = pos
                    }
                }
                Operation::DeleteBefore => {
                    self.focused_buf.get_mut().delete_char_before();
                }
                Operation::DeleteAfter => {
                    let mut buf = self.focused_buf.get_mut();
                    buf.delete_char();
                    if buf.cursor.pos.x != 0 {
                        drop(buf);
                        self.focused_win.get_mut().move_cursor(1, 0);
                    }
                }
                Operation::SwitchMode(m) => self.set_mode(m),
                Operation::None => (),
                Operation::Replace(_) => todo!(),
                Operation::Debug => {
                    write!(self.warning(), "not yet implemented").unwrap();
                }
                Operation::RecenterView => self
                    .focused_win.get_mut()
                    .center_view(&mut self.focused_buf.get_mut().cursor),
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

/// draw text in a region
pub fn draw_text(ctx: &Ctx, region: TermBox, content: impl Display, color: Color) {
    let mut tui = ctx.tui.borrow_mut();
    let s = content.to_string();

    for (y, line) in s.lines()
        .chain(std::iter::repeat(""))
        .take(region.ylen() as usize)
        .enumerate()
        {
            tui.write_line(
                y as u32 + region.start.y,
                region.xrng(),
                color,
                line,
            );
        }
}
