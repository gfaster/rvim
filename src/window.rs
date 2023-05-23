use crate::render::BufId;
use std::io::Write;

use crate::buffer::Buffer;
use crate::buffer::DocPos;
use crate::render::Ctx;
use crate::term;
use crate::term::TermPos;

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
    pub buf_id: BufId,

    /// I use DocPos rather than a flat offset to more easily handle linewise operations, which
    /// seem to be more common than operations that operate on the flat buffer. It also makes
    /// translation more convienent, especially when the buffer is stored as an array of lines
    /// rather than a flat byte array (although it seems like this would slow transversal?).
    pub cursorpos: DocPos,
    pub topline: usize,
}

impl BufCtx {
    pub fn win_pos(&self, _win: &Window) -> TermPos {
        let y = (self.cursorpos.y - self.topline) as u32;
        let x = self.cursorpos.x as u32;
        TermPos { x, y }
    }

    /// draw the window
    pub fn draw(&self, win: &Window, ctx: &Ctx) {
        let buf = ctx.getbuf(self.buf_id).unwrap();
        // self.topline.clamp(self., max)
        let lines = buf.get_lines(self.topline..(self.topline + win.height() as usize));
        let basepos = win.reltoabs(TermPos { x: 0, y: 0 });
        for (i, l) in lines.into_iter().enumerate() {
            term::goto(TermPos {
                x: basepos.x,
                y: basepos.y + i as u32,
            });
            print!("{:w$}", l, w = win.width() as usize);
        }
    }

    pub fn new(buf: BufId) -> Self {
        Self {
            buf_id: buf,
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        }
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
    StatusLine,
    Welcome,
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
            print!("{:4}", l as usize + win.buf_ctx.topline + 1);
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
    fn draw(&self, win: &Window, ctx: &Ctx) {
        let linecnt = ctx.getbuf(win.buf_ctx.buf_id).unwrap().linecnt();
        let y = win.cursorpos().y;
        for l in 0..win.height() {
            let winbase = win.reltoabs(TermPos { x: 0, y: l });

            term::goto(TermPos {
                x: winbase.x - 5,
                y: winbase.y,
            });

            if l == y {
                print!(
                    "\x1b[1;32m{: >3} \x1b[0m",
                    l as usize + win.buf_ctx.topline + 1
                );
            } else if l as usize + win.buf_ctx.topline < linecnt {
                print!("\x1b[1;32m{: >4}\x1b[0m", y.abs_diff(l));
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
impl DispComponent for Welcome {
    fn draw(&self, win: &Window, _ctx: &Ctx) {
        if !win.dirty {
            let s = include_str!("../assets/welcome.txt");
            let top = (win.height() - s.lines().count() as u32) / 2;
            s.lines()
                .enumerate()
                .map(|(idx, line)| {
                    term::goto(win.reltoabs(TermPos {
                        x: 0,
                        y: top + idx as u32,
                    }));
                    print!("{:^w$}", line, w = win.width() as usize);
                })
                .last();
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
impl DispComponent for StatusLine {
    fn padding(&self) -> Padding {
        Padding {
            top: 0,
            bottom: 1,
            left: 0,
            right: 0,
        }
    }

    fn draw(&self, win: &Window, ctx: &Ctx) {
        let base = win.reltoabs(TermPos {
            x: 0,
            y: win.height(),
        });
        term::goto(TermPos {
            x: base.x - win.padding.left,
            y: base.y + 0,
        });

        match ctx.mode {
            crate::Mode::Normal => print!("\x1b[42;1;30m NORMAL \x1b[0m"),
            crate::Mode::Insert => print!("\x1b[44;1;30m INSERT \x1b[0m"),
            crate::Mode::Command => print!("\x1b[44;1;30m COMMAND \x1b[0m"),
        }
        print!(
            "\x1b[40m {: <x$}\x1b[0m",
            ctx.getbuf(win.buf_ctx.buf_id).unwrap().name(),
            x = (win.width() + win.padding.left + win.padding.right - 9) as usize
        );
    }
}

pub struct Window {
    pub buf_ctx: BufCtx,
    topleft: TermPos,
    botright: TermPos,
    components: Vec<Component>,
    padding: Padding,
    dirty: bool,
}

impl Window {
    pub fn new(buf: BufId) -> Self {
        let (terminal_size::Width(tw), terminal_size::Height(th)) =
            terminal_size().unwrap_or((terminal_size::Width(80), terminal_size::Height(40)));
        Self::new_withdim(buf, TermPos { x: 0, y: 0 }, tw as u32, th as u32)
    }

    pub fn new_withdim(buf: BufId, topleft: TermPos, width: u32, height: u32) -> Self {
        let mut components = vec![
            Component::RelLineNumbers(RelLineNumbers),
            Component::StatusLine(StatusLine),
        ];
        let dirty = true;
        if !dirty {
            components.push(Component::Welcome(Welcome));
        }

        let padding = components.iter().fold(
            Padding {
                top: 0,
                bottom: 1,
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
            topleft: TermPos {
                x: topleft.x + padding.left,
                y: topleft.y + padding.top,
            },
            botright: TermPos {
                x: width - padding.right,
                y: height - padding.bottom,
            },
            components,
            padding,
            dirty,
        }
    }

    pub fn resize(&mut self, dx: i32, dy: i32) {
        self.botright.y = self.botright.y.checked_add_signed(dy).unwrap();
        self.botright.x = self.botright.x.checked_add_signed(dx).unwrap();
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

    pub fn draw(&self, ctx: &Ctx) {
        term::rst_cur();
        self.buf_ctx.draw(self, ctx);
        self.components.iter().map(|x| x.draw(self, ctx)).last();
        term::goto(self.reltoabs(self.buf_ctx.win_pos(self)));
        term::flush();
    }

    pub fn cursorpos(&self) -> TermPos {
        self.buf_ctx.win_pos(self)
    }

    /// Move the cursor
    pub fn move_cursor(&mut self, buf: &Buffer, dx: isize, dy: isize) {
        let newy = self.buf_ctx
            .cursorpos
            .y
            .saturating_add_signed(dy)
            .clamp(0, buf.linecnt());
        let line = buf.get_lines(newy..(newy + 1))[0];
        let newx = self.buf_ctx
            .cursorpos
            .x
            .saturating_add_signed(dx)
            .clamp(0, line.len());

        self.buf_ctx.cursorpos.x = newx;
        self.buf_ctx.cursorpos.y = newy;
    }

    pub fn set_pos(&mut self, buf: &Buffer, pos: DocPos) {
        let newy = pos.y.clamp(0, buf.linecnt() - 1);
        self.buf_ctx.cursorpos.y = newy;
        let line = buf.get_lines(newy..(newy + 1))[0];
        self.buf_ctx.cursorpos.x = pos.x.clamp(0, line.len());
    }

    // pub fn insert_char<B: Buffer>(&mut self,
}

impl Write for Window {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut amt = 0;
        for (i, line) in buf
            .split(|b| *b == b'\n')
            .chain([].repeat(self.height() as usize))
            .enumerate()
            .take(self.height() as usize)
        {
            amt += line.len();
            term::goto(self.reltoabs(TermPos { x: 0, y: i as u32 }));
            print!(
                "{}",
                String::from_utf8_lossy(line)
                    .unicode_truncate(self.width() as usize)
                    .0
            )
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
    use super::*;

    fn basic_context() -> Ctx {
        let b = Buffer::from_string("0\n1\n22\n333\n4444\n\nnotrnc\ntruncated line".to_string());
        let mut ctx = Ctx::new_testing(b);
        let bufid = ctx.window.buf_ctx.buf_id;
        ctx.window = Window {
            buf_ctx: BufCtx {
                buf_id: bufid,
                cursorpos: DocPos { x: 0, y: 0 },
                topline: 0,
            },
            topleft: TermPos { x: 0, y: 0 },
            botright: TermPos { x: 7, y: 32 },
            components: vec![],
            padding: Padding::default(),
            dirty: false,
        };
        ctx
    }

    fn scroll_context() -> Ctx {
        let b = Buffer::from_string("0\n1\n22\n333\n4444\n55555\n\n\n\n\n\n\n\nLast".to_string());
        let mut ctx = Ctx::new_testing(b);
        let bufid = ctx.window.buf_ctx.buf_id;
        ctx.window = Window {
            buf_ctx: BufCtx {
                buf_id: bufid,
                cursorpos: DocPos { x: 0, y: 0 },
                topline: 0,
            },
            topleft: TermPos { x: 0, y: 0 },
            botright: TermPos { x: 7, y: 10 },
            components: vec![],
            padding: Padding::default(),
            dirty: false,
        };
        ctx
    }

    fn blank_context() -> Ctx {
        let b = Buffer::from_string("0\n1\n22\n333\n4444\n\nnotrnc\ntruncated line".to_string());
        let mut ctx = Ctx::new_testing(b);
        let bufid = ctx.window.buf_ctx.buf_id;
        ctx.window = Window {
            buf_ctx: BufCtx {
                buf_id: bufid,
                cursorpos: DocPos { x: 0, y: 0 },
                topline: 0,
            },
            topleft: TermPos { x: 0, y: 0 },
            botright: TermPos { x: 7, y: 32 },
            components: vec![],
            padding: Padding::default(),
            dirty: false,
        };
        ctx
    }

    fn scroll_moves_topline() {
        let ctx = scroll_context();
        assert_eq!(ctx.window.buf_ctx.topline, 0);
    }
}
