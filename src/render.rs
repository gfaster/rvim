#![allow(unused)]

use crate::Wrapping;
use std::{borrow::Cow, fmt::Display, ops::Range};
use textwrap::{self, wrap};
use unic_segment::{GraphemeIndices, Graphemes};

enum DisplayMode {
    Ascii,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DocPos {
    line: usize,
    col: usize,
}

impl From<TermPos> for DocPos {
    fn from(value: TermPos) -> Self {
        Self {
            line: value.0 as usize,
            col: value.1 as usize,
        }
    }
}

/// 0-indexed terminal position in the form of (line, col)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TermPos(u16, u16);

#[derive(Clone, Copy, Debug)]
struct WinSpan {
    tl: TermPos,
    br: TermPos,
}

impl WinSpan {
    fn vspan(&self) -> Range<u16> {
        self.tl.0..self.br.0
    }
    fn hspan(&self) -> Range<u16> {
        self.tl.1..self.br.1
    }
    fn contains(&self, pos: &TermPos) -> bool {
        self.hspan().contains(&pos.1) && self.vspan().contains(&pos.0)
    }
    fn width(&self) -> u16 {
        self.hspan().len() as u16
    }
    fn height(&self) -> u16 {
        self.vspan().len() as u16
    }
    fn all(&self) -> impl Iterator<Item = TermPos> + '_ {
        self.vspan()
            .flat_map(move |r| self.hspan().map(move |c| TermPos(r, c)))
    }
    fn from_hvspan(hspan: Range<u16>, vspan: Range<u16>) -> Self {
        Self {
            tl: TermPos(vspan.start, hspan.start),
            br: TermPos(vspan.end, hspan.end),
        }
    }
    fn from_size(w: u16, h: u16) -> Self {
        Self {
            tl: TermPos(0, 0),
            br: TermPos(h, w),
        }
    }
}


/// Decode byte slices to valid utf8
///
/// # Examples
/// ```
/// # use edit::render::decode;
/// let s = [65, 66, 67, 0xD8, 0x00, 68];
/// assert_eq!(&decode(&s), "ABC<0xd8><0x00>D");
/// ```
pub fn decode(s: &[u8]) -> String {
    let mut ret = String::new();
    let mut sv = s.clone();
    loop {
        match std::str::from_utf8(sv) {
            Ok(valid) => {
                ret.push_str(valid);
                break;
            }
            Err(e) => {
                let (valid, invalid) = sv.split_at(e.valid_up_to());

                // TODO: use from_utf8_unchecked
                ret.push_str(std::str::from_utf8(valid).unwrap());

                if let Some(invalid_len) = e.error_len() {
                    ret.push_str(&escape_seq(&invalid[..invalid_len]));
                    sv = &invalid[invalid_len..];
                } else {
                    break;
                };
            }
        }
    }
    escape_noprint(&ret)
}

/// Escape non-printable characters
/// Does not escape newlines
///
/// # Examples
/// ```
/// # use edit::render::escape_noprint;
/// let s = "\t\x1b]0mhello,\0world\n";
/// assert_eq!(escape_noprint(s), "    <0x1b>]0mhello,<0x00>world\n")
pub fn escape_noprint(s: &str) -> String {
    s.chars()
        .map(|x| {
            if !x.is_whitespace() && x.is_control() {
                escape_char(x as u32)
            } else if x == '\t' {
                "    ".into()
            } else {
                x.into()
            }
        })
        .collect()
}

/// Escape a slice of (possibly invalid utf-8) bytes as hex
///
/// # Examples
/// ```
/// # use edit::render::escape_seq;
/// let c = "abc".as_bytes();
/// assert_eq!("<0x61><0x62><0x63>".to_string(), escape_seq(c));
/// ```
pub fn escape_seq(c: &[u8]) -> String {
    c.iter().map(|x| escape_char(*x as u32)).collect()
}

/// Escape a single character as hex
///
/// # Examples
/// ```
/// # use edit::render::escape_char;
/// let c = '\n';
/// let heart = '❤';
/// assert_eq!("<0x0a>".to_string(), escape_char(c as u32));
/// assert_eq!("<0x2764>".to_string(), escape_char(heart as u32));
/// ```
pub fn escape_char(c: u32) -> String {
    format!("<0x{:02x}>", c)
}

struct Window {
    buf: Buffer,
    span: WinSpan,
    screen_buf: Vec<Cell>,
    selected: bool
}

impl Window {
    fn render(&self) -> Vec<Cell> {
        let w = self.span.width();
        let h = self.span.height();
        self.buf.render_cells(w as usize, h as usize).into_iter().map(|x| x.into()).collect()
    }

    fn render_cache(&mut self) {
        let w = self.span.width();
        let h = self.span.height();
        self.screen_buf = self.buf.render_cells(w as usize, h as usize).into_iter().map(|x| x.into()).collect();
    }

    fn new(span: WinSpan) -> Self {
        Self {
            buf: Buffer::new([].to_vec()),
            span,
            screen_buf: vec![],
            selected: false
        }
    }
}

impl Display for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let w = self.span.width();
        let h = self.span.height();
        self.buf.render(w as usize, h as usize).fmt(f)
    }
}

enum Color {
    Normal,
    Red
}

struct CellStyle {
    fg: Color,
    bg: Color,
    bold: bool,
    italic: bool,
}

impl CellStyle {
    fn new() -> Self {
        CellStyle { fg: Color::Normal, bg: Color::Normal, bold: false, italic: false }
    }
}

/// contents of a cell. Needs to be able to accomidate arbitrarily large strings
/// for a single cell because unicode is hard
enum CellCont {
    Char(char),
    Long(String)
}

impl CellCont {
    fn from_str(s: &str) -> Vec<Self> {
        Graphemes::new(&escape_noprint(s)).map(|x| match x.chars().count() {
            0 => panic!("0 length grapheme: {}", x),
            1 => CellCont::Char(x.chars().next().expect("length 1 is nonempty")),
            _ => CellCont::Long(x.into())
        }).collect()
    }
}

impl From<CellCont> for Cell {
    fn from(value: CellCont) -> Self {
        Cell { style: CellStyle::new(), content: value }
    }
}

impl Display for CellCont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CellCont::Char(c) => c.fmt(f),
            CellCont::Long(s) => s.fmt(f)
        }
    }
}

struct Cell {
    style: CellStyle,
    content: CellCont
}

impl Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.content.fmt(f)
    }
}

struct EscapedChars<'a> {
    idx: usize,
    raw: &'a [u8],
    esc: Option<String>
}

impl EscapedChars<'_> {
    fn new(raw: &[u8]) -> Self {
        Self { idx: 0, raw, esc: None }
    }
}

impl Iterator for EscapedChars<'_> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(esc) = self.esc {
            let mut chars = esc.chars();
            let ret = chars.next()?;
            if chars.next() == None {
                self.esc = None;
            }
            return Some(ret);
        }
        let remain = self.raw.len() as isize - self.idx as isize;
        if remain <= 0 {
            return None
        };
        let pointlen = self.raw[self.idx].leading_ones();
        // TODO: turn this into a folding iterator
        let optc = match pointlen {
            0 => {
                self.idx += 1;
                char::from_u32(self.raw[self.idx] as u32)
            },
            2 => {
                self.idx += 2;
                char::from_u32((self.raw[self.idx + 1] as u32) << 8 | self.raw[self.idx] as u32)
            },
            3 => {
                self.idx += 3;
                char::from_u32((self.raw[self.idx + 2] as u32) << 16 | (self.raw[self.idx + 1] as u32) << 8 | self.raw[self.idx] as u32)
            },
            4 => {
                self.idx += 4;
                char::from_u32((self.raw[self.idx + 3] as u32) << 32 |(self.raw[self.idx + 2] as u32) << 16 | (self.raw[self.idx + 1] as u32) << 8 | self.raw[self.idx] as u32)
            },
                _ => None
        };

        match optc {
            Some(_) => optc,
            None => {
                self.esc = Some(escape_char(self.raw[self.idx] as u32));
                self.idx += 1;
                self.next()
            }
        }
    }
}


struct Workspace {
    winv: Vec<Window>,
    span: WinSpan,
    focidx: usize
}

impl Workspace {
    fn new(span: WinSpan) -> Self {
        let mut winv = vec![Window::new(span)];
        winv[0].selected = true;
        Self { winv, span, focidx: 0 }
    }

    fn add_window(&mut self, win: Window) {
        self.winv.push(win);
    }

    fn render(&mut self) {
        self.winv.iter_mut().map(|x| x.render_cache()).last();
    }
}

impl Display for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut v: Vec<_> = g.iter().map(|x| x.iter()).collect();

        for row in self.span.vspan() {
            for col in self.span.hspan() {
                let pos = TermPos(row, col);
                let c = v
                    .iter_mut()
                    .enumerate()
                    .filter(|(i, _)| self.winv[*i].span.contains(&pos))
                    .map(|(_, it)| it.next().unwrap_or(&" "))
                    .last()
                    .unwrap_or(&" ");
                write!(f, "{}", c)?
            }

            if row + 1 != self.span.br.0 {
                write!(f, "\n")?;
            }
        }
        Ok(())
    }
}

struct Buffer {
    raw: Vec<u8>,
    cursor: DocPos,
    top: DocPos,
    loff: Vec<usize>,
    wrapping: Wrapping,
}

impl Buffer {
    fn new(raw: Vec<u8>) -> Self {
        let cursor = DocPos { line: 0, col: 0 };
        let top = DocPos { line: 0, col: 0 };

        // we can do this because ascii does not exist in unicode
        let loff = [0].into_iter().chain(raw.iter().enumerate().filter(|(_, b)| **b == 0x0A).map(|(i, _)| i)).collect();
        let wrapping = Wrapping::Word;
        Self {
            raw,
            cursor,
            top,
            wrapping,
            loff,
        }
    }

    fn render(&self, w: usize, h: usize) -> String {
        let h_efcv = h.min(self.loff.len());
        let idxr = self.loff[self.top.line]..self.loff[self.top.line + h_efcv];
        wrap(&decode(&self.raw[idxr]), w).into_iter().map(|x| format!("{:<w$}", x)).collect::<Vec<_>>().join("\n")
    }

    fn render_cells(&self, w: usize, h: usize) -> Vec<CellCont> {
        let h_efcv = h.min(self.loff.len());
        let idxr = self.loff[self.top.line]..self.loff[self.top.line + h_efcv];
        wrap(&decode(&self.raw[idxr]), w).into_iter().flat_map(|x| CellCont::from_str(&format!("{:<w$}", x)).into_iter())
        .collect()
    }

    fn scroll_abs(&mut self, newl: usize) {
        self.top.line = newl;
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic_box_render_invariant() {
        let truth = include_str!("crossbox.txt");
        let mut test = Buffer::new(include_bytes!("crossbox.txt").to_vec());
        assert_eq!(truth.trim_end().to_owned(), test.render(31, 31));
    }

    #[test]
    fn buffer_pad_horizontal() {
        let init = "1\n22\n333\n4444\n";
        let truth = "1   \n22  \n333 \n4444";
        let mut test = Buffer::new(init.as_bytes().to_vec());
        assert_eq!(truth.to_owned(), test.render(4, 4));
    }

    #[test]
    fn buffer_pad_vertical() {
        let init = "1\n22\n333";
        let truth = "1   \n22  \n333 \n    ";
        let mut test = Buffer::new(init.as_bytes().to_vec());
        assert_eq!(truth.to_owned(), test.render(4, 4));
    }

    #[test]
    fn buffer_empty() {
        let truth = "    \n    \n    \n    ";
        let mut test = Buffer::new([].to_vec());
        assert_eq!(truth.to_owned(), test.render(4, 4));
    }

    #[test]
    fn scroll_abs_one() {
        let init = "1\n22\n333";
        let truth = "22  \n333 \n    \n    ";
        let mut test = Buffer::new(init.as_bytes().to_vec());
        test.scroll_abs(1);
        assert_eq!(truth.to_owned(), test.render(4, 4));
    }

    #[test]
    fn scroll_abs_end() {
        let init = "1\n22\n333";
        let truth = "333 \n    \n    \n    ";
        let mut test = Buffer::new(init.as_bytes().to_vec());
        test.scroll_abs(2);
        assert_eq!(truth.to_owned(), test.render(4, 4));
    }

    #[test]
    fn scroll_abs_past_end() {
        let init = "1\n22\n333";
        let truth = "333 \n    \n    \n    ";
        let mut test = Buffer::new(init.as_bytes().to_vec());
        test.scroll_abs(12);
        assert_eq!(truth.to_owned(), test.render(4, 4));
    }

    #[test]
    fn windspan_no_mixup_dim() {
        let w = 8;
        let h = 16;
        let pos_in = TermPos(12, 4);
        let winspan = WinSpan::from_size(w, h);
        assert!(winspan.contains(&pos_in));
        assert_eq!(winspan.hspan(), 0..w);
        assert_eq!(winspan.vspan(), 0..h);
        assert_eq!(DocPos::from(pos_in).line, 12);
        assert_eq!(DocPos::from(pos_in).col, 4);
    }

    #[test]
    fn winspan_all() {
        let span = WinSpan::from_size(3, 2);
        let mut it = span.all();
        assert_eq!(it.next(), Some(TermPos(0, 0)));
        assert_eq!(it.next(), Some(TermPos(0, 1)));
        assert_eq!(it.next(), Some(TermPos(0, 2)));
        assert_eq!(it.next(), Some(TermPos(1, 0)));
        assert_eq!(it.next(), Some(TermPos(1, 1)));
        assert_eq!(it.next(), Some(TermPos(1, 2)));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn display_workspace_empty() {
        let truth = "    \n    \n    \n    ";
        let span = WinSpan::from_size(4, 4);
        let wrk = Workspace::new(span);
        let real = truth.to_string();
        assert_eq!(truth, real);
    }

    #[test]
    fn display_workspace_one_window() {
        let truth = " 12 \n 34 \n    \n    ";
        let wrkspan = WinSpan::from_size(4, 4);
        let winspan = WinSpan::from_hvspan(1..3, 0..2);
        let mut win = Window::new(winspan);
        win.buf = Buffer::new("12\n34".into());
        assert_eq!("12\n34", win.to_string());
        let mut wrk = Workspace::new(wrkspan);
        wrk.add_window(win);
        assert_eq!(truth, wrk.to_string());
    }

    #[test]
    fn display_workspace_two_window() {
        let truth = " 12 \n 356\n  78\n    ";
        let wrkspan = WinSpan::from_size(4, 4);

        let winspan1 = WinSpan::from_hvspan(1..3, 0..2);
        let mut win1 = Window::new(winspan1);
        win1.buf = Buffer::new("12\n34".into());
        assert_eq!("12\n34", win1.to_string());

        let winspan2 = WinSpan::from_hvspan(2..4, 1..3);
        let mut win2 = Window::new(winspan2);
        win2.buf = Buffer::new("56\n78".into());
        assert_eq!("56\n78", win2.to_string());


        let mut wrk = Workspace::new(wrkspan);
        wrk.add_window(win1);
        wrk.add_window(win2);
        assert_eq!(truth, wrk.to_string());
    }

    #[test]
    fn display_workspace_full_window() {
        let truth = "ABCD\nEFGH\nIJKL\nMNOP";
        let wrkspan = WinSpan::from_size(4, 4);
        let winspan = WinSpan::from_hvspan(0..4, 0..4);
        let mut win = Window::new(winspan);
        win.buf = Buffer::new("ABCD\nEFGH\nIJKL\nMNOP".into());
        assert_eq!("ABCD\nEFGH\nIJKL\nMNOP", win.to_string());
        let mut wrk = Workspace::new(wrkspan);
        wrk.add_window(win);
        assert_eq!(truth, wrk.to_string());
    }

    #[test]
    fn display_workspace_overfull_window() {
        let truth = "ABCD\nEFGH\nIJKL\nMNOP";
        let wrkspan = WinSpan::from_size(4, 4);
        let winspan = WinSpan::from_hvspan(0..4, 0..4);
        let mut win = Window::new(winspan);
        win.buf = Buffer::new("ABCD\nEFGH\nIJKL\nMNOP\nAAAAAAAAAAAAAAAAAAAA".into());
        assert_eq!("ABCD\nEFGH\nIJKL\nMNOP", win.to_string());
        let mut wrk = Workspace::new(wrkspan);
        wrk.add_window(win);
        assert_eq!(truth, wrk.to_string());
    }
}
