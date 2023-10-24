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
            ..Self::from_str(std::fs::read_to_string(file)?)
        })
    }

    fn from_str(s: impl AsRef<str>) -> Self {
        Self {
            data: s.as_ref().to_owned(),
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

    fn delete_char(&mut self, off: usize) -> char {
        let c = self.data.remove(off);
        self.outdated_lines.set(true);
        c
    }

    fn linecnt(&self) -> usize {
        self.line_nums().len()
    }

    fn insert_str(&mut self, ctx: &mut Cursor, s: &str) {
        let off = self.pos_to_offset(ctx.pos);
        self.data.insert_str(off, s);
        self.outdated_lines.set(true);
        let new_off = off + s.len();
        if s.contains('\n') {
            self.update_bufctx(ctx, new_off);
        } else {
            ctx.pos.x += s.len();
            ctx.virtcol = ctx.pos.x;
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

    fn set_path(&mut self, path: std::path::PathBuf) {
        self.path = Some(path);
    }

    fn delete_range(&mut self, range: Range<usize>) -> String {
        let old = self.data[range.clone()].to_owned();
        self.data.replace_range(range, "");
        self.outdated_lines.set(true);
        old
    }

    fn try_pos_to_offset(&self, pos: DocPos) -> Option<usize> {
        let lines = self.line_nums();
        if pos.y != 0 && pos.y >= lines.len() {
            return None;
        }
        // for case of empty buffer
        if pos.y == 0 && pos.x == 0 {
            return Some(0);
        }
        let line = lines[pos.y];
        let max_x = lines.get(pos.y + 1).unwrap_or(&(self.data.len() + 1)) - line - 1;
        if pos.x > max_x {
            None
        } else {
            Some(line + pos.x)
        }
    }

    fn pos_to_offset(&self, pos: DocPos) -> usize {
        self.try_pos_to_offset(pos).expect("valid pos")
    }

    fn offset_to_pos(&self, off: usize) -> DocPos {
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

    fn get_range(&self, rng: Range<usize>) -> String {
        self.data[rng].to_owned()
    }

    fn get_char(&self, pos: usize) -> char {
        self.data[pos..].chars().next().expect("valid pos")
    }

}

// helpers
impl SimpleBuffer {
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

    fn update_bufctx(&self, ctx: &mut Cursor, new_off: usize) {
        let pos = self.offset_to_pos(new_off);
        ctx.pos = pos;
        ctx.virtcol = pos.y;
    }
}


impl SimpleBuffer {
    pub fn chars_fwd(&self, pos: usize) -> impl Iterator<Item = char> + '_ {
        self.data[pos..].chars()
    }

    pub fn chars_bck(&self, pos: usize) -> impl Iterator<Item = char> + '_ {
        // we do this so as to not crash on empty buffer
        self.data[..(pos + 1).min(self.data.len())].chars().rev()
    }
}
