use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io::Write;
use std::iter::Rev;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::io::ErrorKind;
use std::str::Chars;

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
    /// leaf node that contains a string. The actual storage is a Rc<String> and a range that
    /// denotes the characters of the string that the leaft actually contains. This sets us up for
    /// reducing clone calls. There is further optimization to be made here if when the string is
    /// unable to be made mutable that only copies the relevant slice. 
    ///
    /// In this enum variant, the weight is just the length of the range.
    Leaf(Rc<String>, Range<usize>),

    /// Non-leaf node. weight is the total number of bytes of the left subtree (0 if left is None)
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

    /// create a leaf Node. The following invariant must be kept:
    ///
    /// either the string has now LF characters or the last character is an LF
    unsafe fn create_leaf_unchecked(s: &Rc<String>, r: Range<usize>) -> Self {
        let lf_cnt = s[r].matches('\n').count();
        Self { lf_cnt, inner: NodeInner::Leaf(Rc::clone(s), r) }
    }

    fn leaf_is_valid(&self) -> bool {
        match self.inner {
            NodeInner::Leaf(s, r) => {
                if r.len() == 0 {return false}
                let lf_cnt = s[r].matches('\n').count();
                if self.lf_cnt != lf_cnt {return false}

                if lf_cnt >= 1 {
                    s[r].as_bytes().last() == Some(&b'\n')
                } else {
                    true
                }
            },
            NodeInner::NonLeaf { l, r, weight } => {
                true
            },
        }
    }

    fn validate_inner (&self) -> usize {
        match self.inner {
            NodeInner::Leaf(_, _) => {
                assert!(self.leaf_is_valid());
                self.weight()
            },
            NodeInner::NonLeaf { l, r, weight } => {
                let l_size = l.map(|l| l.validate_inner()).unwrap_or(0);
                let r_size = r.map(|r| r.validate_inner()).unwrap_or(0);
                assert_eq!(l_size, weight);
                l_size + r_size
            },
        }
    }

    fn validate (&self) {
        self.validate_inner();
    }

    /// creates a new node from string, following the invarient of each leaf being either a part of
    /// a single line or ending with and LF. Can return None if r is empty.
    fn create_from_string(s: &Rc<String>, r: Range<usize>) -> Option<Self> {
        if r.len() == 0 { return None };
        let lf_cnt = s[r].matches('\n').count();
        let ret = if lf_cnt >= 1 {
            let split_idx = s[r].rfind('\n').expect("multiline string has lf");
            if split_idx == r.len() - 1 {
                Some(Self {
                    lf_cnt,
                    inner: NodeInner::Leaf(s.clone(), r),
                })
            } else {
                Some(Self { 
                    lf_cnt,
                    inner: NodeInner::NonLeaf { l: Some(
                        Box::new(Self { lf_cnt, inner: NodeInner::Leaf(Rc::clone(s), r.start..(r.start + split_idx + 1)) })
                    ), r: Some(
                        Box::new(Self { lf_cnt, inner: NodeInner::Leaf(Rc::clone(s), (r.start + split_idx + 1)..r.end) })
                        )
                        , weight: split_idx }
                })
            }
        } else {
            Some(Self {
                lf_cnt: 0,
                inner: NodeInner::Leaf(s.clone(), r),
            })
        };
        ret.map(|n| n.validate());
        ret
    }

    /// create a new node from left and right optional nodes
    fn merge(left: Option<Self>, right: Option<Self>) -> Option<Self> {
        let ret = match (left, right) {
            (None, None) => None,
            (None, r) => r,
            (l, None) => l,
            _ => Some(Node { 
                lf_cnt: [left, right].into_iter().map(|x| x.map(|n| n.lf_cnt)).sum::<Option<usize>>().unwrap_or(0),
                inner: NodeInner::NonLeaf { 
                    l: left.map(Box::new),
                    r: right.map(Box::new),
                    weight: [left.map(|n| n.total_weight()), right.map(|n| n.total_weight())].into_iter().sum::<Option<_>>().unwrap_or(0),
                }
            })
        };
        ret.map(|n| n.validate());
        ret
    }

    /// split the rope into two sub ropes. The current rope will contain characters from `0..idx` and
    /// the returned rope will contain characters in the range `idx..`
    fn split_offset(self, idx: usize) -> (Option<Self>, Option<Self>) {
        match self.inner {
            NodeInner::Leaf(s, range) => {
                // left split
                let l_range = range.start..(range.start + idx);
                let l_node = Node::create_from_string(&s, l_range);

                // right split
                let r_range = (range.start + idx)..range.end;
                let r_node = Node::create_from_string(&s, r_range);
                (l_node, r_node)
            },
            NodeInner::NonLeaf { l, r, weight } => match (weight + 1).cmp(&idx) {
                // compare with weight + 1 since idx is the exclusive upper bound of the left
                // split, and weight is the number of characters in the left child
                std::cmp::Ordering::Less => {
                    // all in right child
                    // this line likely has an off-by-one error - I should fuzz this
                    let (splitl, splitr) = r.map(|n| n.split_offset(idx - weight)).unwrap_or((None, None));
                    (Node::merge(l.map(|n| *n), splitl), splitr)
                },
                std::cmp::Ordering::Equal => {
                    // split down the middle
                    (l.map(|n| *n), r.map(|n| *n))
                },
                std::cmp::Ordering::Greater => {
                    // all in left child
                    let (splitl, splitr) = l.map(|n| n.split_offset(idx)).unwrap_or((None, None));
                    (splitl, Node::merge(splitr, r.map(|n| *n)))
                },
            },
        }
    }

    fn split(&mut self, pos: DocPos) -> (Option<Node>, Option<Node>){
        self.split_offset(self.doc_pos_to_offset(pos).unwrap())
    }

    fn num_trailing_chars(&self) -> usize {
        if self.lf_cnt == 0 { return self.total_weight() }
        match self.inner {
            NodeInner::Leaf(_, _) => 0,
            NodeInner::NonLeaf { l, r, weight } => {
                r.map_or(0, |r| r.num_trailing_chars()) + 
                r.filter(|r| r.lf_cnt == 0).map_or(0, |_| l.map_or(0, |l| l.num_trailing_chars()))
            },
        }
    }

    /// Find offset from DocPos.
    ///
    /// This doesn't actually need my line invariant - maybe I can get rid of it
    fn doc_pos_to_offset(&self, pos: DocPos) -> Option<usize> {
        if pos.y > self.lf_cnt {return None};
        match self.inner {
            NodeInner::Leaf(s, r) => {
                let line_idx: usize = s[r].lines().map(str::len).take(pos.y).sum();
                if pos.x > s[r][line_idx..].lines().nth(0)?.len() {
                    None
                } else {
                    Some(line_idx + pos.x)
                }
            },
            NodeInner::NonLeaf { l, r, weight } => {
                l.map(|l| l.doc_pos_to_offset(pos))
                    .flatten()
                    .or_else(|| r.map(|r| {
                        r.doc_pos_to_offset(DocPos { x: pos.x - l.map_or(0, |l| l.num_trailing_chars()), y: pos.y - l.map_or(0, |l| l.lf_cnt) })
                            .map(|off| off + weight)
                    }).flatten())
            },
        }
    }

    fn insert_offset(self, idx: usize, s: String) -> Self {
        let (l, r) = self.split_offset(idx);
        let new = Self::create_from_string(&Rc::new(s), 0..(s.len()));
        Self::merge(l, Self::merge(new, r)).unwrap_or_else(|| Self::default())
    }

    fn insert(self, pos: DocPos, s: String) -> Self {
        self.insert_offset(self.doc_pos_to_offset(pos).unwrap(), s)
    }

    fn insert_char(self, pos: DocPos, c: char) -> Self {
        self.insert_offset(self.doc_pos_to_offset(pos).unwrap(), c.into())
    }

    fn delete_range_offset(self, range: Range<usize>) -> Self {
        let (l, upper) = self.split_offset(range.start);
        let (_, r) = upper.map_or((None, None), |upper| upper.split_offset(range.end));
        Self::merge(l, r).unwrap_or_default()
    }

    fn delete_range(self, range: DocRange) -> Self {
        let start = self.doc_pos_to_offset(range.start).unwrap();
        let end = self.doc_pos_to_offset(range.end).unwrap();
        self.delete_range_offset(start..end)
    }

    fn forward_iter(&self, pos: DocPos) -> RopeForwardIter {
        let off = self.doc_pos_to_offset(pos).expect("valid position");
        let mut ret = RopeForwardIter {
            stack: VecDeque::new(),
            curr: None,
            pos
        };
        let mut curr_idx = 0;
        ret.stack.push_front(self);
        while let Some(n) = ret.stack.pop_front() {
            assert!(curr_idx <= off);
            match &n.inner {
                NodeInner::Leaf(s, r) => {
                    assert!(curr_idx + r.len() > off);
                    ret.curr = Some(s.as_str()[*r].chars());
                    if curr_idx > off {
                        ret.curr.expect("just set").nth(off - curr_idx - 1);
                    }
                    break;
                },
                NodeInner::NonLeaf { l, r, weight } => {
                    r.map(|r| ret.stack.push_front(&r));
                    if curr_idx + weight < off {
                        ret.stack.push_front( &l.expect("non-zero weight implies left child and index not more than offset"));
                    }
                },
            }
        }
        ret
    }

    fn backward_iter(&self, pos: DocPos) -> RopeBackwardIter {
        todo!()
    }
}


struct RopeForwardIter<'a> {
    stack: VecDeque<&'a Node>,
    curr: Option<Chars<'a>>,
    pos: DocPos
}

impl Iterator for RopeForwardIter<'_> {
    type Item = (DocPos, char);

    fn next(&mut self) -> Option<Self::Item> {
        let ret_c = {
            if let Some(c) = self.curr?.next() {
                Some(c)
            } else {
                while let Some(front) = self.stack.pop_front() {
                    match front.inner {
                        NodeInner::Leaf(s, r) => {
                            self.curr = Some(s[r].chars());
                            break;
                        },
                        NodeInner::NonLeaf { l, r, weight } => {
                            r.map(|r| self.stack.push_front(&r));
                            l.map(|l| self.stack.push_front(&l));
                        },
                    }
                };
                self.curr?.next()
            }
        }?;

        let ret_p = self.pos;
        if ret_c == '\n' {
            self.pos = DocPos {
                x: 0,
                y: self.pos.y + 1,
            }
        } else {
            self.pos.x += 1;
        }
        Some((ret_p, ret_c))

    }
}

struct RopeBackwardIter<'a> {
    stack: VecDeque<&'a Node>,
    curr: Option<Rev<Chars<'a>>>,
    pos: DocPos
}

impl Iterator for RopeBackwardIter<'_> {
    type Item = (DocPos, char);

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
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
        Self {
            name,
            dirty: !s.is_empty(),
            path: None,
            data: Node::create_from_string(&Rc::new(s), 0..(s.len())).unwrap_or_default(),
        }
    }

    pub fn delete_char(&mut self, _ctx: &mut BufCtx) {
    }

    pub fn delete_range(&mut self, r: DocRange) {
        self.data = self.data.delete_range(r);
    }

    pub fn replace_range(&mut self, _ctx: &mut BufCtx, _r: DocRange, _s: &str) {
    }

    pub fn insert_string(&mut self, ctx: &mut BufCtx, s: &str) {
        self.data = self.data.insert(ctx.cursorpos, s.to_string())
    }

    pub fn get_off(&self, _pos: DocPos) -> usize {
        todo!()
    }

    pub fn get_lines(&self, _lines: Range<usize>) -> Vec<String>
    {
        todo!()
    }

    pub fn serialize<W: Write>(&self, _writer: &mut W) -> std::io::Result<()> {
        todo!();
    }

    pub fn linecnt(&self) -> usize {
        todo!()
    }

    pub fn end(&self) -> DocPos {
        todo!()
    }

    pub fn chars_fwd(&self, pos: DocPos) -> RopeForwardIter {
        self.data.forward_iter(pos)
    }

    pub fn chars_bck(&self, pos: DocPos) -> RopeBackwardIter {
        self.data.backward_iter(pos)
    }
}
