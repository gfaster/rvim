use crate::{window::{Window, BufCtx}, term::TermPos};
use std::{ops::Range, io::Write, path::Path};

/// Position in a document - similar to TermPos but distinct enough semantically to deserve its own
/// struct. In the future, wrapping will mean that DocPos and TermPos will often not correspond
/// one-to-one. Also, using usize since it can very well be more than u32::max (though not for now)
#[derive(Clone, Copy)]
pub struct DocPos {
    pub x: usize,
    pub y: usize
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
    fn open(file: &Path) -> std::io::Result<Self> where Self: Sized;
    fn from_string(s: String) -> Self;
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()>;

    fn get_lines(&self, lines: Range<usize>) -> Vec<&str>;

    /// delete the character immediately to the left of the cursor in ctx
    fn delete_char(&mut self, ctx: &mut BufCtx) -> char;

    /// insert a character at the position of the cursor in ctx
    fn insert_char(&mut self, ctx: &mut BufCtx, c: char);
    fn get_off(&self, pos: DocPos) -> usize;
    fn linecnt(&self) -> usize;
}

#[derive(Clone, Copy)]
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

    fn open(file: &Path) -> Result<Self, std::io::Error> {
        let data = std::fs::read_to_string(file)?;
        Ok(Self::from_string(data))
    }

    fn from_string(s: String) -> Self {
        let name = "new buffer".to_string();
        let orig: Vec<_> = name.lines().map(str::to_string).collect();
        let add = Vec::new();
        let table = vec![PieceEntry { which: PTType::Orig, start: 0, len: orig.len() }];
        Self { name , orig, add, table }
    }

    fn delete_char(&mut self, ctx: &mut BufCtx) -> char {
        todo!()
    }

    fn insert_char(&mut self, ctx: &mut BufCtx, c: char) {
        let pos = ctx.cursorpos;
        let (prev, tidx, start) = self.get_line(pos);
        let tlen = self.table[tidx].len;
        let mut new = prev.to_string();
        new.insert(pos.x, c);

        let addstart = self.add.len();
        self.add.extend(new.split('\n').map(str::to_string));
        let addlen = self.add.len() - addstart;

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

    fn get_lines(&self, lines: Range<usize>) -> Vec<&str> {
        self.lines_fwd_internal(lines.start).take(lines.len()).map(String::as_ref).collect()
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
            PTType::Orig => &self.orig
        }
    }

    /// Iterator over lines starting at table table entry tidx
    fn lines_fwd_internal(&self, tidx: usize) -> impl Iterator<Item = &String> {
        self.table[tidx..].iter().flat_map(|te| self.match_table(&te.which)[te.start..].iter().take(te.len))
    }

    /// Iterator over reverse-order lines starting at table entry tidx
    fn lines_bck_internal(&self, tidx: usize) -> impl Iterator<Item = &String> {
        self.table[..tidx].iter().rev().flat_map(|te| self.match_table(&te.which)[te.start..].iter().rev().take(te.len))
    }

    /// get the table idx and line at pos
    ///
    /// Return (line, tidx, te start line)
    fn get_line(&self, pos: DocPos) -> (&str, usize, usize) {
        let (tidx, first) = self.table_idx(pos);
        let te = &self.table[tidx];
        let rem = pos.y - first;
        let line = &self.match_table(&te.which)[te.start + rem];
        (&line, tidx, first)
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


#[cfg(test)]
mod test {
    use super::*;

    mod buffer {
        use crate::render::BufId;

        use super::*;

        /*
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
        } */

        fn test_trait_construct_passage<B>() where B: Buffer {
            let mut b = B::from_string("".to_string());
            let mut ctx = BufCtx { buf_id: BufId::new(), cursorpos: DocPos { x: 0, y: 0 }, topline: 0 };
            b.insert_char(&mut ctx, 'H');
        }

    }


}
