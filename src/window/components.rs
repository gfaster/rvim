use crate::window::Padding;
use std::fmt::Write;
use crate::tui::TermPos;
use crate::window::WindowInner;
use crate::prelude::*;


pub trait DispComponent {
    /// write the component
    fn draw(&self, win: &WindowInner, buffer: &BufferInner, ctx: &Ctx);

    /// amount of padding needed left, top, bottom, right
    fn padding(&self) -> Padding;
}

pub enum Component {
    RelLineNumbers,
    StatusLine,
    Welcome,
    CommandPrefix,
}

impl DispComponent for Component {
    fn draw(&self, win: &WindowInner, buffer: &BufferInner, ctx: &Ctx) {
        match self {
            Component::RelLineNumbers => RelLineNumbers.draw(win, buffer, ctx),
            Component::StatusLine => StatusLine.draw(win, buffer, ctx),
            Component::Welcome => Welcome.draw(win, buffer, ctx),
            Component::CommandPrefix => CommandPrefix.draw(win, buffer, ctx),
        }
    }

    fn padding(&self) -> Padding {
        match self {
            Component::RelLineNumbers => RelLineNumbers.padding(),
            Component::StatusLine => StatusLine.padding(),
            Component::Welcome => Welcome.padding(),
            Component::CommandPrefix => CommandPrefix.padding(),
        }
    }
}

pub struct RelLineNumbers;
impl DispComponent for RelLineNumbers {
    fn draw(&self, win: &WindowInner, buffer: &BufferInner, ctx: &Ctx) {
        let linecnt = buffer.linecnt();
        let y = buffer.cursor.win_pos(win).y;
        let mut tui = ctx.tui.borrow_mut();

        for l in 0..win.height() {
            let winbase = win.reltoabs(TermPos { x: 0, y: l });

            let mut target = tui
                .refline(winbase.y, (winbase.x - 5)..(winbase.x));

            // write!(target, "X").unwrap();
            // continue;
            let fg = BasicColor::Green;
            let bg = BasicColor::Default;
            if l == y {
                target.set_color(Color { fg, bg, ..Color::new()});
                write!(target, " {:<3} ", l as usize + buffer.cursor.topline + 1).unwrap();
            } else if l as usize + buffer.cursor.topline < linecnt {
                target.set_color(Color { fg, bg, ..Color::new()});
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
    fn draw(&self, win: &WindowInner, _buffer: &BufferInner, ctx: &Ctx) {
        if !win.dirty {
            let s = include_str!("../../assets/welcome.txt");
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
    fn draw(&self, win: &WindowInner, _buffer: &BufferInner, ctx: &Ctx) {
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

    fn draw(&self, win: &WindowInner, _buffer: &BufferInner, ctx: &Ctx) {
        let base = win.reltoabs(TermPos { x: 0, y: 0 });

        let (color, mode_str) = match ctx.mode {
            crate::Mode::Normal => (
                Color {
                    fg: BasicColor::Black,
                    bg: BasicColor::Blue,
                    bold: true,
                },
                " NORMAL ",
            ),
            crate::Mode::Insert => (
                Color {
                    fg: BasicColor::Black,
                    bg: BasicColor::Yellow,
                    bold: true,
                },
                " INSERT ",
            ),
            crate::Mode::Command => (
                Color {
                    fg: BasicColor::Black,
                    bg: BasicColor::Green,
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
        let buf = ctx.focused_buf();
        let name = buf.name();
        refline.set_color(Color {
            bg: BasicColor::Black,
            ..Color::default()
        });
        write!(refline, " {name}").unwrap();
        let _ = write!(refline, "{:x$}", "", x = w as usize);
    }
}
