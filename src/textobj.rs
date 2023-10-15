use crate::prelude::*;

pub enum TextObjectModifier {
    Inner,
    All,
}

/// Enum of the various types of motions, using a trait object because that feels more semantically
/// appropriate
///
/// An alternative would be to use straight function pointers
#[derive(PartialEq, Eq, Debug)]
pub enum Motion {
    ScreenSpace { dy: isize, dx: isize },
    BufferSpace { doff: isize },
    TextObj(TextObject),
    TextMotion(TextMotion),
}

pub type TextMotion = fn(&Buffer, DocPos) -> Option<DocPos>;
pub type TextObject = fn(&Buffer, DocPos) -> Option<DocRange>;

#[derive(PartialEq, Eq)]
enum WordCat {
    Word,
    WordExt,
    Whitespace,
}

trait Word {
    fn is_wordchar(&self) -> bool;
    fn is_wordchar_extended(&self) -> bool;

    fn is_only_wordchar_extended(&self) -> bool {
        !self.is_wordchar() && self.is_wordchar_extended()
    }

    fn category(&self) -> WordCat {
        if self.is_wordchar() {
            WordCat::Word
        } else if self.is_wordchar_extended() {
            WordCat::WordExt
        } else {
            WordCat::Whitespace
        }
    }
}

impl Word for char {
    fn is_wordchar(&self) -> bool {
        self.is_alphanumeric() || self == &'_'
    }

    fn is_wordchar_extended(&self) -> bool {
        !self.is_whitespace()
    }
}

pub mod motions {
    use super::*;

    pub(crate) fn word_forward(buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_fwd(pos).peekable();
        it.next();
        it.peek()?;
        it.skip_while(|c| c.1.is_wordchar())
            .skip_while(|c| !c.1.is_wordchar())
            .map(|(p, _)| p)
            .next()
            .or_else(|| Some(buf.end()))
    }

    pub(crate) fn word_subset_forward(buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_fwd(pos).peekable();
        let init = it.next()?.1.category();
        it.peek()?;
        it.skip_while(|c| c.1.category() == init)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .map(|(p, _)| p)
            .next()
            .or_else(|| Some(buf.end()))
    }

    pub(crate) fn word_end_forward(buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let mut it = buf
            .chars_fwd(pos)
            .skip(1)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .peekable();
        let mut ret = *it.peek()?;
        while {
            let Some(x) = it.peek() else {
                return Some(buf.end());
            };
            x
        }
        .1
        .category()
            != WordCat::Whitespace
        {
            ret = *it.peek()?;
            it.next();
        }
        Some(ret.0)
    }

    pub(crate) fn word_end_subset_forward(buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let mut it = buf
            .chars_fwd(pos)
            .skip(1)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .peekable();
        let mut ret = *it.peek()?;
        let init = ret.1.category();
        while {
            let Some(x) = it.peek() else {
                return Some(buf.end());
            };
            x
        }
        .1
        .category()
            == init
        {
            ret = *it.peek()?;
            it.next();
        }
        Some(ret.0)
    }

    pub(crate) fn word_backward(buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let mut it = buf
            .chars_bck(pos)
            .skip(1)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .peekable();
        let mut ret = *it.peek()?;
        while {
            let Some(x) = it.peek() else {
                return Some(DocPos { x: 0, y: 0 });
            };
            x
        }
        .1
        .category()
            != WordCat::Whitespace
        {
            ret = *it.peek().expect("Checked prior");
            it.next();
        }
        Some(ret.0)
    }

    pub(crate) fn word_subset_backward(buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let mut it = buf
            .chars_bck(pos)
            .skip(1)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .peekable();
        let mut ret = *it.peek()?;
        let init = ret.1.category();
        while {
            let Some(x) = it.peek() else {
                return Some(DocPos { x: 0, y: 0 });
            };
            x
        }
        .1
        .category()
            == init
        {
            ret = *it.peek().expect("checked prior");
            it.next();
        }
        Some(ret.0)
    }

    pub(crate) fn word_end_backward(_buf: &Buffer, _pos: DocPos) -> Option<DocPos> {
        // buf.chars_bck(pos).skip_while(|c| c.1.is_wordchar_extended()).skip_while(|c| !c.1.is_wordchar_extended()).map(|(p, _)| p).next()
        todo!()
    }

    pub(crate) fn word_end_subset_backward(_buf: &Buffer, _pos: DocPos) -> Option<DocPos> {
        // let mut it = buf.chars_bck(pos).skip_while(|c| !c.1.is_wordchar_extended());
        // match it.next().unwrap_or((pos, ' ')).1.is_only_wordchar_extended() {
        //     true => it.skip_while(|c| c.1.is_only_wordchar_extended()).map(|(p, _)| p).next(),
        //     false => it.skip_while(|c| c.1.is_wordchar()).map(|(p, _)| p).next(),
        // }
        todo!();
    }

    pub(crate) fn start_of_line(_buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        Some(DocPos { x: 0, y: pos.y })
    }

    pub(crate) fn end_of_line(buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let x = buf.line(pos.y).len();
        Some(DocPos { x, y: pos.y })
    }
}

/*
*/

pub fn text_object_from_motion(motion: TextMotion, buf: &Buffer, off: DocPos) -> Option<DocRange> {
    let finish = motion(buf, off)?;
    if finish < off {
        Some(DocRange {
            start: finish,
            end: off,
        })
    } else {
        Some(DocRange {
            start: off,
            end: finish,
        })
    }
}

pub fn inner_word_object(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    let first = buf.char_at(pos).unwrap();
    let start = buf
        .chars_bck(pos)
        .skip(1)
        .skip_while(|c| c.1.category() == first.category())
        .next()?
        .0;
    let end = buf
        .chars_fwd(pos)
        .skip(1)
        .skip_while(|c| c.1.category() == first.category())
        .next()?
        .0;
    Some(DocRange { start, end })
}

#[cfg(test)]
mod test {

    use crate::buffer::Buf;

    use super::motions::*;
    use super::TextMotion;
    use super::*;

    // TODO: write some macros for these tests

    fn do_motion_start(buf: &Buffer, motion: TextMotion) -> Option<DocPos> {
        motion(buf, DocPos { x: 0, y: 0 })
    }

    fn apply_motion(buf: &Buffer, motion: TextMotion, pos: &mut Option<DocPos>) {
        *pos = motion(buf, pos.unwrap());
    }

    #[test]
    fn word_fwd_basic() {
        let buf = Buffer::from_str("abcd efg");
        assert_eq!(
            do_motion_start(&buf, word_forward),
            Some(DocPos { x: 5, y: 0 })
        );
    }

    #[test]
    fn word_fwd_short() {
        let buf = Buffer::from_str("a bcd efg");
        assert_eq!(
            do_motion_start(&buf, word_forward),
            Some(DocPos { x: 2, y: 0 })
        );
    }

    #[test]
    fn word_fwd_newl() {
        let buf = Buffer::from_str("abcd\nefg");
        assert_eq!(
            do_motion_start(&buf, word_forward),
            Some(DocPos { x: 0, y: 1 })
        );
    }

    #[test]
    fn word_fwd_newl_then_space() {
        let buf = Buffer::from_str("abcd\n    efg");
        assert_eq!(
            do_motion_start(&buf, word_forward),
            Some(DocPos { x: 4, y: 1 })
        );
    }

    #[test]
    fn word_fwd_end() {
        let buf = Buffer::from_str("abcdefg");
        assert_eq!(
            do_motion_start(&buf, word_forward),
            Some(DocPos { x: 7, y: 0 })
        );
    }

    #[test]
    fn word_fwd_end_at_end() {
        let buf = Buffer::from_str("abcdefg");
        let mut pos = do_motion_start(&buf, word_forward);
        assert_eq!(pos, Some(DocPos { x: 7, y: 0 }));
        apply_motion(&buf, word_forward, &mut pos);
        assert_eq!(pos, None);
    }

    #[test]
    fn word_bck_basic() {
        let buf = Buffer::from_str("abcd efg");
        assert_eq!(word_backward(&buf, buf.last()), Some(DocPos { x: 5, y: 0 }));
    }

    #[test]
    fn word_bck_short() {
        let buf = Buffer::from_str("abcd ef g");
        assert_eq!(
            word_backward(&buf, DocPos { x: 8, y: 0 }),
            Some(DocPos { x: 5, y: 0 })
        );
    }

    #[test]
    fn word_bck_newl() {
        let buf = Buffer::from_str("abcd\nefg\na");
        assert_eq!(word_backward(&buf, buf.last()), Some(DocPos { x: 0, y: 1 }));
    }

    #[test]
    fn word_bck_space_then_newl() {
        let buf = Buffer::from_str("abcd\n    efg\n    ");
        assert_eq!(word_backward(&buf, buf.last()), Some(DocPos { x: 4, y: 1 }));
    }

    #[test]
    fn word_bck_end() {
        let buf = Buffer::from_str("abcdefg");
        assert_eq!(word_backward(&buf, buf.last()), Some(DocPos { x: 0, y: 0 }));
    }

    #[test]
    fn word_bck_end_at_end() {
        let buf = Buffer::from_str("abcdefg");
        assert_eq!(do_motion_start(&buf, word_backward), None);
    }
}
