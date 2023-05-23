use std::ffi::OsStr;
use std::fmt::Write;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::io::ErrorKind;

use crate::buffer::DocPos;
use crate::window::BufCtx;

use super::DocRange;

/// normal operations are done as a standard character-wise rope. However, each node stores the
/// total number of LFs in all of its children for faster line indexing. It's important to remember
/// that there can be more characters after the final LF.
struct Node {
    lf_cnt: usize,
    inner: NodeInner
}

enum NodeInner {
    Leaf(Rc<String>, Range<usize>),
    NonLeaf{l: Option<Box<Node>>, r: Option<Box<Node>>, weight: usize}
}

impl Node {
    /// create a new, empty node without *any* characters
    fn new() -> Self {
        Self { lf_cnt: 0, inner: NodeInner::Leaf(Rc::from(String::new()),0..0) }
    }

    fn weight(&self) -> usize {
        match self.inner {
            NodeInner::Leaf(_, r) => r.len(),
            NodeInner::NonLeaf { l, r, weight } => weight,
        }
    }

    fn total_weight(&self) -> usize {
        match self.inner {
            NodeInner::Leaf(_, r) => r.len(),
            NodeInner::NonLeaf { l, r, weight } => weight + r.map(|n| n.total_weight()).unwrap_or(0),
        }
    }

    /// create a new node from left and right optional nodes
    fn merge(left: Option<Self>, right: Option<Self>) -> Option<Self> {
        match (left, right) {
            (None, None) => None,
            _ => Some(Node { 
                lf_cnt: [left, right].into_iter().map(|x| x.map(|n| n.lf_cnt)).sum::<Option<usize>>().unwrap_or(0),
                inner: NodeInner::NonLeaf { 
                    l: left.map(Box::new),
                    r: right.map(Box::new),
                    weight: [left.map(|n| n.total_weight()), right.map(|n| n.total_weight())].into_iter().sum::<Option<_>>().unwrap_or(0),
                }
            })
        }
    }

    /// split the rope into two sub ropes. The current rope will contain characters from `0..idx` and
    /// the returned rope will contain characters in the range `idx..`
    fn split_offset(self, idx: usize) -> (Option<Self>, Option<Self>) {
        match self.inner {
            NodeInner::Leaf(s, range) => {
                // left split
                let l_str = Rc::clone(&s);
                let l_range = range.start..(range.start + idx);
                let l_node;
                if l_range.len() > 0 {
                    l_node = Some(
                        Self {
                            lf_cnt: l_str[l_range].matches('\n').count(),
                            inner: NodeInner::Leaf(l_str, l_range),
                        }
                    )
                } else {
                    l_node = None
                }

                // right split
                let r_str = Rc::clone(&s);
                let r_range = (range.start + idx)..range.end;
                let r_node;
                if r_range.len() > 0 {
                    r_node = Some(
                        Self {
                            lf_cnt: r_str[r_range].matches('\n').count(),
                            inner: NodeInner::Leaf(r_str, r_range),
                        }
                    )
                } else {
                    r_node = None
                }
                (l_node, r_node)
            },
            NodeInner::NonLeaf { l, r, weight } => match weight.cmp(&idx) {
                std::cmp::Ordering::Less => {
                    todo!()
                },
                std::cmp::Ordering::Equal => {
                    todo!()
                },
                std::cmp::Ordering::Greater => {
                    todo!()
                },
            },
        }
    }

    fn split(&mut self, pos: DocPos) {

    }
}

impl Default for Node {
    fn default() -> Self {
        Self::new()
    }
}

/// Rope Buffer
pub struct RopeBuffer {
    name: String,
    dirty: bool,
    path: Option<PathBuf>,
    data: Node,
}

impl RopeBuffer {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_ref().map(PathBuf::as_path)
    }

    pub fn open(file: &Path) -> Result<Self, std::io::Error> {
        let s = std::fs::read_to_string(file)?;
        let mut res = Self::from_string(s);
        res.path = Some(file.canonicalize()?);
        res.name = file.file_name().map(OsStr::to_str).flatten().map(str::to_string)
            .ok_or_else(|| std::io::Error::from(ErrorKind::InvalidInput))?;
        res.dirty = false;
        Ok(res)
    }

    pub fn from_string(s: String) -> Self {
        let name = "new buffer".to_string();
        let mut orig: Vec<_> = s.lines().map(str::to_string).collect();
        if orig.is_empty() {
            orig.push("".to_string());
        }
        let add = Vec::new();
        let table = vec![PieceEntry {
            which: PTType::Orig,
            start: 0,
            len: orig.len(),
        }];
        Self {
            path: None,
            name,
            dirty: !s.is_empty()
        }
    }

    pub fn delete_char(&mut self, _ctx: &mut BufCtx) {
    }

    pub fn delete_range(&mut self, r: DocRange) {
        let _line_cnt = r.end.y - r.start.y;
        let (first_line, mut tidx, testartln) = self.get_line(r.start);
        let (last_line, _, _) = self.get_line(r.end);
        let _start = &first_line[..r.start.x];
        let _last = &last_line[r.end.x..];
        let _start_tidx = tidx;

        // finding the relevant range
        assert!(testartln <= r.start.y);
        let mut te_off = r.start.y - testartln;
        for _ in r.start.y..r.end.y {
            if te_off >= self.table[tidx].len {
                tidx += 1;
                te_off = 0;
            } else {
                te_off += 1;
            }
        }
        todo!();
    }

    pub fn replace_range(&mut self, _ctx: &mut BufCtx, _r: DocRange, _s: &str) {
    }

    pub fn insert_string(&mut self, ctx: &mut BufCtx, s: &str) {
        let pos = ctx.cursorpos; // since this is just insertion, we always replace one line
        let (prev, tidx, testartln) = self.get_line(pos);
        let te = self.table[tidx];
        // eprintln!("prev: {prev:?}  tidx: {tidx:?}  start: {testartln:?}");
        let mut new = prev.to_string();
        new.replace_range(pos.x..pos.x, s);
        let addv = new.split('\n').map(str::to_string).collect::<Vec<_>>();

        if addv.len() > 1 {
            ctx.cursorpos.x = s.lines().last().unwrap().len();
        } else {
            ctx.cursorpos.x = s.len() + pos.x;
        }
        ctx.cursorpos.y += addv.len() - 1;

        let addstart = self.add.len();
        self.add.extend(addv.into_iter());
        let addlen = self.add.len() - addstart;
        self.table.remove(tidx);

        // the insertion position is before the end of the chunk
        if pos.y + 1 < testartln + te.len {
            self.table.insert(
                tidx,
                PieceEntry {
                    which: te.which,
                    start: te.start + (pos.y + 1 - testartln),
                    len: te.len - (pos.y + 1 - testartln),
                },
            )
        }

        // new stuffs
        self.table.insert(
            tidx,
            PieceEntry {
                which: PTType::Add,
                start: addstart,
                len: addlen,
            },
        );

        // the insertion position is past the beginning of the chunk, so reinsert for those lines
        if pos.y > testartln {
            self.table.insert(
                tidx,
                PieceEntry {
                    which: te.which,
                    start: te.start,
                    len: pos.y - testartln,
                },
            )
        }

        // eprintln!("Inserted {s:?} at {pos:?}\norig: {:?}\nnew: {:?}\ntable: {:?}\n", &self.orig, &self.add, &self.table);
    }

    pub fn get_off(&self, _pos: DocPos) -> usize {
        todo!()
    }

    pub fn get_lines(&self, lines: Range<usize>) -> Vec<&str> {
        let (tidx, start) = self.table_idx(DocPos {
            x: 0,
            y: lines.start,
        });
        let extra = lines.start - start;
        self.lines_fwd_internal(tidx)
            .skip(extra)
            .take(lines.len())
            .map(String::as_ref)
            .collect()
    }

    pub fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        for line in self.lines_fwd_internal(0) {
            writeln!(writer, "{}", line)?;
        }
        Ok(())
    }

    pub fn linecnt(&self) -> usize {
        self.table.iter().map(|te| te.len).sum()
    }

    pub fn end(&self) -> DocPos {
        let y = self.linecnt() - 1;
        let x = self.get_line(DocPos { x: 0, y }).0.len();
        DocPos { x, y }
    }
}
