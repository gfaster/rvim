use std::ops::RangeBounds;

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
pub type Buffer = rope::RopeBuffer;

// pub use piecetable::PTBuffer;
// mod piecetable;

pub use rope::RopeBuffer;
mod rope;
// mod simplebuffer;




#[cfg(test)]
pub(crate) mod test {
    // declared public to allow export of polytest
    //
    // If I ever make the buffer a type alias rather than a trait, then the polytest macro should
    // only be used here, and made private again


    use super::*;
    use crate::render::BufId;
    use crate::window::BufCtx;

    /// make a generic test function run over all buffer implementations
    #[allow(unused)]
    macro_rules! polytest {
        ($func:ident) => {
        };
    }

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

    fn buffer_with_changes() -> Buffer {
        let mut b =
            Buffer::from_string(include_str!("../../assets/test/passage_wrapped.txt").to_string());
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

    #[test]
    fn get_lines_blank() {
        let buf = Buffer::from_string("".to_string());
        assert_eq!(buf.get_lines(0..1), vec![""]);
    }

    #[test]
    fn get_lines_single() {
        let buf = Buffer::from_string("asdf".to_string());
        assert_eq!(buf.get_lines(0..1), vec!["asdf"]);
    }

    #[test]
    fn get_lines_multiple() {
        let buf = Buffer::from_string("asdf\nabcd\nefgh".to_string());
        assert_eq!(buf.get_lines(0..3), vec!["asdf", "abcd", "efgh"]);
    }

    #[test]
    fn get_lines_single_middle() {
        let buf = Buffer::from_string("asdf\nabcd\nefgh".to_string());
        assert_eq!(buf.get_lines(1..2), vec!["abcd"]);
    }

    #[test]
    fn get_lines_multiple_middle() {
        let buf = Buffer::from_string("asdf\nabcd\nefgh\n1234".to_string());
        assert_eq!(buf.get_lines(1..3), vec!["abcd", "efgh"]);
    }

    #[test]
    fn insert_basic() {
        let mut buf = Buffer::from_string("".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "Hello, World");
    }

    #[test]
    fn insert_blank() {
        let mut buf = Buffer::from_string("".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "");
    }

    #[test]
    fn insert_multi() {
        let mut buf = Buffer::from_string("".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "Hello, ");
        assert_trait_add_str(&mut buf, &mut ctx, "World!");
    }

    #[test]
    fn insert_newl() {
        let mut buf = Buffer::from_string("".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "\n");
    }

    #[test]
    fn insert_multinewl() {
        let mut buf = Buffer::from_string("".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "\n");
        assert_trait_add_str(&mut buf, &mut ctx, "\n");
        assert_trait_add_str(&mut buf, &mut ctx, "\n");
    }

    #[test]
    fn insert_offset() {
        let mut buf = Buffer::from_string("0123456789".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 5, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "0000000");
    }

    #[test]
    fn insert_offnewl() {
        let mut buf = Buffer::from_string("0123456789".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 5, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "\n");
    }

    #[test]
    fn insert_prenewl() {
        let mut buf = Buffer::from_string("0123456789".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "\n");
    }

    #[test]
    fn insert_multilinestr() {
        let mut buf = Buffer::from_string("0123456789".to_string());
        let mut ctx = BufCtx {
            buf_id: BufId::new(),
            cursorpos: DocPos { x: 0, y: 0 },
            topline: 0,
        };

        assert_trait_add_str(&mut buf, &mut ctx, "asdf\nzdq\nqwrpi\nmnbv\n");
        assert_trait_add_str(&mut buf, &mut ctx, "\n\n\n104a9zlq");
    }

    #[test]
    fn charsfwd_start() {
        let buf = Buffer::from_string("0123456789".to_string());
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

    #[test]
    fn charsfwd_crosslf() {
        let buf = Buffer::from_string("01234\n56789".to_string());
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

    #[test]
    fn charsfwd_empty() {
        let buf = Buffer::from_string("".to_string());
        let mut it = buf.chars_fwd(DocPos { x: 0, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '\n')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn charsfwd_eol() {
        let buf = Buffer::from_string("01\n34".to_string());
        let mut it = buf.chars_fwd(DocPos { x: 2, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 2, y: 0 }, '\n')));
        assert_eq!(it.next(), Some((DocPos { x: 0, y: 1 }, '3')));
        assert_eq!(it.next(), Some((DocPos { x: 1, y: 1 }, '4')));
        assert_eq!(it.next(), Some((DocPos { x: 2, y: 1 }, '\n')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn charsbck_empty() {
        let buf = Buffer::from_string("".to_string());
        let mut it = buf.chars_bck(DocPos { x: 0, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '\n')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn charsbck_eol() {
        let buf = Buffer::from_string("01\n34".to_string());
        let mut it = buf.chars_bck(DocPos { x: 2, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 2, y: 0 }, '\n')));
        assert_eq!(it.next(), Some((DocPos { x: 1, y: 0 }, '1')));
        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '0')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn charsbck_crosslf() {
        let buf = Buffer::from_string("01234\n56789".to_string());
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

    #[test]
    fn charsbck_end() {
        let buf = Buffer::from_string("0123456789".to_string());
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

    #[test]
    fn charsbck_mid() {
        let buf = Buffer::from_string("0123456789".to_string());
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
        let buf = Buffer::from_string("0123456789".to_string());
        let mut it = buf.chars_bck(DocPos { x: 0, y: 0 });

        assert_eq!(it.next(), Some((DocPos { x: 0, y: 0 }, '0')));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn end_blank() {
        let buf = Buffer::from_string("".to_string());

        assert_eq!(buf.end(), DocPos { x: 0, y: 0 });
    }

    #[test]
    fn end_simple() {
        let buf = Buffer::from_string("0123456789".to_string());

        assert_eq!(buf.end(), DocPos { x: 10, y: 0 });
    }

    #[test]
    fn end_complex() {
        let buf: Buffer = buffer_with_changes();

        assert_eq!(buf.end(), DocPos { x: 10, y: 97 });
    }

    #[test]
    fn path_none() {
        let buf = Buffer::from_string("0123456789".to_string());
        assert_eq!(buf.path(), None);
    }
}
