use crate::{prelude::*, render::BufId, term::TermPos, window::WindowInner};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::{cell::Cell, ops::RangeBounds};
use std::{
    fmt::{Display, Write},
    ops::Range,
};

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

impl DocPos {
    pub fn row(&self) -> usize {
        self.y + 1
    }
    pub fn col(&self) -> usize {
        self.x + 1
    }
}

/// trait for file offset. We don't implement this for [`DocPos`] since it has too many edge cases.
pub trait FileOff: Copy {
    /// file offsets are constrained to representing one byte past the end of the file so that we
    /// can represent ranges. Panics if it corresponds to an invalid position. .
    fn pos(&self, buf: &BufferInner) -> DocPos {
        self.try_pos(buf).expect("Valid position")
    }

    /// like `pos`, but returns `None` on invalid position.
    fn try_pos(&self, buf: &BufferInner) -> Option<DocPos>;
}

impl FileOff for usize {
    fn try_pos(&self, buf: &BufferInner) -> Option<DocPos> {
        if *self > buf.len() {
            None
        } else {
            Some(buf.text.offset_to_pos(*self))
        }
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
type BufferCore = simplebuffer::SimpleBuffer;

// pub use piecetable::PTBuffer;
// mod piecetable;

pub use rope::RopeBuffer;
mod rope;
mod simplebuffer;

pub trait BufCore: Sized {
    fn new() -> Self;
    fn name(&self) -> &str;
    fn open(file: &std::path::Path) -> std::io::Result<Self>;
    fn from_str(s: impl AsRef<str>) -> Self;
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()>;
    fn get_lines(&self, lines: std::ops::Range<usize>) -> Vec<&str>;

    fn delete_range(&mut self, rng: Range<usize>) -> String;
    fn delete_char(&mut self, pos: usize) -> char;
    fn get_range(&self, rng: Range<usize>) -> String;
    fn get_char(&self, pos: usize) -> char;
    fn linecnt(&self) -> usize;
    fn insert_str(&mut self, ctx: &mut Cursor, s: &str);
    fn path(&self) -> Option<&std::path::Path>;
    fn set_path(&mut self, path: std::path::PathBuf);
    fn len(&self) -> usize;
    fn clear(&mut self, ctx: &mut Cursor);

    /// return [`DocPos`] corresponding to the offset, panics if `off > self.len()`
    fn offset_to_pos(&self, off: usize) -> DocPos;
    fn try_pos_to_offset(&self, pos: DocPos) -> Option<usize>;
    fn pos_to_offset(&self, pos: DocPos) -> usize;

    fn line(&self, idx: usize) -> &str {
        self.get_lines(idx..(idx + 1))[0]
    }
}

impl std::fmt::Display for BufferCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = Vec::<u8>::new();
        self.serialize(&mut out).unwrap();
        std::fmt::Display::fmt(&String::from_utf8_lossy(&out), f)
    }
}

impl std::default::Default for BufferCore {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Buffer {
    id: BufId,
    inner: RwLock<BufferInner>
}

impl Buffer {
    pub fn id(&self) -> BufId {
        self.id
    }

    pub fn new() -> Arc<Self> {
        Buffer {
            id: BufId::new(),
            inner: BufferInner::new().into()
        }.into()
    }

    pub fn get(&self) -> RwLockReadGuard<BufferInner> {
        self.inner.try_read().expect("Same-thread deadlock")
    }

    pub fn get_mut(&self) -> RwLockWriteGuard<BufferInner> {
        self.inner.try_write().expect("Same-thread deadlock")
    }

    pub fn open(file: &std::path::Path) -> std::io::Result<Arc<Self>> {
        Ok(Buffer { inner: BufferInner::open(file)?.into(), id: BufId::new() }.into())
    }
}

/// View of a buffer that includes its cursor. I may change this to allow the cursor to have
/// interior mutability
pub struct BufferInner {
    next: Option<Arc<Buffer>>,
    prev: Option<Arc<Buffer>>,
    pub cursor: Cursor,
    text: BufferCore,
}

impl Display for BufferInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <BufferCore as Display>::fmt(&self.text, f)
    }
}

impl BufferInner {
    pub fn new() -> Self {
        BufferInner {
            cursor: Cursor::new(),
            text: BufferCore::new(),
            next: None,
            prev: None,
        }
    }

    pub fn open(file: &std::path::Path) -> std::io::Result<Self> {
        Ok(BufferInner {
            cursor: Cursor::new(),
            text: BufferCore::open(file)?,
            next: None,
            prev: None,
        })
    }

    pub fn from_string(s: impl AsRef<str>) -> Self {
        BufferInner {
            cursor: Cursor::new(),
            text: BufferCore::from_str(s),
            next: None,
            prev: None,
        }
    }

    pub fn from_str(s: &str) -> Self {
        BufferInner {
            cursor: Cursor::new(),
            text: BufferCore::from_str(s),
            next: None,
            prev: None,
        }
    }

    pub fn name(&self) -> &str {
        self.text.name()
    }

    pub fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.text.serialize(writer)
    }

    pub fn get_lines(&self, lines: std::ops::Range<usize>) -> Vec<&str> {
        self.text.get_lines(lines)
    }

    /// delete the character the cursor is on. This is the behavior of 'x' key. The cursor will
    /// keep its position unless its the last non-lf character of the line, in which case it will
    /// be clamped to the line.
    pub fn delete_char(&mut self) -> Option<char> {
        if self.text.len() == 0 {
            return None;
        }
        let len = self.text.line(self.cursor.pos.y).len();
        let res = self.text.delete_char(self.text.pos_to_offset(self.cursor.pos));
        if Some(self.cursor.pos.x) == len.checked_sub(1) {
            self.cursor.pos.x = self.cursor.pos.x.saturating_sub(1);
        };
        Some(res)
    }

    /// delete the character before the cursor's current position. This is the behavior of
    /// backspace in insert mode.
    pub fn delete_char_before(&mut self) -> Option<char> {
        let off = self.text.pos_to_offset(self.cursor.pos).checked_sub(1)?;
        let new_pos = self
            .text
            .offset_to_pos(off);
        self.cursor.set_pos(new_pos);
        Some(self.text.delete_char(off))
    }

    pub fn linecnt(&self) -> usize {
        self.text.linecnt()
    }

    pub fn end(&self) -> DocPos {
        self.text.offset_to_pos(self.text.len())
    }

    pub fn last(&self) -> Option<DocPos> {
        Some(self.text.offset_to_pos(self.text.len().checked_sub(1)?))
    }

    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(&mut self.cursor, s)
    }

    pub fn path(&self) -> Option<&std::path::Path> {
        self.text.path()
    }

    pub fn set_path(&mut self, path: std::path::PathBuf) {
        self.text.set_path(path)
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn clear(&mut self) {
        self.text.clear(&mut self.cursor)
    }

    pub fn char_at(&self, off: usize) -> char {
        self.text.get_char(off)
    }

    pub fn line(&self, idx: usize) -> &str {
        self.get_lines(idx..(idx + 1))[0]
    }

    /// push a character onto the end
    pub fn push(&mut self, c: char) {
        self.text
            .insert_str(&mut self.cursor, c.encode_utf8(&mut [0; 4]))
    }

    /// pop a character from the end
    pub fn pop(&mut self) -> Option<char> {
        let last = self.last()?;
        self.cursor.set_pos(last);
        let ret = self.delete_char()?;
        self.cursor.set_pos(last);
        Some(ret)
    }

    pub fn delete_range(&mut self, range: impl RangeBounds<usize>) -> String {
        let start = match range.start_bound() {
            std::ops::Bound::Included(p) => *p,
            std::ops::Bound::Excluded(p) => *p + 1,
            std::ops::Bound::Unbounded => 0,
        }
        .min(self.text.len());
        let end = match range.end_bound() {
            std::ops::Bound::Included(p) => *p + 1,
            std::ops::Bound::Excluded(p) => *p ,
            std::ops::Bound::Unbounded => self.text.len(),
        }
        .min(self.text.len());
        let init_off = self.text.pos_to_offset(self.cursor.pos);

        let deleted = self.text.delete_range(start..end);
        let new_pos = init_off - init_off.saturating_sub(start).min(deleted.len());
        self.cursor.set_pos(self.text.offset_to_pos(new_pos));
        deleted
    }

    pub fn normalize_range(&self, range: impl RangeBounds<usize>) -> Range<usize> {
        let start = match range.start_bound() {
            std::ops::Bound::Included(p) => *p,
            std::ops::Bound::Excluded(p) => *p + 1,
            std::ops::Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            std::ops::Bound::Included(p) => *p + 1,
            std::ops::Bound::Excluded(p) => *p ,
            std::ops::Bound::Unbounded => self.text.len(),
        };
        start..end
    }

    pub fn clamp_normalize_range(&self, range: impl RangeBounds<usize>) -> Range<usize> {
        let Range { start, end } = self.normalize_range(range);
        start.min(self.text.len())..end.min(self.text.len())
    }

    /// draw this buffer in a window
    pub fn draw(&self, win: &WindowInner, ctx: &Ctx) {
        let mut tui = ctx.tui.borrow_mut();
        let _ = write!(tui.refbox(win.inner_bounds()), "{}", self.text);
    }

    pub fn chars_bck(&self, off: usize) -> impl Iterator<Item = char> + '_ {
        self.text.chars_bck(off)
    }

    pub fn chars_fwd(&self, off: usize) -> impl Iterator<Item = char> + '_ {
        self.text.chars_fwd(off)
    }

    pub fn offset_to_pos(&self, off: usize) -> DocPos {
        self.text.offset_to_pos(off)
    }

    pub fn pos_to_offset(&self, pos: DocPos) -> usize {
        self.text.pos_to_offset(pos)
    }

    pub fn try_pos_to_offset(&self, pos: DocPos) -> Option<usize> {
        self.text.try_pos_to_offset(pos)
    }

    /// current offset of cursor
    pub fn coff(&self) -> usize {
        self.pos_to_offset(self.cursor.pos)
    }
}

/// cursor in an active buffer
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    /// I use DocPos rather than a flat offset to more easily handle linewise operations, which
    /// seem to be more common than operations that operate on the flat buffer. It also makes
    /// translation more convienent, especially when the buffer is stored as an array of lines
    /// rather than a flat byte array (although it seems like this would slow transversal?).
    pub pos: DocPos,
    pub virtcol: usize,
    pub topline: usize,
}

impl Cursor {
    /// gets the relative position of the cursor when displayed in win
    pub fn win_pos(&self, _win: &WindowInner) -> TermPos {
        let y = self
            .pos
            .y
            .checked_sub(self.topline)
            .expect("tried to move cursor above window") as u32;
        // let y = y + win.bounds().start.y;
        // let x = self.pos.x as u32 + win.bounds().start.x;
        let x = self.pos.x as u32;
        TermPos { x, y }
    }

    /// gets the absolute position of the cursor relative to the origin of the window.
    pub fn term_pos(&self, win: &WindowInner) -> TermPos {
        let TermPos { x, y } = self.win_pos(win);
        let x = x + win.inner_bounds().start.x;
        let y = y + win.inner_bounds().start.y;
        TermPos { x, y }
    }

    pub fn new() -> Self {
        Self {
            pos: DocPos { x: 0, y: 0 },
            virtcol: 0,
            topline: 0,
        }
    }

    pub fn draw(&self, win: &WindowInner, tui: &mut TermGrid) {
        tui.set_cursorpos(self.term_pos(win));
    }

    /// sets the position and virtual positon to pos, updating topline if moved above but not if
    /// too far below
    pub fn set_pos(&mut self, pos: DocPos) {
        self.pos = pos;
        self.virtcol = pos.x;
        if self.topline > pos.y {
            self.topline = pos.y
        }
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
pub mod test {
    // declared public to allow export of polytest
    //
    // If I ever make the buffer a type alias rather than a trait, then the polytest macro should
    // only be used here, and made private again

    use super::*;
    use crate::render::BufId;

    #[track_caller]
    fn assert_buf_eq(b: &BufferCore, s: &str) -> String {
        let mut out = Vec::<u8>::new();
        b.serialize(&mut out)
            .expect("buffer will successfully serialize");
        let buf_str = String::from_utf8(out).expect("buffer outputs valid utf-8");
        assert_eq!(buf_str, s);
        buf_str
    }

    #[track_caller]
    fn assert_insert_str(b: &mut BufferInner, s: &str) {
        let mut buf_str = b.to_string();
        let off = b.text.pos_to_offset(b.cursor.pos);
        buf_str.replace_range(off..off, s);
        b.insert_str(s);
        assert_eq!(
            buf_str,
            b.to_string(),
            "inserted string == string insert from buffer"
        );
    }

    fn buffer_with_changes() -> BufferInner {
        let mut b = BufferInner::from_str(include_str!("../../assets/test/passage_wrapped.txt"));
        b.cursor.set_pos(DocPos { x: 8, y: 12 });
        assert_insert_str(&mut b, "This is some new text");
        assert_insert_str(&mut b, "This is some more new text");
        b.cursor.set_pos(DocPos { x: 3, y: 9 });
        assert_insert_str(&mut b, "This is some \nnewline text");
        assert_insert_str(&mut b, "This is some more newline text\n\n");
        b.cursor.set_pos(DocPos { x: 0, y: 0 });
        assert_insert_str(&mut b, "Some text at the beginning");
        b.cursor.set_pos(DocPos { x: 0, y: 0 });
        assert_insert_str(&mut b, "\nope - newl at the beginning");
        b.cursor.set_pos(DocPos { x: 18, y: 1 });
        assert_insert_str(&mut b, "Middle of another edit");
        assert_insert_str(&mut b, "and again at the end of the middle");
        b
    }

    macro_rules! mkbuf {
        ($fn:ident) => {
            $fn()
        };
        ($str:literal) => {
            BufferInner::from_str($str)
        };
    }

    fn normalize_range(s: &str, rng: impl RangeBounds<usize>) -> Range<usize> {
        let start = match rng.start_bound() {
            std::ops::Bound::Included(i) => *i,
            std::ops::Bound::Excluded(i) => *i + 1,
            std::ops::Bound::Unbounded => 0,
        }
        .min(s.len());
        let end = match rng.end_bound() {
            std::ops::Bound::Included(i) => *i + 1,
            std::ops::Bound::Excluded(i) => *i,
            std::ops::Bound::Unbounded => s.len(),
        }
        .clamp(start, s.len());
        start..end
    }

    /// get [`DocPos`] of offset in `&str`
    fn str_doc_pos_off(s: &str, off: usize) -> DocPos {
        let off = off.min(s.len());
        s.lines_inclusive()
            .map(str::len)
            .fold((0, DocPos { x: 0, y: 0 }), |(total, doc), l| {
                if total > off {
                    unreachable!()
                };
                if total == off {
                    (total, doc)
                } else if total + l > off {
                    (
                        off,
                        DocPos {
                            x: off - total,
                            ..doc
                        },
                    )
                } else if total + l == off && off == s.len() {
                    (
                        off,
                        DocPos {
                            x: off - total,
                            ..doc
                        },
                    )
                } else {
                    (total + l, DocPos { x: 0, y: doc.y + 1 })
                }
            })
            .1
    }

    #[test]
    fn helper_str_doc_pos_off() {
        assert_eq!(str_doc_pos_off("as df", 0), DocPos { x: 0, y: 0 });
        assert_eq!(str_doc_pos_off("as df", 1), DocPos { x: 1, y: 0 });
        assert_eq!(str_doc_pos_off("as df", 2), DocPos { x: 2, y: 0 });
        assert_eq!(str_doc_pos_off("as\ndf", 2), DocPos { x: 2, y: 0 });
        assert_eq!(str_doc_pos_off("as\ndf", 3), DocPos { x: 0, y: 1 });
        assert_eq!(str_doc_pos_off("as\ndf", 4), DocPos { x: 1, y: 1 });
        assert_eq!(str_doc_pos_off("as\ndf", 5), DocPos { x: 2, y: 1 });
        assert_eq!(str_doc_pos_off("as\ndf", 6), DocPos { x: 2, y: 1 });
    }

    macro_rules! get_lines_test {
        ($(#[$meta:meta])* $name:ident, $bufdef:tt, $lines:expr) => {
            #[test]
            $(#[$meta])*
            fn $name() {
                let buf = mkbuf!($bufdef);
                let bstr = buf.to_string();
                let expected: Vec<_> = bstr.lines().skip($lines.start).take($lines.len()).collect();
                assert_eq!(buf.get_lines($lines), expected, "actual == expected");
            }
        };
    }

    get_lines_test!(
        #[ignore = "think about correct behavior"]
        get_lines_blank,
        "",
        0..1
    );
    get_lines_test!(get_lines_single, "asdf", 0..1);
    get_lines_test!(get_lines_multiple, "asdf\nabcd\nefgh", 0..3);
    get_lines_test!(get_lines_single_middle, "asdf\nabcd\nefgh", 1..2);
    get_lines_test!(get_lines_multiple_middle, "asdf\nabcd\nefgh\n1234", 1..3);
    get_lines_test!(get_lines_complex, buffer_with_changes, 3..12);

    macro_rules! insert_test {
        ($name:ident, $init:tt, $($rem:tt),* $(,)?) => {
            #[test]
            fn $name() {
                let mut buf = mkbuf!($init);
                insert_test!(@recurse buf @ $($rem),*);
            }
        };
        (@recurse $buf:ident @ (=> $off:literal) $(, $rem:tt)*) => {
            $buf.cursor.set_pos(str_doc_pos_off(&$buf.to_string(), $off));
            insert_test!(@recurse $buf @ $($rem),*);
        };
        (@recurse $buf:ident @ $add:expr $(, $rem:tt)*) => {
            assert_insert_str(&mut $buf, $add);
            insert_test!(@recurse $buf @ $($rem),*);
        };
        (@recurse $buf:ident @ ) => { };
    }

    insert_test!(insert_basic, "", "Hello, World");
    insert_test!(insert_blank, "", "");
    insert_test!(insert_multi, "", "Hello, ", "World!");
    insert_test!(insert_newl, "", "\n");
    insert_test!(insert_newl_multi, "", "\n", "\n", "\n");
    insert_test!(insert_offset, "0123456789", (=> 5), "000000");
    insert_test!(insert_offset_newl, "0123456789", (=> 5), "\n");
    insert_test!(insert_offset_prenewl, "0123456789", "\n");
    insert_test!(
        insert_multiline,
        "0123456789",
        "asdf\nzdq\nqwrpi\nmnbv\n",
        "\n\n\n104a9zlq"
    );
    insert_test!(
        insert_multiline_dirty,
        buffer_with_changes,
        "asdf\nzdq\nqwrpi\nmnbv\n",
        "\n\n\n104a9zlq"
    );

    macro_rules! chars_fwd_test {
        ($name: ident, $str:expr, $start:expr) => {
            #[test]
            fn $name() {
                let buf = BufferInner::from_str($str);
                let mut it_test = buf.chars_fwd($start);
                for c in $str[$start..].chars() {
                    assert_eq!(
                        it_test.next(),
                        Some(c),
                        "actual == expected"
                    );
                }
                assert_eq!(it_test.next(), None, "end of iter");
                assert_eq!(it_test.next(), None, "end of iter 2");
            }
        };
    }

    chars_fwd_test!(chars_fwd_start, "0123456789", 0);
    chars_fwd_test!(chars_fwd_mid, "0123456789", 5);
    chars_fwd_test!(chars_fwd_crosslf, "01234\n56789", 0);
    chars_fwd_test!(chars_fwd_empty, "", 0);
    chars_fwd_test!(chars_fwd_all_lf, "\n\n\n\n", 1);
    chars_fwd_test!(chars_fwd_start_eol, "01\n34", 2);
    chars_fwd_test!(chars_fwd_start_end, "0123456789", 9);

    macro_rules! chars_bck_test {
        ($name: ident, $init:tt, $start:expr) => {
            #[test]
            fn $name() {
                let buf = mkbuf!($init);
                let bufstr = buf.to_string();
                let mut it_test = buf.chars_bck($start);
                for c in bufstr[..($start + 1).min(bufstr.len())].chars().rev() {
                    dbg!(c);
                    assert_eq!(
                        it_test.next(),
                        Some(c),
                        "actual == expected"
                    );
                }
                assert_eq!(it_test.next(), None, "end of iter");
                assert_eq!(it_test.next(), None, "end of iter 2");
            }
        };
    }

    chars_bck_test!(chars_bck_start, "0123456789", 0);
    chars_bck_test!(chars_bck_end, "0123456789", 9);
    chars_bck_test!(chars_bck_crosslf, "0123\n56789", 7);
    chars_bck_test!(chars_bck_empty, "", 0);
    chars_bck_test!(chars_bck_all_lf, "\n\n\n\n", 3);
    chars_bck_test!(chars_bck_start_eol, "01\n34", 2);
    chars_bck_test!(chars_bck_mid, "0123456789", 5);
    chars_bck_test!(chars_bck_dirty, buffer_with_changes, 5);
    chars_bck_test!(chars_bck_dirty2, buffer_with_changes, 80);

    macro_rules! end_tests {
        ($($(#[$meta:meta])*$name:ident => $bufdef:tt),* $(,)?) => {
            $(
            #[test]
            $(#[$meta])*
            fn $name() {
                let buf = mkbuf!($bufdef);
                let bstr = buf.to_string();
                let last = str_doc_pos_off(&bstr, bstr.len().saturating_sub(1));
                assert_eq!(buf.end(), DocPos {x: last.x, ..last}, "actual == expected");
            }
            )*
        };
    }

    end_tests! {
        // #[ignore = "think about correct behavior"]
        end_blank => "",
        end_simple => "0123456789",
        end_complex => buffer_with_changes,
    }

    #[test]
    fn path_none() {
        let buf = BufferCore::from_str("0123456789");
        assert_eq!(buf.path(), None);
    }

    #[test]
    fn last_single() {
        let buf = BufferInner::from_str("0123456789");
        assert_eq!(buf.last(), Some(DocPos { x: 9, y: 0 }))
    }

    #[test]
    fn last_multiline() {
        let buf = BufferInner::from_str("0123456789\nasdf");
        assert_eq!(buf.last(), Some(DocPos { x: 3, y: 1 }))
    }

    macro_rules! delete_char_test {
        ($name:ident, $bufdef:tt, $($pos:expr => $expected_pos:expr),+ $(,)?) => {
            #[test]
            fn $name() {
                let mut buf = mkbuf!($bufdef);
                let mut expected = buf.to_string();
                $(
                buf.cursor.set_pos(str_doc_pos_off(&expected, $pos));
                let expected_rem = if buf.len() > 0 {
                    let rem = expected.remove($pos);
                    eprintln!("removed {rem:?}");
                    Some(rem)
                } else { None };
                assert_eq!(buf.delete_char(), expected_rem, "actual == expected");
                assert_eq!(buf.cursor.pos, str_doc_pos_off(&expected, $expected_pos));
                assert_eq!(buf.to_string(), expected);
                )*
            }
        };
    }

    delete_char_test!(delete_char_simple, "0123456789\nasdf", 5 => 5);
    delete_char_test!(delete_char_first_of_line, "0123456789\nasdf", 11 => 11);
    delete_char_test!(delete_char_newl, "0123456789\nasdf", 10 => 10);
    delete_char_test!(delete_char_last_of_line, "0123456789\nasdf", 9 => 8, 8 => 7, 7 => 6);
    delete_char_test!(delete_char_last_of_buf, "0123456789\nasdf", 14 => 13, 13 => 12, 12 => 11);
    delete_char_test!(delete_char_last_of_line2, "0123\n56789\nasdf", 9 => 8, 8 => 7, 7 => 6);
    delete_char_test!(delete_char_just_newl, "\n\n\n", 1 => 1);
    delete_char_test!(delete_char_first, "asdf", 0 => 0);
    delete_char_test!(delete_char_only, " ", 0 => 0);
    delete_char_test!(delete_char_only_lf, "\n", 0 => 0);
    delete_char_test!(delete_char_empty, "", 0 => 0);

    #[test]
    fn len() {
        let init = "this is a buffer\nasdfasdfasdfa";
        let buf = BufferInner::from_str(init);
        assert_eq!(buf.len(), init.len());
    }

    #[test]
    fn clear() {
        let mut buf = BufferInner::from_str("this is a buffer\nit will be cleared.");
        buf.clear();
        assert_eq!(&buf.to_string(), "");
        assert_eq!(buf.cursor.pos, DocPos::new());
        assert_eq!(buf.len(), 0);
    }

    macro_rules! delete_range_test {
        ($name:ident, $str:literal, $range:expr, $cursor:expr) => {
            #[test]
            fn $name() {
                let range = normalize_range($str, $range);
                let mut buf = BufferInner::from_str($str);
                buf.cursor.set_pos(str_doc_pos_off($str, $cursor));
                let expected_deleted = &$str[$range];
                let mut expected_remain = String::from($str);
                expected_remain.replace_range($range, "");
                let deleted = buf.delete_range($range);
                assert_eq!(&deleted, expected_deleted);
                assert_eq!(buf.to_string(), expected_remain);
                assert_eq!(
                    buf.cursor.pos,
                    str_doc_pos_off(
                        $str,
                        $cursor
                            - ($cursor as usize)
                                .saturating_sub(range.start)
                                .min(range.len())
                    )
                );
            }
        };
    }

    delete_range_test!(delete_range_simple, "simple buffer", 2..8, 0);
    delete_range_test!(delete_range_simple_cursor_start, "simple buffer", 2..8, 2);
    delete_range_test!(delete_range_simple_cursor_in, "simple buffer", 2..8, 4);
    delete_range_test!(delete_range_simple_cursor_last, "simple buffer", 2..8, 7);
    delete_range_test!(delete_range_simple_cursor_end, "simple buffer", 2..8, 8);
    delete_range_test!(delete_range_simple_cursor_after, "simple buffer", 2..8, 10);
    delete_range_test!(delete_range_simple_all, "simple buffer", 0..13, 5);
    delete_range_test!(delete_range_2line, "2 line\nbuffer", 2..8, 0);
    delete_range_test!(delete_range_2line_to_lf, "2 line\nbuffer", 2..7, 0);
    delete_range_test!(delete_range_2line_to_lf_c_end, "2 line\nbuffer", 2..7, 7);
    delete_range_test!(delete_range_2line_to_lf_past_end, "2 line\nbuffer", 2..7, 8);
    delete_range_test!(delete_range_2line_to_lf_c_at_lf, "2 line\nbuffer", 2..7, 6);
    delete_range_test!(delete_range_2line_cursor_start, "2 line\nbuffer", 2..8, 2);
    delete_range_test!(delete_range_2line_cursor_in, "2 line\nbuffer", 2..8, 4);
    delete_range_test!(delete_range_2line_cursor_last, "2 line\nbuffer", 2..8, 7);
    delete_range_test!(delete_range_2line_cursor_end, "2 line\nbuffer", 2..8, 8);
    delete_range_test!(delete_range_2line_cursor_after, "2 line\nbuffer", 2..8, 10);
    delete_range_test!(delete_range_2line_all, "2 line\nbuffer", 0..13, 10);
    delete_range_test!(delete_range_simple_inc, "simple buffer", 2..=8, 0);
    delete_range_test!(
        delete_range_simple_cursor_start_inc,
        "simple buffer",
        2..=8,
        2
    );
    delete_range_test!(delete_range_simple_cursor_in_inc, "simple buffer", 2..=8, 4);
    delete_range_test!(delete_range_simple_all_inc, "simple buffer", 0..=12, 5);
    delete_range_test!(delete_range_2line_inc, "2 line\nbuffer", 2..=8, 0);
    delete_range_test!(delete_range_2line_to_lf_inc, "2 line\nbuffer", 2..=6, 0);
    delete_range_test!(
        delete_range_2line_to_lf_c_end_inc,
        "2 line\nbuffer",
        2..=6,
        7
    );
    delete_range_test!(
        delete_range_2line_to_lf_c_last_inc,
        "2 line\nbuffer",
        2..=6,
        6
    );
    delete_range_test!(
        delete_range_2line_cursor_start_inc,
        "2 line\nbuffer",
        2..=8,
        2
    );
    delete_range_test!(delete_range_2line_cursor_in_inc, "2 line\nbuffer", 2..=8, 4);
    delete_range_test!(
        delete_range_2line_cursor_end_inc,
        "2 line\nbuffer",
        2..=8,
        8
    );
    delete_range_test!(
        delete_range_2line_cursor_after_inc,
        "2 line\nbuffer",
        2..=8,
        10
    );
    delete_range_test!(delete_range_empty, "", 0..0, 0);


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
