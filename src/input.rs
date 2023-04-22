use std::io::Read;
use std::io::stdin;

use crate::Ctx;
use crate::Mode;

pub struct Motion {
    pub dy: isize,
    pub dx: isize
}

pub enum Token {
    Motion(Motion),
    SetMode(Mode),
    Insert(char)
}

pub fn handle_input(ctx: &Ctx) -> Option<Token> {
    match ctx.mode {
        Mode::Normal => handle_normal_input(),
        Mode::Insert => Some({
            let c = stdin().bytes().map(|b| char::from(b.expect("cannot read char"))).next()?;
            eprintln!("{:x}", c as u32);
            match c {
                '\x1b' => Token::SetMode(Mode::Normal),
                _ => Token::Insert(c)
            }
        })
    }
}

fn handle_normal_input() -> Option<Token> {
    let c = stdin().bytes().map(|b| char::from(b.expect("cannot read char"))).next()?;
    match c {
        'h' => Some(Token::Motion(Motion {dx: -1, dy: 0 })),
        'j' => Some(Token::Motion(Motion {dx: 0,  dy: 1 })),
        'k' => Some(Token::Motion(Motion {dx: 0,  dy: -1})),
        'l' => Some(Token::Motion(Motion {dx: 1,  dy: 0 })),
        'i' => Some(Token::SetMode(Mode::Insert)),
        _ => None
    }
}
