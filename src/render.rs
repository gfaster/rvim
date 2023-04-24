use crate::input::Action;
use crate::input::Motion;

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

pub struct Ctx<B>
where
    B: Buffer,
{
    id_counter: usize,
    buffers: std::collections::BTreeMap<BufId, B>,
    termios: Termios,
    orig: Termios,
    pub term: RawFd,
    pub window: Window,
    pub mode: Mode,
}

#[cfg(test)]
impl<B> Ctx<B>
where
    B: Buffer,
{
    pub fn new_testing(buf: B) -> Self {
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

impl<B> Ctx<B>
where
    B: Buffer,
{
    pub fn from_file(term: RawFd, file: &Path) -> std::io::Result<Self> {
        let buf = B::open(file)?;
        Ok(Self::from_buffer(term, buf))
    }
    pub fn from_buffer(term: RawFd, buf: B) -> Self {
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

    pub fn getbuf_mut(&mut self, buf: BufId) -> Option<&mut B> {
        self.buffers.get_mut(&buf)
    }

    pub fn getbuf(&self, buf: BufId) -> Option<&B> {
        self.buffers.get(&buf)
    }

    pub fn render(&self) {
        self.window.draw(self);
    }

    pub fn process_action(&mut self, action: Action) {
        if let Some(m) = action.motion {
            match m {
                Motion::ScreenSpace { dy, dx } => {
                    // type system here is kinda sneaky, can't use getbuf because all of self is
                    // borrowed
                    let bufid = self.window.buf_ctx.buf_id;
                    let buf = &self.buffers[&bufid];
                    let buf_ctx = &mut self.window.buf_ctx;
                    buf_ctx.move_cursor(buf, dx, dy)
                }
                Motion::BufferSpace { doff: _ } => todo!(),
                Motion::TextObj(_) => todo!(),
            }
        }

        match action.operation {
            crate::input::Operation::Change => todo!(),
            crate::input::Operation::Insert(c) => {
                let buf_ctx = &mut self.window.buf_ctx;
                self.buffers
                    .get_mut(&buf_ctx.buf_id)
                    .unwrap()
                    .insert_char(buf_ctx, c.chars().next().unwrap());
                self.window.draw(self);
            }
            crate::input::Operation::ToInsert => self.mode = Mode::Insert,
            crate::input::Operation::Delete => {
                let buf_ctx = &mut self.window.buf_ctx;
                self.buffers
                    .get_mut(&buf_ctx.buf_id)
                    .unwrap()
                    .delete_char(buf_ctx);
                self.window.draw(self);
            }
            crate::input::Operation::ToNormal => self.mode = Mode::Normal,
            crate::input::Operation::None => (),
            crate::input::Operation::Replace(_) => todo!(),
        }
    }
}

impl<B: Buffer> Drop for Ctx<B> {
    fn drop(&mut self) {
        termios::tcsetattr(self.term, termios::SetArg::TCSANOW, &self.orig).unwrap_or(());
    }
}
