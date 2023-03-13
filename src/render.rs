#![allow(unused)]

use crate::Wrapping;
use std::{borrow::Cow, fmt::Display, ops::Range};
use textwrap;
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

struct DisplayCache {
    /// Byte range this covers in original str
    cover: Range<usize>,

    w: usize,
    h: usize,

    cache: String,
}

impl DisplayCache {
    fn lines<'a>(&'a self) -> impl Iterator<Item = &'a str> {
        self.cache.lines()
    }

    fn new(raw: &[u8], start: usize, w: usize, h: usize) -> Self {
        let cover = start..(start + raw.len());
        let cache = {
            let decoded = decode(&raw[start..]);
            let mut v = textwrap::wrap(&decoded, w);
            v.resize(h, "".into());
            v.into_iter()
                .map(|x| format!("{:<w$}", x))
                .collect::<Vec<_>>()
                .join("\n")
        };
        Self { cover, cache, w, h }
    }
}

impl Display for DisplayCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cache.fmt(f)
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
/// Does not escape whitespace characters
///
/// # Examples
/// ```
/// # use edit::render::escape_noprint;
/// let s = "\t\x1b]0mhello,\0world\n";
/// assert_eq!(escape_noprint(s), "\t<0x1b>]0mhello,<0x00>world\n")
pub fn escape_noprint(s: &str) -> String {
    s.chars()
        .map(|x| {
            if !x.is_whitespace() && x.is_control() {
                escape_char(x as u32)
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
}

impl Window {
    fn render_cache(&mut self) {
        let w = self.span.width();
        let h = self.span.height();
        self.buf.render_cache(w, h);
    }

    fn render(&self) -> DisplayCache {
        let w = self.span.width();
        let h = self.span.height();
        self.buf.render(w, h)
    }

    fn new(span: WinSpan) -> Self {
        Self {
            buf: Buffer::new([].to_vec()),
            span,
        }
    }
}

impl Display for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.render().fmt(f)
    }
}

struct Workspace {
    winv: Vec<Window>,
    span: WinSpan,
}

impl Workspace {
    fn new(span: WinSpan) -> Self {
        let winv = vec![];
        Self { winv, span }
    }

    fn add_window(&mut self, win: Window) {
        self.winv.push(win);
    }
}

impl Display for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: Vec<_> = self.winv.iter().map(|x| x.render().to_string()).collect();
        let g: Vec<_> = s
            .iter()
            .map(|x| Graphemes::new(x).filter(|x| x != &"\n").collect::<Vec<_>>())
            .collect();
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
    cache: Option<DisplayCache>,
    top: DocPos,
    loff: Vec<usize>,
    wrapping: Wrapping,
}

impl Buffer {
    fn new(raw: Vec<u8>) -> Self {
        let cursor = DocPos { line: 0, col: 0 };
        let cache = None;
        let top = DocPos { line: 0, col: 0 };
        let loff = [0]
            .into_iter()
            .chain(
                String::from_utf8_lossy(&raw)
                    .char_indices()
                    .filter(|(_, c)| *c == '\n')
                    .map(|(i, _)| i + 1),
            )
            .collect();
        let wrapping = Wrapping::Word;
        Self {
            raw,
            cursor,
            cache,
            top,
            wrapping,
            loff,
        }
    }

    fn render(&self, w: u16, h: u16) -> DisplayCache {
        let line = self.top.line.min(self.loff.len() - 1);
        DisplayCache::new(&self.raw, self.loff[line], w as usize, h as usize)
    }

    fn render_cache(&mut self, w: u16, h: u16) {
        let line = self.top.line.min(self.loff.len() - 1);
        self.cache = Some(DisplayCache::new(
            &self.raw,
            self.loff[line],
            w as usize,
            h as usize,
        ));
    }

    fn scroll_abs(&mut self, newl: usize) {
        self.top.line = newl;
    }
}

impl Display for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.cache {
            Some(cache) => cache.fmt(f),
            None => Err(std::fmt::Error),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic_box_render_invariant() {
        let truth = include_str!("crossbox.txt");
        let mut test = Buffer::new(include_bytes!("crossbox.txt").to_vec());
        test.render_cache(31, 31);
        assert_eq!(truth.trim_end().to_owned(), test.to_string());
    }

    #[test]
    fn buffer_pad_horizontal() {
        let init = "1\n22\n333\n4444\n";
        let truth = "1   \n22  \n333 \n4444";
        let mut test = Buffer::new(init.as_bytes().to_vec());
        test.render_cache(4, 4);
        assert_eq!(truth.to_owned(), test.to_string());
    }

    #[test]
    fn buffer_pad_vertical() {
        let init = "1\n22\n333";
        let truth = "1   \n22  \n333 \n    ";
        let mut test = Buffer::new(init.as_bytes().to_vec());
        test.render_cache(4, 4);
        assert_eq!(truth.to_owned(), test.to_string());
    }

    #[test]
    fn buffer_empty() {
        let truth = "    \n    \n    \n    ";
        let mut test = Buffer::new([].to_vec());
        test.render_cache(4, 4);
        assert_eq!(truth.to_owned(), test.to_string());
    }

    #[test]
    fn scroll_abs_one() {
        let init = "1\n22\n333";
        let truth = "22  \n333 \n    \n    ";
        let mut test = Buffer::new(init.as_bytes().to_vec());
        test.scroll_abs(1);
        test.render_cache(4, 4);
        assert_eq!(truth.to_owned(), test.to_string());
    }

    #[test]
    fn scroll_abs_end() {
        let init = "1\n22\n333";
        let truth = "333 \n    \n    \n    ";
        let mut test = Buffer::new(init.as_bytes().to_vec());
        test.scroll_abs(2);
        test.render_cache(4, 4);
        assert_eq!(truth.to_owned(), test.to_string());
    }

    #[test]
    fn scroll_abs_past_end() {
        let init = "1\n22\n333";
        let truth = "333 \n    \n    \n    ";
        let mut test = Buffer::new(init.as_bytes().to_vec());
        test.scroll_abs(12);
        test.render_cache(4, 4);
        assert_eq!(truth.to_owned(), test.to_string());
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
