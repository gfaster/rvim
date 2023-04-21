use crate::buffer::*;
use std::os::unix::io::RawFd;
use nix::sys::termios::{Termios, LocalFlags};
use nix::sys::termios;
use textwrap::wrap;

pub mod term {
    use std::io::{self, Write};

    use super::TermPos;

    pub fn rst_cur() {
        print!("\x1b[1;1H");
    }

    pub fn altbuf_enable() {
        print!("\x1b[?1049h");
    }

    pub fn altbuf_disable() {
        print!("\x1b[?1049l");
    }

    pub fn goto(pos: TermPos) {
        print!("\x1b[{};{}H", pos.row(), pos.col());
    }

    pub fn flush() {
        io::stdout().flush().unwrap();
    }
}

#[derive(Clone, Copy)]
pub struct TermPos {
    x: u32,
    y: u32
}

impl TermPos {
    pub fn row(&self) -> u32 {
        self.y + 1
    }

    pub fn col(&self) -> u32 {
        self.x + 1
    }
}

pub struct Window {
    buf: Buffer,
    topline: usize,
    cursorpos: TermPos,
    topleft: TermPos,
    botright: TermPos
}

impl Window {
    fn new(buf: Buffer) -> Self {
        Self {
            buf,
            topline: 0,
            cursorpos: TermPos { x: 0, y: 0 },
            topleft: TermPos { x: 10, y: 5 },
            botright: TermPos { x: 90, y: 37 },
        }
    }

    fn reltoabs(&self, pos: TermPos) -> TermPos {
        TermPos { x: pos.x + self.topleft.x, y: pos.y + self.topleft.y }
    }

    fn draw(&self) {
        term::rst_cur();
        self.buf.get_lines(self.topline..(self.topline + (self.botright.y - self.topleft.y) as usize))
            .flat_map(|x| wrap(x, (self.botright.x - self.topleft.x) as usize)).take((self.botright.y - self.topleft.y) as usize).enumerate().map(|(i, l)| {
            term::goto(self.reltoabs(TermPos { x: 0, y: i as u32 }));
            print!("{l}");
        }).last();
        term::goto(self.reltoabs(self.cursorpos));
        term::flush();
    }
}

pub struct Ctx {
    termios: Termios,
    orig: Termios,
    term: RawFd,
    window: Window,
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
            window
        }
    }

    pub fn render(&self) {
        self.window.draw();
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        termios::tcsetattr(self.term, termios::SetArg::TCSANOW, &self.orig).unwrap();
    }
}




#[cfg(test)]
mod test {
    // use super::*;
}
