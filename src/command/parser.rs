use crate::buffer::Buffer;

use super::{Edit, Write, Command};





fn parse_command<B: Buffer>(s: &str) -> Option<Box<dyn Command<B>>> {
    let args = s.split_whitespace().collect::<Vec<_>>();
    Some(match *args.get(0)? {
        "w" | "write" => Box::new(Write{ filename: args.get(1)?.into() }),
        "e" | "edit" => Box::new(Edit{ filename: args.get(1)?.into() }),
        _ => None?
    })
}
