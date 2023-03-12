#![allow(dead_code)]

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
}

trait RenderedDyn {
    fn render(&self, w: usize, h: usize) -> String;
}

trait RenderedAbs {
    fn render(&self, d: String) -> String;
}

impl RenderedDyn for Buffer {
    fn render(&self, w: usize, h: usize) -> String {
        String::from_iter(self.data.lines().skip(self.top.line).take(h).map(|x| {
            let (li, s): (Vec<usize>, String) = Graphemes::new(x).take(w).enumerate().unzip();
            let l = *li.last().unwrap_or(&0);
            let pad = " ".repeat(w - l - 1);
            format!("{}{}\n", s, pad)
        }))
    }
}

enum WindowBorder {
    Simple,
    None,
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
    };
    let b2 = Buffer {
        data: include_str!("./crossbox.txt").to_owned(),
        cursor: DocPos { line: 18, col: 9 },
        top: DocPos { line: 0, col: 0 },
        mode: DisplayMode::Ascii,
    };

    let win1 = Window {
        topleft: DocPos { line: 3, col: 4 },
        botrght: DocPos { line: 12, col: 31 },
        border: WindowBorder::Simple,
        buf: b1,
    };

    let win2 = Window {
        topleft: DocPos { line: 0, col: 12 },
        botrght: DocPos { line: 15, col: 24 },
        border: WindowBorder::Simple,
        buf: b2,
    };

    let wrk = Workspace {
        w: 60,
        h: 30,
        winv: vec![win2, win1],
    };

    println!("{}", wrk.render(String::new()));
}
