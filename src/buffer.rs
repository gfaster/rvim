use std::ops::Range;

pub struct Buffer {
    data: String
}


impl<'a> Buffer {
    pub fn new(file: &str) -> Result<Self, std::io::Error> {
        let data = std::fs::read_to_string(file)?; 
        Ok(Self{
            data,
        })
    }

    pub fn new_fromstring(s: String) -> Self {
        let data = s;
        Self {
            data
        }
    }

    pub fn get_lines(self: &'a Self, range: Range<usize>) -> impl Iterator<Item = &str> {
        self.data.lines().skip(range.start).take(range.len())
    }
}

#[cfg(test)]
mod test {
    use super::*;


    #[test]
    fn test_line_range() {
        let b = Buffer::new_fromstring("0\n1\n2\n3\n4".to_string());
        let mut it = b.get_lines(1..3);
        assert_eq!(it.next(), Some("1"));
        assert_eq!(it.next(), Some("2"));
        assert_eq!(it.next(), None);
    }
}
