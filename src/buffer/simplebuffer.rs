use std::{cell::{RefCell, Cell}, path::{PathBuf, Path}};
use crate::prelude::*;

use super::{DocPos, Buf};

pub struct SimpleBuffer {
    data: String,
    path: Option<PathBuf>,
    lines: RefCell<Vec<usize>>,

    outdated_lines: Cell<bool>
}

impl super::Buf for  SimpleBuffer {
    fn new() -> Self {
        Self {
            data: String::new(),
            lines: Vec::new().into(),
            outdated_lines: true.into(),
            path: None,
        }
    }

    fn name(&self) -> &str {
        "new simple buffer"
    }

    fn open(file: &std::path::Path) -> std::io::Result<Self>
    {
        Ok(Self {
            path: Some(file.to_owned()),
            ..Self::from_string(std::fs::read_to_string(file)?)
        })
    }

    fn from_string(s: String) -> Self {
        Self { data: s, ..Self::new()}
    }

    fn from_str(s: &str) -> Self {
        Self { data: s.to_string(), ..Self::new()}
    }

    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(self.data.as_bytes())
    }

    fn get_lines(&self, lines: std::ops::Range<usize>) -> Vec<&str> {
        self.data.lines().skip(lines.start).take(lines.len()).collect()
    }

    fn delete_char(&mut self, ctx: &mut crate::window::BufCtx) -> char {
        self.outdated_lines.set(true);
        let off = self.to_fileoff(ctx.cursorpos);
        self.data.remove(off)
    }

    fn get_off(&self, pos: super::DocPos) -> usize {
        self.to_fileoff(pos)
    }

    fn linecnt(&self) -> usize {
        self.line_nums().len()
    }

    fn end(&self) -> super::DocPos {
        super::DocPos {
            x: self.data.len() - *self.line_nums().last().unwrap_or(&0),
            y: self.linecnt().saturating_sub(1),
        }
    }

    fn last(&self) -> DocPos {
        let linecnt = self.linecnt();
        let y = linecnt.saturating_sub(1);
        let line_nums = self.line_nums();
        let x = *line_nums.get(y + 1).unwrap_or(&self.data.len().saturating_sub(1)) - *line_nums.get(y).unwrap_or(&0);

        DocPos { x, y }
    }

    fn insert_str(&mut self, ctx: &mut crate::window::BufCtx, s: &str) {
        self.outdated_lines.set(true);
        self.data.insert_str(self.to_fileoff(ctx.cursorpos), s);
    }

    fn path(&self) -> Option<&Path> {
        self.path.as_ref().map(PathBuf::as_path)
    }
}

// helpers
impl SimpleBuffer {
    fn to_fileoff(&self, pos: DocPos) -> usize {
        self.line_nums().get(pos.y).map_or(self.data.len(), |l| l + pos.x)
    }

    fn line_nums<'a>(&'a self) -> impl std::ops::Deref<Target = Vec<usize>> + 'a {
        if self.outdated_lines.get() {
            self.outdated_lines.set(false);
            let mut lines = self.lines.borrow_mut();
            lines.clear();
            let mut sum = 0;
            lines.extend(self.data.lines_inclusive().map(str::len).map(|l| {
                let ret = sum;
                sum += l;
                ret
            }));
            drop(lines)
        }
        self.lines.borrow()
    }
}

pub struct SimpleBufferForwardIter<'a> {
    source: &'a SimpleBuffer,
    pos: DocPos,
    off: usize,
}

impl Iterator for SimpleBufferForwardIter<'_> {
    type Item = (DocPos, char);

    fn next(&mut self) -> Option<Self::Item> {
        if self.off >= self.source.data.len() {
            return None
        } 
        let ret = self.source.data[self.off..].chars().next().expect("in bounds");
        let ret_pos = self.pos;

        if ret == '\n' {
            self.pos.x = 0;
            self.pos.y += 1;
        } else {
            self.pos.x += ret.len_utf8();
        }
        self.off += ret.len_utf8();
        Some ((ret_pos, ret))
    }
}

pub struct SimpleBufferBackwardIter<'a> {
    source: &'a SimpleBuffer,
    pos: DocPos,
    off: usize,
}

impl Iterator for SimpleBufferBackwardIter<'_> {
    type Item = (DocPos, char);

    fn next(&mut self) -> Option<Self::Item> {
        if self.off >= self.source.data.len() {
            return None
        } 
        let ret = self.source.data[self.off..].chars().next().expect("in bounds");
        let ret_pos = self.pos;

        if self.off == 0 {
            self.off = usize::MAX;
            return Some ((DocPos { x: 0, y: 0 }, ret))
        }
        if self.pos.x == 0 {
            let lines = self.source.line_nums();
            self.pos.x = lines[self.pos.y] - lines[self.pos.y - 1];
            self.pos.y -= 1;
        } 
        self.off -= 1;
        self.pos.x -= 1;
        while !self.source.data.is_char_boundary(self.pos.x) {
            self.off -= 1;
            self.pos.x -= 1;
        }
        Some ((ret_pos, ret))
    }
}

impl SimpleBuffer {
    pub fn chars_fwd(&self, pos: DocPos) -> impl Iterator<Item = (DocPos, char)> + '_ {
        let off = self.get_off(pos);
        SimpleBufferForwardIter {
            source: self,
            pos,
            off,
        }
    }

    pub fn chars_bck(&self, pos: DocPos) -> impl Iterator<Item = (DocPos, char)> + '_ {
        let off = self.get_off(pos);
        SimpleBufferBackwardIter {
            source: self,
            pos,
            off,
        }
    }
}
