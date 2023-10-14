use std::fmt::Write;
use crate::debug::log;
use crate::prelude::*;
use crate::render::BufId;
use crate::screen_write;
use crate::tui::TermBox;

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
        let mut tui = ctx.tui.borrow_mut();
        let buf = ctx.getbuf(self.buf_id).unwrap();
        write!(tui.refbox(win.bounds), "{}", buf).unwrap();
    }

    pub fn new(buf: BufId) -> Self {
        Self {
            buf_id: buf,
            cursorpos: DocPos { x: 0, y: 0 },
            virtual_pos: DocPos { x: 0, y: 0 },
            topline: 0,
        }
    }

    pub fn new_anon() -> Self {
        Self::new(BufId::new_anon())
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
pub enum Component {
    RelLineNumbers,
    StatusLine,
    Welcome,
}


pub struct RelLineNumbers;
impl DispComponent for RelLineNumbers {
    fn draw(&self, win: &Window, ctx: &Ctx) {
        let linecnt = ctx.getbuf(win.buf_ctx.buf_id).unwrap().linecnt();
        let y = win.cursorpos().y;
        let mut tui = ctx.tui.borrow_mut();

        for l in 0..win.height() {
            let winbase = win.reltoabs(TermPos { x: 0, y: l });

            let mut target = tui.refline(winbase.y, winbase.x..(winbase.x + 5)).colored(Color {
                fg: BasicColor::Green,
                ..Color::new()
            });

            if l == y {
                write!(
                    target,
                    "{: >3} ",
                    l as usize + win.buf_ctx.topline + 1
                ).unwrap();
            } else if l as usize + win.buf_ctx.topline < linecnt {
                write!(target, "{:4}", y.abs_diff(l)).unwrap();
            } else {
                write!(target, "{:4}", ' ').unwrap();
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

pub struct Welcome;
impl DispComponent for Welcome {
    fn draw(&self, win: &Window, ctx: &Ctx) {
        if !win.dirty {
            let s = include_str!("../assets/welcome.txt");
            let top = (win.height() - s.lines().count() as u32) / 2;
            let mut target = ctx.tui.borrow_mut();
            s.lines()
                .enumerate()
                .map(|(idx, line)| {
                    write!(target.refline(top + idx as u32, ..), "{:^w$}", line, w = win.width() as usize).unwrap();
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

pub struct StatusLine;
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
            crate::Mode::Normal => (Color {fg: BasicColor::Black, bg: BasicColor::Green, bold: true}, " NORMAL "),
            crate::Mode::Insert => (Color {fg: BasicColor::Black, bg: BasicColor::Blue, bold: true}, " INSERT "),
            crate::Mode::Command => (Color {fg: BasicColor::Black, bg: BasicColor::Blue, bold: true}, " COMMAND "),
        };
        let mut target = ctx.tui.borrow_mut();
        let y = base.y;
        let end = mode_str.len() as u32;
        let w = target.dim().0;
        write!(target.refline(y, 0..end).colored(color), "{mode_str}").unwrap();
        let start = end;
        let name = ctx.getbuf(win.buf_ctx.buf_id).unwrap().name();
        let end = end + name.len() as u32 + 1;
        write!(target.refline(y, start..end).colored(Color { bg: BasicColor::Black, ..Color::default() }), " {name}").unwrap();
        let start = end;
        write!(target.refline(y, start..).colored(Color { bg: BasicColor::Black, ..Color::default()
        }), "{:x$}", "", x=w as usize).unwrap();
    }
}

pub struct Window {
    pub buf_ctx: BufCtx,
    bounds: TermBox,
    components: Vec<Component>,
    padding: Padding,
    dirty: bool,
}

impl Window {
    pub fn new(buf: BufId, tui: &TermGrid) -> Self {
        let components = vec![
            Component::RelLineNumbers(RelLineNumbers),
        ];
        let (w, h) = tui.dim();
        Self::new_withdim(buf, TermPos { x: 0, y: 0 }, w, h, components)
    }

    pub fn new_withdim(buf: BufId, topleft: TermPos, width: u32, height: u32, mut components: Vec<Component>) -> Self {
        // let mut components = vec![
        //     Component::RelLineNumbers(RelLineNumbers),
        //     Component::StatusLine(StatusLine),
        // ];
        let dirty = true;
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
            bounds: TermBox { 
                start: TermPos {
                    x: topleft.x + padding.left,
                    y: topleft.y + padding.top,
                },
                end: TermPos {
                    x: width - padding.right - 1,
                    y: height - padding.bottom - 1,
                },
            },
            components,
            padding,
            dirty,
        }
    }

    /// probably don't want to use this since it erases padding
    pub fn set_size(&mut self, newx: u32, newy: u32) {
        self.bounds.end.x = self.bounds.start.x + newx;
        self.bounds.end.y = self.bounds.start.y + newy;
    }

    pub fn set_size_padded(&mut self, newx: u32, newy: u32) {
        let w = newx - self.padding.left - self.padding.right;
        let h = newy - self.padding.top - self.padding.bottom;
        self.bounds.end.x = self.bounds.start.x + w - 1;
        self.bounds.end.y = self.bounds.start.y + h - 1;
    }

    pub fn width(&self) -> u32 {
        self.bounds.end.x - self.bounds.start.x
    }

    pub fn height(&self) -> u32 {
        self.bounds.end.y - self.bounds.start.y
    }

    fn reltoabs(&self, pos: TermPos) -> TermPos {
        TermPos {
            x: pos.x + self.bounds.start.x,
            y: pos.y + self.bounds.start.y,
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

    pub fn fit_ctx_frame(&mut self) {
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

#[cfg(test)]
mod test {
    use super::*;

    fn basic_context() -> Ctx {
        let b = Buffer::from_str("0\n1\n22\n333\n4444\n\nnotrnc\ntruncated line");
        let mut ctx = Ctx::new_testing(b);
        let bufid = ctx.window.buf_ctx.buf_id;
        ctx.window = Window {
            buf_ctx: BufCtx::new(bufid),
            bounds: TermBox {
                start: TermPos { x: 0, y: 0 },
                end: TermPos { x: 7, y: 32 },
            },
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
            bounds: TermBox {
                start: TermPos { x: 0, y: 0 },
                end: TermPos { x: 7, y: 10 },
            },
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
            bounds: TermBox {
                start: TermPos { x: 0, y: 0 },
                end: TermPos { x: 7, y: 32 },
            },
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
