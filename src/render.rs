use crate::buffer::*;
use std::os::unix::io::RawFd;
use nix::sys::termios::Termios;
use nix::sys::termios;

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
            topleft: TermPos { x: 3, y: 0 },
            botright: TermPos { x: 83, y: 32 },
        }
    }

    fn draw(&self) {
        term::rst_cur();
        self.buf.get_lines(self.topline..(self.topline + (self.botright.y - self.topleft.y) as usize)).enumerate().map(|(i, l)| {
            term::goto(TermPos { x: self.topleft.x, y: i as u32 });
            print!("{l}");
        }).last();
        term::flush();
    }
}

pub struct Ctx {
    termios: Termios,
    window: Window,
}

impl Ctx {
    pub fn new(term: RawFd, buf: Buffer) -> Self {
        term::altbuf_enable();
        let mut termios = termios::tcgetattr(term).unwrap();
        termios::cfmakeraw(&mut termios);

        let window = Window::new(buf);

        Self {
            termios, 
            window
        }
    }


    pub fn render(&self) {
        self.window.draw();
    }
}





#[cfg(test)]
mod test {
    // use super::*;
}
