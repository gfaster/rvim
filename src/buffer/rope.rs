use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::fmt::Display;
use std::io::ErrorKind;
use std::io::Write;
use std::iter::Rev;
use std::ops::Deref;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::str::Chars;

use crate::buffer::DocPos;
use crate::window::BufCtx;

use super::DocRange;

/// normal operations are done as a standard character-wise rope. However, each node stores the
/// total number of LFs in all of its children for faster line indexing. It's important to remember
/// that there can be more characters after the final LF.
struct Rope {
    lf_cnt: usize,
    inner: NodeInner,
}

enum NodeInner {
    /// leaf node that contains a string. The actual storage is a Rc<String> and a range that
    /// denotes the characters of the string that the leaft actually contains. This sets us up for
    /// reducing clone calls. There is further optimization to be made here if when the string is
    /// unable to be made mutable that only copies the relevant slice.
    ///
    /// In this enum variant, the weight is just the length of the range.
    Leaf(Rc<str>, Range<usize>),

    /// Non-leaf node. weight is the total number of bytes of the left subtree (0 if left is None)
    NonLeaf {
        l: Option<Box<Rope>>,
        r: Option<Box<Rope>>,
        weight: usize,
    },
}

impl Rope {
    fn new() -> Self {
        Self {
            lf_cnt: 0,
            inner: NodeInner::Leaf(Rc::from(String::new()), 0..0),
        }
    }

    fn weight(&self) -> usize {
        match &self.inner {
            NodeInner::Leaf(_, r) => r.len(),
            NodeInner::NonLeaf { l: _, r: _, weight } => *weight,
        }
    }

    fn total_weight(&self) -> usize {
        match &self.inner {
            NodeInner::Leaf(_, r) => r.len(),
            NodeInner::NonLeaf { l: _, r, weight } => {
                weight + r.as_ref().map(|n| n.total_weight()).unwrap_or(0)
            }
        }
    }

    fn validate_inner(&self) -> usize {
        match &self.inner {
            NodeInner::Leaf(_, _) => {
                assert!(self.weight() >= 1);
                self.weight()
            }
            NodeInner::NonLeaf { l, r, weight } => {
                let l_size = l.as_ref().map(|l| l.validate_inner()).unwrap_or(0);
                let r_size = r.as_ref().map(|r| r.validate_inner()).unwrap_or(0);
                assert_eq!(
                    l_size,
                    *weight,
                    "Rope: {:?}\nhas weight {} but should be {}",
                    l.as_ref().map_or("".to_string(), |l| l.to_string()),
                    weight,
                    l_size
                );
                l_size + r_size
            }
        }
    }

    fn validate(&self) {
        self.validate_inner();
    }

    fn regen_weight_inner(&mut self) -> usize {
        // println!("regenerating weight for {self:#?}");
        match &mut self.inner {
            NodeInner::Leaf(_, r) => r.len(),
            NodeInner::NonLeaf {
                ref mut l,
                ref mut r,
                ref mut weight,
            } => {
                *weight = l.as_mut().map_or(0, |l| l.regen_weight_inner());
                r.as_mut().map_or(0, |r| r.regen_weight_inner()) + *weight
            }
        }
    }

    fn regen_weight(&mut self) {
        self.regen_weight_inner();
    }

    /// creates a new node from string, following the invarient of each leaf being either a part of
    /// a single line or ending with and LF. Can return None if r is empty.
    fn create_from_string(s: &Rc<str>, r: Range<usize>) -> Option<Self> {
        if r.len() == 0 {
            return None;
        };
        let lf_cnt = s[r.clone()].matches('\n').count();
        let ret = if lf_cnt >= 1 {
            let split_idx = s[r.clone()].rfind('\n').expect("multiline string has lf");
            if split_idx == r.len() - 1 {
                Some(Self {
                    lf_cnt,
                    inner: NodeInner::Leaf(Rc::clone(s), r),
                })
            } else {
                assert_eq!((r.start..(r.start + split_idx + 1)).len(), split_idx + 1);
                Some(Self {
                    lf_cnt,
                    inner: NodeInner::NonLeaf {
                        l: Some(Box::new(Self {
                            lf_cnt,
                            inner: NodeInner::Leaf(
                                Rc::clone(s),
                                r.start..(r.start + split_idx + 1),
                            ),
                        })),
                        r: Some(Box::new(Self {
                            lf_cnt,
                            inner: NodeInner::Leaf(Rc::clone(s), (r.start + split_idx + 1)..r.end),
                        })),
                        weight: split_idx + 1,
                    },
                })
            }
        } else {
            Some(Self {
                lf_cnt: 0,
                inner: NodeInner::Leaf(s.clone(), r),
            })
        };
        ret.as_ref().map(|n| n.validate());
        ret
    }

    /// create a new node from left and right optional nodes
    fn merge(left: Option<Self>, right: Option<Self>) -> Option<Self> {
        let mut ret = match (left, right) {
            (None, None) => None,
            (None, r) => r,
            (l, None) => l,
            (l, r) => Some(Rope {
                lf_cnt: l.as_ref().map_or(0, |l| l.lf_cnt) + r.as_ref().map_or(0, |r| r.lf_cnt),
                inner: NodeInner::NonLeaf {
                    weight: l.as_ref().map_or(0, |l| l.total_weight())
                        + r.as_ref().map_or(0, |r| r.total_weight()),
                    l: l.map(Box::new),
                    r: r.map(Box::new),
                },
            }),
        };
        ret.as_mut().map(Rope::regen_weight);
        ret.as_ref().map(|n| n.validate());
        ret
    }

    /// split the rope into two sub ropes. The current rope will contain characters from `0..idx` and
    /// the returned rope will contain characters in the range `idx..`
    fn split_offset(self, idx: usize) -> (Option<Self>, Option<Self>) {
        let ret = match self.inner {
            NodeInner::Leaf(s, range) => {
                // left split
                let l_range = range.start..(range.start + idx);
                let l_node = Rope::create_from_string(&s, l_range);

                // right split
                let r_range = (range.start + idx)..range.end;
                let r_node = Rope::create_from_string(&s, r_range);
                (l_node, r_node)
            }
            NodeInner::NonLeaf { l, r, weight } => match weight.cmp(&idx) {
                std::cmp::Ordering::Less => {
                    // all in right child
                    let (splitl, splitr) = r
                        .map(|n| n.split_offset(idx - weight))
                        .unwrap_or((None, None));
                    (Rope::merge(l.map(|n| *n), splitl), splitr)
                }
                std::cmp::Ordering::Equal => {
                    // split down the middle
                    (l.map(|n| *n), r.map(|n| *n))
                }
                std::cmp::Ordering::Greater => {
                    // all in left child
                    let (splitl, splitr) = l.map(|n| n.split_offset(idx)).unwrap_or((None, None));
                    (splitl, Rope::merge(splitr, r.map(|n| *n)))
                }
            },
        };

        // println!("{}: {:#?}", line!(), &ret);
        ret
    }

    fn split(self, pos: DocPos) -> (Option<Rope>, Option<Rope>) {
        let off = self.doc_pos_to_offset(pos).unwrap();
        self.split_offset(off)
    }

    fn num_trailing_chars(&self) -> usize {
        if self.lf_cnt == 0 {
            return self.total_weight();
        }
        match &self.inner {
            NodeInner::Leaf(_, _) => 0,
            NodeInner::NonLeaf { l, r, weight: _ } => {
                r.as_ref().map_or(0, |r| r.num_trailing_chars())
                    + r.as_ref()
                        .filter(|r| r.lf_cnt == 0)
                        .map_or(0, |_| l.as_ref().map_or(0, |l| l.num_trailing_chars()))
            }
        }
    }

    /// Find offset from DocPos.
    ///
    /// TODO: When the output of this is passed to functions that use the offset, they will likely
    /// traverse the tree again. This is wasteful and should be fixed
    fn doc_pos_to_offset(&self, pos: DocPos) -> Option<usize> {
        if pos.y > self.lf_cnt {
            return None;
        };
        match &self.inner {
            NodeInner::Leaf(s, r) => {
                let line_idx: usize = s[r.clone()].lines().map(str::len).take(pos.y).sum();
                if pos.x > s[r.clone()][line_idx..].lines().nth(0)?.len() {
                    None
                } else {
                    Some(line_idx + pos.x)
                }
            }
            NodeInner::NonLeaf { l, r, weight } => l
                .as_ref()
                .map(|l| l.doc_pos_to_offset(pos))
                .flatten()
                .or_else(|| {
                    r.as_ref()
                        .map(|r| {
                            r.doc_pos_to_offset(DocPos {
                                x: pos.x - l.as_ref().map_or(0, |l| l.num_trailing_chars()),
                                y: pos.y - l.as_ref().map_or(0, |l| l.lf_cnt),
                            })
                            .map(|off| off + weight)
                        })
                        .flatten()
                }),
        }
    }

    /// Insert at byte offset. Uses `&str` since converting to `Rc<str>` will require reallocation
    /// anyway
    fn insert_offset(self, idx: usize, s: &str) -> Self {
        let (l, r) = self.split_offset(idx);
        let range = 0..(s.len());
        let new = Self::create_from_string(&s.into(), range);
        Self::merge(l, Self::merge(new, r)).unwrap_or_else(|| Self::default())
    }
    /// Insert at `DocPos`. Uses `&str` since converting to `Rc<str>` will require reallocation
    /// anyway
    fn insert(self, pos: DocPos, s: &str) -> Self {
        let off = self.doc_pos_to_offset(pos).unwrap();
        self.insert_offset(off, s)
    }

    fn insert_char(self, pos: DocPos, c: char) -> Self {
        let off = self.doc_pos_to_offset(pos).unwrap();
        self.insert_offset(off, &String::from(c))
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
            pos,
        };
        let mut curr_idx = 0;
        ret.stack.push_front(self);
        while let Some(n) = ret.stack.pop_front() {
            assert!(curr_idx <= off);
            match &n.inner {
                NodeInner::Leaf(s, r) => {
                    assert!(curr_idx + r.len() > off);
                    ret.curr = Some(s[r.clone()].chars());
                    if curr_idx > off {
                        ret.curr.as_mut().expect("just set").nth(off - curr_idx - 1);
                    }
                    break;
                }
                NodeInner::NonLeaf { l, r, weight } => {
                    r.as_ref().map(|r| ret.stack.push_front(&r));
                    if curr_idx + weight < off {
                        ret.stack.push_front(&l.as_ref().expect(
                            "non-zero weight implies left child and index not more than offset",
                        ));
                        curr_idx += weight;
                    }
                }
            }
        }
        ret
    }

    fn backward_iter(&self, _pos: DocPos) -> RopeBackwardIter {
        todo!()
    }

    fn leaves(&self) -> RopeLeafIter {
        RopeLeafIter {
            stack: vec![self].into(),
        }
    }
}

impl Debug for Rope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rope")
            .field("lf_cnt", &self.lf_cnt)
            .field("weight", &self.weight())
            .field("inner", &self.inner)
            .finish()
    }
}

impl Debug for NodeInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeInner::Leaf(s, r) => f
                .debug_struct("Leaf")
                .field("content", &&s[r.clone()])
                .finish(),
            NodeInner::NonLeaf { l, r, weight: _ } => {
                let mut d = f.debug_struct("NonLeaf");
                match l {
                    Some(l) => d.field("left", l),
                    None => d.field("left", &None::<()>),
                };

                match r {
                    Some(r) => d.field("right", r),
                    None => d.field("right", &None::<()>),
                };

                d.finish()
            }
        }
    }
}

struct RopeLeafIter<'a> {
    stack: VecDeque<&'a Rope>,
}

impl<'a> Iterator for RopeLeafIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(front) = self.stack.pop_front() {
            match &front.inner {
                NodeInner::Leaf(s, r) => {
                    return Some(&s[r.clone()]);
                }
                NodeInner::NonLeaf { l, r, weight: _ } => {
                    r.as_ref().map(|r| self.stack.push_front(&r));
                    l.as_ref().map(|l| self.stack.push_front(&l));
                }
            }
        }
        None
    }
}

impl Display for Rope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for s in self.leaves() {
            f.write_str(s)?;
        }
        Ok(())
    }
}

impl<S: AsRef<str>> From<S> for Rope {
    fn from(value: S) -> Self {
        let len = value.as_ref().len();
        Self::create_from_string(&value.as_ref().into(), 0..len)
            .expect("creating from string succeeds")
    }
}

pub struct RopeForwardIter<'a> {
    stack: VecDeque<&'a Rope>,
    curr: Option<Chars<'a>>,
    pos: DocPos,
}

impl Iterator for RopeForwardIter<'_> {
    type Item = (DocPos, char);

    fn next(&mut self) -> Option<Self::Item> {
        let ret_c = {
            if let Some(c) = self.curr.as_mut()?.next() {
                Some(c)
            } else {
                while let Some(front) = self.stack.pop_front() {
                    match &front.inner {
                        NodeInner::Leaf(s, r) => {
                            self.curr = Some(s[r.clone()].chars());
                            break;
                        }
                        NodeInner::NonLeaf { l, r, weight: _ } => {
                            r.as_ref().map(|r| self.stack.push_front(&r));
                            l.as_ref().map(|l| self.stack.push_front(&l));
                        }
                    }
                }
                self.curr.as_mut()?.next()
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

pub struct RopeBackwardIter<'a> {
    stack: VecDeque<&'a Rope>,
    curr: Option<Rev<Chars<'a>>>,
    pos: DocPos,
}

impl Iterator for RopeBackwardIter<'_> {
    type Item = (DocPos, char);

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

impl Default for Rope {
    fn default() -> Self {
        Self::new()
    }
}

/// Rope Buffer
pub struct RopeBuffer {
    name: String,
    dirty: bool,
    path: Option<PathBuf>,
    data: Rope,
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
        let mut res = Self::from_str(&s);
        res.path = Some(file.canonicalize()?);
        res.name = file
            .file_name()
            .map(OsStr::to_str)
            .flatten()
            .map(str::to_string)
            .ok_or_else(|| std::io::Error::from(ErrorKind::InvalidInput))?;
        res.dirty = false;
        Ok(res)
    }

    pub fn from_str(s: &str) -> Self {
        let name = "new buffer".to_string();
        let range = 0..(s.len());
        Self {
            name,
            dirty: !s.is_empty(),
            path: None,
            data: Rope::create_from_string(&s.into(), range).unwrap_or_default(),
        }
    }

    pub fn delete_char(&mut self, _ctx: &mut BufCtx) {}

    pub fn delete_range(&mut self, r: DocRange) {
        let new = std::mem::take(&mut self.data).delete_range(r);
        self.data = new;
    }

    pub fn replace_range(&mut self, _ctx: &mut BufCtx, _r: DocRange, _s: &str) {}

    pub fn insert_str(&mut self, ctx: &mut BufCtx, s: &str) {
        let new = std::mem::take(&mut self.data).insert(ctx.cursorpos, s);
        self.data = new;
    }

    pub fn get_off(&self, _pos: DocPos) -> usize {
        todo!()
    }

    pub fn get_lines(&self, _lines: Range<usize>) -> Vec<String> {
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

    pub fn chars_fwd(&self, pos: DocPos) -> impl Iterator<Item = (DocPos, char)> + '_ {
        self.data.forward_iter(pos)
    }

    pub fn chars_bck(&self, pos: DocPos) -> RopeBackwardIter {
        self.data.backward_iter(pos)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn empty_rope() {
        assert_eq!(Rope::new().to_string(), "");
    }

    #[test]
    fn nonempty_rope() {
        assert_eq!(Rope::from("asdf").to_string(), "asdf");
    }

    #[test]
    fn insert_into_rope_simple() {
        assert_eq!(
            Rope::from("abcd")
                .insert_offset(2, "---".into())
                .to_string(),
            "ab---cd"
        );
    }

    #[test]
    fn insert_into_rope_end() {
        assert_eq!(
            Rope::from("abcd")
                .insert_offset(4, "---".into())
                .to_string(),
            "abcd---"
        );
    }

    #[test]
    fn insert_into_rope_begin() {
        assert_eq!(
            Rope::from("abcd")
                .insert_offset(0, "---".into())
                .to_string(),
            "---abcd"
        );
    }

    #[test]
    fn insert_into_rope_repeat() {
        let mut rope = Rope::from("abcd").insert_offset(2, "---".into());
        assert_eq!(rope.to_string(), "ab---cd");
        rope = dbg!(rope).insert_offset(3, "+++".into());
        assert_eq!(rope.to_string(), "ab-+++--cd");
    }

    #[test]
    fn insert_into_rope_begin_of_insertion() {
        let mut rope = Rope::from("abcd").insert_offset(2, "---".into());
        assert_eq!(rope.to_string(), "ab---cd");
        rope = rope.insert_offset(2, "+++".into());
        assert_eq!(rope.to_string(), "ab+++---cd");
    }

    #[test]
    fn insert_into_rope_end_of_insertion() {
        let mut rope = Rope::from("abcd").insert_offset(2, "---".into());
        assert_eq!(rope.to_string(), "ab---cd");
        rope = rope.insert_offset(5, "+++".into());
        assert_eq!(rope.to_string(), "ab---+++cd");
    }

    #[test]
    fn doc_pos_to_offset_simple() {
        assert_eq!(
            Rope::from("asdf").doc_pos_to_offset(DocPos { x: 2, y: 0 }),
            Some(2)
        );
    }

    #[test]
    fn doc_pos_to_offset_multiline() {
        assert_eq!(
            dbg!(Rope::from("asdf\n1234\nqwer").doc_pos_to_offset(DocPos { x: 2, y: 1 })),
            Some(7)
        );
    }
}
