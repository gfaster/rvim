use crate::buffer::Buffer;
use crate::render::Ctx;
use crate::term;
use crate::term::TermPos;
use crate::textobj::{TextObj, TextObject};
use enum_dispatch::enum_dispatch;
use terminal_size::terminal_size;
use unicode_truncate::UnicodeTruncateStr;

#[derive(Default)]
struct Padding {
    top: u32,
    bottom: u32,
    left: u32,
    right: u32,
}

#[enum_dispatch]
trait DispComponent<B> where B: Buffer{
    /// write the component
    fn draw(&self, win: &Window, ctx: &Ctx<B>);

    /// amount of padding needed left, top, bottom, right
    fn padding(&self) -> Padding;
}

#[enum_dispatch(DispComponent)]
enum Component {
    LineNumbers,
    RelLineNumbers,
    StatusLine,
    Welcome
}

struct LineNumbers;
impl<B> DispComponent<B> for LineNumbers {
    fn draw(&self, win: &Window, _ctx: &Ctx<B>) {
        for l in 0..win.height() {
            let winbase = win.reltoabs(TermPos { x: 0, y: l });
            term::goto(TermPos {
                x: winbase.x - 4,
                y: winbase.y,
            });
            print!("{:4}", l as usize + win.topline + 1);
        }
    }

    fn padding(&self) -> Padding {
        Padding {
            top: 0,
            bottom: 0,
            left: 4,
            right: 0,
        }
    }
}

struct RelLineNumbers;
impl<B> DispComponent<B> for RelLineNumbers {
    fn draw(&self, win: &Window, _ctx: &Ctx<B>) {
        for l in 0..win.height() {
            let winbase = win.reltoabs(TermPos { x: 0, y: l });

            term::goto(TermPos {
                x: winbase.x - 5,
                y: winbase.y,
            });
            if l == win.cursorpos.y {
                print!("\x1b[1;32m{: >3} \x1b[0m", l as usize + win.topline + 1);
            } else if l as usize + win.topline < win.buf.working_linecnt() {
                print!("\x1b[1;32m{: >4}\x1b[0m", win.cursorpos.y.abs_diff(l));
            } else {
                print!("{: >4}", ' ');
            }
        }
    }

    fn padding(&self) -> Padding {
        Padding {
            top: 0,
            bottom: 0,
            left: 5,
            right: 0,
        }
    }
}

struct Welcome;
impl<B> DispComponent<B> for Welcome {
    fn draw(&self, win: &Window, _ctx: &Ctx<B>) {
        if !win.dirty {
            let s = include_str!("../assets/welcome.txt");
            let top = (win.height() - s.lines().count() as u32)/2;
            s.lines().enumerate().map(|(idx, line)|{
                term::goto(win.reltoabs(TermPos { x: 0, y: top + idx as u32 }));
                print!("{:^w$}", line, w = win.width() as usize);
            }).last();
        }
    }

    fn padding(&self) -> Padding {
        Padding {
            top: 0,
            bottom: 0,
            left: 0,
            right: 0,
        }
    }
}

struct StatusLine;
impl<B> DispComponent<B> for StatusLine {
    fn padding(&self) -> Padding {
        Padding { top: 0, bottom: 1, left: 0, right: 0 }
    }

    fn draw(&self, win: &Window<B>, ctx: &Ctx<B>) {
        let base = win.reltoabs(TermPos { x: 0, y: win.height() - 1 });
        term::goto(TermPos { x: base.x - win.padding.left, y: base.y + 1 });
        
        match ctx.mode {
            crate::Mode::Normal => print!("\x1b[42;1;30m NORMAL \x1b[0m"),
            crate::Mode::Insert => print!("\x1b[44;1;30m INSERT \x1b[0m"),
        }
        print!("\x1b[40m {: <x$}\x1b[0m",win.buf.name(), x = (win.width() + win.padding.left + win.padding.right - 9) as usize);
    }
}

pub struct Window<B> where B: Buffer {
    buf: B,
    topline: usize,
    cursorpos: TermPos,
    cursoroff: usize,
    topleft: TermPos,
    botright: TermPos,
    components: Vec<Component>,
    padding: Padding,
    dirty: bool
}

impl Window {
    pub fn new(buf: Buffer) -> Self {
        let (terminal_size::Width(tw), terminal_size::Height(th)) = terminal_size().unwrap_or((terminal_size::Width(80), terminal_size::Height(40)));
        Self::new_withdim(buf, TermPos { x: 0, y: 0 }, tw as u32, th as u32)
    }

    pub fn new_withdim(buf: Buffer, topleft: TermPos, width: u32, height: u32) -> Self {
        let mut components = vec![Component::RelLineNumbers(RelLineNumbers), Component::StatusLine(StatusLine)];
        let dirty = buf.len() != 0;
        if !dirty {
            components.push(Component::Welcome(Welcome));
        }

        let padding = components.iter().fold(
            Padding {
                top: 0,
                bottom: 0,
                left: 0,
                right: 0,
            },
            |acc, x| {
                let pad = x.padding();
                Padding {
                    top: acc.top + pad.top,
                    bottom: acc.bottom + pad.bottom,
                    left: acc.left + pad.left,
                    right: acc.right + pad.right,
                }
            },
        );
        Self {
            buf,
            topline: 0,
            cursoroff: 0,
            cursorpos: TermPos { x: 0, y: 0 },
            topleft: TermPos {
                x: topleft.x + padding.left,
                y: topleft.y + padding.top,
            },
            botright: TermPos {
                x: width as u32 - padding.right,
                y: height as u32 - padding.bottom,
            },
            components,
            padding,
            dirty
        }
    }

    pub fn width(&self) -> u32 {
        self.botright.x - self.topleft.x
    }

    pub fn height(&self) -> u32 {
        self.botright.y - self.topleft.y
    }

    pub fn clear(&self) {
        (0..self.height())
            .map(|l| {
                term::goto(self.reltoabs(TermPos { x: 0, y: l }));
                print!("{}", " ".repeat(self.width() as usize));
            })
            .last();
    }

    fn reltoabs(&self, pos: TermPos) -> TermPos {
        TermPos {
            x: pos.x + self.topleft.x,
            y: pos.y + self.topleft.y,
        }
    }

    pub fn move_cursor(&mut self, dx: isize, dy: isize) {
        let prev_line = self
            .buf
            .lines_start()
            .iter()
            .enumerate()
            .rev()
            .find(|(_, off)| **off <= self.cursoroff)
            .unwrap()
            .0;
        let prev_lineoff = self.cursoroff - self.buf.lines_start()[prev_line];
        let newline = prev_line
            .saturating_add_signed(dy)
            .clamp(0, self.buf.working_linecnt() - 1);
        let newline_range = self.buf.line_range(newline);

        if newline_range.len() > 0 {
        self.cursoroff = (newline_range.start as isize + dx + prev_lineoff as isize)
            .clamp(newline_range.start as isize, newline_range.end as isize - 1)
            as usize;
        } else {
            self.cursoroff = newline_range.start;
        }

        let x = self.cursoroff - newline_range.start;

        // move window if it's off the screen
        match newline as isize - self.topline as isize {
            l if l < 0 => self.topline -= l.unsigned_abs(),
            l if l >= self.height() as isize => {
                self.topline += (l - self.height() as isize) as usize + 1
            }
            _ => (),
        }

        let y = newline - self.topline;

        // this should be screen space
        assert!((x as u32) < self.width());
        assert!((y as u32) < self.height());
        self.cursorpos = TermPos {
            x: x as u32,
            y: y as u32,
        };
    }

    /// get the lines that can be displayed - going to have to be done at a later date, linewrap
    /// trimming whitespace is an absolute nightmare
    fn truncated_lines(&self) -> impl Iterator<Item = &str> {
        // self.buf.get_lines(self.topline..(self.topline + (self.botright.y - self.topleft.y) as usize))
        //     .flat_map(|x| wrap(x, (self.botright.x - self.topleft.x) as usize)).take((self.botright.y - self.topleft.y) as usize);

        self.buf
            .get_lines(self.topline..(self.topline + self.height() as usize))
            .map(|l| l.unicode_truncate(self.width() as usize).0)
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
        self.buf
            .get_lines(self.topline..(self.topline + self.height() as usize))
            .map(|l| l.unicode_truncate(self.width() as usize).1)
    }

    pub fn draw(&self, ctx: &Ctx) {
        term::rst_cur();
        self.truncated_lines()
            .take(self.height() as usize)
            .enumerate()
            .map(|(i, l)| {
                term::goto(self.reltoabs(TermPos { x: 0, y: i as u32 }));
                print!("{:<w$}", l.trim_end_matches('\n'), w = self.width() as usize);
            })
            .last();
        self.components.iter().map(|x| x.draw(self, ctx)).last();
        term::goto(self.reltoabs(self.cursorpos));
        term::flush();
    }

    /// recalculate cursoroff based on cursorpos
    fn get_off(&self) -> usize {
        self
            .buf
            .get_lines(0..self.topline)
            .fold(0, |acc, l| acc + l.len() + 1)
        + self
            .truncated_lines()
            .take(self.cursorpos.y as usize)
            .fold(0, |acc, l| acc + l.len() + 1)
        + self.cursorpos.x as usize
    }

    pub fn insert_char(&mut self, c: char) {
        let off = self.cursoroff;
        if !self.dirty {
            self.clear();
            self.dirty = true;
        }
        match c {
            '\r' => {
                self.buf.insert_char(off, '\n');
                self.move_cursor(-(self.width() as isize), 1);
            }
            _ => {
                self.buf.insert_char(off, c);
                self.move_cursor(1, 0);
            }
        }
    }

    fn set_cursoroff(&mut self, newoff: usize) {
        self.cursoroff = newoff;
    }

    /// deletes the character to the left of the cursor
    pub fn delete_char(&mut self) {
        if self.cursoroff == 0 {return};

        self.buf.delete_char(self.cursoroff - 1);
        self.move_cursor(-1, 0);
    }

    pub fn delete_range(&mut self, t: &TextObject) -> Option<()>{
        let r = t.find_bounds(&self.buf, self.cursoroff, crate::textobj::TextObjectModifier::Inner)?;
        let start = r.start;
        r.map(|_| self.buf.delete_char(start)).last();
        self.set_cursoroff(start);
        self.move_cursor(0, 0);
        Some(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn basic_window() -> Window {
        let b = Buffer::new_fromstring("0\n1\n22\n333\n4444\n\nnotrnc\ntruncated line".to_string());
        Window {
            buf: b,
            topline: 0,
            cursorpos: TermPos { x: 0, y: 0 },
            cursoroff: 0,
            topleft: TermPos { x: 0, y: 0 },
            botright: TermPos { x: 7, y: 32 },
            components: vec![],
            padding: Padding::default(),
            dirty: true
        }
    }

    fn blank_window() -> Window {
        let b = Buffer::new_fromstring("".to_string());
        Window {
            buf: b,
            topline: 0,
            cursorpos: TermPos { x: 0, y: 0 },
            cursoroff: 0,
            topleft: TermPos { x: 0, y: 0 },
            botright: TermPos { x: 7, y: 32 },
            components: vec![],
            padding: Padding::default(),
            dirty: false
        }
    }

    #[test]
    fn test_insert_blank_window() {
        let mut w = blank_window();
        w.insert_char('\n');
        w.insert_char('\n');
        w.insert_char('\n');
    }

    #[test]
    fn test_move_in_blank_window() {
        let mut w = blank_window();
        w.move_cursor(0, 1);
        w.move_cursor(1, 0);
        assert_eq!(w.cursoroff, 0);
        assert_eq!(w.cursorpos, TermPos {x: 0, y: 0});
    }

    #[test]
    fn test_truncated_lines_len() {
        let w = basic_window();
        assert_eq!(
            w.truncated_lines_len().collect::<Vec<_>>(),
            vec![1, 1, 2, 3, 4, 0, 6, 7]
        )
    }

    #[test]
    fn test_move_cursor_down_basic() {
        let mut w = basic_window();
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 0 });
        assert_eq!(w.cursoroff, 0); // 0

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 1 });
        assert_eq!(w.cursoroff, 2); // 1

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 2 });
        assert_eq!(w.cursoroff, 4); // 22

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 3 });
        assert_eq!(w.cursoroff, 7); // 333

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 4 });
        assert_eq!(w.cursoroff, 11); // 4444
    }

    #[test]
    fn test_move_cursor_down_truncated() {
        let mut w = basic_window();
        w.cursoroff = 11;
        w.cursorpos = TermPos { x: 0, y: 0 };
        w.topline = 4;

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 1 });
        assert_eq!(w.cursoroff, 16); // LF

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 2 });
        assert_eq!(w.cursoroff, 17); // notrnc

        w.move_cursor(0, 1);
        assert_eq!(w.cursorpos, TermPos { x: 0, y: 3 });
        assert_eq!(w.cursoroff, 24); // "truncated line"
    }


}
