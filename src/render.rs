use crate::input::Action;
use crate::input::Motion;
use crate::input::Operation;
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

pub struct Ctx<B> where B: Buffer {
    id_counter: usize,
    buffers: std::collections::BTreeMap<BufId, B>,
    termios: Termios,
    orig: Termios,
    pub term: RawFd,
    pub window: Window,
    pub mode: Mode,
}

impl<B> Ctx<B> where B: Buffer {
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

    pub fn render(&self) {
        self.window.draw(self);
    }

    pub fn process_action(&mut self, action: Action) {
        if let (Some(Motion::TextObj(m)), Operation::Delete) = (&action.motion, &action.operation) {
            self.window.delete_range(m);
            return;
        }

        match action.motion {
            Some(m) => match m {
                Motion::ScreenSpace { dy, dx } => self.window.move_cursor(dx, dy),
                Motion::BufferSpace { doff: _ } => todo!(),
                Motion::TextObj(_) => todo!()
            },
            None => (),
        }

        match action.operation {
            crate::input::Operation::Change => todo!(),
            crate::input::Operation::Insert(c) => self.window.insert_char(c.chars().next().unwrap()),
            crate::input::Operation::ToInsert => self.mode = Mode::Insert,
            crate::input::Operation::Delete => self.window.delete_char(),
            crate::input::Operation::ToNormal => self.mode = Mode::Normal,
            crate::input::Operation::None => (),
            crate::input::Operation::Replace(_) => todo!()
        }
    }
}

impl<B: Buffer> Drop for Ctx<B> {
    fn drop(&mut self) {
        termios::tcsetattr(self.term, termios::SetArg::TCSANOW, &self.orig).unwrap_or(());
    }
}
