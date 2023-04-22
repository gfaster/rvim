use std::io::{self, Write};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TermPos {
    pub x: u32,
    pub y: u32
}

impl TermPos {
    pub fn row(&self) -> u32 {
        self.y + 1
    }

    pub fn col(&self) -> u32 {
        self.x + 1
    }
}

pub fn rst_cur() {
    print!("\x1b[1;1H");
}

pub fn altbuf_enable() {
    print!("\x1b[?1049h");
}

pub fn altbuf_disable() {
    print!("\x1b[?1049l");
}

pub fn goto(pos: TermPos) {
    print!("\x1b[{};{}H", pos.row(), pos.col());
}

pub fn flush() {
    io::stdout().flush().unwrap();
}

