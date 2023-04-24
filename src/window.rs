use std::io::Write;
use crate::render::BufId;
use crate::textobj::TextObj;
use crate::buffer::DocPos;
use crate::buffer::Buffer;
use crate::render::Ctx;
use crate::term;
use crate::term::TermPos;
use crate::textobj::TextObject;
use enum_dispatch::enum_dispatch;
use terminal_size::terminal_size;
use unicode_truncate::UnicodeTruncateStr;

/// structure for a focused buffer. This is not a window and not a buffer. It holds the context of a
/// buffer editing session for later use. A window can display this, but shouldn't be limited  to
/// displaying only this. The reason I'm making this separate from window is that I want window to
/// be strictly an abstraction for rendering and/or focusing. 
///
/// I need to think about what niche the command line fits into. Is it another window, or is it its
/// own thing?
///
/// I should consider a "text field" or something similar as a trait for
/// an area that can be focused and take input.
///
/// Adhearing to Rust conventions for this will be challenging I want each Buffer to be referenced
/// by multiple BufCtx, and to be mutated by multiple BufCtx. I think this should be done by making
/// BufCtx only interact with Buffers when the BufCtx functions are called.
#[derive(Clone, Copy)]
pub struct BufCtx {
    buf_id: BufId,

    /// I use DocPos rather than a flat offset to more easily handle linewise operations, which
    /// seem to be more common than operations that operate on the flat buffer. It also makes
    /// translation more convienent, especially when the buffer is stored as an array of lines
    /// rather than a flat byte array (although it seems like this would slow transversal?).
    cursorpos: DocPos,
    topline: usize
}

impl BufCtx {
    pub fn win_pos(&self, _win: &Window) -> TermPos {
        let y = (self.topline - self.cursorpos.y) as u32;
        let x = self.cursorpos.x as u32;
        TermPos { x, y }
    }

    pub fn draw(&self, win: &Window) {

    }

    pub fn new(buf: BufId) -> Self {
        Self { buf_id: buf, cursorpos: DocPos { x: 0, y: 0 }, topline: 0 }
    }
}

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
impl<B> DispComponent<B> for LineNumbers where B: Buffer {
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
impl<B> DispComponent<B> for RelLineNumbers where B: Buffer {
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
impl<B> DispComponent<B> for Welcome where B: Buffer {
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
impl<B> DispComponent<B> for StatusLine where B: Buffer {
    fn padding(&self) -> Padding {
        Padding { top: 0, bottom: 1, left: 0, right: 0 }
    }

    fn draw(&self, win: &Window, ctx: &Ctx<B>) {
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
    buf_ctx: BufCtx,
    topline: usize,
    cursorpos: TermPos,
    cursoroff: usize,
    topleft: TermPos,
    botright: TermPos,
    components: Vec<Component>,
    padding: Padding,
    dirty: bool
}

impl Window{
    pub fn new(buf: BufId) -> Self {
        let (terminal_size::Width(tw), terminal_size::Height(th)) = terminal_size().unwrap_or((terminal_size::Width(80), terminal_size::Height(40)));
        Self::new_withdim(buf, TermPos { x: 0, y: 0 }, tw as u32, th as u32)
    }

    pub fn new_withdim(buf: BufId, topleft: TermPos, width: u32, height: u32) -> Self {
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
            buf_ctx: BufCtx::new(buf),
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

    pub fn draw<B: Buffer>(&self, ctx: &Ctx<B>) {
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

}

impl Write for Window {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut amt = 0;
        for (i, line) in buf.split(|b| *b == '\n' as u8).chain([].repeat(self.height() as usize)).enumerate().take(self.height() as usize) {
            amt += line.len();
            term::goto(self.reltoabs(TermPos { x: 0, y: i as u32 }));
            print!("{}", String::from_utf8_lossy(line).unicode_truncate(self.width() as usize).0)
        }

        Ok(amt)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        term::flush();
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::buffer::PTBuffer;

    use super::*;

    fn basic_window() -> Window<PTBuffer> {
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

    fn blank_window() -> Window<PTBuffer> {
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
