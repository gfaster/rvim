use crate::buffer::Buffer;
use std::ops::Range;
use enum_dispatch::enum_dispatch;

#[enum_dispatch]
trait TextObj {
    fn find_bounds(&self, buf: &Buffer, off: usize) -> Range<usize>;
}

#[enum_dispatch(TextObj)]
enum TextObject {
    WordObject
}



struct WordObject;
impl TextObj for WordObject {
    fn find_bounds(&self, _buf: &Buffer, _off:usize) -> Range<usize> {
        todo!();
    }
}
