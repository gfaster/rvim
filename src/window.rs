mod components;
pub use components::*;
pub mod org;

use crate::debug::{log, sleep};
use crate::prelude::*;
use crate::render::BufId;
use crate::tui::{TermBox, TermSz};
use std::fmt::Write;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::buffer::{Buffer, DocPos};
use crate::render::Ctx;
use crate::term;
use crate::term::TermPos;

use terminal_size::terminal_size;
use unicode_truncate::UnicodeTruncateStr;

#[derive(Default, Debug)]
pub struct Padding {
    top: u32,
    bottom: u32,
    left: u32,
    right: u32,
}

impl Padding {
    const fn sz(&self) -> TermSz {
        TermSz::new(self.w(), self.h())
    }

    const fn w(&self) -> u32 {
        self.left + self.right
    }

    const fn h(&self) -> u32 {
        self.top + self.bottom
    }
}

/// A Window. Equality is done via pointer equality
pub struct Window {
    inner: RwLock<WindowInner>
}

impl Window {
    pub fn get(&self) -> RwLockReadGuard<WindowInner> {
        self.inner.read().unwrap()
    }

    pub fn get_mut(&self) -> RwLockWriteGuard<WindowInner> {
        self.inner.write().unwrap()
    }

    pub fn new(bounds: TermBox, buffer: Arc<Buffer>) -> Arc<Self> {
        let components = vec![Component::RelLineNumbers];
        Self::new_withdim(bounds.start, bounds.sz().w, bounds.sz().h, components, buffer)
    }

    pub fn new_withdim(
        topleft: TermPos,
        width: u32,
        height: u32,
        mut components: Vec<Component>,
        buffer: Arc<Buffer>,
    ) -> Arc<Self> {
        let dirty = true;
        if !dirty {
            components.push(Component::Welcome);
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
        let out = WindowInner {
            bounds: TermBox {
                start: TermPos {
                    x: topleft.x + padding.left,
                    y: topleft.y + padding.top,
                },
                end: TermPos {
                    x: topleft.x + width - padding.right,
                    y: topleft.y + height - padding.bottom,
                },
            },
            components,
            padding,
            dirty,
            next: None,
            prev: None,
            buffer,
        };
        out.bounds.assert_valid();
        let out: Window = out.into();
        out.into()
    }
}

impl std::cmp::PartialEq for Window {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl std::cmp::Eq for Window {}

impl From<WindowInner> for Window {
    fn from(value: WindowInner) -> Self {
        Window { inner: value.into() }
    }
}


pub struct WindowInner {
    pub buffer: Arc<Buffer>,
    pub next: Option<Arc<Window>>,
    pub prev: Option<Arc<Window>>,
    bounds: TermBox,
    components: Vec<Component>,
    padding: Padding,
    dirty: bool,
}

impl WindowInner {

    pub fn inner_bounds(&self) -> TermBox {
        self.bounds
    }

    pub fn outer_bounds(&self) -> TermBox {
        let start = TermPos {
            x: self.bounds.start.x - self.padding.left,
            y: self.bounds.start.y - self.padding.top,
        };
        let end = TermPos {
            x: self.bounds.end.x + self.padding.right,
            y: self.bounds.end.y + self.padding.bottom,
        };
        TermBox { start, end }
    }

    pub fn set_bounds_outer(&mut self, bounds: TermBox) {
        let start = TermPos {
            x: bounds.start.x - self.padding.left,
            y: bounds.start.y - self.padding.top,
        };
        let end = TermPos {
            x: bounds.end.x + self.padding.right,
            y: bounds.end.y + self.padding.bottom,
        };
        self.bounds = TermBox { start, end }
    }

    pub fn set_bounds_inner(&mut self, bounds: TermBox) {
        self.bounds = bounds
    }

    /// do not use directly - should be through window org
    pub fn set_size_outer(&mut self, w: u32, h: u32) {
        let w = w - self.padding.left - self.padding.right;
        let h = h - self.padding.top - self.padding.bottom;
        self.bounds.end.x = self.bounds.start.x + w;
        self.bounds.end.y = self.bounds.start.y + h;
        self.bounds.assert_valid();
    }

    /// do not use directly - should be through window org
    pub fn set_size_inner(&mut self, w: u32, h: u32) {
        self.bounds.end.x = self.bounds.start.x + w;
        self.bounds.end.y = self.bounds.start.y + h;
    }

    /// clamp the window to the screen, moving the window and also shrinking if necessary.
    pub fn clamp_to_bounds(&mut self, bounds: &TermBox) {
        let TermSz {w: tw, h: th} = bounds.sz();
        let real = self.outer_bounds();
        let w = real.xlen().min(tw);
        let h = real.ylen().min(th);
        self.set_size_outer(w, h);
        if real.end.x >= tw {
            assert!(self.padding.w() < tw, "resize too small");
            self.bounds.end.x = tw - self.padding.right;
            self.bounds.start.x = 
                tw.saturating_sub(w) + self.padding.left;
        }
        if real.end.y >= th {
            assert!(self.padding.h() < th, "resize too small");
            self.bounds.end.y = th - self.padding.bottom;
            self.bounds.start.y = 
                th.saturating_sub(h) + self.padding.top;
        }
    }

    /// snap the window to the bottom of the bounds
    pub fn snap_to_bottom(&mut self, bounds: &TermBox) {
        let TermSz { h, .. } = bounds.sz();
        let ch = self.outer_bounds().ylen();
        self.bounds.start.y = h - ch + self.padding.top;
        self.bounds.end.y = h - self.padding.bottom;
        debug_assert_eq!(ch, self.outer_bounds().ylen());
        self.clamp_to_bounds(bounds);
    }

    pub fn width(&self) -> u32 {
        self.bounds.xlen()
    }

    pub fn height(&self) -> u32 {
        self.bounds.ylen()
    }

    fn reltoabs(&self, pos: TermPos) -> TermPos {
        TermPos {
            x: pos.x + self.bounds.start.x,
            y: pos.y + self.bounds.start.y,
        }
    }

    pub fn draw(&self, ctx: &Ctx) {
        self.draw_buf_colored(ctx, &self.buffer.get(), Color::default());
    }

    pub fn draw_colored(&self, ctx: &Ctx, color: Color) {
        self.draw_buf_colored(ctx, &self.buffer.get(), color);
    }

    fn draw_buf(&self, ctx: &Ctx, buf: &BufferInner) {
        self.draw_buf_colored(ctx, buf, Color::default());
    }

    fn draw_buf_colored(&self, ctx: &Ctx, buf: &BufferInner, color: Color) {
        {
            let mut tui = ctx.tui.borrow_mut();
            let range = buf.cursor.topline
                ..(buf.cursor.topline + self.height() as usize).min(buf.linecnt());
            for (y, line) in buf
                .get_lines(range.clone())
                .into_iter()
                .chain(std::iter::repeat(""))
                .take(self.height() as usize)
                .enumerate()
            {
                tui.write_line(
                    y as u32 + self.bounds.start.y,
                    self.bounds.xrng(),
                    color,
                    line,
                );
            }
            buf.cursor.draw(self, &mut tui)
        }
        self.components.iter().for_each(|x| x.draw(self, &buf, ctx));
    }

    pub fn draw_cursor(&self, tui: &mut TermGrid) {
        self.buffer.get().cursor.draw(self, tui)
    }

    pub fn move_cursor(&mut self, dx: isize, dy: isize) {
        let mut buf = self.buffer.get_mut();
        let newy = buf
            .cursor
            .pos
            .y
            .saturating_add_signed(dy)
            .clamp(0, buf.linecnt().saturating_sub(1));
        let line = &buf.line(newy);
        let newx = buf
            .cursor
            .virtcol
            .saturating_add_signed(dx)
            .clamp(0, line.len().saturating_sub(1));

        if dx != 0 {
            buf.cursor.virtcol = newx;
        }

        buf.cursor.pos.x = newx;
        buf.cursor.pos.y = newy;
        self.fit_ctx_frame(&mut buf.cursor);
    }

    pub fn set_pos(&mut self, pos: DocPos) {
        let mut buf = self.buffer.get_mut();
        let newy = pos.y.clamp(0, buf.linecnt().saturating_sub(1));
        buf.cursor.pos.y = newy;
        let line = &buf.line(newy);
        buf.cursor.pos.x = pos.x.clamp(0, line.len());
        buf.cursor.virtcol = buf.cursor.pos.x;
        self.fit_ctx_frame(&mut buf.cursor);
    }

    pub fn fit_ctx_frame(&self, cursor: &mut Cursor) {
        let y = cursor.pos.y;
        let top = cursor.topline;
        let h = self.height() as usize;
        cursor.topline = top.clamp(y.saturating_sub(h - 1), y);
    }

    pub fn center_view(&mut self, cursor: &mut Cursor) {
        let y = cursor.pos.y;
        cursor.topline = y.saturating_sub(self.height() as usize / 2);
    }

    // pub fn insert_char<B: Buffer>(&mut self,
}

#[cfg(test)]
mod test {
    use super::*;

    // fn basic_context() -> Ctx {
    //     let b = BufferInner::from_str("0\n1\n22\n333\n4444\n\nnotrnc\ntruncated line");
    //     let mut ctx = Ctx::new_testing(b);
    //     ctx.window = WindowInner {
    //         bounds: TermBox {
    //             start: TermPos { x: 0, y: 0 },
    //             end: TermPos { x: 7, y: 32 },
    //         },
    //         components: vec![],
    //         padding: Padding::default(),
    //         dirty: false,
    //         next: None,
    //         prev: None,
    //     };
    //     ctx
    // }
    //
    // fn scroll_context() -> Ctx {
    //     let b = BufferInner::from_str("0\n1\n22\n333\n4444\n55555\n\n\n\n\n\n\n\nLast");
    //     let mut ctx = Ctx::new_testing(b);
    //     ctx.window = WindowInner {
    //         bounds: TermBox {
    //             start: TermPos { x: 0, y: 0 },
    //             end: TermPos { x: 7, y: 10 },
    //         },
    //         components: vec![],
    //         padding: Padding::default(),
    //         dirty: false,
    //         next: None,
    //         prev: None,
    //     };
    //     ctx
    // }
    //
    // fn blank_context() -> Ctx {
    //     let b = BufferInner::from_str("0\n1\n22\n333\n4444\n\nnotrnc\ntruncated line");
    //     let mut ctx = Ctx::new_testing(b);
    //     ctx.window = WindowInner {
    //         bounds: TermBox {
    //             start: TermPos { x: 0, y: 0 },
    //             end: TermPos { x: 7, y: 32 },
    //         },
    //         components: vec![],
    //         padding: Padding::default(),
    //         dirty: false,
    //         next: None,
    //         prev: None,
    //     };
    //     ctx
    // }
}
