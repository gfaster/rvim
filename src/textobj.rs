use crate::buffer::{Buffer, DocRange, DocPos};
use enum_dispatch::enum_dispatch;

pub enum TextObjectModifier {
    Inner,
    All,
}

pub enum Motion {
    ScreenSpace { dy: isize, dx: isize },
    BufferSpace { doff: isize },
    TextObj(TextObject),
    TextMotion(TextMotion)
}

#[derive(PartialEq, Eq)]
enum WordCat {
    Word,
    WordExt,
    Whitespace
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


pub trait TextMot<B>
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos>;
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

impl<B: Buffer> TextMot<B> for TextMotion {
    // for whatever reason enum_dispatch isn't working here, so I have to do it manually
    //
    // ugh.
    fn find_dest(&self,buf: &B,pos:DocPos) -> Option<DocPos> {
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
impl<B> TextMot<B> for WordForward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        buf.chars_fwd(pos).skip_while(|c| c.1.is_wordchar()).skip_while(|c| !c.1.is_wordchar()).map(|(p, _)| p).next()
    }
}

pub struct WordSubsetForward;
impl<B> TextMot<B> for WordSubsetForward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_fwd(pos);
        let init = it.next()?.1.category();
        it.skip_while(|c| c.1.category() == init).skip_while(|c| c.1.category() == WordCat::Whitespace).map(|(p, _)| p).next()
    }
}

pub struct WordEndForward;
impl<B> TextMot<B> for WordEndForward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_fwd(pos).skip(1).skip_while(|c| c.1.category() == WordCat::Whitespace).peekable();
        let mut ret = *it.peek()?;
        while it.peek()?.1.category() != WordCat::Whitespace {
            ret = *it.peek()?;
            it.next();
        };
        Some(ret.0)
    }
}

pub struct WordEndSubsetForward;
impl<B> TextMot<B> for WordEndSubsetForward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_fwd(pos).skip(1).skip_while(|c| c.1.category() == WordCat::Whitespace).peekable();
        let mut ret = *it.peek()?;
        let init = ret.1.category();
        while it.peek()?.1.category() == init {
            ret = *it.peek()?;
            it.next();
        };
        Some(ret.0)
    }
}

pub struct WordBackward;
impl<B> TextMot<B> for WordBackward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_bck(pos).skip(1).skip_while(|c| c.1.category() == WordCat::Whitespace).peekable();
        let mut ret = *it.peek()?;
        while it.peek()?.1.category() != WordCat::Whitespace {
            ret = *it.peek()?;
            it.next();
        };
        Some(ret.0)
    }
}

pub struct WordSubsetBackward;
impl<B> TextMot<B> for WordSubsetBackward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_bck(pos).skip(1).skip_while(|c| c.1.category() == WordCat::Whitespace).peekable();
        let mut ret = *it.peek()?;
        let init = ret.1.category();
        while it.peek()?.1.category() == init {
            ret = *it.peek()?;
            it.next();
        };
        Some(ret.0)
    }
}

pub struct WordEndBackward;
impl<B> TextMot<B> for WordEndBackward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        buf.chars_bck(pos).skip_while(|c| c.1.is_wordchar_extended()).skip_while(|c| !c.1.is_wordchar_extended()).map(|(p, _)| p).next()
    }
}

pub struct WordEndSubsetBackward;
impl<B> TextMot<B> for WordEndSubsetBackward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_bck(pos).skip_while(|c| !c.1.is_wordchar_extended());
        match it.next().unwrap_or((pos, ' ')).1.is_only_wordchar_extended() {
            true => it.skip_while(|c| c.1.is_only_wordchar_extended()).map(|(p, _)| p).next(),
            false => it.skip_while(|c| c.1.is_wordchar()).map(|(p, _)| p).next(),
        }
    }
}

pub struct StartOfLine;
impl<B> TextMot<B> for StartOfLine
where
    B: Buffer,
{
    fn find_dest(&self, _buf: &B, pos: DocPos) -> Option<DocPos> {
        Some(DocPos { x: 0, y: pos.y })
    }
}

pub struct EndOfLine;
impl<B> TextMot<B> for EndOfLine
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        let x = buf.get_lines(pos.y..(pos.y + 1)).get(0)?.len();
        Some(DocPos { x, y: pos.y })
    }
}


/*
*/

#[enum_dispatch]
pub trait TextObj<B>
where
    B: Buffer,
{
    fn find_bounds(&self, buf: &B, off: DocPos) -> Option<DocRange>;
}

#[enum_dispatch(TextObj)]
pub enum TextObject {
    WordObject,
    MotionObject(TextMotion)
}

pub struct MotionObject (TextMotion);
impl<B> TextObj<B> for MotionObject
where 
    B: Buffer
{
    fn find_bounds(&self, buf: &B, off: DocPos) -> Option<DocRange> {
        let Some(finish) = self.0.find_dest(buf, off) else { return None };
        if finish < off {
            Some(DocRange { 
                start: finish,
                end: off
            })
        } else {
            Some(DocRange {
                start: off,
                end: finish 
            })
        }
    }
}


pub struct WordObject;
impl<B> TextObj<B> for WordObject
where
    B: Buffer,
{
    fn find_bounds(&self, _buf: &B, _pos: DocPos) -> Option<DocRange> {
        todo!()
    }
}
