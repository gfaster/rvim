use std::{collections::BTreeMap, ops::Range, str::Chars, borrow::BorrowMut};

/// Position in a document - similar to TermPos but distinct enough semantically to deserve its own
/// struct. In the future, wrapping will mean that DocPos and TermPos will often not correspond
/// one-to-one. Also, using usize since it can very well be more than u32::max (though not for now)
pub struct DocPos {
    x: usize,
    y: usize
}

impl DocPos {
    fn row(&self) -> usize {
        self.y + 1
    }
    fn col(&self) -> usize {
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
    fn open(file: &str) -> Result<Self, std::io::Error>;
    fn from_string(s: String) -> Self;

    fn chars_fwd(&self, start: DocPos) -> Chars;
    fn chars_bck(&self, start: DocPos) -> Chars;
    fn delete_char(&mut self, afterpos: DocPos) -> char;
    fn insert_char(&mut self, afterpos: DocPos, c: char);
    fn get_off(&self, pos: DocPos) -> usize;
}

enum PTType {
    Add,
    Orig
}

// This is linewise, not characterwise
struct PieceEntry {
    which: PTType,
    start: usize,
    len: usize
}

/// Piece Table Buffer
pub struct PTBuffer {
    name: String,
    orig: Vec<String>,
    add: Vec<String>,
    table: Vec<PieceEntry>
}

impl Buffer for PTBuffer {
    fn name(&self) -> &str { &self.name }

    fn open(file: &str) -> Result<Self, std::io::Error> {
        let data = std::fs::read_to_string(file)?;
        Ok(Self::from_string(data))
    }

    fn from_string(s: String) -> Self {
        let name = "new buffer".to_string();
        let orig = Vec::from(name.lines());
        let add = Vec::new();
        let table = vec![PieceEntry { which: PTType::Orig, start: 0, len: orig.len() }];
        Self { name , orig, add, table }
    }

    fn chars_fwd(&self, start: DocPos) -> Chars {
        self.lines_fwd(self.table_idx(start)).enumerate().flat_map(|x| {
            if let (0, s) = x {
                s[start.x..]
            } else {
                x.1
            }
        })
    }

    fn chars_bck(&self, start: DocPos) -> Chars {
        self.lines_bck(self.table_idx(start)).enumerate().flat_map(|x| {
            if let (0, s) = x {
                s[..=start.x].iter().rev()
            } else {
                x.1.iter.rev()
            }
        })
    }

    fn delete_char(&mut self, afterpos: DocPos) -> char {

    }

    fn insert_char(&mut self, pos: DocPos, c: char) {
        let (prev, tidx, start) = self.get_line(pos);
        let tlen = self.table[tidx].len;
        let mut new = prev.to_string();
        new.insert(pos.x, c);
        let newv: Vec<_> = new.split('\n').collect();

        let addstart = self.add.len();
        let addlen = newv.len();
        self.add.extend_from_slice(newv);

        let prevte = self.table.remove(tidx);
        self.table.insert(tidx, PieceEntry { which: PTType::Add, start: addstart, len: addlen });
        if pos.y + 1 < start + tlen {
            // cut above
            self.table.insert(tidx + 1, PieceEntry { which: prevte.which, start: pos.y + addlen, len: start + tlen - pos.y - 1 })
        }
        if pos.y > start {
            // cut below
            self.table.insert(tidx, PieceEntry { which: prevte.which, start: pos.y + addlen, len: start - pos.y})
        }
    }

    fn get_off(&self, pos: DocPos) -> usize {
        todo!()
    }
}

impl PTBuffer {
    fn match_table(&self, which: &PTType) -> [&str] {
        match which {
            PTType::Add => self.add,
            PTType::Orig => self.orig,
        }
    }

    /// Iterator over lines starting at table table entry tidx
    fn lines_fwd(&self, tidx: usize) -> impl Iterator<Item = &str> {
        self.table[tidx..].iter().flat_map(|te| self.match_table(&te.which)[te.start..].iter().take(te.len));
    }

    /// Iterator over reverse-order lines starting at table entry tidx
    fn lines_bck(&self, tidx: usize) -> impl Iterator<Item = &str> {
        self.table[..tidx].iter().rev().flat_map(|te| self.match_table(&te.which)[te.start..].iter().rev().take(te.len));
    }

    /// get the table idx and line at pos
    ///
    /// Return (line, tidx, te start line)
    fn get_line(&self, pos: DocPos) -> (&str, usize, usize) {
        let (tidx, first) = self.table_idx(pos);
        let te = self.table[tidx];
        let rem = pos.y - first;
        let line = self.match_table(&te.which)[te.start + rem];
        (line, tidx, first)
    }

    /// returns the table idx and start line of entry for pos
    ///
    /// Returns: (table index, te start line)
    fn table_idx(&self, pos: DocPos) -> (usize, usize) {
        let mut line = 0;
        let tidx = self.table.iter().enumerate().take_while(|x| {
            if line + x.1.len <= pos.y {
                line += x.1.len;
                true
            } else { false }
        }).last().unwrap_or((0, &self.table[0])).0;
        (tidx, line)
    }
}


pub struct SimpleBuffer {
    name: String,
    data: String,
    lines: Vec<usize>,
}

impl<'a> SimpleBuffer {
    pub fn new(file: &str) -> Result<Self, std::io::Error> {
        let data = std::fs::read_to_string(file)?;
        Ok(Self::new_fromstring(data))
    }

    pub fn new_fromstring(s: String) -> Self {
        let data = s;
        let changes = BTreeMap::new();
        let lines = [0]
            .into_iter()
            .chain(data.bytes().enumerate().filter_map(|x| match x.1 {
                0x0A => Some(x.0 + 1),
                _ => None,
            }))
            .collect();

        Self {
            name: "new buffer".to_owned(),
            data,
            lines,
        }
    }

    /// Gets an iterator over lines in a range
    pub fn get_lines(&'a self, range: Range<usize>) -> impl Iterator<Item = &str> {
        self.data.lines().skip(range.start).take(range.len())
    }

    /// Gets an array of byte offsets for the start of each line
    pub fn lines_start(&self) -> &[usize] {
        &self.lines
    }

    /// get the offset of the start of `line`
    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.lines.get(line).copied()
    }

    /// get the virtual start of line - if line doesn't exist, return one past end of buffer
    pub fn virtual_getline(&self, line: usize) -> usize {
        self.lines.get(line).map_or_else(|| self.data.len(), |i| *i)
    }

    /// get the bytes range of the line, ~~not~~ including trailing LF
    /// I might want to change this to include trailing LF - that gives garuntee that every line is
    /// at least one character long, and lets me "select" it on screen
    pub fn line_range(&self, line: usize) -> Range<usize> {
        self.virtual_getline(line)..self.virtual_getline(line + 1)
    }

    pub fn insert_char(&mut self, pos: usize, c: char) {
        if c == '\n' {
            let start = self
                .lines
                .iter()
                .enumerate()
                .rev()
                .find(|(_, i)| **i <= pos)
                .unwrap()
                .0;
            self.lines.insert(start + 1, pos);
            self.lines
                .iter_mut()
                .skip(start + 1)
                .map(|i| *i += 1)
                .last();
        } else {
            self.lines
                .iter_mut()
                .skip_while(|i| **i <= pos)
                .map(|i| *i += 1)
                .last();
        };
        self.data.insert(pos, c);
    }

    pub fn delete_char(&mut self, pos: usize) {
        let lidx;
        let rem = self.data.remove(pos);
        if rem == '\n' {
            lidx = self.lines.iter().enumerate().find(|(_, l)| **l == pos + 1).expect("can find newline").0;
            self.lines.remove(lidx);
        } else {
            lidx = 1 + self
                .lines
                .iter()
                .enumerate()
                .rev()
                .find(|(_, loff)| **loff <= pos)
                .unwrap()
                .0;
        }
        self.lines
            .iter_mut()
            .skip(lidx)
            .map(|i| *i -= 1)
            .last();

    }

    pub fn char_atoff(&self, off: usize) -> char {
        self.data.split_at(off).1.chars().next().expect("in bounds")
    }

    pub fn working_linecnt(&self) -> usize {
        self.lines.len()
            - if *self.data.as_bytes().last().unwrap_or(&(' ' as u8)) == '\n' as u8 {
                1
            } else {
                0
            }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn revoff_chars(&self, off: usize) -> impl Iterator<Item = (usize, char)> + '_ {
        self.data.split_at(off).0.char_indices().map(move |x| (off - x.0, x.1)).rev()
    }

    pub fn off_chars(&self, off: usize) -> impl Iterator<Item = (usize, char)> + '_ {
        self.data.split_at(off).1.char_indices().map(move |x| (off + x.0, x.1))
    }
}

/// structure for a focused buffer. This is not a window and not a buffer. It holds the context of a
/// buffer editing session for later use. A window can display this, but shouldn't be limited  to
/// displaying only this. The reason I'm making this separate from window is that I want window to
/// be strictly an abstraction for rendering and/or focusing. 
///
/// I need to think about what niche the command line fits into. Is it another window, or is it its
/// own thing?
///
/// I should consider a "text field" or something similar as a trait for
/// an area that can be focused and take input.
struct BufCxt<'a, B> where B: Buffer {
    buf: &'a B,

    /// I use DocPos rather than a flat offset to more easily handle linewise operations, which
    /// seem to be more common than operations that operate on the flat buffer. It also makes
    /// translation more convienent, especially when the buffer is stored as an array of lines
    /// rather than a flat byte array (although it seems like this would slow transversal?).
    cursorpos: DocPos,
    topline: usize
}

impl<B> BufCxt<'_, B> {

}

#[cfg(test)]
mod test {
    use super::*;

    mod buffer {
        use super::*;

        #[test]
        fn test_ptbuf_chars_fwd() { test_trait_chars_fwd::<PTBuffer>() }
        fn test_trait_chars_fwd<B>() where B: Buffer {
            let b = B::from_string("0\n1\n2A2\n3\n4\n".to_string());
            let mut it = b.chars_fwd(0);
            assert_eq!(it.next(), Some('0'));
            assert_eq!(it.next(), Some('\n'));
            assert_eq!(it.next(), Some('1'));
            assert_eq!(it.next(), Some('\n'));
            assert_eq!(it.next(), Some('2'));
            assert_eq!(it.next(), Some('A'));
            assert_eq!(it.next(), Some('2'));
            assert_eq!(it.next(), Some('\n'));
            assert_eq!(it.next(), Some('3'));
            assert_eq!(it.next(), Some('\n'));
            assert_eq!(it.next(), Some('4'));
            assert_eq!(it.next(), None);
        }

        #[test]
        fn test_ptbuf_chars_fwd_mid() { test_trait_chars_fwd_mid::<PTBuffer>() }
        fn test_trait_chars_fwd_mid<B>() where B: Buffer {
            let b = B::from_string("0\n1\n2A2\n3\n4\n".to_string());
            let mut it = b.chars_fwd(6);
            assert_eq!(it.next(), Some('2'));
            assert_eq!(it.next(), Some('\n'));
            assert_eq!(it.next(), Some('3'));
            assert_eq!(it.next(), Some('\n'));
            assert_eq!(it.next(), Some('4'));
            assert_eq!(it.next(), None);
        }

        #[test]
        fn test_ptbuf_chars_bck() { test_trait_chars_bck::<PTBuffer>() }
        fn test_trait_chars_bck<B>() where B: Buffer {
            let b = B::from_string("0\n1\n2A2\n3\n4\n".to_string());
            let mut it = b.chars_bck(11);
            assert_eq!(it.next(), Some('4'));
            assert_eq!(it.next(), Some('\n'));
            assert_eq!(it.next(), Some('3'));
            assert_eq!(it.next(), Some('\n'));
            assert_eq!(it.next(), Some('2'));
            assert_eq!(it.next(), Some('A'));
            assert_eq!(it.next(), Some('2'));
            assert_eq!(it.next(), Some('\n'));
            assert_eq!(it.next(), Some('1'));
            assert_eq!(it.next(), Some('\n'));
            assert_eq!(it.next(), Some('0'));
            assert_eq!(it.next(), None);
        }

        #[test]
        fn test_ptbuf_chars_bck_mid() { test_trait_chars_bck_mid::<PTBuffer>() }
        fn test_trait_chars_bck_mid<B>() where B: Buffer {
            let b = B::from_string("0\n1\n2A2\n3\n4\n".to_string());
            let mut it = b.chars_bck(4);
            assert_eq!(it.next(), Some('2'));
            assert_eq!(it.next(), Some('\n'));
            assert_eq!(it.next(), Some('1'));
            assert_eq!(it.next(), Some('\n'));
            assert_eq!(it.next(), Some('0'));
            assert_eq!(it.next(), None);
        }

        fn test_trait_construct_passage<B>() where B: Buffer {
            let mut b = B::from_string("".to_string());

        }

    }

    #[test]
    fn test_delete_char() {
        let mut b = Buffer::new_fromstring("0\n1\n2A2\n3\n4\n".to_string());
        assert_eq!(b.data, "0\n1\n2A2\n3\n4\n");
        assert_eq!(b.lines, [0, 2, 4, 8, 10, 12]);

        b.delete_char(0);
        assert_eq!(b.data, "\n1\n2A2\n3\n4\n");
        assert_eq!(b.lines, [0, 1, 3, 7, 9, 11]);
        b.delete_char(1);
        assert_eq!(b.data, "\n\n2A2\n3\n4\n");
        assert_eq!(b.lines, [0, 1, 2, 6, 8, 10]);
        b.delete_char(0);
        assert_eq!(b.data, "\n2A2\n3\n4\n");
        assert_eq!(b.lines, [0, 1, 5, 7, 9]);
        b.delete_char(2);
        assert_eq!(b.data, "\n22\n3\n4\n");
        assert_eq!(b.lines, [0, 1, 4, 6, 8]);
        b.delete_char(3);
        assert_eq!(b.data, "\n223\n4\n");
        assert_eq!(b.lines, [0, 1, 5, 7]);
    }

    #[test]
    fn test_char_atoff() {
        let b = Buffer::new_fromstring("0\n1\n22\n3\n4\n".to_string());
        assert_eq!(b.char_atoff(0), '0');
        assert_eq!(b.char_atoff(1), '\n');
        assert_eq!(b.char_atoff(9), '4');
        assert_eq!(b.char_atoff(10), '\n');
    }

    #[test]
    fn test_insert_lf() {
        let mut b = Buffer::new_fromstring("0\n1\n22\n3\n4".to_string());
        assert_eq!(b.lines[2], 4);
        assert_eq!(b.lines[3], 7);
        b.insert_char(5, '\n');
        assert_eq!(b.lines[2], 4);
        assert_eq!(b.lines[3], 6);
        assert_eq!(b.lines[4], 8);
    }

    #[test]
    fn test_insert_lf_after_lf() {
        let mut b = Buffer::new_fromstring("0\n1\n22\n3\n4".to_string());
        assert_eq!(b.lines[2], 4);
        assert_eq!(b.lines[3], 7);
        b.insert_char(4, '\n');
        assert_eq!(b.lines[2], 4);
        assert_eq!(b.lines[3], 5);
        assert_eq!(b.lines[4], 8);
    }

    #[test]
    fn test_insert_lf_at_lf() {
        let mut b = Buffer::new_fromstring("0\n1\n22\n3\n4".to_string());
        assert_eq!(b.lines[2], 4);
        assert_eq!(b.lines[3], 7);
        b.insert_char(3, '\n');
        assert_eq!(b.lines[2], 4);
        assert_eq!(b.lines[3], 5);
        assert_eq!(b.lines[4], 8);
    }

    #[test]
    fn test_get_range_of_lines() {
        let b = Buffer::new_fromstring("0\n1\n2\n3\n4".to_string());
        let mut it = b.get_lines(1..3);
        assert_eq!(it.next(), Some("1"));
        assert_eq!(it.next(), Some("2"));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn lines_align() {
        println!("lines vector should index first bytes of lines");
        let b = Buffer::new_fromstring("0\n1\n22\n3\n4".to_string());
        assert_eq!(b.lines[0], 0);
        assert_eq!(b.lines[1], 2);
        assert_eq!(b.lines[2], 4);
        assert_eq!(b.lines[3], 7);
        assert_eq!(b.lines[4], 9);
        assert_eq!(b.lines.len(), 5);
    }

    #[test]
    fn test_get_virt_line() {
        let b = Buffer::new_fromstring("0\n1\n2\n3\n4".to_string());
        assert_eq!(b.virtual_getline(0), 0);
        assert_eq!(b.virtual_getline(1), 2);
        assert_eq!(b.virtual_getline(4), 8);
        assert_eq!(b.virtual_getline(5), 9);
    }

    #[test]
    fn test_get_virt_line_trailing_lf() {
        let b = Buffer::new_fromstring("0\n1\n2\n3\n4\n".to_string());
        assert_eq!(b.virtual_getline(0), 0);
        assert_eq!(b.virtual_getline(1), 2);
        assert_eq!(b.virtual_getline(4), 8);
        assert_eq!(b.virtual_getline(5), 10);
    }

    #[test]
    fn test_line_range() {
        let b = Buffer::new_fromstring("0\n1\n22\n333\n4".to_string());
        assert_eq!(b.line_range(0), 0..2);
        assert_eq!(b.line_range(1), 2..4);
        assert_eq!(b.line_range(2), 4..7);
        assert_eq!(b.line_range(3), 7..11);
        assert_eq!(b.line_range(4), 11..12);
    }
}
