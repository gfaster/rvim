use crate::buffer::Buffer;
use crate::render::Ctx;
use crate::term;
use crate::term::TermPos;
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
trait DispComponent {
    /// write the component
    fn draw(&self, win: &Window, ctx: &Ctx);

    /// amount of padding needed left, top, bottom, right
    fn padding(&self) -> Padding;
}

#[enum_dispatch(DispComponent)]
enum Component {
    LineNumbers,
    RelLineNumbers,
    StatusLine
}

struct LineNumbers;
impl DispComponent for LineNumbers {
    fn draw(&self, win: &Window, _ctx: &Ctx) {
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
impl DispComponent for RelLineNumbers {
    fn draw(&self, win: &Window, _ctx: &Ctx) {
        for l in 0..win.height() {
            let winbase = win.reltoabs(TermPos { x: 0, y: l });

            term::goto(TermPos {
                x: winbase.x - 5,
                y: winbase.y,
            });
            if l == win.cursorpos.y {
                print!("\x1b[1;32m{: >3} \x1b[0m", l as usize + win.topline + 1);
            } else {
                print!("\x1b[1;32m{: >4}\x1b[0m", win.cursorpos.y.abs_diff(l));
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

struct StatusLine;
impl DispComponent for StatusLine {
    fn padding(&self) -> Padding {
        Padding { top: 0, bottom: 1, left: 0, right: 0 }
    }

    fn draw(&self, win: &Window, ctx: &Ctx) {
        let base = win.reltoabs(TermPos { x: 0, y: win.height() - 1 });
        term::goto(TermPos { x: base.x - win.padding.left, y: base.y + 1 });
        
        match ctx.mode {
            crate::Mode::Normal => print!("\x1b[42;1;30m NORMAL \x1b[0m"),
            crate::Mode::Insert => print!("\x1b[44;1;30m INSERT \x1b[0m"),
        }
        print!("\x1b[40m {: <x$}\x1b[0m",win.buf.name(), x = (win.width() + win.padding.left + win.padding.right - 9) as usize);
    }
}

pub struct Window {
    buf: Buffer,
    topline: usize,
    cursorpos: TermPos,
    cursoroff: usize,
    topleft: TermPos,
    botright: TermPos,
    components: Vec<Component>,
    padding: Padding,
}

impl Window {
    pub fn new(buf: Buffer) -> Self {
        let (terminal_size::Width(tw), terminal_size::Height(th)) = terminal_size().unwrap_or((terminal_size::Width(80), terminal_size::Height(40)));
        Self::new_withdim(buf, TermPos { x: 0, y: 0 }, tw as u32, th as u32)
    }

    pub fn new_withdim(buf: Buffer, topleft: TermPos, width: u32, height: u32) -> Self {
        let components = vec![Component::RelLineNumbers(RelLineNumbers), Component::StatusLine(StatusLine)];
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

        self.cursoroff = (newline_range.start as isize + dx + prev_lineoff as isize)
            .clamp(newline_range.start as isize, newline_range.end as isize - 1)
            as usize;

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

    pub fn insert_char(&mut self, c: char) {
        let off = self
            .buf
            .get_lines(0..self.topline)
            .fold(0, |acc, l| acc + l.len() + 1)
            + self
                .truncated_lines()
                .take(self.cursorpos.y as usize)
                .fold(0, |acc, l| acc + l.len() + 1)
            + self.cursorpos.x as usize;
        self.clear();
        match c {
            '\r' => {
                self.move_cursor(1, 0);
                self.buf.insert_char(off, '\n');
                self.move_cursor(-(self.width() as isize), 0);
            }
            _ => {
                self.buf.insert_char(off, c);
                self.move_cursor(1, 0);
            }
        }
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
        }
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
