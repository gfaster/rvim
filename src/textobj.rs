use crate::buffer::Buffer;
use std::ops::Range;
use enum_dispatch::enum_dispatch;

enum TextObjectMode {
    Unrestricted,
    LineRestricted
}

pub enum TextObjectModifier {
    Inner,
    All
}


/*
*/

#[enum_dispatch]
pub trait TextObj {
    fn find_bounds(&self, buf: &Buffer, off: usize, toi: TextObjectModifier) -> Option<Range<usize>>;
}

#[enum_dispatch(TextObj)]
pub enum TextObject {
    WordObject
}

pub struct WordObject;
impl TextObj for WordObject {
    fn find_bounds(&self, buf: &Buffer, off:usize, _toi: TextObjectModifier) -> Option<Range<usize>> {
        let start;
        let end;
        let c = buf.char_atoff(off);
        if c.is_whitespace() {
            start = buf.revoff_chars(off).find(|(_, c)| !c.is_whitespace() )?.0;
            end = buf.off_chars(off).find(|(_, c)| !c.is_whitespace() )?.0;
        } else if c.is_ascii_alphanumeric() {
            start = buf.revoff_chars(off).find(|(_, c)| !c.is_ascii_alphanumeric() )?.0;
            end = buf.off_chars(off).find(|(_, c)| !c.is_ascii_alphanumeric() )?.0;
        } else {
            start = buf.revoff_chars(off).find(|(_, c)| c.is_ascii_alphanumeric() || c.is_whitespace())?.0;
            end = buf.off_chars(off).find(|(_, c)| c.is_ascii_alphanumeric() || c.is_whitespace() )?.0;
        };
        Some(start..end)
    }
}
