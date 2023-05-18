use crate::input::Action;
use crate::textobj::Motion;

use crate::term;
use crate::textobj::TextMot;
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

pub struct Ctx
{
    id_counter: usize,
    buffers: std::collections::BTreeMap<BufId, Buffer>,
    termios: Termios,
    orig: Termios,
    pub term: RawFd,
    pub window: Window,
    pub mode: Mode,
}

#[cfg(test)]
impl Ctx
{
    pub fn new_testing(buf: Buffer) -> Self {
        let term = libc::STDIN_FILENO;
        let termios = termios::tcgetattr(term).unwrap();
        let bufid = BufId(1);
        let window = Window::new(bufid);
        Self {
            id_counter: 2,
            buffers: BTreeMap::from([(bufid, buf)]),
            termios: termios.clone(),
            orig: termios,
            term,
            mode: Mode::Normal,
            window,
        }
    }
}

impl Ctx
{
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
        let bufid = BufId(1);
        let window = Window::new(bufid);
        Self {
            id_counter: 2,
            buffers: BTreeMap::from([(bufid, buf)]),
            termios,
            orig,
            term,
            mode: Mode::Normal,
            window,
        }
    }

    pub fn getbuf_mut(&mut self, buf: BufId) -> Option<&mut Buffer> {
        self.buffers.get_mut(&buf)
    }

    pub fn getbuf(&self, buf: BufId) -> Option<&Buffer> {
        self.buffers.get(&buf)
    }

    pub fn render(&self) {
        self.window.draw(self);
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
            .expect("Buf insertion does not reuse ids");
        self.window.buf_ctx.buf_id = buf_id;
    }

    pub fn process_action(&mut self, action: Action) {
        if let Some(m) = action.motion {
            match m {
                Motion::ScreenSpace { dy, dx } => {
                    // type system here is kinda sneaky, can't use getbuf because all of self is
                    // borrowed
                    let buf = &self.buffers[&self.focused()];
                    let buf_ctx = &mut self.window.buf_ctx;
                    buf_ctx.move_cursor(buf, dx, dy)
                }
                Motion::BufferSpace { doff: _ } => todo!(),
                Motion::TextObj(_) => todo!(),
                Motion::TextMotion(m) => {
                    let buf = &self.buffers[&self.focused()];
                    let buf_ctx = &mut self.window.buf_ctx;
                    if let Some(newpos) = m.find_dest(buf, buf_ctx.cursorpos) {
                        buf_ctx.set_pos(buf, newpos);
                    }
                }
            }
        }

        match action.operation {
            crate::input::Operation::Change => todo!(),
            crate::input::Operation::Insert(c) => {
                let buf_ctx = &mut self.window.buf_ctx;
                let buf = self.buffers.get_mut(&buf_ctx.buf_id).unwrap();
                buf.insert_string(buf_ctx, c.replace('\r', "\n").as_str());
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
            crate::input::Operation::SwitchMode(m) => self.mode = m,
            crate::input::Operation::None => (),
            crate::input::Operation::Replace(_) => todo!(),
            crate::input::Operation::Debug => {
                let buf_ctx = self.window.buf_ctx;
                let id = buf_ctx.buf_id;
                let buf = &self.buffers[&id];
                let lines = buf.get_lines(buf_ctx.cursorpos.y..(buf_ctx.cursorpos.y + 1));
                eprintln!("line: {:?}", lines);
                eprintln!("len: {:?}", lines.first().unwrap_or(&"").len());
            }
        }
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        termios::tcsetattr(self.term, termios::SetArg::TCSANOW, &self.orig).unwrap_or(());
    }
}
