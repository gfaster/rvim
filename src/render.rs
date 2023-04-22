use crate::input::Token;
use crate::{buffer::*, Mode};
use std::os::unix::io::RawFd;
use nix::sys::termios::{Termios, LocalFlags};
use nix::sys::termios;
use crate::window::*;
use crate::term;


pub struct Ctx {
    termios: Termios,
    orig: Termios,
    pub term: RawFd,
    pub window: Window,
    pub mode: Mode
}

impl Ctx {
    pub fn new(term: RawFd, buf: Buffer) -> Self {
        term::altbuf_enable();
        term::flush();
        let mut termios = termios::tcgetattr(term).unwrap();
        let orig = termios.clone();
        termios::cfmakeraw(&mut termios);
        termios.local_flags.remove(LocalFlags::ECHO);
        termios.local_flags.insert(LocalFlags::ISIG);
        termios::tcsetattr(term, termios::SetArg::TCSANOW, &termios).unwrap();


        let window = Window::new(buf);

        Self {
            termios, 
            orig,
            term,
            mode: Mode::Normal,
            window
        }
    }

    pub fn render(&self) {
        self.window.draw(self);
    }

    pub fn process_token(&mut self, token: Token) {
        match token {
            Token::Motion(m) => self.window.move_cursor(m.dx, m.dy),
            Token::SetMode(m) => self.mode = m,
            Token::Insert(c) => self.window.insert_char(c),
        }
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        termios::tcsetattr(self.term, termios::SetArg::TCSANOW, &self.orig).unwrap_or(());
    }
}




