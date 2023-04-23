use std::io::stdin;
use std::io::Read;

use crate::Ctx;
use crate::Mode;

pub enum Motion {
    ScreenSpace {dy: isize, dx: isize},
    BufferSpace {doff: isize},
}

pub enum Operation {
    Change,
    Typing(String),
    ToInsert,
    Delete,
    ToNormal,
    None
}

pub struct Action {
    pub motion: Option<Motion>,
    pub operation: Operation,
    pub repeat: Option<u32>
}

pub fn handle_input(ctx: &Ctx) -> Option<Action> {
    match ctx.mode {
        Mode::Normal => handle_normal_input(),
        Mode::Insert => Some({
            let c = stdin()
                .bytes()
                .map(|b| Some(char::from(b.ok()?)))
                .next()??;
            eprintln!("{:x}", c as u32);
            match c {
                '\x1b' => Action {motion: None, operation: Operation::ToNormal, repeat: None},
                '\x7f' => Action { motion: None, operation: Operation::Delete, repeat: None },
                _ => Action { motion: None, operation: Operation::Typing(c.to_string()), repeat: None  },
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

fn handle_normal_input() -> Option<Action> {
    let c = stdin()
        .bytes()
        .map(|b| char::from(b.expect("cannot read char")))
        .next()?;
    match c {
        'h' | 'j' | 'k' | 'l' => Some(Action { motion: handle_motion(c), operation: Operation::None, repeat: None }),
        'a' => Some(Action { motion: Some(Motion::ScreenSpace { dy: 0, dx: 1 }), operation: Operation::ToInsert, repeat: None }),
        'i' => Some(Action { motion: None, operation: Operation::ToInsert, repeat: None }),
        'x' => Some(Action { motion: Some(Motion::ScreenSpace { dy: 0, dx: 1 }), operation: Operation::Delete, repeat: None }),
        _ => None,
    }
}
