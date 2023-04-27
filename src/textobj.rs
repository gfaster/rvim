use crate::buffer::{Buffer, DocRange, DocPos};
use enum_dispatch::enum_dispatch;

enum TextObjectMode {
    Unrestricted,
    LineRestricted,
}

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


#[enum_dispatch]
pub trait TextMot<B>
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos>;
}

pub enum TextMotion {
    WordForward,
    StartOfLine,
    EndOfLine
}

impl<B: Buffer> TextMot<B> for TextMotion {
    fn find_dest(&self,buf: &B,pos:DocPos) -> Option<DocPos> {
        match self {
            TextMotion::WordForward => WordForward.find_dest(buf, pos),
            TextMotion::StartOfLine => StartOfLine.find_dest(buf, pos),
            TextMotion::EndOfLine => EndOfLine.find_dest(buf, pos),
        }
    }
}


pub struct WordForward;
impl<B> TextMot<B> for WordForward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        buf.chars_fwd(pos).skip_while(|c| !c.1.is_whitespace()).skip_while(|c| c.1.is_whitespace()).map(|(p, _)| p).next()
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
