use std::cell::RefCell;
use std::fmt::Write;
use std::{io::stdout, sync::Mutex};

thread_local! {
    static SCREEN: RefCell<Screen> =  const { RefCell::new(Screen { buf: String::new() }) };
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TermPos {
    pub x: u32,
    pub y: u32,
}

impl TermPos {
    pub fn row(&self) -> u32 {
        self.y + 1
    }

    pub fn col(&self) -> u32 {
        self.x + 1
    }
}

pub struct Screen {
    buf: String,
}

impl Screen {
    pub fn write(args: std::fmt::Arguments) {
        SCREEN.with(|s| {
            s.borrow_mut()
                .buf
                .write_fmt(args)
                .expect("can write to string")
        })
    }
}

#[macro_export]
macro_rules! screen_write {
    ($($tt:tt)*) => {
        $crate::term::Screen::write(format_args!($($tt)*))
    };
}

pub fn rst_cur() {
    screen_write!("\x1b[1;1H");
}

pub fn altbuf_enable() {
    screen_write!("\x1b[?1049h");
}

pub fn altbuf_disable() {
    screen_write!("\x1b[?1049l");
}

pub fn goto(pos: TermPos) {
    screen_write!("\x1b[{};{}H", pos.row(), pos.col());
}

pub fn flush() {
    SCREEN.with(|s| {
        let mut s = s.borrow_mut();
        print!("{}", s.buf);
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        s.buf.clear();
    })
}
