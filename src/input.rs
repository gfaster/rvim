use crate::textobj::TextObject;
use std::io::stdin;
use std::io::Read;

use crate::Ctx;
use crate::Mode;
use crate::textobj::TextObjectModifier;

pub enum Motion {
    ScreenSpace {dy: isize, dx: isize},
    BufferSpace {doff: isize},
    TextObj (TextObject)
}

pub enum Operation {
    Change,
    Replace(String),
    Insert(String),
    ToInsert,
    Delete,
    ToNormal,
    None
}

enum Accepting {
    Normal,
    MotionOrTextObj {op: Operation},
    TextObject {op: Operation, md: TextObjectModifier},
    Complete (Action)
}

pub struct Action {
    pub motion: Option<Motion>,
    pub operation: Operation,
    pub repeat: Option<u32>
}

pub fn handle_input(ctx: &Ctx) -> Option<Action> {
    match ctx.mode {
        Mode::Normal => handle_normal_mode(),
        Mode::Insert => Some({
            let c = stdin()
                .bytes()
                .map(|b| Some(char::from(b.ok()?)))
                .next()??;
            eprintln!("{:x}", c as u32);
            match c {
                '\x1b' => Action {motion: None, operation: Operation::ToNormal, repeat: None},
                '\x7f' => Action { motion: None, operation: Operation::Delete, repeat: None },
                _ => Action { motion: None, operation: Operation::Insert(c.to_string()), repeat: None  },
            }
        }),
    }
}

fn handle_motion(c: char) -> Option<Motion> {
    match c {
        'h' => Some(Motion::ScreenSpace { dy: 0,  dx: -1 }),
        'j' => Some(Motion::ScreenSpace { dy: 1,  dx: 0  }),
        'k' => Some(Motion::ScreenSpace { dy: -1, dx: 0  }),
        'l' => Some(Motion::ScreenSpace { dy: 0,  dx: 1  }),
        _ => None
    }
}

fn handle_textobj(a: Accepting, c: char) -> Option<Accepting> {
    match (a, c) {
        (Accepting::TextObject { op, md: _ }, _ ) => match c {
            'w' => Some(Accepting::Complete(Action { motion: Some(Motion::TextObj(TextObject::WordObject(crate::textobj::WordObject))), operation: op, repeat: None })),
            _ => None
        }
        _ => panic!("this function should only be called on motion/textobj step")
    }
}

fn handle_motion_or_textobj(a: Accepting, c: char) -> Option<Accepting> {
    match (a, c) {
        (Accepting::MotionOrTextObj { op }, _ ) => match c {
            'h' | 'j' | 'k' | 'l' => Some(Accepting::Complete(Action { motion: handle_motion(c), operation: op, repeat: None })),
            'i' => Some(Accepting::TextObject { op, md: TextObjectModifier::Inner }),
            'a' => Some(Accepting::TextObject { op, md: TextObjectModifier::All }),
            _ => None
        }
        _ => panic!("this function should only be called on motion/textobj step")
    }
}

fn state_machine_step<T>(a: Accepting, reader: &mut T) -> Option<Accepting> 
where
T: Read
{
    let c = reader.bytes()
        .map(|b| char::from(b.expect("cannot read char")))
        .next()?;

    let next = match (&a, c) {
        (Accepting::Normal, _) => handle_normal_input(c),
        (Accepting::MotionOrTextObj { op: _ }, _) => handle_motion_or_textobj(a, c),
        (Accepting::TextObject { op: _, md: _ }, _) => handle_textobj(a, c),
        _ => Some(a)
    };

    next
}

fn handle_normal_input(c: char) -> Option<Accepting> 
{
    match c {
        'h' | 'j' | 'k' | 'l' => Some(Accepting::Complete(Action { motion: handle_motion(c), operation: Operation::None, repeat: None })),
        'a' => Some(Accepting::Complete (Action { motion: Some(Motion::ScreenSpace { dy: 0, dx: 1 }), operation: Operation::ToInsert, repeat: None })),
        'i' => Some(Accepting::Complete (Action { motion: None, operation: Operation::ToInsert, repeat: None })),
        'x' => Some(Accepting::Complete (Action { motion: Some(Motion::ScreenSpace { dy: 0, dx: 1 }), operation: Operation::Delete, repeat: None })),
        'd' => Some(Accepting::MotionOrTextObj { op: Operation::Delete }),
        'c' => Some(Accepting::MotionOrTextObj { op: Operation::Change }),
        _ => None
    }
}

fn handle_normal_mode() -> Option<Action> {
    let mut stdin = stdin().lock();
    let mut wip = Accepting::Normal;
    while !matches!(wip, Accepting::Complete(_)) {
        wip = state_machine_step(wip, &mut stdin)?;
    };
    match wip {
        Accepting::Complete(x) => Some(x),
        _ => panic!("Only complete input sequences should be able to get here")
    }
}
