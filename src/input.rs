use crate::textobj::motions::*;
use crate::textobj::word_object;
use crate::textobj::Motion;
use std::io::stdin;
use std::io::Read;

use crate::textobj::TextObjectModifier;
use crate::Ctx;
use crate::Mode;

pub enum Operation {
    Change,
    Replace(String),
    Insert(String),
    Delete,
    SwitchMode(Mode),
    Debug,
    None,
}

enum Accepting {
    Normal,
    MotionOrTextObj {
        op: Operation,
    },
    TextObject {
        op: Operation,
        md: TextObjectModifier,
    },
    Complete(Action),
}

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

pub fn handle_input(ctx: &Ctx) -> Option<Action> {
    match ctx.mode {
        Mode::Normal => handle_normal_mode(),
        Mode::Insert | Mode::Command => Some({
            let c = stdin()
                .bytes()
                .map(|b| Some(char::from(b.ok()?)))
                .next()
                .flatten()?;
            // log!("{:x}", c as u32);
            match c {
                '\x1b' => Action {
                    // escape key, this needs to be more sophisticated for pasting
                    operation: Operation::SwitchMode(Mode::Normal),
                    ..Action::new()
                },
                '\x7f' | '\x08' => Action {
                    // delete/backspace keys
                    motion: None,
                    operation: Operation::Delete,
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

fn handle_motion(c: char) -> Option<Motion> {
    match c {
        'h' => Some(Motion::ScreenSpace { dy: 0, dx: -1 }),
        'j' => Some(Motion::ScreenSpace { dy: 1, dx: 0 }),
        'k' => Some(Motion::ScreenSpace { dy: -1, dx: 0 }),
        'l' => Some(Motion::ScreenSpace { dy: 0, dx: 1 }),
        '0' => Some(Motion::TextMotion(start_of_line)),
        'b' => Some(Motion::TextMotion(word_subset_backward)),
        'B' => Some(Motion::TextMotion(word_backward)),
        'w' => Some(Motion::TextMotion(word_subset_forward)),
        'W' => Some(Motion::TextMotion(word_forward)),
        'e' => Some(Motion::TextMotion(word_end_subset_forward)),
        'E' => Some(Motion::TextMotion(word_end_forward)),
        '$' => Some(Motion::TextMotion(end_of_line)),
        _ => panic!("handle_motion should only be called with motion"),
    }
}

fn handle_textobj(a: Accepting, c: char) -> Option<Accepting> {
    match (a, c) {
        (Accepting::TextObject { op, md: _ }, _) => match c {
            'w' => Some(Accepting::Complete(Action {
                motion: Some(Motion::TextObj(word_object)),
                operation: op,
                ..Action::new()
            })),
            _ => None,
        },
        _ => panic!("this function should only be called on motion/textobj step"),
    }
}

fn handle_motion_or_textobj(a: Accepting, c: char) -> Option<Accepting> {
    match (a, c) {
        (Accepting::MotionOrTextObj { op }, _) => match c {
            'h' | 'j' | 'k' | 'l' | '0' | 'w' | 'W' | '$' | 'b' | 'B' | 'e' | 'E' => {
                Some(Accepting::Complete(Action {
                    motion: handle_motion(c),
                    operation: op,
                    ..Action::new()
                }))
            }
            'i' => Some(Accepting::TextObject {
                op,
                md: TextObjectModifier::Inner,
            }),
            'a' => Some(Accepting::TextObject {
                op,
                md: TextObjectModifier::All,
            }),
            _ => None,
        },
        _ => panic!("this function should only be called on motion/textobj step"),
    }
}

fn state_machine_step(a: Accepting, reader: &mut impl Read) -> Option<Accepting> {
    let c = reader
        .bytes()
        .map(|b| char::from(b.expect("cannot read char")))
        .next()?;

    match (&a, c) {
        (Accepting::Normal, _) => handle_normal_input(c),
        (Accepting::MotionOrTextObj { op: _ }, _) => handle_motion_or_textobj(a, c),
        (Accepting::TextObject { op: _, md: _ }, _) => handle_textobj(a, c),
        _ => Some(a),
    }
}

fn handle_normal_input(c: char) -> Option<Accepting> {
    match c {
        'h' | 'j' | 'k' | 'l' | 'w' | 'W' | '0' | '$' | 'b' | 'B' | 'e' | 'E' => {
            Some(Accepting::Complete(Action {
                motion: handle_motion(c),
                ..Action::new()
            }))
        }
        'a' => Some(Accepting::Complete(Action {
            motion: Some(Motion::ScreenSpace { dy: 0, dx: 1 }),
            operation: Operation::SwitchMode(Mode::Insert),
            ..Action::new()
        })),
        'i' => Some(Accepting::Complete(Action {
            operation: Operation::SwitchMode(Mode::Insert),
            ..Action::new()
        })),
        ':' => Some(Accepting::Complete(Action {
            operation: Operation::SwitchMode(Mode::Command),
            ..Action::new()
        })),
        'x' => Some(Accepting::Complete(Action {
            operation: Operation::Delete,
            post_motion: Some(Motion::ScreenSpace { dy: 0, dx: 1 }),
            ..Action::new()
        })),
        'd' => Some(Accepting::MotionOrTextObj {
            op: Operation::Delete,
        }),
        'c' => Some(Accepting::MotionOrTextObj {
            op: Operation::Change,
        }),
        'p' => Some(Accepting::Complete(Action {
            motion: None,
            operation: Operation::Debug,
            ..Action::new()
        })),
        _ => None,
    }
}

fn handle_normal_mode() -> Option<Action> {
    let mut stdin = stdin().lock();
    let mut wip = Accepting::Normal;
    loop {
        match wip {
            Accepting::Complete(x) => return Some(x),
            _ => wip = state_machine_step(wip, &mut stdin)?,
        };
    }
}
