use crate::buffer::*;
use std::os::unix::io::RawFd;
use nix::sys::termios::Termios;
use nix::sys::termios;

pub mod term {
    pub fn rst_cur() {
        print!("\x1b[1;1H");
    }

    pub fn altbuf_enable() {
        print!("\x1b[?1049h");
    }

    pub fn altbuf_disable() {
        print!("\x1b[?1049l");
    }
}


pub struct Ctx {
    termios: Termios,
    buf: Buffer,
    topline: usize,
    cursorpos: (u32, u32),
}

impl Ctx {
    pub fn new(term: RawFd, buf: Buffer) -> Self {
        term::altbuf_enable();
        let mut termios = termios::tcgetattr(term).unwrap();
        termios::cfmakeraw(&mut termios);
        Self {
            termios, 
            buf,
            topline: 0,
            cursorpos: (0, 0),
        }
    }


    pub fn render(&self) {
        term::rst_cur();
        self.buf.get_lines(self.topline..(self.topline + 15)).map(|l| {
            println!("{l}");
        }).last();
    }
}



#[cfg(test)]
mod test {
    // use super::*;
}
