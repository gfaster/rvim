use crate::debug::log;
use crate::prelude::*;
use crate::render::BufId;
use crate::screen_write;
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
    pub virtual_pos: DocPos,
    pub topline: usize,
}

impl BufCtx {
    pub fn win_pos(&self, _win: &Window) -> TermPos {
        let y = self
            .cursorpos
            .y
            .checked_sub(self.topline)
            .expect("tried to move cursor above window") as u32;
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
            screen_write!("{:w$}", l, w = win.width() as usize);
        }
    }

    pub fn new(buf: BufId) -> Self {
        Self {
            buf_id: buf,
            cursorpos: DocPos { x: 0, y: 0 },
            virtual_pos: DocPos { x: 0, y: 0 },
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
            screen_write!("{:4}", l as usize + win.buf_ctx.topline + 1);
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
                screen_write!(
                    "\x1b[1;32m{: >3} \x1b[0m",
                    l as usize + win.buf_ctx.topline + 1
                );
            } else if l as usize + win.buf_ctx.topline < linecnt {
                screen_write!("\x1b[1;32m{: >4}\x1b[0m", y.abs_diff(l));
            } else {
                screen_write!("{: >4}", ' ');
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
                    screen_write!("{:^w$}", line, w = win.width() as usize);
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

        let (color, mode_str) = match ctx.mode {
            crate::Mode::Normal => ("\x1b[42;1;30m", " NORMAL "),
            crate::Mode::Insert => ("\x1b[44;1;30m", " INSERT "),
            crate::Mode::Command => ("\x1b[44;1;30m", " COMMAND "),
        };
        screen_write!(
            "{color}{mode_str}\x1b[0m\x1b[40m {: <x$}\x1b[0m",
            ctx.getbuf(win.buf_ctx.buf_id).unwrap().name(),
            x = (win.width() + win.padding.left + win.padding.right - mode_str.len() as u32)
                as usize
                - 1
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

    /// probably don't want to use this since it erases padding
    pub fn set_size(&mut self, newx: u32, newy: u32) {
        self.botright.x = self.topleft.x + newx;
        self.botright.y = self.topleft.y + newy;
    }

    pub fn set_size_padded(&mut self, newx: u32, newy: u32) {
        let newx = newx - self.padding.left - self.padding.right;
        let newy = newy - self.padding.top - self.padding.bottom;
        self.botright.x = self.topleft.x + newx;
        self.botright.y = self.topleft.y + newy;
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
                screen_write!("{: >w$}", "", w = self.width() as usize);
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
        // log!("height: {}", self.height());
        term::rst_cur();
        self.buf_ctx.draw(self, ctx);
        self.components.iter().map(|x| x.draw(self, ctx)).last();
        term::goto(self.reltoabs(self.buf_ctx.win_pos(self)));
    }

    pub fn cursorpos(&self) -> TermPos {
        // log!("calling cursorpos from {:}", std::panic::Location::caller());
        self.buf_ctx.win_pos(self)
    }

    pub fn move_cursor(&mut self, buf: &Buffer, dx: isize, dy: isize) {
        let newy = self
            .buf_ctx
            .cursorpos
            .y
            .saturating_add_signed(dy)
            .clamp(0, buf.linecnt().saturating_sub(1));
        let line = &buf.get_lines(newy..(newy + 1))[0];
        let newx = self
            .buf_ctx
            .virtual_pos
            .x
            .saturating_add_signed(dx)
            .clamp(0, line.len());

        if dx != 0 {
            self.buf_ctx.virtual_pos.x = newx;
        }
        self.buf_ctx.virtual_pos.y = newy;

        self.buf_ctx.cursorpos.x = newx;
        self.buf_ctx.cursorpos.y = newy;
        self.fit_ctx_frame();
    }

    pub fn set_pos(&mut self, buf: &Buffer, pos: DocPos) {
        let newy = pos.y.clamp(0, buf.linecnt().saturating_sub(1));
        self.buf_ctx.cursorpos.y = newy;
        self.buf_ctx.virtual_pos.y = newy;
        let line = &buf.get_lines(newy..(newy + 1))[0];
        self.buf_ctx.cursorpos.x = pos.x.clamp(0, line.len());
        self.buf_ctx.virtual_pos.x = self.buf_ctx.cursorpos.x;
        self.fit_ctx_frame();
    }

    fn fit_ctx_frame(&mut self) {
        let y = self.buf_ctx.cursorpos.y;
        let top = self.buf_ctx.topline;
        let h = self.height() as usize;
        self.buf_ctx.topline = top.clamp(y.saturating_sub(h - 1), y);
    }

    fn center_view(&mut self) {
        let y = self.buf_ctx.cursorpos.y;
        self.buf_ctx.topline = y.saturating_sub(self.height() as usize / 2);
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
            screen_write!(
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

pub struct TextBox {
    pub buf: String,
    topleft: TermPos,
    botright: TermPos,
}

impl TextBox {
    pub fn new() -> Self {
        let (terminal_size::Width(tw), terminal_size::Height(th)) =
            terminal_size().unwrap_or((terminal_size::Width(80), terminal_size::Height(40)));
        Self::new_withdim(TermPos { x: 0, y: 0 }, tw as u32, th as u32)
    }

    pub fn new_withdim(topleft: TermPos, width: u32, height: u32) -> Self {
        Self {
            buf: String::new(),
            topleft: TermPos {
                x: topleft.x,
                y: topleft.y,
            },
            botright: TermPos {
                x: topleft.x + width,
                y: topleft.y + height,
            },
        }
    }

    pub fn draw(&self) -> TermPos {
        term::rst_cur();
        for (i, line) in self
            .buf
            .lines()
            .chain(std::iter::repeat(""))
            .take(self.height() as usize)
            .enumerate()
        {
            term::goto(TermPos {
                x: self.topleft.x,
                y: self.topleft.y + i as u32,
            });
            screen_write!("{: <width$}", line, width = self.width() as usize);
        }
        self.botright
    }

    pub fn resize(&mut self, newx: u32, newy: u32) {
        self.botright.x = self.topleft.x + newx;
        self.botright.y = self.topleft.y + newy;
    }

    pub fn clamp_to_screen(&mut self) {
        let (w, h) = terminal_size::terminal_size().unwrap();
        let (w, h) = (w.0 as u32, h.0 as u32);
        if self.botright.x > w {
            let diff = self.botright.x - w;
            let act = self.topleft.x.saturating_sub(diff);
            self.topleft.x -= act;
            self.botright.x -= act;
        }
        if self.botright.y > h {
            let diff = self.botright.y - h;
            let act = self.topleft.y.saturating_sub(diff);
            self.topleft.y -= act;
            self.botright.y -= act;
        }
    }

    pub fn width(&self) -> u32 {
        self.botright.x - self.topleft.x
    }

    pub fn height(&self) -> u32 {
        self.botright.y - self.topleft.y
    }

    fn reltoabs(&self, pos: TermPos) -> TermPos {
        TermPos {
            x: pos.x + self.topleft.x,
            y: pos.y + self.topleft.y,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn basic_context() -> Ctx {
        let b = Buffer::from_str("0\n1\n22\n333\n4444\n\nnotrnc\ntruncated line");
        let mut ctx = Ctx::new_testing(b);
        let bufid = ctx.window.buf_ctx.buf_id;
        ctx.window = Window {
            buf_ctx: BufCtx::new(bufid),
            topleft: TermPos { x: 0, y: 0 },
            botright: TermPos { x: 7, y: 32 },
            components: vec![],
            padding: Padding::default(),
            dirty: false,
        };
        ctx
    }

    fn scroll_context() -> Ctx {
        let b = Buffer::from_str("0\n1\n22\n333\n4444\n55555\n\n\n\n\n\n\n\nLast");
        let mut ctx = Ctx::new_testing(b);
        let bufid = ctx.window.buf_ctx.buf_id;
        ctx.window = Window {
            buf_ctx: BufCtx::new(bufid),
            topleft: TermPos { x: 0, y: 0 },
            botright: TermPos { x: 7, y: 10 },
            components: vec![],
            padding: Padding::default(),
            dirty: false,
        };
        ctx
    }

    fn blank_context() -> Ctx {
        let b = Buffer::from_str("0\n1\n22\n333\n4444\n\nnotrnc\ntruncated line");
        let mut ctx = Ctx::new_testing(b);
        let bufid = ctx.window.buf_ctx.buf_id;
        ctx.window = Window {
            buf_ctx: BufCtx::new(bufid),
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
