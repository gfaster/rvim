pub use crate::tui::TermPos;
use std::cell::RefCell;
use std::fmt::Write;
use std::{io::stdout, sync::Mutex};

pub fn altbuf_enable() {
    print!("\x1b[?1049h");
}

pub fn altbuf_disable() {
    print!("\x1b[?1049l");
}

pub fn goto(_pos: TermPos) {
    // screen_write!("\x1b[{};{}H", pos.row(), pos.col());
}

pub fn flush() {
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
}
