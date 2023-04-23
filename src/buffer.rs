use std::{collections::BTreeMap, ops::Range};

pub struct Buffer {
    name: String,
    data: String,
    changes: BTreeMap<usize, String>,
    lines: Vec<usize>,
}

impl<'a> Buffer {
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
            changes,
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
