use crate::buffer::{Buffer, DocPos, DocRange};
use enum_dispatch::enum_dispatch;

pub enum TextObjectModifier {
    Inner,
    All,
}

pub enum Motion {
    ScreenSpace { dy: isize, dx: isize },
    BufferSpace { doff: isize },
    TextObj(TextObject),
    TextMotion(TextMotion),
}

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

pub trait TextMot
{
    fn find_dest(&self, buf: &Buffer, pos: DocPos) -> Option<DocPos>;
}

/// enum of the possible text motions.
///
/// I would like to use dynamic dispatch here because it feels like it fits better than enum
/// dispatch, but that's a little bit difficult. I don't really care about resolution speed for the
/// find_dest trait function, but I don't really want each call to the character iterator to have
/// to go through a vtable becuase that's gross. Additionally, the bufiter trait would have to go
/// through an extra layer of indirection - it already doesn't like trait objects.
///
/// I could add a generic to the declaration of all the enums and structs that use `TextMotion`,
/// but explicitly tying up the structs themselves to a buffer implementation also seems wrong -
/// input should not care about the buffer implementation. It doesn't matter now, but in the future
/// I want buffers to open using different implementations based on the formatting (i.e. I don't
/// want one long line to be super slow). It may be that buffers will all have to be trait objects
/// in the end, but I'm not crazy about that.
///
/// Come to think of it, I could put a function pointer of the signature of find_dest, but that
/// doesn't solve the problem of decoupling Motions from Buffer implementations
///
/// I could also make the caller of the text motions a impl on Buffer, and that can take a motion
/// trait object
///
/// This was a long winded way of saying that I use an enum here because it is convienient to keep
/// everything `Sized`.
pub enum TextMotion {
    StartOfLine,
    EndOfLine,
    WordForward,
    WordSubsetForward,
    WordBackward,
    WordSubsetBackward,
    WordEndForward,
    WordEndSubsetForward,
    WordEndBackward,
    WordEndSubsetBackward,
}

impl TextMot for TextMotion {
    // for whatever reason enum_dispatch isn't working here, so I have to do it manually
    //
    // ugh.
    fn find_dest(&self, buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        match self {
            TextMotion::StartOfLine => StartOfLine.find_dest(buf, pos),
            TextMotion::EndOfLine => EndOfLine.find_dest(buf, pos),
            TextMotion::WordForward => WordForward.find_dest(buf, pos),
            TextMotion::WordSubsetForward => WordSubsetForward.find_dest(buf, pos),
            TextMotion::WordBackward => WordBackward.find_dest(buf, pos),
            TextMotion::WordSubsetBackward => WordSubsetBackward.find_dest(buf, pos),
            TextMotion::WordEndForward => WordEndForward.find_dest(buf, pos),
            TextMotion::WordEndSubsetForward => WordEndSubsetForward.find_dest(buf, pos),
            TextMotion::WordEndBackward => WordEndBackward.find_dest(buf, pos),
            TextMotion::WordEndSubsetBackward => WordSubsetBackward.find_dest(buf, pos),
        }
    }
}

pub struct WordForward;
impl TextMot for WordForward
{
    fn find_dest(&self, buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_fwd(pos).peekable();
        it.next();
        it.peek()?;
        it.skip_while(|c| c.1.is_wordchar())
            .skip_while(|c| !c.1.is_wordchar())
            .map(|(p, _)| p)
            .next()
            .or_else(|| Some(buf.end()))
    }
}

pub struct WordSubsetForward;
impl TextMot for WordSubsetForward
{
    fn find_dest(&self, buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_fwd(pos).peekable();
        let init = it.next()?.1.category();
        it.peek()?;
        it.skip_while(|c| c.1.category() == init)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .map(|(p, _)| p)
            .next()
            .or_else(|| Some(buf.end()))
    }
}

pub struct WordEndForward;
impl TextMot for WordEndForward
{
    fn find_dest(&self, buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let mut it = buf
            .chars_fwd(pos)
            .skip(1)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .peekable();
        let mut ret = *it.peek()?;
        while {
            let Some(x) = it.peek() else {return Some(buf.end())};
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
}

pub struct WordEndSubsetForward;
impl TextMot for WordEndSubsetForward
{
    fn find_dest(&self, buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let mut it = buf
            .chars_fwd(pos)
            .skip(1)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .peekable();
        let mut ret = *it.peek()?;
        let init = ret.1.category();
        while {
            let Some(x) = it.peek() else {return Some(buf.end())};
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
}

pub struct WordBackward;
impl TextMot for WordBackward
{
    fn find_dest(&self, buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let mut it = buf
            .chars_bck(pos)
            .skip(1)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .peekable();
        let mut ret = *it.peek()?;
        while {
            let Some(x) = it.peek() else {return Some(DocPos { x: 0, y: 0 })};
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
}

pub struct WordSubsetBackward;
impl TextMot for WordSubsetBackward
{
    fn find_dest(&self, buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let mut it = buf
            .chars_bck(pos)
            .skip(1)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .peekable();
        let mut ret = *it.peek()?;
        let init = ret.1.category();
        while {
            let Some(x) = it.peek() else {return Some(DocPos { x: 0, y: 0 })};
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
}

pub struct WordEndBackward;
impl TextMot for WordEndBackward
{
    fn find_dest(&self, _buf: &Buffer, _pos: DocPos) -> Option<DocPos> {
        // buf.chars_bck(pos).skip_while(|c| c.1.is_wordchar_extended()).skip_while(|c| !c.1.is_wordchar_extended()).map(|(p, _)| p).next()
        todo!()
    }
}

pub struct WordEndSubsetBackward;
impl TextMot for WordEndSubsetBackward
{
    fn find_dest(&self, _buf: &Buffer, _pos: DocPos) -> Option<DocPos> {
        // let mut it = buf.chars_bck(pos).skip_while(|c| !c.1.is_wordchar_extended());
        // match it.next().unwrap_or((pos, ' ')).1.is_only_wordchar_extended() {
        //     true => it.skip_while(|c| c.1.is_only_wordchar_extended()).map(|(p, _)| p).next(),
        //     false => it.skip_while(|c| c.1.is_wordchar()).map(|(p, _)| p).next(),
        // }
        todo!();
    }
}

pub struct StartOfLine;
impl TextMot for StartOfLine
{
    fn find_dest(&self, _buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        Some(DocPos { x: 0, y: pos.y })
    }
}

pub struct EndOfLine;
impl TextMot for EndOfLine
{
    fn find_dest(&self, buf: &Buffer, pos: DocPos) -> Option<DocPos> {
        let x = buf.get_lines(pos.y..(pos.y + 1)).first()?.len();
        Some(DocPos { x, y: pos.y })
    }
}

/*
*/

#[enum_dispatch]
pub trait TextObj
{
    fn find_bounds(&self, buf: &Buffer, off: DocPos) -> Option<DocRange>;
}

#[enum_dispatch(TextObj)]
pub enum TextObject {
    WordObject,
    MotionObject(TextMotion),
}

pub struct MotionObject(TextMotion);
impl TextObj for MotionObject
{
    fn find_bounds(&self, buf: &Buffer, off: DocPos) -> Option<DocRange> {
        self.0.find_bounds(buf, off)
    }
}

impl<T: TextMot> TextObj for T {
    fn find_bounds(&self,buf: &Buffer,off:DocPos) -> Option<DocRange> {
        let finish = self.find_dest(buf, off)?;
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
}

pub struct WordObject;
impl TextObj for WordObject
{
    fn find_bounds(&self, _buf: &Buffer, _pos: DocPos) -> Option<DocRange> {
        todo!()
    }
}

#[cfg(test)]
mod test {

    use super::*;

    // TODO: write some macros for these tests

    fn do_motion_start(buf: &Buffer, motion: &dyn TextMot) -> Option<DocPos> {
        motion.find_dest(buf, DocPos { x: 0, y: 0 })
    }

    fn apply_motion(buf: &Buffer, motion: &dyn TextMot, pos: &mut Option<DocPos>) {
        *pos = motion.find_dest(buf, pos.unwrap());
    }

    #[test]
    fn word_fwd_basic() {
        let buf = Buffer::from_string("abcd efg".to_string());
        assert_eq!(
            do_motion_start(&buf, &WordForward),
            Some(DocPos { x: 5, y: 0 })
        );
    }

    #[test]
    fn word_fwd_short() {
        let buf = Buffer::from_string("a bcd efg".to_string());
        assert_eq!(
            do_motion_start(&buf, &WordForward),
            Some(DocPos { x: 2, y: 0 })
        );
    }

    #[test]
    fn word_fwd_newl() {
        let buf = Buffer::from_string("abcd\nefg".to_string());
        assert_eq!(
            do_motion_start(&buf, &WordForward),
            Some(DocPos { x: 0, y: 1 })
        );
    }

    #[test]
    fn word_fwd_newl_then_space() {
        let buf = Buffer::from_string("abcd\n    efg".to_string());
        assert_eq!(
            do_motion_start(&buf, &WordForward),
            Some(DocPos { x: 4, y: 1 })
        );
    }

    #[test]
    fn word_fwd_end() {
        let buf = Buffer::from_string("abcdefg".to_string());
        assert_eq!(
            do_motion_start(&buf, &WordForward),
            Some(DocPos { x: 7, y: 0 })
        );
    }

    #[test]
    fn word_fwd_end_at_end() {
        let buf = Buffer::from_string("abcdefg".to_string());
        let mut pos = do_motion_start(&buf, &WordForward);
        assert_eq!(pos, Some(DocPos { x: 7, y: 0 }));
        apply_motion(&buf, &WordForward, &mut pos);
        assert_eq!(pos, None);
    }

    #[test]
    fn word_bck_basic() {
        let buf = Buffer::from_string("abcd efg".to_string());
        assert_eq!(
            WordBackward.find_dest(&buf, buf.end()),
            Some(DocPos { x: 5, y: 0 })
        );
    }

    #[test]
    fn word_bck_short() {
        let buf = Buffer::from_string("abcd ef g".to_string());
        assert_eq!(
            WordBackward.find_dest(&buf, DocPos { x: 8, y: 0 }),
            Some(DocPos { x: 5, y: 0 })
        );
    }

    #[test]
    fn word_bck_newl() {
        let buf = Buffer::from_string("abcd\nefg\na".to_string());
        assert_eq!(
            WordBackward.find_dest(&buf, buf.end()),
            Some(DocPos { x: 0, y: 2 })
        );
    }

    #[test]
    fn word_bck_space_then_newl() {
        let buf = Buffer::from_string("abcd\n    efg\n    ".to_string());
        assert_eq!(
            WordBackward.find_dest(&buf, buf.end()),
            Some(DocPos { x: 4, y: 1 })
        );
    }

    #[test]
    fn word_bck_end() {
        let buf = Buffer::from_string("abcdefg".to_string());
        assert_eq!(
            WordBackward.find_dest(&buf, buf.end()),
            Some(DocPos { x: 0, y: 0 })
        );
    }

    #[test]
    fn word_bck_end_at_end() {
        let buf = Buffer::from_string("abcdefg".to_string());
        assert_eq!(do_motion_start(&buf, &WordBackward), None);
    }
}
