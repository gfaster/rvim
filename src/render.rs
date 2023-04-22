use crate::input::Token;
use crate::{buffer::*, Mode};
use std::os::unix::io::RawFd;
use nix::sys::termios::{Termios, LocalFlags};
use nix::sys::termios;
use unicode_truncate::UnicodeTruncateStr;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

    pub fn width(&self) -> u32 {
        self.botright.x - self.topleft.x
    }

    pub fn height(&self) -> u32 {
        self.botright.y - self.topleft.y
    }

    pub fn clear(&self) {
        term::goto(self.reltoabs(TermPos { x: 0, y: 0 }));
        (self.topleft.y..self.botright.y).map(|l| {
            print!("{}", " ".repeat(self.width() as usize));
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

        let x = self.cursoroff - newline_range.start;
        let y = newline - self.topline;

        // this should be screen space
        assert!((x as u32) < self.width());
        self.cursorpos = TermPos { x: x as u32, y: y as u32 };
    }

    /// get the lines that can be displayed - going to have to be done at a later date, linewrap
    /// trimming whitespace is an absolute nightmare
    fn truncated_lines(&self) -> impl Iterator<Item = &str> {
        // self.buf.get_lines(self.topline..(self.topline + (self.botright.y - self.topleft.y) as usize))
        //     .flat_map(|x| wrap(x, (self.botright.x - self.topleft.x) as usize)).take((self.botright.y - self.topleft.y) as usize);

        self.buf.get_lines(self.topline..(self.topline + self.height() as usize))
            .map(|l| {
                l.unicode_truncate(self.width() as usize).0
            })
    }

    /// get the length (in bytes) of the underlying buffer each screenspace line represents. This
    /// is a separate function because `&str.lines()` does not include newlines, so that data is
    /// lost in the process of wrapping
    fn truncated_lines_len(&self) -> impl Iterator<Item = usize> + '_ {
        // self.buf.get_lines(self.topline..(self.topline + (self.botright.y - self.topleft.y) as usize))
        //     .flat_map(|x| {
        //         let mut v: Vec<usize> = wrap(x, (self.botright.x - self.topleft.x) as usize).into_iter().map(|wl| wl.len()).collect();
        //         *v.last_mut().unwrap() += 1;
        //         v.into_iter()
        //     }).collect()
        self.buf.get_lines(self.topline..(self.topline + self.height() as usize)).map(|l| {
            l.unicode_truncate(self.width() as usize).1
        })
    }

    fn draw(&self) {
        term::rst_cur();
        self.truncated_lines().take((self.botright.y - self.topleft.y) as usize).enumerate().map(|(i, l)| {
            term::goto(self.reltoabs(TermPos { x: 0, y: i as u32 }));
            print!("{}", l.trim_end_matches('\n'));
        }).last();
        term::goto(self.reltoabs(self.cursorpos));
        term::flush();
    }

    fn insert_char(&mut self, c: char) {
        let off = self.buf.get_lines(0..self.topline).fold(0, |acc, l| acc + l.len() + 1) + 
        self.truncated_lines().take(self.cursorpos.y as usize).fold(0, |acc, l| acc + l.len() + 1) + self.cursorpos.x as usize;
        self.clear();
        match c {
            '\r' => {
                self.move_cursor(1, 0);
                self.buf.insert_char(off, '\n');
                self.move_cursor(-(self.width() as isize), 0);
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
    use super::*;


    fn basic_window() -> Window {
        let b = Buffer::new_fromstring("0\n1\n22\n333\n4444\n\nnotrnc\ntruncated line".to_string());
        Window { buf: b, topline: 0, cursorpos: TermPos { x: 0, y: 0 }, cursoroff: 0, topleft: TermPos { x: 0, y: 0 }, botright: TermPos { x: 7, y: 32 } }
    }

    #[test]
    fn test_truncated_lines_len() {
        let w = basic_window();
        assert_eq!(w.truncated_lines_len().collect::<Vec<_>>(), vec![1, 1, 2, 3, 4, 0, 6, 7])
    }

    #[test]
    fn test_move_cursor_down_basic() {
        let mut w = basic_window();
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 0});
        assert_eq!(w.cursoroff, 0); // 0

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 1});
        assert_eq!(w.cursoroff, 2); // 1

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 2});
        assert_eq!(w.cursoroff, 4); // 22

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 3});
        assert_eq!(w.cursoroff, 7); // 333

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 4});
        assert_eq!(w.cursoroff, 11); // 4444
    }
    
    #[test]
    fn test_move_cursor_down_truncated() {
        let mut w = basic_window();
        w.cursoroff = 11;
        w.cursorpos = TermPos {x: 0, y: 0};
        w.topline = 4;

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 1});
        assert_eq!(w.cursoroff, 16); // LF

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 2});
        assert_eq!(w.cursoroff, 17); // notrnc

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 3});
        assert_eq!(w.cursoroff, 24); // "truncated line"
    }
}
