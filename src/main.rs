#![allow(dead_code)]

use std::{str::FromStr, ops::Range};

use textwrap;
use unic_segment::Graphemes;

enum DisplayMode {
    Ascii,
}

struct DocPos {
    line: usize,
    col: usize,
}

struct Buffer {
    data: String,
    cursor: DocPos,
    top: DocPos,
    mode: DisplayMode,
    wrapping: Wrapping,
}

trait RenderedDyn {
    fn render(&self, w: usize, h: usize) -> String;
}

trait RenderedAbs {
    fn render(&self, d: String) -> String;
}

impl Buffer {
    fn line_range<'a>(&'a self, r: Range<usize>) -> impl Iterator<Item = &'a str> {
        self.data.lines().skip(r.start).take(r.len())
    }

    fn line_vis(&self, h: usize) -> Range<usize> {
        self.top.line..(self.top.line + h)
    }
}

impl RenderedDyn for Buffer {
    fn render(&self, w: usize, h: usize) -> String {
        match self.wrapping {
            Wrapping::None => {
                self.line_range(self.line_vis(h)).map(|x| fit_line(x, w)).collect::<Vec<_>>().join("\n")
            }
            Wrapping::Character => 
                self.line_range(self.line_vis(h))
                    .flat_map(|x| {
                        Graphemes::new(x)
                            .collect::<Vec<_>>()
                            .chunks(w)
                            .map(|l| l.to_owned().into_iter().collect::<String>())
                            .collect::<Vec<_>>()
                    })
                    .take(h)
                    .map(|x| {
                        fit_line(&x, w)
                    }).collect::<Vec<_>>().join("\n"),
            Wrapping::Word => textwrap::wrap(
                &self
                    .data
                    .lines()
                    .skip(self.top.line)
                    .collect::<Vec<_>>()
                    .join("\n"),
                w
            )
            .into_iter()
            .take(h)
            .map(|x| fit_line(&x, w))
            .collect::<Vec<_>>()
            .join("\n"),
        }
    }
}


fn fit_line(s: &str, w: usize) -> String {
    let mut v: Vec<_> = Graphemes::new(s).take(w).collect();
    let l = v.len();
    v.extend([" "].iter().cycle().take( 0isize.max(w as isize - l as isize) as usize));
    v.join("")
}

enum WindowBorder {
    Simple,
    None,
}

enum Wrapping {
    None,
    Word,
    Character,
}

struct Window {
    topleft: DocPos,
    botrght: DocPos,
    buf: Buffer,
    border: WindowBorder,
}

impl Window {
    fn buf_bordered(&self) -> String {
        match self.border {
            WindowBorder::Simple => self.border_simple(),
            WindowBorder::None => self.border_none(),
        }
    }

    fn border_none(&self) -> String {
        let w = self.botrght.col - self.topleft.col;
        let h = self.botrght.line - self.topleft.line;
        self.buf.render(w, h)
    }

    fn border_simple(&self) -> String {
        let w = self.botrght.col - self.topleft.col - 2;
        let h = self.botrght.line - self.topleft.line - 2;

        let cap = format!("+{}+", "-".repeat(w));
        let b = self.buf.render(w, h);

        cap.lines()
            .map(|x| x.to_string())
            .chain(b.lines().map(|x| format!("|{}|", x)))
            .chain(cap.lines().map(|x| x.to_string()))
            .collect::<Vec<_>>()
                            .join("\n")
    }
}

impl RenderedAbs for Window {
    fn render(&self, d: String) -> String {
        d.lines()
            .take(self.topleft.line)
            .map(|x| x.to_string())
            .chain(self.buf_bordered().lines().zip(d.lines()).map(|(b, d)| {
                format!(
                    "{}{}{}",
                    d[0..self.topleft.col].to_string(),
                    b.to_string(),
                    d[self.botrght.col..].to_string()
                )
            }))
            .chain(d.lines().skip(self.botrght.line).map(|x| x.to_string()))
            .collect::<Vec<_>>()
            .join("\n")
            .to_owned()
    }
}

struct Workspace {
    w: usize,
    h: usize,
    winv: Vec<Window>,
}

impl RenderedAbs for Workspace {
    fn render(&self, d: String) -> String {
        let _ = d;
        self.winv.iter().fold(
            format!("{}\n", " ".repeat(self.w)).repeat(self.h),
            |s, w| w.render(s),
        )
    }
}

fn main() {
    let b1 = Buffer {
        data: include_str!("./crossbox.txt").to_owned(),
        cursor: DocPos { line: 18, col: 9 },
        top: DocPos { line: 7, col: 0 },
        mode: DisplayMode::Ascii,
        wrapping: Wrapping::None,
    };
    let b2 = Buffer {
        data: include_str!("./passage.txt").to_owned(),
        cursor: DocPos { line: 0, col: 0 },
        top: DocPos { line: 0, col: 0 },
        mode: DisplayMode::Ascii,
        wrapping: Wrapping::Word,
    };

    let win1 = Window {
        topleft: DocPos { line: 3, col: 4 },
        botrght: DocPos { line: 12, col: 31 },
        border: WindowBorder::Simple,
        buf: b1,
    };

    let win2 = Window {
        topleft: DocPos { line: 0, col: 0 },
        botrght: DocPos { line: 30, col: 60 },
        border: WindowBorder::Simple,
        buf: b2,
    };

    let wrk = Workspace {
        w: 60,
        h: 30,
        winv: vec![win2],
    };

    println!("{}", wrk.render(String::new()));
}
