use std::{ops::{RangeInclusive, Range, RangeBounds}, fmt::Write};
use crate::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Ord)]
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

/// shorthand for term pos (x, y)
macro_rules! tp {
    ($x:expr, $y:expr) => {
        TermPos{ x: $x, y: $y}
    };
}

impl PartialOrd for TermPos {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let ycmp = self.y.cmp(&other.y);
        if matches!(ycmp, std::cmp::Ordering::Equal) {
            Some(self.x.cmp(&other.x))
        } else {
            Some(ycmp)
        }
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermBox {
    pub start: TermPos,
    pub end: TermPos,
}

impl std::ops::RangeBounds<TermPos> for TermBox {
    fn start_bound(&self) -> std::ops::Bound<&TermPos> {
        std::ops::Bound::Included(&self.start)
    }

    fn end_bound(&self) -> std::ops::Bound<&TermPos> {
        std::ops::Bound::Included(&self.end)
    }
}

impl TermBox {
    const fn xrng (&self) -> RangeInclusive<u32> {
        self.start.x..=self.end.x
    }
    const fn yrng (&self) -> RangeInclusive<u32> {
        self.start.y..=self.end.y
    }
    fn from_ranges(xrng: impl RangeBounds<u32>, yrng: impl RangeBounds<u32> ) -> Self {
        let xrng = TermGrid::rangebounds_to_range(xrng);
        let yrng = TermGrid::rangebounds_to_range(yrng);
        Self {
            start: tp!(xrng.start, yrng.start),
            end: tp!(xrng.end - 1, yrng.end - 1)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasicColor {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Gray,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub bold: bool,
    pub fg: BasicColor,
    pub bg: BasicColor,
}

impl Color {
    pub const fn new() -> Self  {
        Self {
            bold: false,
            fg: BasicColor::Default,
            bg: BasicColor::Default,
        }
    }

    const fn fg(&self) -> u8 {
        match self.fg {
            BasicColor::Default => 39,
            BasicColor::Black => 30,
            BasicColor::Red => 31,
            BasicColor::Green => 32,
            BasicColor::Yellow => 33,
            BasicColor::Blue => 34,
            BasicColor::Magenta => 35,
            BasicColor::Cyan => 36,
            BasicColor::White => 37,
            BasicColor::Gray => 90,
            BasicColor::BrightRed => 91,
            BasicColor::BrightGreen => 92,
            BasicColor::BrightYellow => 93,
            BasicColor::BrightBlue => 94,
            BasicColor::BrightMagenta => 95,
            BasicColor::BrightCyan => 96,
            BasicColor::BrightWhite => 97,
        }
    }

    const fn bg(&self) -> u8 {
        match self.bg {
            BasicColor::Default => 49,
            BasicColor::Black => 40,
            BasicColor::Red => 41,
            BasicColor::Green => 42,
            BasicColor::Yellow => 43,
            BasicColor::Blue => 44,
            BasicColor::Magenta => 45,
            BasicColor::Cyan => 46,
            BasicColor::White => 47,
            BasicColor::Gray => 100,
            BasicColor::BrightRed => 101,
            BasicColor::BrightGreen => 102,
            BasicColor::BrightYellow => 103,
            BasicColor::BrightBlue => 104,
            BasicColor::BrightMagenta => 105,
            BasicColor::BrightCyan => 106,
            BasicColor::BrightWhite => 107,
        }
    }

    const fn bold(&self) -> u8 {
        if self.bold {
            1
        } else {
            22
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TermCell {
    color: Color,
    content: Option<char>,
}

impl TermCell {
    const fn new() -> Self {
        Self {
            color: Color::new(),
            content: None,
        }
    }
}

impl Default for TermCell {
    fn default() -> Self {
        Self::new()
    }
}

impl From<char> for TermCell {
    fn from(value: char) -> Self {
        TermCell { color: Color::default(), content: Some(value) }
    }
}

pub struct TermGrid {
    w: u32,
    h: u32,
    cells: Vec<TermCell>,
    cursorpos: TermPos,
}

impl std::ops::Index<TermPos> for TermGrid {
    type Output = TermCell;

    fn index(&self, index: TermPos) -> &Self::Output {
        let TermPos { x, y } = index;
        assert!(x < self.w);
        assert!(y < self.h);
        &self.cells[(self.w * y + x) as usize]
    }
}

impl std::ops::IndexMut<TermPos> for TermGrid {
    fn index_mut(&mut self, index: TermPos) -> &mut Self::Output {
        let TermPos { x, y } = index;
        assert!(x < self.w);
        assert!(y < self.h);
        &mut self.cells[(self.w * y + x) as usize]
    }
}

impl TermGrid {
    pub fn new() -> Self {
        let mut out = Self { w: 0, h: 0, cells: Vec::new(), cursorpos: tp!(0, 0) };
        out.resize_auto();
        out
    }

    pub fn dim(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    pub fn put_cell(&mut self, pos: TermPos, c: impl Into<TermCell>) {
        self[pos] = c.into();
    }

    /// resize the grid to given dimensions, returns true if resize occured;
    pub fn resize(&mut self, w: u32, h: u32) -> bool {
        if w == self.w && h == self.h {
            return false;
        }
        self.clear();
        self.cells.resize_with((w * h) as usize, || TermCell::new());
        self.w = w;
        self.h = h;
        true
    }

    /// resize the grid to fit the terminal, returns true if resize occurred. 
    pub fn resize_auto(&mut self) -> bool {
        let (w, h) = terminal_size::terminal_size().map_or((80, 40), |(w, h)| (w.0 as u32, h.0 as u32));
        self.resize(w, h)
    }

    pub fn clear(&mut self) {
        self.cells.fill(TermCell::new());
    }

    fn line_rng(&self, y: u32, xrng: impl RangeBounds<u32>) -> Range<usize> {
        let start = match xrng.start_bound() {
            std::ops::Bound::Included(x) => *x,
            std::ops::Bound::Excluded(x) => *x + 1,
            std::ops::Bound::Unbounded => 0,
        };
        let end = match xrng.end_bound() {
            std::ops::Bound::Included(x) => *x + 1,
            std::ops::Bound::Excluded(x) => *x,
            std::ops::Bound::Unbounded => self.w,
        };
        assert!(start <= end);
        assert!(end <= self.w);
        let yoff = y * self.w;
        assert!(yoff + end <= self.w * self.h);
        ((yoff + start) as usize)..((yoff + end) as usize)
    }

    pub fn clear_bounds(&mut self, bounds: TermBox) {
        for y in bounds.yrng() {
            let rng = self.line_rng(y, bounds.xrng());
            self.cells[rng].fill(TermCell::new());
        }
    }

    fn rangebounds_to_range(range: impl RangeBounds<u32>) -> Range<u32> {
        match (range.start_bound(), range.end_bound()) {
            (std::ops::Bound::Included(start), std::ops::Bound::Included(end)) => *start..(*end + 1),
            (std::ops::Bound::Included(start), std::ops::Bound::Excluded(end)) => *start..*end,
            (std::ops::Bound::Unbounded, _) | (_, std::ops::Bound::Unbounded) => panic!("needs bounds"),
            (std::ops::Bound::Excluded(_), _) => panic!("no excluded start"),
        }
    }

    fn normalize_xrng(&self, xrng: impl RangeBounds<u32>) -> Range<u32> {
        let start = match xrng.start_bound() {
            std::ops::Bound::Included(x) => *x,
            std::ops::Bound::Excluded(x) => *x + 1,
            std::ops::Bound::Unbounded => 0,
        };
        let end = match xrng.start_bound() {
            std::ops::Bound::Included(x) => *x + 1,
            std::ops::Bound::Excluded(x) => *x,
            std::ops::Bound::Unbounded => self.w,
        };
        start..end
    }

    pub fn write_line(&mut self, y: u32, xrng: impl RangeBounds<u32>, color: Color, content: &str) -> usize {
        let mut cnt = 0;
        let mut last = 0;
        let xrng = Self::rangebounds_to_range(xrng);
        for (c, x) in content.chars().zip(xrng.clone()) {
            last = x;
            if c == '\n' {
                break;
            }
            self[tp!(x, y)] = TermCell {
                color,
                content: Some(c),
            };
            cnt += 1;
        };
        let rng = self.line_rng(y, (last + 1)..xrng.end);
        self.cells[rng].fill(TermCell::new());
        cnt
    }

    pub fn line_bounds(&self, y: u32) -> TermBox {
        assert!(y < self.h);
        TermBox { start: tp!(0, y), end: tp!(self.w - 1, y) }
    }

    pub fn write_box(&mut self, bounds: TermBox, color: Color, content: &str) -> usize {
        let mut cnt = 0;
        for (l, y) in content.lines().zip(bounds.yrng()) {
            cnt += self.write_line(y, bounds.xrng(), color, l);
        }
        cnt
    }

    pub fn render(&self, dest: &mut impl std::io::Write) -> std::io::Result<()> {
        let mut curr = Color::new();
        write!(dest, "\x1b[1;1H")?;
        for cell in &self.cells {
            let Some(content) = cell.content else {
                write!(dest, " ")?;
                continue;
            };
            let color = cell.color;
            match (color.fg == curr.fg, color.bg == curr.bg, color.bold == curr.bold) {
                (true, true, true) => (),
                (true, true, false) => write!(dest, "\x1b[{}m", color.bold())?,
                (false, true, true) => write!(dest, "\x1b[{}m", color.fg())?,
                (true, false, true) => write!(dest, "\x1b[{}m", color.bg())?,
                _ => write!(dest, "\x1b[{};{};{}m", color.fg(), color.bg(), color.bold())?,
            }
            curr = color;
            write!(dest, "{}", content)?;
        }
        write!(dest, "\x1b[{};{}H", self.cursorpos.row(), self.cursorpos.col())?;
        Ok(())
    }

    pub fn refbox(&mut self, bounds: TermBox) -> TermGridBox {
        TermGridBox { grid: self, color: Color::new(), range: bounds }
    }

    pub fn refline(&mut self, y: u32, xrng: impl RangeBounds<u32>) -> TermGridBox {
        let xrng = Self::rangebounds_to_range(xrng);
        assert!(y < self.h);
        assert!(xrng.end <= self.w);
        let bounds = TermBox::from_ranges(xrng, y..=y);
        TermGridBox { grid: self, color: Color::new(), range: bounds }
    }

    pub fn set_cursorpos(&mut self, pos: TermPos) {
        assert!(pos.x < self.w);
        assert!(pos.y < self.h);
        self.cursorpos = pos;
    }
}


pub struct TermGridBox<'a> {
    grid: &'a mut TermGrid,
    color: Color,
    range: TermBox,
}

impl TermGridBox<'_> {
    pub const fn colored(self, color: Color) -> Self {
        Self {
            color,
            ..self
        }
    }
}

impl Write for TermGridBox<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.grid.write_box(self.range.clone(), self.color, s);
        Ok(())
    }
}
