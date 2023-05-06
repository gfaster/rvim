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

trait Word {
    fn is_wordchar(&self) -> bool;
    fn is_wordchar_extended(&self) -> bool;

    fn is_only_wordchar_extended(&self) -> bool {
        !self.is_wordchar() && self.is_wordchar_extended()
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


#[enum_dispatch]
pub trait TextMot<B>
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos>;
}

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
    fn find_dest(&self,buf: &B,pos:DocPos) -> Option<DocPos> {
        match self {
            TextMotion::StartOfLine => StartOfLine.find_dest(buf, pos),
            TextMotion::EndOfLine => EndOfLine.find_dest(buf, pos),
            TextMotion::WordForward => WordForward.find_dest(buf, pos),
            TextMotion::WordSubsetForward => WordSubsetForward.find_dest(buf, pos),
            TextMotion::WordBackward => WordBackward.find_dest(buf, pos),
            TextMotion::WordSubsetBackward => WordSubsetBackward.find_dest(buf, pos),
            TextMotion::WordEndForward => WordForward.find_dest(buf, pos),
            TextMotion::WordEndSubsetForward => WordSubsetForward.find_dest(buf, pos),
            TextMotion::WordEndBackward => WordBackward.find_dest(buf, pos),
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
        match it.next().unwrap_or((pos, ' ')).1.is_only_wordchar_extended() {
            true => it.skip_while(|c| c.1.is_only_wordchar_extended()).skip_while(|c| !c.1.is_wordchar()).map(|(p, _)| p).next(),
            false => it.skip_while(|c| c.1.is_wordchar()).skip_while(|c| !c.1.is_wordchar()).map(|(p, _)| p).next(),
        }
    }
}

pub struct WordEndForward;
impl<B> TextMot<B> for WordEndForward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_bck(pos).skip(1).peekable();
        while let Some((_, c)) = it.next() {
            if c.is_wordchar_extended() { break }
        }
        while let Some((p, _)) = it.next() {
            if let Some((_, c)) = it.peek() {
                if !c.is_wordchar_extended() { return Some(p) }
            }
        }
        None
    }
}

pub struct WordEndSubsetForward;
impl<B> TextMot<B> for WordEndSubsetForward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_bck(pos).skip(1);
        match it.next().unwrap_or((pos, ' ')).1.is_only_wordchar_extended() {
            true => it.skip_while(|c| !c.1.is_only_wordchar_extended()).skip_while(|c| !c.1.is_wordchar()).map(|(p, _)| p).next(),
            false => it.skip_while(|c| c.1.is_wordchar()).skip_while(|c| !c.1.is_wordchar()).map(|(p, _)| p).next(),
        }
    }
}

pub struct WordBackward;
impl<B> TextMot<B> for WordBackward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        buf.chars_bck(pos).skip_while(|c| !c.1.is_wordchar_extended()).skip_while(|c| c.1.is_wordchar_extended()).map(|(p, _)| p).next()
    }
}

pub struct WordSubsetBackward;
impl<B> TextMot<B> for WordSubsetBackward
where
    B: Buffer,
{
    fn find_dest(&self, buf: &B, pos: DocPos) -> Option<DocPos> {
        let mut it = buf.chars_bck(pos);
        if let Some(_) = it.next() {} else {
            return Some(pos)
        };
        match it.next().unwrap_or((pos, ' ')).1.is_only_wordchar_extended() {
            true => it.skip_while(|c| c.1.is_only_wordchar_extended()).map(|(p, _)| p).next(),
            false => it.skip_while(|c| c.1.is_wordchar()).skip_while(|c| !c.1.is_wordchar()).map(|(p, _)| p).next(),
        }
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
