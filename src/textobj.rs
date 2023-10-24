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

    /// is same type for word subsets
    fn eq_sub(&self, other: &Self) -> bool {
        (self.is_wordchar() && other.is_wordchar()) 
        || (self.is_only_wordchar_extended() && other.is_only_wordchar_extended())
        || (!self.is_wordchar_extended() && !other.is_wordchar_extended())
    }

    /// is same type for word broadly
    fn eq_super(&self, other: &Self) -> bool {
        self.is_wordchar_extended() == other.is_wordchar_extended()
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

impl From<DynRange> for Range<usize> {
    fn from(value: DynRange) -> Self {
        let start = match value.start_bound() {
            std::ops::Bound::Included(p) => *p,
            std::ops::Bound::Excluded(p) => *p + 1,
            std::ops::Bound::Unbounded => unreachable!(),
        };
        let end = match value.end_bound() {
            std::ops::Bound::Included(p) => *p + 1,
            std::ops::Bound::Excluded(p) => *p ,
            std::ops::Bound::Unbounded => unreachable!(),
        };
        start..end
    }
}

pub mod motions {
    use super::*;

    #[must_use]
    fn empty_is_none(buf: &Buffer) -> Option<()> {
        if buf.len() == 0 {
            None
        } else {
            Some(())
        }
    }

    pub(crate) fn word_forward(buf: &Buffer, pos: usize) -> Option<usize> {
        empty_is_none(buf)?;
        let mut it = buf.chars_fwd(pos).enumerate().peekable();
        it.next();
        it.peek()?;
        it.skip_while(|c| c.1.is_wordchar_extended())
            .skip_while(|c| c.1.is_whitespace())
            .map(|(p, _)| p + pos)
            .next()
            .or_else(|| Some(buf.len()))
    }

    pub(crate) fn word_subset_forward(buf: &Buffer, pos: usize) -> Option<usize> {
        empty_is_none(buf)?;
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
        empty_is_none(buf)?;
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
        empty_is_none(buf)?;
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
        empty_is_none(buf)?;
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
        empty_is_none(buf)?;
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

    fn word_end_backward_base(buf: &Buffer, pos: usize, eq: impl Fn(&char, &char) -> bool) -> Option<usize>{
        empty_is_none(buf)?;
        let first = buf.char_at(pos);
        let pos = pos.saturating_sub(1);
        let back = buf.chars_bck(pos).enumerate()
        .skip_while(|c| eq(&c.1, &first) && !c.1.is_whitespace())
        .skip_while(|c| c.1.is_whitespace())
        .next()
            .map_or(pos, |(i, _)| i);

        Some(pos - back)
    }

    pub(crate) fn word_end_backward(buf: &Buffer, pos: usize) -> Option<usize> {
        word_end_backward_base(buf, pos, <char as Word>::eq_super)
    }

    pub(crate) fn word_end_subset_backward(buf: &Buffer, pos: usize) -> Option<usize> {
        word_end_backward_base(buf, pos, <char as Word>::eq_sub)
    }

    pub(crate) fn start_of_line(buf: &Buffer, pos: usize) -> Option<usize> {
        empty_is_none(buf)?;
        Some(buf.chars_bck(pos).enumerate().filter(|&(_, c)| c == '\n').nth(0).map_or(0, |(i, _)| pos - i))
    }

    pub(crate) fn end_of_line(buf: &Buffer, pos: usize) -> Option<usize> {
        empty_is_none(buf)?;
        Some(buf.chars_fwd(pos).enumerate().filter(|&(_, c)| c == '\n').nth(0)
            .map_or(buf.len().saturating_sub(1), |(i, _)| pos + i.saturating_sub(1)))
    }

    pub(crate) fn end_of_buffer(buf: &Buffer, _pos: usize) -> Option<usize> {
        buf.len().checked_sub(1)
    }

    pub(crate) fn start_of_buffer(buf: &Buffer, _pos: usize) -> Option<usize> {
        if buf.len() == 0 {
            None
        } else {
            Some(0)
        }
    }

    #[cfg(test)]
    mod test {
        use std::fmt::Write;
        use std::ops::Add;

        use super::*;

        fn print_pos(buf: &Buffer, pos: usize) -> String {
            let slice_start = pos.saturating_sub(5);
            let slice_end = pos.add(5).min(buf.len());
            let s = buf.to_string().replace('\n', "$");
            let mut out = String::new();
            writeln!(out, "\n{}", &s[slice_start..slice_end]).unwrap();
            for i in slice_start..slice_end {
                let c = if i == pos {
                    '^'
                } else {
                    ' '
                };
                out.push(c);
            }
            out.push('\n');
            out
        }

        macro_rules! motion_test {
            ($motion:ident, $({$($check:tt)*}),* $(,)?) => {
                #[test]
                fn $motion() {
                    $(motion_test!(@template $motion @ $($check)*);)*
                }
            };
            (@template $motion:ident @ $str:expr => $($res:tt)*) => {
                motion_test!(@template $motion @ $str, 0 => $($res)*);
            };
            (@template $motion:ident @ $str:expr, $pos:expr => None) => {
                motion_test!(@check $motion @ $pos, $str => None);
            };
            (@template $motion:ident @ $str:expr, $pos:expr => $res:expr) => {
                let expected = $str.find($res).expect(
                    concat!("invalid check paramenter: \"",
                        stringify!($res), "\" was not found in test string"));
                motion_test!(@check $motion @ $pos, $str => Some(expected));
            };
            (@check $motion:ident @ $pos:expr, $str:expr => $res:expr) => {
                let buf = Buffer::from_str($str);
                let res = motions::$motion(&buf, $pos);
                if let Some(expected) = $res {
                    if let Some(res) = res {
                        assert_eq!(res, expected, "\nexpected {}...but found{}", print_pos(&buf, expected), print_pos(&buf, res))
                    } else {
                        panic!("\nexpected {}...but found None", print_pos(&buf, expected));
                    }
                } else {
                    assert!(res.is_none(), "\nexpected None but found{}", print_pos(&buf, res.unwrap()));
                }
            }
        }

        motion_test!(
            word_subset_forward, 
            {"asdfa asdfasd" => "asdfasd"},
            {"1023aczlr falsdkf pasdfoq", 5 => "falsdkf"},
            {"a.b" => "."},
            {"a..b" => "."},
            {"aa..b" => "."},
            {"a .b" => "."},
            {"a. b" => "."},
            {"aa( b" => "("},
            {"a) b" => ")"},
            {"a\n. b" => "."},
            {"a\n    . b" => "."},
            {"a    \n. b" => "."},
            {"a    \n    . b" => "."},
            {".,a" => "a"},
            {".,?.a" => "a"},
            {".,?. a" => "a"},
            {"{\".,?.\"} a" => "a"},
        );

        motion_test!(
            word_forward, 
            {"asdfa asdfasd" => "asdfasd"},
            {"1023aczlr falsdkf pasdfoq", 5 => "falsdkf"},
            {"aa( b" => "b"},
            {"a) b" => "b"},
            {"a\n. b" => "."},
            {"a\n    . b" => "."},
            {"a    \n. b" => "."},
            {"a    \n    . b" => "."},
            {".,a b" => "b"},
            {".,?.a b" => "b"},
            {".,?. a b" => "a"},
            {"{\".,?.\"} a" => "a"},
            {"a'b c" => "c"},
        );

        motion_test!(
            word_end_backward, 
            {"012345", 5 => "0"},
            {"0123 5", 5 => "3"},
            {"012 45", 5 => "2"},
            {"01 .45", 5 => "1"},
            {"01 3.5", 5 => "1"},
            {"0 .3.5", 5 => "0"},
        );

        motion_test!(
            word_end_subset_backward, 
            {"012345", 5 => "0"},
            {"0123 5", 5 => "3"},
            {"0123 5", 4 => "3"},
            {"0123\n5", 5 => "3"},
            {"012\n\n5", 5 => "2"},
            {"012 45", 5 => "2"},
            {"01 .45", 5 => "."},
            {"0  .45", 4 => "."},
            {"0  .45", 3 => "0"},
            {"0  .45", 2 => "0"},
            {"0 ,.45", 3 => "0"},
            {"0  .45", 5 => "."},
            {"01 3.5", 5 => "."},
            {"0 ,3.5", 5 => "."},
            {"" => None},
        );

        motion_test!(
            start_of_buffer, 
            {"asdfa 1230" => "asdfa"},
            {"asdfa 1230", 3 => "asdfa"},
            {"asdfa 1230", 9 => "asdfa"},
            {"" => None},
        );

        motion_test!(
            end_of_line,
            {"asdf" => "f"},
            {"01234\n6789" => "4"},
            {"01234\n6789", 4 => "4"},
        );

        motion_test!(
            word_subset_backward,
            {"012345", 5 => "0"},
            {"01 345", 5 => "3"},
            {"01  45", 5 => "4"},
            {"01 3 5", 5 => "3"},
            {"01 3 5", 4 => "3"},
            {"01.3 5", 4 => "3"},
            {"01.3.5", 4 => "3"},
            {"01!,.5", 4 => "!"},
            {"01., 5", 4 => "."},
        );

        motion_test!(
            word_backward,
            {"012345", 5 => "0"},
            {"01 345", 5 => "3"},
            {"01  45", 5 => "4"},
            {"01 3 5", 5 => "3"},
            {"01 3 5", 4 => "3"},
            {"01 3.5", 5 => "3"},
            {"01.3.5", 4 => "0"},
            {"01! .5", 4 => "0"},
            {" 1., 5", 4 => "1"},
        );

        motion_test!(
            end_of_buffer, 
            {"asdfa 1230" => "0"},
            {"asdfa 1230", 3 => "0"},
            {"asdfa 1230", 9 => "0"},
            {"" => None},
        );
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
        .take_while(|c| c.1.category() == first.category())
        .last()
        .map_or(0, |(i, _)| pos - i);
    let end = buf
        .chars_fwd(pos).enumerate()
        .skip_while(|c| c.1.category() == first.category())
        .next()
        .map_or_else(|| buf.len(), |(i, _)| pos + i);
    assert!(start <= end);

    Some(start..end)
}

pub fn a_word(buf: &Buffer, pos: usize) -> Option<Range<usize>> {
    let mut found_white_space = buf.char_at(pos).is_whitespace();
    let start = pos - buf.chars_bck(pos).take_while(|c| c.is_whitespace()).count().saturating_sub(1);
    let pos = pos + buf.chars_fwd(pos).take_while(|c| c.is_whitespace()).count();
    let first = buf.char_at(pos);

    let trail_whitespace = !found_white_space;
    let lead_whitespace = found_white_space;
    let end = buf
        .chars_fwd(pos).enumerate()
        .skip_while(|c| c.1.category() == WordCat::Whitespace)
        .skip_while(|c| c.1.category() == first.category())
        .skip_while(|c| {
            if c.1.is_whitespace() && trail_whitespace{
                found_white_space = true;
                true
            } else {
                false
            }
        })
        .next()
        .map_or_else(|| buf.len(), |(i, _)| i + pos);

    // eprintln!("{}", test::print_cursor(buf, start..pos, init));

    let start = if lead_whitespace {
        start
    } else if found_white_space {
        buf
            .chars_bck(start).enumerate()
            .take_while(|c| c.1.category() == first.category())
            .last()
            .map_or(start, |(i, _)| start - (i))
    } else {
        buf
            .chars_bck(start).enumerate()
            .skip_while(|c| c.1.category() == first.category())
            .take_while(|c| c.1.is_whitespace())
            .last()
            .map_or(0, |(i, _)| start - (i))
    };
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

    assert!(start <= end);
    Some(
        DynRange {
            inc_start: !inner,
            start,
            end,
            inc_end: !inner,
        }.into()
    )
}


#[cfg(test)]
mod test {
    use std::ops::Add;
    use std::fmt::Write;

    use super::*;

    pub fn print_cursor(buf: &Buffer, range: Range<usize>, start: usize) -> String {
        let slice_start = range.start.min(start).saturating_sub(5);
        let slice_end = range.end.max(start).add(5).min(buf.len());
        let s = buf.to_string().replace('\n', "$");
        let mut out = String::new();
        writeln!(out, "\n{}", &s[slice_start..slice_end]).unwrap();
        let slice = slice_start..slice_end;
        for i in slice_start..slice_end {
            let c = if i == 0 && i == range.start {
                '|'
            } else if i + 1 == range.start {
                '>'
            } else if i == range.end {
                '<'
            } else if range.contains(&i) {
                if i + 1 == slice.end {
                    '|'
                } else {
                    '-'
                }
            } else {
                ' '
            };
            out.push(c);
        }
        out.push('\n');
        for i in slice_start..slice_end {
            let c = if i == start {
                '^'
            } else {
                ' '
            };
            out.push(c);
        };
        out.push('\n');
        out
    }

    macro_rules! obj_test {
        ($obj:ident, $({$str:expr $(, $idx:expr)? => $res:expr}),* $(,)?) => {
            #[test]
            fn $obj() {
                $(obj_test!(@template $obj @ $str $(, $idx)* => $res);)*
            }
        };
        (@template $obj:ident @ $str:expr => $res:expr) => {
            let s = $str;
            obj_test!(@template $obj @ s, 0 => $res);
        };
        (@template $obj:ident @ $str:expr, $pos:expr => None) => {
            let s = $str;
            obj_test!(@check $obj @ $pos, s => None);
        };
        (@template $obj:ident @ $str:expr, $pos:expr => $res:expr) => {
            let s = $str;
            let expected = s.find($res).expect(
                concat!("invalid check paramenter: \"",
                    stringify!($res), "\" was not found in test string"));
            obj_test!(@check $obj @ $pos, s => Some(expected..(expected + $res.len())));
        };
        (@check $obj:ident @ $pos:expr, $str:expr => $res:expr) => {
            let buf = Buffer::from_str($str);
            let res = super::$obj(&buf, $pos);
            if let Some(expected) = $res {
                if let Some(res) = res {
                    assert_eq!(res, expected, "\nexpected range:{}actual range:{}",
                        print_cursor(&buf, expected.clone(), $pos), print_cursor(&buf, res.clone(), $pos));
                } else {
                    panic!("expected range: {} but got None", print_cursor(&buf, expected.clone(), $pos));
                }
            } else {
                assert!(res.is_none(), "\nexpect failure but got:{}", print_cursor(&buf, res.unwrap(), $pos));
            }
        }
    }

    obj_test!{
        inner_word,
        {"asdf" => "asdf"},
        {"asdf 1234" => "asdf"},
        {"asdf 1234", 3 => "asdf"},
        {"asdf 1234", 4 => " "},
        {"asdf 1234", 5 => "1234"},
    }

    obj_test!{
        a_word,
        {"asdf" => "asdf"},
        {"asdf 1234" => "asdf "},
        {"asdf 1234", 3 => "asdf "},
        {"asdf 1234", 4 => " 1234"},
        {"asdf 1234", 5 => " 1234"},
        {" a ", 1 => "a "},
        {"  a ", 1 => "  a"},
    }
}
