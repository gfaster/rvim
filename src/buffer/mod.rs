use std::ops::RangeBounds;
use crate::prelude::*;

/// Position in a document - similar to TermPos but distinct enough semantically to deserve its own
/// struct. In the future, wrapping will mean that DocPos and TermPos will often not correspond
/// one-to-one. Also, using usize since it can very well be more than u32::max (though not for now)
#[derive(Ord, Debug, Clone, Copy, PartialEq, Eq, Default)]
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

impl DocPos {
    fn new() -> Self {
        Self::default()
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
// pub type Buffer = rope::RopeBuffer;
pub type Buffer = simplebuffer::SimpleBuffer;

// pub use piecetable::PTBuffer;
// mod piecetable;

pub use rope::RopeBuffer;
mod rope;
mod simplebuffer;

pub trait Buf: Sized {
    fn new() -> Self;
    fn name(&self) -> &str;
    fn open(file: &std::path::Path) -> std::io::Result<Self>;
    fn from_string(s: String) -> Self;
    fn from_str(s: &str) -> Self;
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()>;
    fn get_lines(&self, lines: std::ops::Range<usize>) -> Vec<&str>;
    fn delete_char(&mut self, ctx: &mut crate::window::BufCtx) -> char;
    fn delete_char_before(&mut self, ctx: &mut crate::window::BufCtx) -> Option<char>;
    fn get_off(&self, pos: DocPos) -> usize;
    fn linecnt(&self) -> usize;
    fn end(&self) -> DocPos;
    fn last(&self) -> DocPos;
    fn insert_str(&mut self, ctx: &mut crate::window::BufCtx, s: &str);
    fn path(&self) -> Option<&std::path::Path>;
    fn len(&self) -> usize;
    fn clear(&mut self, ctx: &mut BufCtx);

    fn line(&self, idx: usize) -> &str {
        self.get_lines(idx..idx)[0]
    }

    /// push a character onto the end
    fn push(&mut self, ctx: &mut BufCtx, c: char) {
        let mut tmp = [0; 4];
        self.insert_str(ctx, c.encode_utf8(&mut tmp))
    }

    /// pop a character from the end
    fn pop(&mut self, ctx: &mut BufCtx) -> char {
        ctx.cursorpos = self.last();
        ctx.virtual_pos = ctx.cursorpos;
        self.delete_char(ctx)
    }
}

impl std::fmt::Display for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = Vec::<u8>::new();
        self.serialize(&mut out).unwrap();
        std::fmt::Display::fmt(&String::from_utf8_lossy(&out), f)
    }
}

impl std::default::Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LinesInclusiveIter<'a>(std::str::SplitInclusive<'a, char>);

impl<'a> Iterator for LinesInclusiveIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    fn last(mut self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.next_back()
    }
}

impl DoubleEndedIterator for LinesInclusiveIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}

pub trait LinesInclusive {
    /// returns an iterator over every line, including the trailing LF
    fn lines_inclusive(&self) -> LinesInclusiveIter;
}

impl LinesInclusive for str {
    fn lines_inclusive(&self) -> LinesInclusiveIter {
        LinesInclusiveIter(self.split_inclusive('\n'))
    }
}

#[cfg(test)]
pub(crate) mod test {
    // declared public to allow export of polytest
    //
    // If I ever make the buffer a type alias rather than a trait, then the polytest macro should
    // only be used here, and made private again

    use super::*;
    use crate::render::BufId;
    use crate::window::BufCtx;

    #[track_caller]
    fn assert_buf_eq(b: &Buffer, s: &str) -> String {
        let mut out = Vec::<u8>::new();
        b.serialize(&mut out)
            .expect("buffer will successfully serialize");
        let buf_str = String::from_utf8(out).expect("buffer outputs valid utf-8");
        assert_eq!(buf_str, s);
        buf_str
    }

    fn assert_trait_add_str(b: &mut Buffer, ctx: &mut BufCtx, s: &str) {
        let mut out = Vec::<u8>::new();
        b.serialize(&mut out).expect("buffer will serialize");
        let mut buf_str = String::from_utf8(out.clone()).expect("buffer outputs valid utf-8");

        let pos = ctx.cursorpos;
        let off = pos.x
            + buf_str
                .lines_inclusive()
                .take(pos.y)
                .map(str::len)
                .sum::<usize>();
        buf_str.replace_range(off..off, s);
        b.insert_str(ctx, s);

        out.clear();
        b.serialize(&mut out).expect("buffer will serialize");
        let out_str = String::from_utf8(out).expect("buffer outputs valid utf-8");

        assert_eq!(
            buf_str, out_str,
            "inserted string == string insert from buffer"
        );
    }

    fn buffer_with_changes() -> Buffer {
        let mut b = Buffer::from_str(include_str!("../../assets/test/passage_wrapped.txt"));
        let mut ctx = BufCtx {
            cursorpos: DocPos { x: 8, y: 12 },
            ..BufCtx::new(BufId::new())
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

    #[test]
    #[ignore = "think about correct behavior"]
    fn get_lines_blank() {
        let buf = Buffer::from_str("");
        assert_eq!(buf.get_lines(0..1), vec![""]);
    }

    #[test]
    fn get_lines_single() {
        let buf = Buffer::from_str("asdf");
        assert_eq!(buf.get_lines(0..1), vec!["asdf"]);
    }

    #[test]
    fn get_lines_multiple() {
        let buf = Buffer::from_str("asdf\nabcd\nefgh");
        assert_eq!(buf.get_lines(0..3), vec!["asdf", "abcd", "efgh"]);
    }

    #[test]
    fn get_lines_single_middle() {
        let buf = Buffer::from_str("asdf\nabcd\nefgh");
        assert_eq!(buf.get_lines(1..2), vec!["abcd"]);
    }

    #[test]
    fn get_lines_multiple_middle() {
        let buf = Buffer::from_str("asdf\nabcd\nefgh\n1234");
        assert_eq!(buf.get_lines(1..3), vec!["abcd", "efgh"]);
    }

    #[test]
    fn insert_basic() {
        let mut buf = Buffer::from_str("");
        let mut ctx = BufCtx {
            ..BufCtx::new(BufId::new())
        };

        assert_trait_add_str(&mut buf, &mut ctx, "Hello, World");
    }

    #[test]
    fn insert_blank() {
        let mut buf = Buffer::from_str("");
        let mut ctx = BufCtx {
            ..BufCtx::new(BufId::new())
        };

        assert_trait_add_str(&mut buf, &mut ctx, "");
    }

    #[test]
    fn insert_multi() {
        let mut buf = Buffer::from_str("");
        let mut ctx = BufCtx {
            ..BufCtx::new(BufId::new())
        };

        assert_trait_add_str(&mut buf, &mut ctx, "Hello, ");
        assert_trait_add_str(&mut buf, &mut ctx, "World!");
    }

    #[test]
    fn insert_newl() {
        let mut buf = Buffer::from_str("");
        let mut ctx = BufCtx {
            ..BufCtx::new(BufId::new())
        };

        assert_trait_add_str(&mut buf, &mut ctx, "\n");
    }

    #[test]
    fn insert_multinewl() {
        let mut buf = Buffer::from_str("");
        let mut ctx = BufCtx {
            ..BufCtx::new(BufId::new())
        };

        assert_trait_add_str(&mut buf, &mut ctx, "\n");
        assert_trait_add_str(&mut buf, &mut ctx, "\n");
        assert_trait_add_str(&mut buf, &mut ctx, "\n");
    }

    #[test]
    fn insert_offset() {
        let mut buf = Buffer::from_str("0123456789");
        let mut ctx = BufCtx {
            cursorpos: DocPos { x: 5, y: 0 },
            ..BufCtx::new(BufId::new())
        };

        assert_trait_add_str(&mut buf, &mut ctx, "0000000");
    }

    #[test]
    fn insert_offnewl() {
        let mut buf = Buffer::from_str("0123456789");
        let mut ctx = BufCtx {
            cursorpos: DocPos { x: 5, y: 0 },
            ..BufCtx::new(BufId::new())
        };

        assert_trait_add_str(&mut buf, &mut ctx, "\n");
    }

    #[test]
    fn insert_prenewl() {
        let mut buf = Buffer::from_str("0123456789");
        let mut ctx = BufCtx {
            cursorpos: DocPos { x: 0, y: 0 },
            ..BufCtx::new(BufId::new())
        };

        assert_trait_add_str(&mut buf, &mut ctx, "\n");
    }

    #[test]
    fn insert_multilinestr() {
        let mut buf = Buffer::from_str("0123456789");
        let mut ctx = BufCtx {
            ..BufCtx::new(BufId::new())
        };

        assert_trait_add_str(&mut buf, &mut ctx, "asdf\nzdq\nqwrpi\nmnbv\n");
        assert_trait_add_str(&mut buf, &mut ctx, "\n\n\n104a9zlq");
    }

    #[test]
    fn charsfwd_start() {
        let buf = Buffer::from_str("0123456789");
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
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn charsfwd_crosslf() {
        let buf = Buffer::from_str("01234\n56789");
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
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn charsfwd_empty() {
        let buf = Buffer::from_str("");
        let mut it = buf.chars_fwd(DocPos { x: 0, y: 0 });

        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn charsfwd_eol() {
        let buf = Buffer::from_str("01\n34");
        let mut it = buf.chars_fwd(DocPos { x: 2, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 2, y: 0 }, '\n')));
        assert_eq!(it.next(), Some((DocPos { x: 0, y: 1 }, '3')));
        assert_eq!(it.next(), Some((DocPos { x: 1, y: 1 }, '4')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn charsbck_empty() {
        let buf = Buffer::from_str("");
        let mut it = buf.chars_bck(DocPos { x: 0, y: 0 });

        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn charsbck_eol() {
        let buf = Buffer::from_str("01\n34");
        let mut it = buf.chars_bck(DocPos { x: 1, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 1, y: 0 }, '1')));
        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '0')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn charsbck_crosslf() {
        let buf = Buffer::from_str("01234\n56789");
        let mut it = buf.chars_bck(DocPos { x: 4, y: 1 });

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

    #[test]
    fn charsbck_end() {
        let buf = Buffer::from_str("0123456789");
        let mut it = buf.chars_bck(DocPos { x: 9, y: 0 });

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

    #[test]
    fn charsbck_mid() {
        let buf = Buffer::from_str("0123456789");
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

    #[test]
    fn charsbck_start() {
        let buf = Buffer::from_str("0123456789");
        let mut it = buf.chars_bck(DocPos { x: 0, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '0')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn end_blank() {
        let buf = Buffer::from_str("");

        assert_eq!(buf.end(), DocPos { x: 0, y: 0 });
    }

    #[test]
    fn end_simple() {
        let buf = Buffer::from_str("0123456789");

        assert_eq!(buf.end(), DocPos { x: 10, y: 0 });
    }

    #[test]
    #[ignore = "I don't understand it"]
    fn end_complex() {
        let buf: Buffer = buffer_with_changes();

        assert_eq!(buf.end(), DocPos { x: 10, y: 97 });
    }

    #[test]
    fn path_none() {
        let buf = Buffer::from_str("0123456789");
        assert_eq!(buf.path(), None);
    }

    #[test]
    fn last_single() {
        let buf = Buffer::from_str("0123456789");
        assert_eq!(buf.last(), DocPos { x: 9, y: 0 })
    }

    #[test]
    fn last_multiline() {
        let buf = Buffer::from_str("0123456789\nasdf");
        assert_eq!(buf.last(), DocPos { x: 3, y: 1 })
    }

    #[test]
    fn delete_char() {
        let mut buf = Buffer::from_str("0123456789\nasdf");
        let expected = "012346789\nasdf";
        let mut ctx = BufCtx {
            cursorpos: DocPos { x: 5, y: 0},
            ..BufCtx::new(BufId::new())
        };
        assert_eq!(buf.delete_char(&mut ctx), '5');
        assert_eq!(ctx.cursorpos, DocPos { x: 4, y: 0 });
        assert_buf_eq(&buf, expected);
    }

    #[test]
    fn delete_char_first_of_line() {
        let mut buf = Buffer::from_str("0123456789\nasdf");
        let expected = "0123456789\nsdf";
        let mut ctx = BufCtx {
            cursorpos: DocPos { x: 0, y: 1},
            ..BufCtx::new(BufId::new())
        };
        assert_eq!(buf.delete_char(&mut ctx), 'a');
        assert_eq!(ctx.cursorpos, DocPos { x: 0, y: 1 });
        assert_buf_eq(&buf, expected);
    }

    #[test]
    fn delete_char_newl() {
        let mut buf = Buffer::from_str("0123456789\nasdf");
        let expected = "0123456789asdf";
        let mut ctx = BufCtx {
            cursorpos: DocPos { x: 10, y: 0},
            ..BufCtx::new(BufId::new())
        };
        assert_eq!(buf.delete_char(&mut ctx), '\n');
        assert_eq!(ctx.cursorpos, DocPos { x: 9, y: 0 });
        assert_buf_eq(&buf, expected);
    }

    #[test]
    fn delete_char_just_newl() {
        let mut buf = Buffer::from_str("\n\n\n");
        let expected = "\n\n";
        let mut ctx = BufCtx {
            cursorpos: DocPos { x: 0, y: 1},
            ..BufCtx::new(BufId::new())
        };
        assert_eq!(buf.delete_char(&mut ctx), '\n');
        assert_eq!(ctx.cursorpos, DocPos { x: 0, y: 0 });
        assert_buf_eq(&buf, expected);
    }

    #[test]
    fn delete_char_first() {
        let mut buf = Buffer::from_str("asdf");
        let expected = "sdf";
        let mut ctx = BufCtx {
            cursorpos: DocPos { x: 0, y: 0},
            ..BufCtx::new(BufId::new())
        };
        assert_eq!(buf.delete_char(&mut ctx), 'a');
        assert_eq!(ctx.cursorpos, DocPos { x: 0, y: 0 });
        assert_buf_eq(&buf, expected);
    }

    #[test]
    fn delete_char_only() {
        let mut buf = Buffer::from_str(" ");
        let expected = "";
        let mut ctx = BufCtx {
            cursorpos: DocPos { x: 0, y: 0},
            ..BufCtx::new(BufId::new())
        };
        assert_eq!(buf.delete_char(&mut ctx), ' ');
        assert_eq!(ctx.cursorpos, DocPos { x: 0, y: 0 });
        assert_buf_eq(&buf, expected);
    }

    #[test]
    fn delete_char_only_lf() {
        let mut buf = Buffer::from_str("\n");
        let expected = "";
        let mut ctx = BufCtx {
            ..BufCtx::new(BufId::new())
        };
        assert_eq!(buf.delete_char(&mut ctx), '\n');
        assert_eq!(ctx.cursorpos, DocPos { x: 0, y: 0 });
        assert_buf_eq(&buf, expected);
    }

    #[test]
    fn len() {
        let init = "this is a buffer\nasdfasdfasdfa";
        let buf = Buffer::from_str(init);
        assert_eq!(buf.len(), init.len());
    }

    #[test]
    fn clear() {
        let mut buf = Buffer::from_str("this is a buffer\nit will be cleared.");
        let mut ctx = BufCtx {
            cursorpos: DocPos {x: 5, y: 1},
            ..BufCtx::new(BufId::new())
        };
        buf.clear(&mut ctx);
        assert_eq!(&buf.to_string(), "");
        assert_eq!(ctx.cursorpos, DocPos::new());
        assert_eq!(buf.len(), 0);
    }

    mod lines_inclusive {
        use super::*;

        macro_rules! lines_test {
            ($(#[$meta:meta])* $name:ident: $($part:literal)*) => {
                #[test]
                $(#[$meta])*
                fn $name() {
                    let orig = concat!($($part, )*);
                    let mut it = orig.lines_inclusive();
                    #[allow(unused_mut)]
                    let mut count = 0;
                    $(
                        assert_eq!(it.next(), Some($part), "part {count} doesn't match");
                        count += 1;
                    )*
                    let _ = count;
                    assert_eq!(it.next(), None);
                    assert_eq!(it.next(), None);
                }
            };
        }

        lines_test!(oneline: "asdf");
        lines_test!(trailing_lf: "asdf\n");
        lines_test!(multiline: "asdf\n" "basdf");
        lines_test!(multiline_trailing_lf: "asdf\n" "basdf\n");
        lines_test!(#[ignore = "think about correct behavior"] blank: );
        lines_test!(just_lf: "\n");
        lines_test!(just_lf_many: "\n" "\n" "\n");
        lines_test!(multi_blank_in_middle: "hello\n" "\n" "\n" "world");
        lines_test!(leading_lf: "\n" "\n" "hello\n" "world\n");
    }
}
