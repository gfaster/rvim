use crate::window::BufCtx;
use std::{io::Write, ops::{Range, RangeBounds}, path::Path};

/// Position in a document - similar to TermPos but distinct enough semantically to deserve its own
/// struct. In the future, wrapping will mean that DocPos and TermPos will often not correspond
/// one-to-one. Also, using usize since it can very well be more than u32::max (though not for now)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocPos {
    pub x: usize,
    pub y: usize,
}

impl PartialOrd for DocPos {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let Some(y) = self.y.partial_cmp(&other.y) else { return None };
        match y {
            std::cmp::Ordering::Equal => {
                self.x.partial_cmp(&other.x)
            },
            _ => Some(y)
        }
    }
}

impl Ord for DocPos {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).expect("DocPos is comparable")
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DocRange {
    pub start: DocPos,
    pub end: DocPos,
}

impl RangeBounds<DocPos> for DocRange {
    fn start_bound(&self) -> std::ops::Bound<&DocPos> {
        std::ops::Bound::Included(&self.start)
    }

    fn end_bound(&self) -> std::ops::Bound<&DocPos> {
        std::ops::Bound::Excluded(&self.end)
    }
}

impl DocPos {
    pub fn row(&self) -> usize {
        self.y + 1
    }
    pub fn col(&self) -> usize {
        self.x + 1
    }
}

/// Represents a file open in memory. A buffer provides some interesting challenges that I need to
/// figure out. All of the following must hold for a buffer of L lines:
///  1) getting line N from the buffer should be at least in O(log2 L)
///  2) inserting a line at any point should be at least in O(log2 L)
///  3) the number of modifications made should not increase Insert or remove times
///  4) I think it's OK for edits of line length N to be O(N)
///
/// It's clear that storing lines individually is a must, and for the sake of undo, at least some
/// number of changes will have to be stored as well. The trouble is two main things:
///  1) how do we avoid having to apply the entire changes stack to read the current state
///  2) how do we avoid having to move all lines in order to insert another one
/// Doing one or the other is pretty straight forward, but I haven't figured out a way to do both.
///
/// Some brief research tells us three possible solutions: Gap Buffer, Rope, or Piece Table. It
/// seems like Piece Tables would be the best for now due to its simplicity, but I'll make Buffer
/// into a trait since it seems worthwhile to implement all of them.
pub trait Buffer {
    fn name(&self) -> &str;
    fn open(file: &Path) -> std::io::Result<Self>
    where
        Self: Sized;

    fn from_string(s: String) -> Self;
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()>;

    /// get a vec of lines, if `lines` is nonempty, then return must be nonempty
    fn get_lines(&self, lines: Range<usize>) -> Vec<&str>;

    /// delete the character immediately to the left of the cursor in ctx
    fn delete_char(&mut self, ctx: &mut BufCtx) -> char;

    /// insert a string at the position of the cursor in ctx.
    /// The cursor should be moved to the end of the inserted text.
    fn insert_string(&mut self, ctx: &mut BufCtx, s: &str);

    fn get_off(&self, pos: DocPos) -> usize;
    fn linecnt(&self) -> usize;

    
    /// return the nearest valid position that is not past the end of line or file
    fn clamp(&self, _pos: DocPos) -> DocPos {todo!()}




    fn chars_fwd(&self, pos: DocPos) -> BufIter<Self> where Self: Sized {
        BufIter { buf: self, line: None, pos, dir: BufIterDir::Forward, next_none: false}
    }

    fn chars_bck(&self, pos: DocPos) -> BufIter<Self> where Self: Sized {
        BufIter { buf: self, line: None, pos, dir: BufIterDir::Backward, next_none: false}
    }
}


enum BufIterDir {
    Forward,
    Backward,
}

/// Iterator over the characters in a buffer - I should maybe make this into one for forward and
/// one for backward
pub struct BufIter<'a, B: Buffer> {
    buf: &'a B,
    line: Option<&'a str>,
    pos: DocPos,
    dir: BufIterDir,
    next_none: bool
}

impl<B: Buffer> Iterator for BufIter<'_, B> {
    type Item = (DocPos, char);

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos.y >= self.buf.linecnt() || self.next_none {
            return None;
        }

        let line = self.line.unwrap_or_else(|| {
            let l = self.buf.get_lines(self.pos.y..(self.pos.y + 1))[0];
            self.pos = DocPos { x: self.pos.x.min(l.len()), y: self.pos.y };
            self.line = Some(l);
            l
        });

        let virt = self.pos;

        match self.dir {
            BufIterDir::Forward => {
                if virt.x + 1 > line.len() {
                    self.pos.x = 0;
                    self.pos.y += 1;
                    self.line = None;
                } else {
                    self.pos.x += 1;
                }
                let c = line.chars().chain(['\n']).skip(virt.x).next().expect("iterate to real char (does this line have non-ascii?)");
                Some((virt, c))
            },
            BufIterDir::Backward => {
                if virt.x == 0 {
                    self.pos.x = usize::MAX;
                    if self.pos.y == 0 {
                        self.next_none = true;
                    } else {
                        self.pos.y -= 1;
                    }
                    self.line = None;
                } else {
                    self.pos.x -= 1;
                }
                let c = line.chars().chain(['\n']).skip(virt.x).next().expect("iterate to real char (does this line have non-ascii?)");
                Some((virt, c))
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PTType {
    Add,
    Orig,
}

// This is linewise, not characterwise
#[derive(Debug, Clone, Copy)]
struct PieceEntry {
    which: PTType,
    start: usize,
    len: usize,
}

/// Piece Table Buffer
pub struct PTBuffer {
    name: String,
    orig: Vec<String>,
    add: Vec<String>,
    table: Vec<PieceEntry>,
}

impl Buffer for PTBuffer {
    fn name(&self) -> &str {
        &self.name
    }

    fn open(file: &Path) -> Result<Self, std::io::Error> {
        let data = std::fs::read_to_string(file)?;
        Ok(Self::from_string(data))
    }

    fn from_string(s: String) -> Self {
        let name = "new buffer".to_string();
        let mut orig: Vec<_> = s.lines().map(str::to_string).collect();
        if orig.len() == 0 {
            orig.push("".to_string());
        }
        let add = Vec::new();
        let table = vec![PieceEntry {
            which: PTType::Orig,
            start: 0,
            len: orig.len(),
        }];
        Self {
            name,
            orig,
            add,
            table,
        }
    }

    fn delete_char(&mut self, _ctx: &mut BufCtx) -> char {
        todo!()
    }

    fn insert_string(&mut self, ctx: &mut BufCtx, s: &str) {
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

    fn get_off(&self, _pos: DocPos) -> usize {
        todo!()
    }

    fn get_lines(&self, lines: Range<usize>) -> Vec<&str> {
        let (tidx, start) = self.table_idx(DocPos { x: 0, y: lines.start });
        let extra = lines.start - start;
        self.lines_fwd_internal(tidx)
            .skip(extra)
            .take(lines.len())
            .map(String::as_ref)
            .collect()
    }

    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        for line in self.lines_fwd_internal(0) {
            writeln!(writer, "{}", line)?;
        }
        Ok(())
    }

    fn linecnt(&self) -> usize {
        self.table.iter().map(|te| te.len).sum()
    }
}

impl PTBuffer {
    fn match_table(&self, which: &PTType) -> &[String] {
        match which {
            PTType::Add => &self.add,
            PTType::Orig => &self.orig,
        }
    }

    /// Iterator over lines starting at table table entry tidx
    fn lines_fwd_internal(&self, tidx: usize) -> impl Iterator<Item = &String> {
        self.table[tidx..]
            .iter()
            .flat_map(|te| self.match_table(&te.which)[te.start..].iter().take(te.len))
    }

    /// Iterator over reverse-order lines starting at table entry tidx
    fn lines_bck_internal(&self, tidx: usize) -> impl Iterator<Item = &String> {
        self.table[..tidx].iter().rev().flat_map(|te| {
            self.match_table(&te.which)[te.start..]
                .iter()
                .rev()
                .take(te.len)
        })
    }

    /// get the table idx and line at pos
    ///
    /// Return (line, tidx, te start line)
    fn get_line(&self, pos: DocPos) -> (&str, usize, usize) {
        let (tidx, first) = self.table_idx(pos);
        let te = &self.table[tidx];
        let rem = pos.y - first;
        let line = &self.match_table(&te.which)[te.start + rem];

        let truefirst = self.table[..tidx].iter().map(|te| te.len).sum();
        assert!((truefirst..(truefirst + te.len)).contains(&pos.y), "{:?} does not contain {pos:?}", self.table[tidx] );

        (line, tidx, first)
    }

    /// returns the table idx and start line of entry for pos
    ///
    /// Returns: (table index, te start line)
    fn table_idx(&self, pos: DocPos) -> (usize, usize) {
        let mut line = 0;
        let tidx = self
            .table
            .iter()
            .enumerate()
            .take_while(|x| {
                if line + x.1.len <= pos.y {
                    line += x.1.len;
                    true
                } else {
                    false
                }
            }).map(|(i, _)| i + 1)
            .last()
            .unwrap_or(0);

        let truefirst = self.table[..tidx].iter().map(|te| te.len).sum();
        assert!((truefirst..(truefirst + self.table[tidx].len)).contains(&pos.y), "{:?} does not contain {pos:?}", self.table[tidx] );

        (tidx, line)
    }
}


#[cfg(test)]
mod test {
    use super::*;

    mod buffer {
        use crate::render::BufId;

        use super::*;

        fn assert_buf_eq<B: Buffer> (b: &B, s: &str) -> String {
            let mut out = Vec::<u8>::new();
            b.serialize(&mut out).expect("buffer will successfully serialize");
            let buf_str = String::from_utf8(out).expect("buffer outputs valid utf-8");
            assert_eq!(buf_str, s);
            buf_str
        }

        fn assert_trait_add_str<B: Buffer> (b: &mut B, ctx: &mut BufCtx,  s: &str) {
            let mut out = Vec::<u8>::new();
            b.serialize(&mut out).expect("buffer will serialize");
            let mut buf_str = String::from_utf8(out.clone()).expect("buffer outputs valid utf-8");

            let pos = ctx.cursorpos;
            let off = pos.x + buf_str.lines().take(pos.y).map(|l| l.len() + 1).sum::<usize>();
            buf_str.replace_range(off..off, s);
            b.insert_string(ctx, s);

            out.clear();
            b.serialize(&mut out).expect("buffer will serialize");
            let out_str = String::from_utf8(out).expect("buffer outputs valid utf-8");

            assert_eq!(buf_str, out_str, "inserted string == string insert from buffer");
        }

        #[test]
        fn test_ptbuf_insert_basic() { test_trait_insert_basic::<PTBuffer>() }
        fn test_trait_insert_basic<B: Buffer>() {
            let mut buf = B::from_string("".to_string());
            let mut ctx = BufCtx { buf_id: BufId::new(), cursorpos: DocPos { x: 0, y: 0 }, topline: 0 };

            assert_trait_add_str(&mut buf, &mut ctx, "Hello, World");
        }

        #[test]
        fn test_ptbuf_insert_blank() { test_trait_insert_blank::<PTBuffer>() }
        fn test_trait_insert_blank<B: Buffer>() {
            let mut buf = B::from_string("".to_string());
            let mut ctx = BufCtx { buf_id: BufId::new(), cursorpos: DocPos { x: 0, y: 0 }, topline: 0 };

            assert_trait_add_str(&mut buf, &mut ctx, "");
        }

        #[test]
        fn test_ptbuf_insert_multi() { test_trait_insert_multi::<PTBuffer>() }
        fn test_trait_insert_multi<B: Buffer>() {
            let mut buf = B::from_string("".to_string());
            let mut ctx = BufCtx { buf_id: BufId::new(), cursorpos: DocPos { x: 0, y: 0 }, topline: 0 };

            assert_trait_add_str(&mut buf, &mut ctx, "Hello, ");
            assert_trait_add_str(&mut buf, &mut ctx, "World!");
        }

        #[test]
        fn test_ptbuf_insert_newl() { test_trait_insert_newl::<PTBuffer>() }
        fn test_trait_insert_newl<B: Buffer>() {
            let mut buf = B::from_string("".to_string());
            let mut ctx = BufCtx { buf_id: BufId::new(), cursorpos: DocPos { x: 0, y: 0 }, topline: 0 };

            assert_trait_add_str(&mut buf, &mut ctx, "\n");
        }

        #[test]
        fn test_ptbuf_insert_multinewl() { test_trait_insert_multinewl::<PTBuffer>() }
        fn test_trait_insert_multinewl<B: Buffer>() {
            let mut buf = B::from_string("".to_string());
            let mut ctx = BufCtx { buf_id: BufId::new(), cursorpos: DocPos { x: 0, y: 0 }, topline: 0 };

            assert_trait_add_str(&mut buf, &mut ctx, "\n");
            assert_trait_add_str(&mut buf, &mut ctx, "\n");
            assert_trait_add_str(&mut buf, &mut ctx, "\n");
        }

        #[test]
        fn test_ptbuf_insert_offset() { test_trait_insert_offset::<PTBuffer>() }
        fn test_trait_insert_offset<B: Buffer>() {
            let mut buf = B::from_string("0123456789".to_string());
            let mut ctx = BufCtx { buf_id: BufId::new(), cursorpos: DocPos { x: 5, y: 0 }, topline: 0 };

            assert_trait_add_str(&mut buf, &mut ctx, "0000000");
        }

        #[test]
        fn test_ptbuf_insert_offnewl() { test_trait_insert_offnewl::<PTBuffer>() }
        fn test_trait_insert_offnewl<B: Buffer>() {
            let mut buf = B::from_string("0123456789".to_string());
            let mut ctx = BufCtx { buf_id: BufId::new(), cursorpos: DocPos { x: 5, y: 0 }, topline: 0 };

            assert_trait_add_str(&mut buf, &mut ctx, "\n");
        }

        #[test]
        fn test_ptbuf_insert_prenewl() { test_trait_insert_prenewl::<PTBuffer>() }
        fn test_trait_insert_prenewl<B: Buffer>() {
            let mut buf = B::from_string("0123456789".to_string());
            let mut ctx = BufCtx { buf_id: BufId::new(), cursorpos: DocPos { x: 0, y: 0 }, topline: 0 };

            assert_trait_add_str(&mut buf, &mut ctx, "\n");
        }

        #[test]
        fn test_ptbuf_insert_multilinestr() { test_trait_insert_multilinestr::<PTBuffer>() }
        fn test_trait_insert_multilinestr<B: Buffer>() {
            let mut buf = B::from_string("0123456789".to_string());
            let mut ctx = BufCtx { buf_id: BufId::new(), cursorpos: DocPos { x: 0, y: 0 }, topline: 0 };

            assert_trait_add_str(&mut buf, &mut ctx, "asdf\nzdq\nqwrpi\nmnbv\n");
            assert_trait_add_str(&mut buf, &mut ctx, "\n\n\n104a9zlq");
        }

        #[test]
        fn test_ptbuf_charsfwd_start() { test_trait_charsfwd_start::<PTBuffer>() }
        fn test_trait_charsfwd_start<B: Buffer>() {
            let buf = B::from_string("0123456789".to_string());
            let mut it = buf.chars_fwd(DocPos { x: 0, y: 0 });

            assert_eq!(it.next(), Some((DocPos { x: 0, y: 0}, '0')));
            assert_eq!(it.next(), Some((DocPos { x: 1, y: 0}, '1')));
            assert_eq!(it.next(), Some((DocPos { x: 2, y: 0}, '2')));
            assert_eq!(it.next(), Some((DocPos { x: 3, y: 0}, '3')));
            assert_eq!(it.next(), Some((DocPos { x: 4, y: 0}, '4')));
            assert_eq!(it.next(), Some((DocPos { x: 5, y: 0}, '5')));
            assert_eq!(it.next(), Some((DocPos { x: 6, y: 0}, '6')));
            assert_eq!(it.next(), Some((DocPos { x: 7, y: 0}, '7')));
            assert_eq!(it.next(), Some((DocPos { x: 8, y: 0}, '8')));
            assert_eq!(it.next(), Some((DocPos { x: 9, y: 0}, '9')));
            assert_eq!(it.next(), Some((DocPos { x: 10, y: 0}, '\n')));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn test_ptbuf_charsfwd_crosslf() { test_trait_charsfwd_crosslf::<PTBuffer>() }
        fn test_trait_charsfwd_crosslf<B: Buffer>() {
            let buf = B::from_string("01234\n56789".to_string());
            let mut it = buf.chars_fwd(DocPos { x: 0, y: 0 });

            assert_eq!(it.next(), Some((DocPos { x: 0, y: 0}, '0')));
            assert_eq!(it.next(), Some((DocPos { x: 1, y: 0}, '1')));
            assert_eq!(it.next(), Some((DocPos { x: 2, y: 0}, '2')));
            assert_eq!(it.next(), Some((DocPos { x: 3, y: 0}, '3')));
            assert_eq!(it.next(), Some((DocPos { x: 4, y: 0}, '4')));
            assert_eq!(it.next(), Some((DocPos { x: 5, y: 0}, '\n')));
            assert_eq!(it.next(), Some((DocPos { x: 0, y: 1}, '5')));
            assert_eq!(it.next(), Some((DocPos { x: 1, y: 1}, '6')));
            assert_eq!(it.next(), Some((DocPos { x: 2, y: 1}, '7')));
            assert_eq!(it.next(), Some((DocPos { x: 3, y: 1}, '8')));
            assert_eq!(it.next(), Some((DocPos { x: 4, y: 1}, '9')));
            assert_eq!(it.next(), Some((DocPos { x: 5, y: 1}, '\n')));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn test_ptbuf_charsfwd_empty() { test_trait_charsfwd_empty::<PTBuffer>() }
        fn test_trait_charsfwd_empty<B: Buffer>() {
            let buf = B::from_string("".to_string());
            let mut it = buf.chars_fwd(DocPos { x: 0, y: 0 });

            assert_eq!(it.next(), Some((DocPos { x: 0, y: 0}, '\n')));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn test_ptbuf_charsfwd_eol() { test_trait_charsfwd_eol::<PTBuffer>() }
        fn test_trait_charsfwd_eol<B: Buffer>() {
            let buf = B::from_string("01\n34".to_string());
            let mut it = buf.chars_fwd(DocPos { x: 2, y: 0 });

            assert_eq!(it.next(), Some((DocPos { x: 2, y: 0}, '\n')));
            assert_eq!(it.next(), Some((DocPos { x: 0, y: 1}, '3')));
            assert_eq!(it.next(), Some((DocPos { x: 1, y: 1}, '4')));
            assert_eq!(it.next(), Some((DocPos { x: 2, y: 1}, '\n')));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn test_ptbuf_charsbck_empty() { test_trait_charsbck_empty::<PTBuffer>() }
        fn test_trait_charsbck_empty<B: Buffer>() {
            let buf = B::from_string("".to_string());
            let mut it = buf.chars_bck(DocPos { x: 0, y: 0 });

            assert_eq!(it.next(), Some((DocPos { x: 0, y: 0}, '\n')));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn test_ptbuf_charsbck_eol() { test_trait_charsbck_eol::<PTBuffer>() }
        fn test_trait_charsbck_eol<B: Buffer>() {
            let buf = B::from_string("01\n34".to_string());
            let mut it = buf.chars_bck(DocPos { x: 2, y: 0 });

            assert_eq!(it.next(), Some((DocPos { x: 2, y: 0}, '\n')));
            assert_eq!(it.next(), Some((DocPos { x: 1, y: 0}, '1')));
            assert_eq!(it.next(), Some((DocPos { x: 0, y: 0}, '0')));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn test_ptbuf_charsbck_crosslf() { test_trait_charsbck_crosslf::<PTBuffer>() }
        fn test_trait_charsbck_crosslf<B: Buffer>() {
            let buf = B::from_string("01234\n56789".to_string());
            let mut it = buf.chars_bck(DocPos { x: 5, y: 1 });

            assert_eq!(it.next(), Some((DocPos { x: 5, y: 1}, '\n')));
            assert_eq!(it.next(), Some((DocPos { x: 4, y: 1}, '9')));
            assert_eq!(it.next(), Some((DocPos { x: 3, y: 1}, '8')));
            assert_eq!(it.next(), Some((DocPos { x: 2, y: 1}, '7')));
            assert_eq!(it.next(), Some((DocPos { x: 1, y: 1}, '6')));
            assert_eq!(it.next(), Some((DocPos { x: 0, y: 1}, '5')));
            assert_eq!(it.next(), Some((DocPos { x: 5, y: 0}, '\n')));
            assert_eq!(it.next(), Some((DocPos { x: 4, y: 0}, '4')));
            assert_eq!(it.next(), Some((DocPos { x: 3, y: 0}, '3')));
            assert_eq!(it.next(), Some((DocPos { x: 2, y: 0}, '2')));
            assert_eq!(it.next(), Some((DocPos { x: 1, y: 0}, '1')));
            assert_eq!(it.next(), Some((DocPos { x: 0, y: 0}, '0')));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn test_ptbuf_charsbck_end() { test_trait_charsbck_end::<PTBuffer>() }
        fn test_trait_charsbck_end<B: Buffer>() {
            let buf = B::from_string("0123456789".to_string());
            let mut it = buf.chars_bck(DocPos { x: 10, y: 0 });

            assert_eq!(it.next(), Some((DocPos { x: 10, y: 0}, '\n')));
            assert_eq!(it.next(), Some((DocPos { x: 9, y: 0}, '9')));
            assert_eq!(it.next(), Some((DocPos { x: 8, y: 0}, '8')));
            assert_eq!(it.next(), Some((DocPos { x: 7, y: 0}, '7')));
            assert_eq!(it.next(), Some((DocPos { x: 6, y: 0}, '6')));
            assert_eq!(it.next(), Some((DocPos { x: 5, y: 0}, '5')));
            assert_eq!(it.next(), Some((DocPos { x: 4, y: 0}, '4')));
            assert_eq!(it.next(), Some((DocPos { x: 3, y: 0}, '3')));
            assert_eq!(it.next(), Some((DocPos { x: 2, y: 0}, '2')));
            assert_eq!(it.next(), Some((DocPos { x: 1, y: 0}, '1')));
            assert_eq!(it.next(), Some((DocPos { x: 0, y: 0}, '0')));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn test_ptbuf_charsbck_mid() { test_trait_charsbck_mid::<PTBuffer>() }
        fn test_trait_charsbck_mid<B: Buffer>() {
            let buf = B::from_string("0123456789".to_string());
            let mut it = buf.chars_bck(DocPos { x: 5, y: 0 });

            assert_eq!(it.next(), Some((DocPos { x: 5, y: 0}, '5')));
            assert_eq!(it.next(), Some((DocPos { x: 4, y: 0}, '4')));
            assert_eq!(it.next(), Some((DocPos { x: 3, y: 0}, '3')));
            assert_eq!(it.next(), Some((DocPos { x: 2, y: 0}, '2')));
            assert_eq!(it.next(), Some((DocPos { x: 1, y: 0}, '1')));
            assert_eq!(it.next(), Some((DocPos { x: 0, y: 0}, '0')));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn test_ptbuf_charsbck_start() { test_trait_charsbck_start::<PTBuffer>() }
        fn test_trait_charsbck_start<B: Buffer>() {
            let buf = B::from_string("0123456789".to_string());
            let mut it = buf.chars_bck(DocPos { x: 0, y: 0 });

            assert_eq!(it.next(), Some((DocPos { x: 0, y: 0}, '0')));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }
    }
}
