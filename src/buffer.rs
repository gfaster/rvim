use std::{ops::Range, collections::BTreeMap};

pub struct Buffer {
    data: String,
    changes: BTreeMap<usize, String>,
    lines: Vec<usize>
}


impl<'a> Buffer {
    pub fn new(file: &str) -> Result<Self, std::io::Error> {
        let data = std::fs::read_to_string(file)?; 
        Ok(Self::new_fromstring(data))
    }

    pub fn new_fromstring(s: String) -> Self {
        let data = s;
        let changes = BTreeMap::new();
        let lines = [0].into_iter().chain(data.bytes().enumerate().filter_map(|x| match x.1 {
            0x0A => Some(x.0 + 1),
            _ => None
        })).collect();

        Self {
            data,
            lines,
            changes
        }
    }

    /// Gets an iterator over lines in a range
    pub fn get_lines(&'a self, range: Range<usize>) -> impl Iterator<Item = &str> {
        // TODO: don't need to iterate everything before start
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

    /// get the bytes range of the line, not including trailing LF
    /// I might want to change this to include trailing LF - that gives garuntee that every line is
    /// at least one character long, and lets me "select" it on screen
    pub fn line_range(&self, line: usize) -> Range<usize> {
        self.virtual_getline(line)..self.virtual_getline(line + 1)
    }

    pub fn insert_char(&mut self, pos: usize, c: char) {
        if c == '\n' {
            let start = self.lines.iter().enumerate().rev().find(|(_, i)| **i <= pos).unwrap().0;
            self.lines.insert(start + 1, pos);
            self.lines.iter_mut().skip(start + 1).map(|i| *i+=1).last();
        } else {
            self.lines.iter_mut().skip_while(|i| **i < pos).map(|i| *i+=1).last();
        };
        self.data.insert(pos, c);
    }
}

#[cfg(test)]
mod test {
    use super::*;

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
