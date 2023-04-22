use crate::input::Token;
use crate::{buffer::*, Mode};
use std::borrow::Cow;
use std::io::stdout;
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
    cursoroff: usize,
    topleft: TermPos,
    botright: TermPos
}

impl Window {
    fn new(buf: Buffer) -> Self {
        Self {
            buf,
            topline: 0,
            cursoroff: 0,
            cursorpos: TermPos { x: 0, y: 0 },
            topleft: TermPos { x: 10, y: 5 },
            botright: TermPos { x: 90, y: 37 },
        }
    }

    pub fn clear(&self) {
        term::goto(self.reltoabs(TermPos { x: 0, y: 0 }));
        (self.topleft.y..self.botright.y).map(|l| {
            print!("{}", " ".repeat((self.botright.x - self.topleft.x) as usize));
            term::goto(TermPos { x: self.topleft.x, y: l })
        }).last();
    }

    fn reltoabs(&self, pos: TermPos) -> TermPos {
        TermPos { x: pos.x + self.topleft.x, y: pos.y + self.topleft.y }
    }

    fn move_cursor(&mut self, dx: isize, dy: isize) {
        // Is this off-by-one?
        let prev_line = self.buf.lines_start().iter().enumerate().rev().find(|(_, off)| **off <= self.cursoroff).unwrap().0;
        let prev_lineoff = self.cursoroff - self.buf.lines_start()[prev_line];
        let newline = prev_line.saturating_add_signed(dy);
        let newline_range = self.buf.line_range(newline);

        self.cursoroff = (newline_range.start as isize + dx + prev_lineoff as isize).clamp(newline_range.start as isize, newline_range.end as isize) as usize;
        dbg!(&self.cursoroff);

        let mut rem = self.buf.line_range(self.topline).start;
        let (y, _) = self.wrapped_lines().map(|x| x.len() + 1).enumerate().map(|x| (x.0 + 1, x.1)).take_while(|x| {
            if dbg!(rem) + dbg!(x.1) <= self.cursoroff {
                rem += x.1;
                true
            } else {
                false
            }
        }).last().unwrap_or((0,0));
        // }).last().unwrap();
        let x = self.cursoroff - rem;
        dbg!(y);

        // this should be screen space
        debug_assert!(dbg!(x as u32) < self.botright.x - self.topleft.x);
        self.cursorpos = TermPos { x: x as u32, y: y as u32 };
    }

    fn wrapped_lines(&self) -> impl Iterator<Item = Cow<str>> {
        self.buf.get_lines(self.topline..(self.topline + (self.botright.y - self.topleft.y) as usize))
            .flat_map(|x| wrap(x, (self.botright.x - self.topleft.x) as usize)).take((self.botright.y - self.topleft.y) as usize)
    }

    fn draw(&self) {
        term::rst_cur();
        self.wrapped_lines().take((self.botright.y - self.topleft.y) as usize).enumerate().map(|(i, l)| {
            term::goto(self.reltoabs(TermPos { x: 0, y: i as u32 }));
            print!("{l}");
        }).last();
        term::goto(self.reltoabs(self.cursorpos));
        term::flush();
    }

    fn insert_char(&mut self, c: char) {
        let off = self.buf.get_lines(0..self.topline).fold(0, |acc, l| acc + l.len() + 1) + 
        self.wrapped_lines().take(self.cursorpos.y as usize).fold(0, |acc, l| acc + l.len() + 1) + self.cursorpos.x as usize;
        self.clear();
        match c {
            '\r' => {
                self.move_cursor(0, 1);
                self.buf.insert_char(off, '\n');
            },
            _  => {
                self.move_cursor(1, 0);
                self.buf.insert_char(off, c);
            },
        }
    }
}

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
        self.window.draw();
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
        termios::tcsetattr(self.term, termios::SetArg::TCSANOW, &self.orig).unwrap();
    }
}




#[cfg(test)]
mod test {
    // use super::*;
}
