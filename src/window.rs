use crate::debug::{log, sleep};
use crate::prelude::*;
use crate::render::BufId;
use crate::tui::TermBox;
use std::fmt::Write;

use crate::buffer::DocPos;
use crate::render::Ctx;
use crate::term;
use crate::term::TermPos;

use enum_dispatch::enum_dispatch;
use terminal_size::terminal_size;
use unicode_truncate::UnicodeTruncateStr;

#[derive(Default, Debug)]
struct Padding {
    top: u32,
    bottom: u32,
    left: u32,
    right: u32,
}

#[enum_dispatch]
trait DispComponent {
    /// write the component
    fn draw(&self, win: &Window, buffer: &Buffer, ctx: &Ctx);

    /// amount of padding needed left, top, bottom, right
    fn padding(&self) -> Padding;
}

#[enum_dispatch(DispComponent)]
pub enum Component {
    RelLineNumbers,
    StatusLine,
    Welcome,
    CommandPrefix,
}

pub struct RelLineNumbers;
impl DispComponent for RelLineNumbers {
    fn draw(&self, win: &Window, buffer: &Buffer, ctx: &Ctx) {
        let linecnt = buffer.linecnt();
        let y = buffer.cursor.win_pos(win).y;
        let mut tui = ctx.tui.borrow_mut();

        for l in 0..win.height() {
            let winbase = win.reltoabs(TermPos { x: 0, y: l });

            let mut target = tui
                .refline(winbase.y, (winbase.x - 5)..(winbase.x))
                .colored(Color {
                    fg: BasicColor::Green,
                    ..Color::new()
                });

            // write!(target, "X").unwrap();
            // continue;

            if l == y {
                write!(target, " {:<3} ", l as usize + buffer.cursor.topline + 1).unwrap();
            } else if l as usize + buffer.cursor.topline < linecnt {
                write!(target, "{:>4} ", y.abs_diff(l)).unwrap();
            } else {
                write!(target, "{:5}", ' ').unwrap();
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
    fn draw(&self, win: &Window, _buffer: &Buffer, ctx: &Ctx) {
        if !win.dirty {
            let s = include_str!("../assets/welcome.txt");
            let top = (win.height() - s.lines().count() as u32) / 2;
            let mut target = ctx.tui.borrow_mut();
            s.lines()
                .enumerate()
                .map(|(idx, line)| {
                    let mut refline = target.refline(top + idx as u32, ..);
                    write!(refline, "{:^w$}", line, w = win.width() as usize).unwrap();
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

pub struct CommandPrefix;
impl DispComponent for CommandPrefix {
    fn draw(&self, win: &Window, _buffer: &Buffer, ctx: &Ctx) {
        use crate::command::cmdline::CommandType;
        let base = win.reltoabs(TermPos { x: 0, y: 0 });
        let lead = match ctx.cmdtype() {
            CommandType::Ex => ':',
            CommandType::None => ' ',
            CommandType::Find => '/',
        };

        let mut target = ctx.tui.borrow_mut();
        target.put_cell(
            TermPos {
                x: base.x - 1,
                y: base.y,
            },
            lead,
        );
    }

    fn padding(&self) -> Padding {
        Padding {
            top: 0,
            bottom: 0,
            left: 1,
            right: 0,
        }
    }
}

pub struct StatusLine;
impl DispComponent for StatusLine {
    fn padding(&self) -> Padding {
        Padding {
            top: 1,
            bottom: 0,
            left: 0,
            right: 0,
        }
    }

    fn draw(&self, win: &Window, buffer: &Buffer, ctx: &Ctx) {
        let base = win.reltoabs(TermPos { x: 0, y: 0 });

        let (color, mode_str) = match ctx.mode {
            crate::Mode::Normal => (
                Color {
                    fg: BasicColor::Black,
                    bg: BasicColor::Green,
                    bold: true,
                },
                " NORMAL ",
            ),
            crate::Mode::Insert => (
                Color {
                    fg: BasicColor::Black,
                    bg: BasicColor::Blue,
                    bold: true,
                },
                " INSERT ",
            ),
            crate::Mode::Command => (
                Color {
                    fg: BasicColor::Black,
                    bg: BasicColor::Blue,
                    bold: true,
                },
                " COMMAND ",
            ),
        };
        let mut target = ctx.tui.borrow_mut();
        let w = target.dim().0;
        let y = base.y - 1;
        let mut refline = target.refline(y, ..).colored(color);
        write!(refline, "{mode_str}").unwrap();
        let name = buffer.name();
        refline.set_color(Color {
            bg: BasicColor::Black,
            ..Color::default()
        });
        write!(refline, " {name}").unwrap();
        let _ = write!(refline, "{:x$}", "", x = w as usize);
    }
}

pub struct Window {
    bounds: TermBox,
    components: Vec<Component>,
    padding: Padding,
    dirty: bool,
}

impl Window {
    pub fn new(tui: &TermGrid) -> Self {
        let components = vec![Component::RelLineNumbers(RelLineNumbers)];
        let (w, h) = tui.dim();
        Self::new_withdim(TermPos { x: 0, y: 0 }, w, h, components)
    }

    pub fn new_withdim(
        topleft: TermPos,
        width: u32,
        height: u32,
        mut components: Vec<Component>,
    ) -> Self {
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
        let out = Self {
            bounds: TermBox {
                start: TermPos {
                    x: topleft.x + padding.left,
                    y: topleft.y + padding.top,
                },
                end: TermPos {
                    x: topleft.x + width - padding.right - 1,
                    y: topleft.y + height - padding.bottom - 1,
                },
            },
            components,
            padding,
            dirty,
        };
        out.bounds.assert_valid();
        out
    }

    pub fn bounds(&self) -> TermBox {
        self.bounds
    }

    fn real_bounds(&self) -> TermBox {
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
        self.bounds.assert_valid();
    }

    /// clamp the window to the screen, moving the window and also shrinking if necessary.
    pub fn clamp_to_screen(&mut self, tui: &TermGrid) {
        let (tw, th) = tui.dim();
        let real = self.real_bounds();
        let w = real.xlen().min(tw);
        let h = real.ylen().min(th);
        // std::thread::sleep(std::time::Duration::from_secs(10));
        self.set_size_padded(w, h);
        if real.end.x >= tw {
            let diff = (real.end.x - tw) + 1;
            self.bounds.end.x -= diff;
            self.bounds.start.x -= diff;
        }
        if real.end.y >= th {
            let diff = (real.end.y - th) + 1;
            self.bounds.end.y -= diff;
            self.bounds.start.y -= diff;
        }
    }

    /// snap the window to the bottom of the screen
    pub fn snap_to_bottom(&mut self, tui: &TermGrid) {
        let (_, h) = tui.dim();
        self.bounds.start.y += h;
        self.bounds.end.y += h;
        self.clamp_to_screen(tui);
    }

    pub fn width(&self) -> u32 {
        self.bounds.xlen()
    }

    pub fn height(&self) -> u32 {
        self.bounds.ylen()
    }

    fn reltoabs(&self, pos: TermPos) -> TermPos {
        // log!("{:?} + {pos:?}", self.bounds);
        // sleep(10);
        TermPos {
            x: pos.x + self.bounds.start.x,
            y: pos.y + self.bounds.start.y,
        }
    }

    pub fn draw_buf(&self, ctx: &Ctx, buf: &Buffer) {
        self.draw_buf_colored(ctx, buf, Color::default());
    }

    pub fn draw_buf_colored(&self, ctx: &Ctx, buf: &Buffer, color: Color) {
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
                // log!("{line:?}");
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

    pub fn move_cursor(&mut self, buf: &mut Buffer, dx: isize, dy: isize) {
        let newy = buf
            .cursor
            .pos
            .y
            .saturating_add_signed(dy)
            .clamp(0, buf.linecnt().saturating_sub(1));
        let line = &buf.get_lines(newy..(newy + 1))[0];
        let newx = buf
            .cursor
            .virtpos
            .x
            .saturating_add_signed(dx)
            .clamp(0, line.len());

        if dx != 0 {
            buf.cursor.virtpos.x = newx;
        }
        buf.cursor.virtpos.y = newy;

        buf.cursor.pos.x = newx;
        buf.cursor.pos.y = newy;
        self.fit_ctx_frame(&mut buf.cursor);
    }

    pub fn set_pos(&mut self, buf: &mut Buffer, pos: DocPos) {
        let newy = pos.y.clamp(0, buf.linecnt().saturating_sub(1));
        buf.cursor.pos.y = newy;
        buf.cursor.virtpos.y = newy;
        let line = &buf.get_lines(newy..(newy + 1))[0];
        buf.cursor.pos.x = pos.x.clamp(0, line.len());
        buf.cursor.virtpos.x = buf.cursor.pos.x;
        self.fit_ctx_frame(&mut buf.cursor);
    }

    pub fn fit_ctx_frame(&mut self, cursor: &mut Cursor) {
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

    fn basic_context() -> Ctx {
        let b = Buffer::from_str("0\n1\n22\n333\n4444\n\nnotrnc\ntruncated line");
        let mut ctx = Ctx::new_testing(b);
        ctx.window = Window {
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
        ctx.window = Window {
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
        ctx.window = Window {
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
}
