use crate::window::BufCtx;
use std::{
    io::Write,
    ops::{Range, RangeBounds},
    path::Path,
};

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
        let y = self.y.partial_cmp(&other.y)?;
        match y {
            std::cmp::Ordering::Equal => self.x.partial_cmp(&other.x),
            _ => Some(y),
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
    fn clamp(&self, _pos: DocPos) -> DocPos {
        todo!()
    }

    /// get the position of the last character
    fn end(&self) -> DocPos;

    fn chars_fwd(&self, pos: DocPos) -> BufIter<Self>
    where
        Self: Sized,
    {
        BufIter {
            buf: self,
            line: None,
            pos,
            dir: BufIterDir::Forward,
            next_none: false,
        }
    }

    fn chars_bck(&self, pos: DocPos) -> BufIter<Self>
    where
        Self: Sized,
    {
        BufIter {
            buf: self,
            line: None,
            pos,
            dir: BufIterDir::Backward,
            next_none: false,
        }
    }
}

pub(crate) mod piecetable;

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
    next_none: bool,
}

impl<B: Buffer> Iterator for BufIter<'_, B> {
    type Item = (DocPos, char);

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos.y >= self.buf.linecnt() || self.next_none {
            return None;
        }

        let line = self.line.unwrap_or_else(|| {
            let l = self.buf.get_lines(self.pos.y..(self.pos.y + 1))[0];
            self.pos = DocPos {
                x: self.pos.x.min(l.len()),
                y: self.pos.y,
            };
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
                let c = line
                    .chars()
                    .chain(['\n'])
                    .skip(virt.x)
                    .next()
                    .expect("iterate to real char (does this line have non-ascii?)");
                Some((virt, c))
            }
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
                let c = line
                    .chars()
                    .chain(['\n'])
                    .skip(virt.x)
                    .next()
                    .expect("iterate to real char (does this line have non-ascii?)");
                Some((virt, c))
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test {
    // declared public to allow export of polytest

    use super::*;
    use crate::render::BufId;

    /// make a generic test function run over all buffer implementations
    macro_rules! polytest {
        ($func:ident) => {
            mod $func {
                use super::$func;
                use crate::buffer::piecetable::PTBuffer;

                #[test]
                fn pt() {
                    $func::<PTBuffer>();
                }
            }
        };
    }
    pub(crate) use polytest;

    fn assert_buf_eq<B: Buffer>(b: &B, s: &str) -> String {
        let mut out = Vec::<u8>::new();
        b.serialize(&mut out)
            .expect("buffer will successfully serialize");
        let buf_str = String::from_utf8(out).expect("buffer outputs valid utf-8");
        assert_eq!(buf_str, s);
        buf_str
    }

    fn assert_trait_add_str<B: Buffer>(b: &mut B, ctx: &mut BufCtx, s: &str) {
        let mut out = Vec::<u8>::new();
        b.serialize(&mut out).expect("buffer will serialize");
        let mut buf_str = String::from_utf8(out.clone()).expect("buffer outputs valid utf-8");

        let pos = ctx.cursorpos;
        let off = pos.x
            + buf_str
                .lines()
                .take(pos.y)
                .map(|l| l.len() + 1)
                .sum::<usize>();
        buf_str.replace_range(off..off, s);
        b.insert_string(ctx, s);

        out.clear();
        b.serialize(&mut out).expect("buffer will serialize");
        let out_str = String::from_utf8(out).expect("buffer outputs valid utf-8");

        assert_eq!(
            buf_str, out_str,
            "inserted string == string insert from buffer"
        );
    }

    fn buffer_with_changes<B: Buffer>() -> B {
        let mut b =
            B::from_string(include_str!("../../assets/test/passage_wrapped.txt").to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 8, y: 12 },
            topline: 0,
        };
        assert_trait_add_str(&mut b, &mut ctx, "This is some new text");
        assert_trait_add_str(&mut b, &mut ctx, "This is some more new text");
        ctx.cursorpos = DocPos { x: 3, y: 9 };
        assert_trait_add_str(&mut b, &mut ctx, "This is some \nnewline text");
        assert_trait_add_str(&mut b, &mut ctx, "This is some more newline text\n\n");
        ctx.cursorpos = DocPos { x: 0, y: 0 };
        assert_trait_add_str(&mut b, &mut ctx, "Some text at the beginning");
        ctx.cursorpos = DocPos { x: 0, y: 0 };
        assert_trait_add_str(&mut b, &mut ctx, "\nope - newl at the beginning");
        ctx.cursorpos = DocPos { x: 18, y: 1 };
        assert_trait_add_str(&mut b, &mut ctx, "Middle of another edit");
        assert_trait_add_str(&mut b, &mut ctx, "and again at the end of the middle");

        b
    }

    polytest!(insert_basic);
    fn insert_basic<B: Buffer>() {
        let mut buf = B::from_string("".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "Hello, World");
    }

    polytest!(insert_blank);
    fn insert_blank<B: Buffer>() {
        let mut buf = B::from_string("".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "");
    }

    polytest!(insert_multi);
    fn insert_multi<B: Buffer>() {
        let mut buf = B::from_string("".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "Hello, ");
        assert_trait_add_str(&mut buf, &mut ctx, "World!");
    }

    polytest!(insert_newl);
    fn insert_newl<B: Buffer>() {
        let mut buf = B::from_string("".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "\n");
    }

    polytest!(insert_multinewl);
    fn insert_multinewl<B: Buffer>() {
        let mut buf = B::from_string("".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "\n");
        assert_trait_add_str(&mut buf, &mut ctx, "\n");
        assert_trait_add_str(&mut buf, &mut ctx, "\n");
    }

    polytest!(insert_offset);
    fn insert_offset<B: Buffer>() {
        let mut buf = B::from_string("0123456789".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 5, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "0000000");
    }

    polytest!(insert_offnewl);
    fn insert_offnewl<B: Buffer>() {
        let mut buf = B::from_string("0123456789".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 5, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "\n");
    }

    polytest!(insert_prenewl);
    fn insert_prenewl<B: Buffer>() {
        let mut buf = B::from_string("0123456789".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "\n");
    }

    polytest!(insert_multilinestr);
    fn insert_multilinestr<B: Buffer>() {
        let mut buf = B::from_string("0123456789".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "asdf\nzdq\nqwrpi\nmnbv\n");
        assert_trait_add_str(&mut buf, &mut ctx, "\n\n\n104a9zlq");
    }

    polytest!(charsfwd_start);
    fn charsfwd_start<B: Buffer>() {
        let buf = B::from_string("0123456789".to_string());
        let mut it = buf.chars_fwd(DocPos { x: 0, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '0')));
        assert_eq!(it.next(), Some((DocPos { x: 1, y: 0 }, '1')));
        assert_eq!(it.next(), Some((DocPos { x: 2, y: 0 }, '2')));
        assert_eq!(it.next(), Some((DocPos { x: 3, y: 0 }, '3')));
        assert_eq!(it.next(), Some((DocPos { x: 4, y: 0 }, '4')));
        assert_eq!(it.next(), Some((DocPos { x: 5, y: 0 }, '5')));
        assert_eq!(it.next(), Some((DocPos { x: 6, y: 0 }, '6')));
        assert_eq!(it.next(), Some((DocPos { x: 7, y: 0 }, '7')));
        assert_eq!(it.next(), Some((DocPos { x: 8, y: 0 }, '8')));
        assert_eq!(it.next(), Some((DocPos { x: 9, y: 0 }, '9')));
        assert_eq!(it.next(), Some((DocPos { x: 10, y: 0 }, '\n')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    polytest!(charsfwd_crosslf);
    fn charsfwd_crosslf<B: Buffer>() {
        let buf = B::from_string("01234\n56789".to_string());
        let mut it = buf.chars_fwd(DocPos { x: 0, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '0')));
        assert_eq!(it.next(), Some((DocPos { x: 1, y: 0 }, '1')));
        assert_eq!(it.next(), Some((DocPos { x: 2, y: 0 }, '2')));
        assert_eq!(it.next(), Some((DocPos { x: 3, y: 0 }, '3')));
        assert_eq!(it.next(), Some((DocPos { x: 4, y: 0 }, '4')));
        assert_eq!(it.next(), Some((DocPos { x: 5, y: 0 }, '\n')));
        assert_eq!(it.next(), Some((DocPos { x: 0, y: 1 }, '5')));
        assert_eq!(it.next(), Some((DocPos { x: 1, y: 1 }, '6')));
        assert_eq!(it.next(), Some((DocPos { x: 2, y: 1 }, '7')));
        assert_eq!(it.next(), Some((DocPos { x: 3, y: 1 }, '8')));
        assert_eq!(it.next(), Some((DocPos { x: 4, y: 1 }, '9')));
        assert_eq!(it.next(), Some((DocPos { x: 5, y: 1 }, '\n')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    polytest!(charsfwd_empty);
    fn charsfwd_empty<B: Buffer>() {
        let buf = B::from_string("".to_string());
        let mut it = buf.chars_fwd(DocPos { x: 0, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '\n')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    polytest!(charsfwd_eol);
    fn charsfwd_eol<B: Buffer>() {
        let buf = B::from_string("01\n34".to_string());
        let mut it = buf.chars_fwd(DocPos { x: 2, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 2, y: 0 }, '\n')));
        assert_eq!(it.next(), Some((DocPos { x: 0, y: 1 }, '3')));
        assert_eq!(it.next(), Some((DocPos { x: 1, y: 1 }, '4')));
        assert_eq!(it.next(), Some((DocPos { x: 2, y: 1 }, '\n')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    polytest!(charsbck_empty);
    fn charsbck_empty<B: Buffer>() {
        let buf = B::from_string("".to_string());
        let mut it = buf.chars_bck(DocPos { x: 0, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '\n')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    polytest!(charsbck_eol);
    fn charsbck_eol<B: Buffer>() {
        let buf = B::from_string("01\n34".to_string());
        let mut it = buf.chars_bck(DocPos { x: 2, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 2, y: 0 }, '\n')));
        assert_eq!(it.next(), Some((DocPos { x: 1, y: 0 }, '1')));
        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '0')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    polytest!(charsbck_crosslf);
    fn charsbck_crosslf<B: Buffer>() {
        let buf = B::from_string("01234\n56789".to_string());
        let mut it = buf.chars_bck(DocPos { x: 5, y: 1 });

        assert_eq!(it.next(), Some((DocPos { x: 5, y: 1 }, '\n')));
        assert_eq!(it.next(), Some((DocPos { x: 4, y: 1 }, '9')));
        assert_eq!(it.next(), Some((DocPos { x: 3, y: 1 }, '8')));
        assert_eq!(it.next(), Some((DocPos { x: 2, y: 1 }, '7')));
        assert_eq!(it.next(), Some((DocPos { x: 1, y: 1 }, '6')));
        assert_eq!(it.next(), Some((DocPos { x: 0, y: 1 }, '5')));
        assert_eq!(it.next(), Some((DocPos { x: 5, y: 0 }, '\n')));
        assert_eq!(it.next(), Some((DocPos { x: 4, y: 0 }, '4')));
        assert_eq!(it.next(), Some((DocPos { x: 3, y: 0 }, '3')));
        assert_eq!(it.next(), Some((DocPos { x: 2, y: 0 }, '2')));
        assert_eq!(it.next(), Some((DocPos { x: 1, y: 0 }, '1')));
        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '0')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    polytest!(charsbck_end);
    fn charsbck_end<B: Buffer>() {
        let buf = B::from_string("0123456789".to_string());
        let mut it = buf.chars_bck(DocPos { x: 10, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 10, y: 0 }, '\n')));
        assert_eq!(it.next(), Some((DocPos { x: 9, y: 0 }, '9')));
        assert_eq!(it.next(), Some((DocPos { x: 8, y: 0 }, '8')));
        assert_eq!(it.next(), Some((DocPos { x: 7, y: 0 }, '7')));
        assert_eq!(it.next(), Some((DocPos { x: 6, y: 0 }, '6')));
        assert_eq!(it.next(), Some((DocPos { x: 5, y: 0 }, '5')));
        assert_eq!(it.next(), Some((DocPos { x: 4, y: 0 }, '4')));
        assert_eq!(it.next(), Some((DocPos { x: 3, y: 0 }, '3')));
        assert_eq!(it.next(), Some((DocPos { x: 2, y: 0 }, '2')));
        assert_eq!(it.next(), Some((DocPos { x: 1, y: 0 }, '1')));
        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '0')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    polytest!(charsbck_mid);
    fn charsbck_mid<B: Buffer>() {
        let buf = B::from_string("0123456789".to_string());
        let mut it = buf.chars_bck(DocPos { x: 5, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 5, y: 0 }, '5')));
        assert_eq!(it.next(), Some((DocPos { x: 4, y: 0 }, '4')));
        assert_eq!(it.next(), Some((DocPos { x: 3, y: 0 }, '3')));
        assert_eq!(it.next(), Some((DocPos { x: 2, y: 0 }, '2')));
        assert_eq!(it.next(), Some((DocPos { x: 1, y: 0 }, '1')));
        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '0')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    polytest!(charsbck_start);
    fn charsbck_start<B: Buffer>() {
        let buf = B::from_string("0123456789".to_string());
        let mut it = buf.chars_bck(DocPos { x: 0, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '0')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    polytest!(end_blank);
    fn end_blank<B: Buffer>() {
        let buf = B::from_string("".to_string());

        assert_eq!(buf.end(), DocPos { x: 0, y: 0 });
    }

    polytest!(end_simple);
    fn end_simple<B: Buffer>() {
        let buf = B::from_string("0123456789".to_string());

        assert_eq!(buf.end(), DocPos { x: 10, y: 0 });
    }

    polytest!(end_complex);
    fn end_complex<B: Buffer>() {
        let buf: B = buffer_with_changes();

        assert_eq!(buf.end(), DocPos { x: 10, y: 97 });
    }
}
