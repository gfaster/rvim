use crate::log;
use crate::prelude::*;
use crate::textobj::Motion;
use std::io::stdin;
use std::io::Read;

use crate::Ctx;
use crate::Mode;

#[derive(PartialEq, Eq, Debug)]
pub enum Operation {
    Change,
    Replace(String),
    Insert(String),
    DeleteBefore,
    DeleteAfter,
    SwitchMode(Mode),
    RecenterView,
    Debug,
    None,
}

#[derive(PartialEq, Eq, Debug)]
pub struct Action {
    pub motion: Option<Motion>,
    pub operation: Operation,
    pub repeat: Option<u32>,
    pub post_motion: Option<Motion>,
}

impl Action {
    pub const fn new() -> Self {
        Action {
            motion: None,
            operation: Operation::None,
            repeat: None,
            post_motion: None,
        }
    }
}

impl From<Motion> for Action {
    fn from(value: Motion) -> Self {
        Self {
            motion: Some(value),
            ..Action::new()
        }
    }
}

impl From<Operation> for Action {
    fn from(value: Operation) -> Self {
        Self {
            operation: value,
            ..Action::new()
        }
    }
}

fn read_char(reader: &mut impl Read) -> Option<char> {
    let mut buf = [0u8];
    reader.read_exact(&mut buf).ok()?;
    let c = char::try_from(buf[0]).ok()?;
    if c == '\x03' {
        crate::exit();
        return None;
    }
    // log!("read: {c:?}");
    Some(c)
}

pub fn handle_input(ctx: &Ctx, reader: &mut impl Read) -> Option<Action> {
    match ctx.mode {
        Mode::Normal => syn::parse_normal_command(reader),
        Mode::Insert | Mode::Command => Some({
            let c = read_char(reader)?;
            // log!("{:x}", c as u32);
            match c {
                '\x03' => {
                    crate::exit();
                    return None;
                }
                '\x1b' => Action {
                    // escape key, this needs to be more sophisticated for pasting
                    operation: Operation::SwitchMode(Mode::Normal),
                    ..Action::new()
                },
                '\x7f' | '\x08' => Action {
                    // delete/backspace keys
                    motion: None,
                    operation: Operation::DeleteBefore,
                    ..Action::new()
                },
                _ => Action {
                    motion: None,
                    operation: Operation::Insert(c.to_string()),
                    ..Action::new()
                },
            }
        }),
    }
}

/// syntax and structure of commands
mod syn {
    use super::read_char;
    use crate::textobj;
    use textobj::motions;

    use super::Action;
    use super::Mode;
    use super::Motion;
    use super::Operation;
    use super::Read;

    fn is_motion_start(c: char) -> bool {
        let mots = load_motions();
        for def in mots {
            if def.comps[0] == CommComp::Char(c) {
                return true;
            }
        }
        false
    }

    #[derive(PartialEq, Eq, Debug)]
    enum CommComp {
        Char(char),
        Motion,
    }

    #[derive(Debug)]
    enum CommType {
        Normal,
        Motion,
        TextObject,
    }

    #[derive(Debug)]
    struct CommDef {
        name: &'static str,
        ctype: CommType,
        comps: Vec<CommComp>,
        action: Action,
    }

    fn parse_motion(first: char, reader: &mut impl Read) -> Option<Motion> {
        let mut defs: Vec<_> = load_motions()
            .into_iter()
            .filter(|d| d.comps[0] == CommComp::Char(first))
            .collect();
        let mut idx = 0;
        let mut rem = vec![];
        while !defs.is_empty() {
            let c = if idx != 0 { read_char(reader)? } else { first };
            for (i, CommDef { comps, .. }) in defs.iter().enumerate() {
                match &comps[idx] {
                    CommComp::Char(xc) if c == *xc => {
                        if idx == comps.len() - 1 {
                            return Some(
                                defs.swap_remove(i)
                                    .action
                                    .motion
                                    .expect("motion has motion"),
                            );
                        }
                    }
                    CommComp::Motion => {
                        panic!("motion token in motion")
                    }
                    _ => rem.push(i),
                };
            }
            if rem.len() == defs.len() {
                return None;
            }
            for i in rem.iter().rev() {
                defs.swap_remove(*i);
            }
            rem.clear();
            idx += 1;
        }
        return None;
    }

    pub(super) fn parse_normal_command(reader: &mut impl Read) -> Option<super::Action> {
        let mut idx = 0;
        let mut defs: Vec<_> = load_comps()
            .into_iter()
            .filter(|d| !matches!(d.ctype, CommType::TextObject))
            .collect();
        let mut rem = vec![];
        loop {
            let c = read_char(reader)?;
            let maybe_motion = is_motion_start(c);
            for (i, CommDef { comps, .. }) in defs.iter().enumerate() {
                // if comps.len() == idx && !matches!(comps.last(), Some(CommComp::Motion)) {
                //     assert_ne!(comps.last(), Some(&CommComp::Motion));
                //     return Some(defs.swap_remove(i).action);
                // }
                match &comps[idx] {
                    CommComp::Char(xc) if c == *xc => {
                        if comps.len() == idx + 1 {
                            return Some(defs.swap_remove(i).action);
                        }
                    }
                    CommComp::Motion if maybe_motion => {
                        assert_eq!(
                            defs.len() - rem.len(),
                            1,
                            "motion command should imply only possibility"
                        );
                        let base = defs.swap_remove(i);
                        assert!(
                            base.action.motion.is_none(),
                            "commands with motion should not include motion"
                        );
                        return Some(Action {
                            motion: Some(parse_motion(c, reader)?),
                            ..base.action
                        });
                    }
                    _ => rem.push(i),
                };
            }
            if rem.len() == defs.len() {
                return None;
            }
            for i in rem.iter().rev() {
                defs.swap_remove(*i);
            }
            rem.clear();
            idx += 1;
        }
        // match defs.len() {
        //     0 => None,
        //     1 => {
        //         assert!(!matches!(defs[0].comps.last().unwrap(), CommComp::Motion) || defs[0].action.motion.is_some());
        //         Some(defs.swap_remove(0).action)
        //     },
        //     _ => unreachable!()
        // }
    }

    macro_rules! commdef {
        ($($name:ident: $type:ident = ($lead:literal $($seq:tt)*) => $action:expr),* $(,)?) => {
            fn load_comps() -> Vec<CommDef> {
                vec![$( CommDef {
                    comps: {
                        let mut v = vec![];
                        v.push(CommComp::Char($lead));
                        commdef!(@pseq v @ $($seq)*);
                        v
                    },
                    ctype: CommType::$type,
                    action: $action.into(),
                    name: stringify!($name),
                },)*]
            }
            fn load_motions() -> Vec<CommDef> {
                [$( CommDef {
                    comps: {
                        let mut v = vec![];
                        v.push(CommComp::Char($lead));
                        commdef!(@pseq v @ $($seq)*);
                        v
                    },
                    ctype: CommType::$type,
                    action: $action.into(),
                    name: stringify!($name),
                    // .inspect(|i| {dbg!(i);})
                },)*].into_iter().filter(|d| matches!(d.ctype, CommType::Motion | CommType::TextObject)).collect()
            }
        };
        (@pseq $v:ident @ $next:literal $($rem:tt)*) => {
            $v.push(CommComp::Char($next));
            commdef!(@pseq $v @ $($rem)*);
        };
        (@pseq $v:ident @ {motion}) => {
            $v.push(CommComp::Motion);
        };
        (@pseq $v:ident @ ) => { };
    }

    commdef! {
        insert: Normal = ('i') => Operation::SwitchMode(Mode::Insert),
        append: Normal = ('a') => Action {
            motion: Some(Motion::ScreenSpace { dy: 0, dx: 1 }),
            operation: Operation::SwitchMode(Mode::Insert),
            ..Action::new()
        },
        delete_char: Normal = ('x') => Action {
            operation: Operation::DeleteAfter,
            // post_motion: Some(Motion::ScreenSpace { dy: 0, dx: 1 }),
            ..Action::new()
        },
        ex: Normal = (':') => Operation::SwitchMode(Mode::Command),
        debug: Normal = ('p') => Operation::Debug,

        change: Normal = ('c' {motion}) => Operation::Change,
        delete: Normal = ('d' {motion}) => Operation::DeleteBefore,


        left: Motion = ('h') => Motion::ScreenSpace { dy: 0, dx: -1 },
        down: Motion = ('j') => Motion::ScreenSpace { dy: 1, dx: 0 },
        up: Motion = ('k') => Motion::ScreenSpace { dy: -1, dx: 0 },
        right: Motion = ('l') => Motion::ScreenSpace { dy: 0, dx: 1 },

        recenter: Normal = ('z' 'z') => Operation::RecenterView,

        inner_word: TextObject = ('i' 'w') => Motion::TextObj(textobj::inner_word_object),

        start_of_line:           Motion = ('0') => Motion::TextMotion(motions::start_of_line),
        word_subset_backward:    Motion = ('b') => Motion::TextMotion(motions::word_subset_backward),
        word_backward:           Motion = ('B') => Motion::TextMotion(motions::word_backward),
        word_subset_forward:     Motion = ('w') => Motion::TextMotion(motions::word_subset_forward),
        word_forward:            Motion = ('W') => Motion::TextMotion(motions::word_forward),
        word_end_subset_forward: Motion = ('e') => Motion::TextMotion(motions::word_end_subset_forward),
        word_end_forward:        Motion = ('E') => Motion::TextMotion(motions::word_end_forward),
        end_of_line:             Motion = ('$') => Motion::TextMotion(motions::end_of_line),
    }

    #[cfg(test)]
    mod test {
        use super::*;

        macro_rules! input_test {
            ($name:ident, $input:literal => match $expected:pat) => {
                #[test]
                fn $name() {
                    let res = parse_normal_command(&mut $input.as_bytes()).expect("success");
                    assert!(
                        matches!(res, $expected),
                        "expected {}, found {:?}",
                        stringify!($expected),
                        &res
                    );
                }
            };
            ($name:ident, $input:literal => None) => {
                #[test]
                fn $name() {
                    let res = parse_normal_command(&mut $input.as_bytes());
                    assert_eq!(res, None);
                }
            };
            ($name:ident, $input:literal => $expected:expr) => {
                #[test]
                fn $name() {
                    let res = parse_normal_command(&mut $input.as_bytes());
                    let expected = $expected.into();
                    assert_eq!(res, Some(expected));
                }
            };
        }

        input_test!(single_normal, "i" => Operation::SwitchMode(Mode::Insert));
        input_test!(single_normal_extra, "iXXXX" => Operation::SwitchMode(Mode::Insert));
        input_test!(single_motion, "h" => Motion::ScreenSpace{ dy: 0, dx: -1 });
        input_test!(single_motion2, "k" => Motion::ScreenSpace{ dy: -1, dx: 0 });
        input_test!(partial_textobj_not_accept, "ci" => None);
        input_test!(single_with_textobj, "ciw" => 
            match Action { motion: Some(Motion::TextObj(_)), operation: Operation::Change, ..});
        input_test!(single_with_motion, "ch" => 
            match Action { motion: Some(Motion::ScreenSpace{..}), operation: Operation::Change, ..});
    }
}
