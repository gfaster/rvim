use crate::prelude::*;

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

// keeping position as separate argument for potential future proofing
pub type TextMotion = fn(&Buffer, DocPos) -> Option<DocPos>;
pub type TextObject = fn(&Buffer, DocPos) -> Option<DocRange>;

#[derive(PartialEq, Eq)]
enum WordCat {
    Word,
    WordExt,
    Whitespace,
    Lf,
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

    fn is_sentence_delim(&self) -> bool;
}

impl Word for char {
    fn is_wordchar(&self) -> bool {
        self.is_alphanumeric() || self == &'_'
    }

    fn is_wordchar_extended(&self) -> bool {
        !self.is_whitespace()
    }

    fn is_sentence_delim(&self) -> bool {
        matches!(self, '.' | '!' | '?')
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

    pub(crate) fn end_of_buffer(buf: &Buffer, _pos: DocPos) -> Option<DocPos> {
        Some(buf.last())
    }

    pub(crate) fn start_of_buffer(_buf: &Buffer, _pos: DocPos) -> Option<DocPos> {
        Some(DocPos {x: 0, y: 0 })
    }
}

/*
*/

pub fn text_object_from_motion(motion: TextMotion, buf: &Buffer, off: DocPos) -> Option<DocRange> {
    let finish = motion(buf, off)?;
    if finish < off {
        Some(DocRange {
            start_inclusive: true,
            start: finish,
            end: off,
            end_inclusive: true,
        })
    } else {
        Some(DocRange {
            start_inclusive: true,
            start: off,
            end: finish,
            end_inclusive: true,
        })
    }
}

pub fn inner_word(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    let first = buf.char_at(pos).expect("calling on valid pos");
    let start = buf
        .chars_bck(pos)
        .skip_while(|c| c.1.category() == first.category())
        .next()
        .map_or(DocPos { x: 0, y: 0 }, |(pos, _)| pos);
    let end = buf
        .chars_fwd(pos)
        .skip_while(|c| c.1.category() == first.category())
        .next()
        .map_or_else(|| buf.end(), |(pos, _)| pos);

    Some(DocRange {
        start_inclusive: false,
        start,
        end,
        end_inclusive: false,
    })
}

pub fn a_word(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    let mut found_white_space = false;
    let first = buf.char_at(pos).expect("calling on valid pos");

    let end = buf
        .chars_fwd(pos)
        .skip_while(|c| c.1.category() == first.category())
        .skip_while(|c| {
            if c.1.category() == WordCat::Whitespace {
                found_white_space = true;
                true
            } else {
                false
            }
        })
        .next()
        .map_or_else(|| buf.end(), |(pos, _)| pos);

    let start = buf
        .chars_bck(pos)
        .skip_while(|c| c.1.category() == first.category())
        .skip_while(|c| !found_white_space && c.1.category() == WordCat::Whitespace)
        .next()
        .map_or(DocPos { x: 0, y: 0 }, |(pos, _)| pos);

    Some(DocRange {
        start_inclusive: false,
        start,
        end,
        end_inclusive: false,
    })
}

pub fn inner_paragraph(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    let lines = buf.get_lines(0..buf.linecnt());
    let first = lines.get(pos.y).expect("valid pos").trim();
    let is_para = first.len() > 0;
    let mut start = pos.y;
    for (i, line) in lines[..pos.y].iter().enumerate().rev() {
        if (line.trim().len() > 0) != is_para {
            break;
        }
        start = i;
    }
    let start = DocPos { x: 0, y: start };
    let mut end = pos.y;
    for (i, line) in lines[pos.y..].iter().enumerate() {
        if (line.trim().len() > 0) != is_para {
            break;
        }
        end = i + pos.y;
    }
    let end = DocPos {
        x: lines[end].len(),
        y: end,
    };
    Some(DocRange {
        start_inclusive: true,
        start,
        end,
        end_inclusive: false,
    })
}

pub fn inner_sentence(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    let mut cnt = 1;
    let start = buf
        .chars_bck(pos)
        .skip_while(|c| {
            c.1 != '\n' && {
                cnt -= 1;
                cnt >= 0
            }
        }) // skip one back but not if it's lf
        .skip_while(|c| !c.1.is_sentence_delim() && c.1 != '\n')
        .next()
        .map_or(DocPos { x: 0, y: 0 }, |(pos, _)| pos);
    let mut it = buf.chars_fwd(pos).peekable();
    let mut end = pos;
    while let Some(c) = it.next() {
        end = c.0;
        if c.1.is_sentence_delim()
            && it
                .peek()
                .map_or(true, |p| p.1.category() == WordCat::Whitespace)
        {
            break;
        }
    }
    Some(DocRange {
        start_inclusive: false,
        start,
        end,
        end_inclusive: false,
    })
}

pub fn a_sentence(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    let mut cnt = 1;
    let start = buf
        .chars_bck(pos)
        .skip_while(|c| {
            c.1 != '\n' && {
                cnt -= 1;
                cnt >= 0
            }
        }) // skip one back but not if it's lf
        .skip_while(|c| !c.1.is_sentence_delim() && c.1 != '\n')
        .next()
        .map_or(DocPos { x: 0, y: 0 }, |(pos, _)| pos);
    let mut it = buf.chars_fwd(pos).peekable();
    let mut end = pos;
    while let Some(c) = it.next() {
        end = c.0;
        if c.1.is_sentence_delim()
            && it
                .peek()
                .map_or(true, |p| p.1.category() == WordCat::Whitespace)
        {
            break;
        }
    }
    Some(DocRange {
        start_inclusive: false,
        start,
        end,
        end_inclusive: true,
    })
}
pub fn inner_paren(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    delim_text_object(buf, pos, '(', ')', true)
}

pub fn a_paren(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    delim_text_object(buf, pos, '(', ')', false)
}

pub fn inner_curly(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    delim_text_object(buf, pos, '{', '}', true)
}

pub fn a_curly(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    delim_text_object(buf, pos, '{', '}', false)
}

pub fn inner_bracket(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    delim_text_object(buf, pos, '[', ']', true)
}

pub fn a_bracket(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    delim_text_object(buf, pos, '[', ']', false)
}

pub fn inner_quote(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    delim_text_object(buf, pos, '"', '"', true)
}

pub fn a_quote(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    delim_text_object(buf, pos, '"', '"', false)
}

pub fn inner_tick(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    delim_text_object(buf, pos, '\'', '\'', true)
}

pub fn a_tick(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    delim_text_object(buf, pos, '\'', '\'', false)
}

pub fn inner_backtick(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    delim_text_object(buf, pos, '`', '`', true)
}

pub fn a_backtick(buf: &Buffer, pos: DocPos) -> Option<DocRange> {
    delim_text_object(buf, pos, '`', '`', false)
}

// FIXME: it can't handle "[]S[]" (starting at 'S')
#[inline(always)]
fn delim_text_object(
    buf: &Buffer,
    pos: DocPos,
    open: char,
    close: char,
    inner: bool,
) -> Option<DocRange> {
    let mut right_stack = 0;
    let mut do_skip = false;
    let end = buf
        .chars_fwd(pos)
        .skip_while(|c| {
            if c.1 == close {
                if right_stack == 0 {
                    if c.0 == pos {
                        do_skip = true
                    }
                    return false;
                } else {
                    right_stack -= 1;
                };
            } else if c.1 == open {
                right_stack += 1;
            }
            true
        })
        .next()?
        .0;

    let mut left_stack = 0;
    let start = buf
        .chars_bck(pos)
        .skip(if do_skip { 1 } else { 0 })
        .skip_while(|c| {
            if c.1 == open {
                if left_stack == 0 {
                    return false;
                } else {
                    left_stack -= 1;
                };
            } else if c.1 == close {
                left_stack += 1;
            }
            true
        })
        .next()?
        .0;

    Some(DocRange {
        start_inclusive: inner,
        start,
        end,
        end_inclusive: inner,
    })
}

#[cfg(test)]
mod test {

    use crate::buffer::BufCore;

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
