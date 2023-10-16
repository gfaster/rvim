use crate::{debug::log, prelude::*};
use std::{
    cell::{Cell, RefCell},
    default,
    ops::Range,
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
};

use super::{BufCore, DocPos};

pub struct SimpleBuffer {
    data: String,
    path: Option<PathBuf>,
    lines: RefCell<Vec<usize>>,
    name: String,
    outdated_lines: Cell<bool>,
}

impl super::BufCore for SimpleBuffer {
    fn new() -> Self {
        Self {
            data: String::new(),
            lines: Vec::new().into(),
            outdated_lines: true.into(),
            name: "new simple buffer".to_string(),
            path: None,
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn open(file: &std::path::Path) -> std::io::Result<Self> {
        let name = String::from_utf8_lossy(file.file_name().map_or(b"file", |os| os.as_bytes()))
            .to_string();
        Ok(Self {
            path: Some(file.to_owned()),
            name,
            ..Self::from_string(std::fs::read_to_string(file)?)
        })
    }

    fn from_string(s: String) -> Self {
        Self {
            data: s,
            ..Self::new()
        }
    }

    fn from_str(s: &str) -> Self {
        Self {
            data: s.to_string(),
            ..Self::new()
        }
    }

    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(self.data.as_bytes())
    }

    fn get_lines(&self, lines: std::ops::Range<usize>) -> Vec<&str> {
        let mut out = Vec::with_capacity(lines.len());
        let line_nums = self.line_nums();
        if line_nums.len() == 0 {
            return Vec::new();
        }
        let mut it = line_nums[lines.clone()].iter().peekable();
        while let Some(&start) = it.next() {
            let &end = it
                .peek()
                .map(std::ops::Deref::deref)
                .or_else(|| line_nums.get(lines.end))
                .unwrap_or(&self.data.len());
            out.push(self.data[start..end].trim_end_matches('\n'))
        }
        out
    }

    fn delete_char(&mut self, pos: DocPos) -> char {
        let off = self.to_fileoff(pos);
        let c = self.data.remove(off);
        self.outdated_lines.set(true);
        c
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
        let x = *line_nums
            .get(y + 1)
            .unwrap_or(&self.data.len().saturating_sub(1))
            - *line_nums.get(y).unwrap_or(&0);

        DocPos { x, y }
    }

    fn insert_str(&mut self, ctx: &mut Cursor, s: &str) {
        let off = self.to_fileoff(ctx.pos);
        self.data.insert_str(off, s);
        self.outdated_lines.set(true);
        let new_off = off + s.len();
        if s.contains('\n') {
            self.update_bufctx(ctx, new_off);
        } else {
            ctx.pos.x += s.len();
            ctx.virtpos.x = ctx.pos.x;
        }
    }

    fn path(&self) -> Option<&Path> {
        self.path.as_ref().map(PathBuf::as_path)
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn clear(&mut self, ctx: &mut Cursor) {
        self.data.clear();
        *ctx = Cursor::new();
        self.outdated_lines.set(true);
    }

    fn char_at(&self, pos: DocPos) -> Option<char> {
        let off = self.to_fileoff(pos);
        if off >= self.data.len() {
            return None;
        }
        self.data[off..].chars().next()
    }

    fn set_path(&mut self, path: std::path::PathBuf) {
        self.path = Some(path);
    }

    fn delete_range(&mut self, range: impl std::ops::RangeBounds<DocPos>) -> String {
        let start = match range.start_bound() {
            std::ops::Bound::Included(p) => self.pos_to_offset(*p),
            std::ops::Bound::Excluded(p) => self.pos_to_offset(*p) + 1,
            std::ops::Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            std::ops::Bound::Included(p) => self.pos_to_offset(*p).min(self.data.len()),
            std::ops::Bound::Excluded(p) => self.pos_to_offset(*p),
            std::ops::Bound::Unbounded => self.data.len(),
        };
        assert!(start <= end);
        debug_assert!(end <= self.data.len());
        let old = self.data[start..end].to_owned();
        self.data.replace_range(start..end, "");
        self.outdated_lines.set(true);
        old
    }

    fn offset_to_pos(&self, off: usize) -> DocPos {
        self.off_to_docpos(off)
    }

    fn pos_delta(&self, pos: DocPos, off: isize) -> DocPos {
        let start = self.to_fileoff(pos);
        let end = start.saturating_add_signed(off);
        self.off_to_docpos(end)
    }

    fn pos_to_offset(&self, pos: DocPos) -> usize {
        self.to_fileoff(pos)
    }
}

// helpers
impl SimpleBuffer {
    fn to_fileoff(&self, pos: DocPos) -> usize {
        self.line_nums()
            .get(pos.y)
            .map_or(self.data.len(), |l| l + pos.x)
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

    fn off_to_docpos(&self, off: usize) -> DocPos {
        let lines = self.line_nums();
        let y = lines
            .iter()
            .enumerate()
            .find(|&(_, &l)| l > off)
            .map_or(lines.len(), |(i, _)| i)
            .saturating_sub(1);
        let y_off = lines.get(y).or(lines.last()).unwrap_or(&0);
        let line_len = lines.get(y + 1).unwrap_or(&self.data.len()) - y_off;
        let x = (off - y_off).min(line_len.saturating_sub(1));
        DocPos { x, y }
    }

    fn update_bufctx(&self, ctx: &mut Cursor, new_off: usize) {
        let pos = self.off_to_docpos(new_off);
        ctx.pos = pos;
        ctx.virtpos = pos;
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
            return None;
        }
        let ret = self.source.data[self.off..]
            .chars()
            .next()
            .expect("in bounds");
        let ret_pos = self.pos;

        if ret == '\n' {
            self.pos.x = 0;
            self.pos.y += 1;
        } else {
            self.pos.x += ret.len_utf8();
        }
        self.off += ret.len_utf8();
        Some((ret_pos, ret))
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
            return None;
        }
        let ret = self.source.data[self.off..]
            .chars()
            .next()
            .expect("in bounds");
        let ret_pos = self.pos;

        if self.off == 0 {
            self.off = usize::MAX;
            return Some((DocPos { x: 0, y: 0 }, ret));
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
        Some((ret_pos, ret))
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
