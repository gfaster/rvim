use std::ops::{Range, RangeBounds};

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
pub type TextMotion = fn(&Buffer, usize) -> Option<usize>;
pub type TextObject = fn(&Buffer, usize) -> Option<Range<usize>>;

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

struct DynRange {
    inc_start: bool,
    start: usize,
    end: usize,
    inc_end: bool,
}

impl RangeBounds<usize> for DynRange {
    fn start_bound(&self) -> std::ops::Bound<&usize> {
        if self.inc_start {
            std::ops::Bound::Included(&self.start)
        } else {
            std::ops::Bound::Excluded(&self.start)
        }
    }

    fn end_bound(&self) -> std::ops::Bound<&usize> {
        if self.inc_end {
            std::ops::Bound::Included(&self.end)
        } else {
            std::ops::Bound::Excluded(&self.end)
        }
    }
}


pub mod motions {
    use super::*;

    pub(crate) fn word_forward(buf: &Buffer, pos: usize) -> Option<usize> {
        let mut it = buf.chars_fwd(pos).enumerate().peekable();
        it.next();
        it.peek()?;
        it.skip_while(|c| c.1.is_wordchar())
            .skip_while(|c| !c.1.is_wordchar())
            .map(|(p, _)| p + pos)
            .next()
            .or_else(|| Some(buf.len()))
    }

    pub(crate) fn word_subset_forward(buf: &Buffer, pos: usize) -> Option<usize> {
        let mut it = buf.chars_fwd(pos).enumerate().peekable();
        let init = it.next()?.1.category();
        it.peek()?;
        it.skip_while(|c| c.1.category() == init)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .map(|(p, _)| p + pos)
            .next()
            .or_else(|| Some(buf.len()))
    }

    pub(crate) fn word_end_forward(buf: &Buffer, pos: usize) -> Option<usize> {
        let mut it = buf
            .chars_fwd(pos).enumerate()
            .skip(1)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .peekable();
        let mut ret = *it.peek()?;
        while {
            let Some(x) = it.peek() else {
                return Some(buf.len());
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

    pub(crate) fn word_end_subset_forward(buf: &Buffer, pos: usize) -> Option<usize> {
        let mut it = buf
            .chars_fwd(pos).enumerate()
            .skip(1)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .peekable();
        let mut ret = *it.peek()?;
        let init = ret.1.category();
        while {
            let Some(x) = it.peek() else {
                return Some(buf.len());
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
        Some(ret.0 + pos)
    }

    pub(crate) fn word_backward(buf: &Buffer, pos: usize) -> Option<usize> {
        let mut it = buf
            .chars_bck(pos).enumerate()
            .skip(1)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .peekable();
        let mut ret = *it.peek()?;
        while {
            let Some(x) = it.peek() else {
                return Some(0);
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
        Some(pos - ret.0)
    }

    pub(crate) fn word_subset_backward(buf: &Buffer, pos: usize) -> Option<usize> {
        let mut it = buf
            .chars_bck(pos).enumerate()
            .skip(1)
            .skip_while(|c| c.1.category() == WordCat::Whitespace)
            .peekable();
        let mut ret = *it.peek()?;
        let init = ret.1.category();
        while {
            let Some(x) = it.peek() else {
                return Some(0);
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
        Some(pos - ret.0)
    }

    pub(crate) fn word_end_backward(_buf: &Buffer, _pos: usize) -> Option<usize> {
        // buf.chars_bck(pos).skip_while(|c| c.1.is_wordchar_extended()).skip_while(|c| !c.1.is_wordchar_extended()).map(|(p, _)| p).next()
        todo!()
    }

    pub(crate) fn word_end_subset_backward(_buf: &Buffer, _pos: usize) -> Option<usize> {
        // let mut it = buf.chars_bck(pos).skip_while(|c| !c.1.is_wordchar_extended());
        // match it.next().unwrap_or((pos, ' ')).1.is_only_wordchar_extended() {
        //     true => it.skip_while(|c| c.1.is_only_wordchar_extended()).map(|(p, _)| p).next(),
        //     false => it.skip_while(|c| c.1.is_wordchar()).map(|(p, _)| p).next(),
        // }
        todo!();
    }

    pub(crate) fn start_of_line(buf: &Buffer, pos: usize) -> Option<usize> {
        Some(buf.chars_bck(pos).enumerate().filter(|&(_, c)| c == '\n').nth(0).map_or(0, |(i, _)| pos - i))
    }

    pub(crate) fn end_of_line(buf: &Buffer, pos: usize) -> Option<usize> {
        Some(buf.chars_fwd(pos).enumerate().filter(|&(_, c)| c == '\n').nth(0).map_or(0, |(i, _)| pos - i))
    }

    pub(crate) fn end_of_buffer(buf: &Buffer, _pos: usize) -> Option<usize> {
        Some(buf.len().saturating_sub(1))
    }

    pub(crate) fn start_of_buffer(_buf: &Buffer, _pos: usize) -> Option<usize> {
        Some(0)
    }
}



// pub fn text_object_from_motion(motion: TextMotion, buf: &Buffer, off: usize) -> Option<Range<usize>> {
//     let finish = motion(buf, off)?;
//     if finish < off {
//         Some(Range<usize> {
//             start_inclusive: true,
//             start: finish,
//             end: off,
//             end_inclusive: true,
//         })
//     } else {
//         Some(Range<usize> {
//             start_inclusive: true,
//             start: off,
//             end: finish,
//             end_inclusive: true,
//         })
//     }
// }

pub fn inner_word(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    let first = buf.char_at(pos);
    let start = buf
        .chars_bck(pos).enumerate()
        .skip_while(|c| c.1.category() == first.category())
        .next()
        .map_or(0, |(i, _)| pos - i);
    let end = buf
        .chars_fwd(pos).enumerate()
        .skip_while(|c| c.1.category() == first.category())
        .next()
        .map_or_else(|| buf.len(), |(pos, _)| pos);

    Some(start..end)
}

pub fn a_word(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    let mut found_white_space = false;
    let first = buf.char_at(pos);

    let end = buf
        .chars_fwd(pos).enumerate()
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
        .map_or_else(|| buf.len(), |(i, _)| i + pos);

    let start = buf
        .chars_bck(pos).enumerate()
        .skip_while(|c| c.1.category() == first.category())
        .skip_while(|c| !found_white_space && c.1.category() == WordCat::Whitespace)
        .next()
        .map_or(0, |(i, _)| pos - i);

    Some(start..end)
}

pub fn inner_paragraph(_buf: &Buffer, _pos: usize) -> Option<Range<usize>> {
    todo!()
}

pub fn inner_sentence(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    let mut cnt = 1;
    let start = buf
        .chars_bck(pos).enumerate()
        .skip_while(|c| {
            c.1 != '\n' && {
                cnt -= 1;
                cnt >= 0
            }
        }) // skip one back but not if it's lf
        .skip_while(|c| !c.1.is_sentence_delim() && c.1 != '\n')
        .next()
        .map_or(0, |(i, _)| pos - i);
    let mut it = buf.chars_fwd(pos).enumerate().peekable();
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
    Some(start..end)
}

pub fn a_sentence(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    let mut cnt = 1;
    let start = buf
        .chars_bck(pos).enumerate()
        .skip_while(|c| {
            c.1 != '\n' && {
                cnt -= 1;
                cnt >= 0
            }
        }) // skip one back but not if it's lf
        .skip_while(|c| !c.1.is_sentence_delim() && c.1 != '\n')
        .next()
        .map_or(0, |(i, _)| pos - i);
    let mut it = buf.chars_fwd(pos).enumerate().peekable();
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
    Some(start..end)
}
pub fn inner_paren(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    delim_text_object(buf, pos, '(', ')', true)
}

pub fn a_paren(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    delim_text_object(buf, pos, '(', ')', false)
}

pub fn inner_curly(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    delim_text_object(buf, pos, '{', '}', true)
}

pub fn a_curly(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    delim_text_object(buf, pos, '{', '}', false)
}

pub fn inner_bracket(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    delim_text_object(buf, pos, '[', ']', true)
}

pub fn a_bracket(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    delim_text_object(buf, pos, '[', ']', false)
}

pub fn inner_quote(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    delim_text_object(buf, pos, '"', '"', true)
}

pub fn a_quote(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    delim_text_object(buf, pos, '"', '"', false)
}

pub fn inner_tick(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    delim_text_object(buf, pos, '\'', '\'', true)
}

pub fn a_tick(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    delim_text_object(buf, pos, '\'', '\'', false)
}

pub fn inner_backtick(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    delim_text_object(buf, pos, '`', '`', true)
}

pub fn a_backtick(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    delim_text_object(buf, pos, '`', '`', false)
}

// FIXME: it can't handle "[]S[]" (starting at 'S')
#[inline(always)]
fn delim_text_object(
    buf: &Buffer,
    pos: usize,
    open: char,
    close: char,
    inner: bool,
) -> Option<Range<usize>> {
    let mut right_stack = 0;
    let mut do_skip = false;
    let end = buf
        .chars_fwd(pos).enumerate()
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
        .0 + pos;

    let mut left_stack = 0;
    let start = pos - buf
        .chars_bck(pos).enumerate()
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

    Some(start..end)
}

#[cfg(test)]
mod test {

    use crate::buffer::BufCore;

    use super::motions::*;
    use super::TextMotion;
    use super::*;

    // TODO: write some macros for these tests

    fn do_motion_start(buf: &Buffer, motion: TextMotion) -> Option<usize> {
        motion(buf, 0)
    }

    fn apply_motion(buf: &Buffer, motion: TextMotion, pos: &mut Option<usize>) {
        *pos = motion(buf, pos.unwrap());
    }
}
