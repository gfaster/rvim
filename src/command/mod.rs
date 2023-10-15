use crate::prelude::*;
use crate::{buffer::Buffer, render::Ctx};
use std::fmt::Write;
use std::{error::Error, fmt::Display, fs::OpenOptions, io::Read, path::PathBuf};
pub mod cmdline;
mod parser;

pub enum Command {
    Write { path: Option<PathBuf> },
    Edit { path: PathBuf },
    ListBuffers,
    Substitute,
    Global,
    Help,
    Quit,
}

#[derive(Debug)]
struct WriteCommandError;
impl Display for WriteCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Write command error")
    }
}
impl Error for WriteCommandError {}

impl Command {
    pub fn exec(self, ctx: &mut Ctx) -> Result<(), Box<dyn Error>> {
        match self {
            Command::Write { path } => {
                let path = path
                    .or_else(|| ctx.focused_buf().path().map(|p| p.to_path_buf()))
                    .ok_or(Box::new(WriteCommandError))?;
                let mut f = OpenOptions::new().write(true).create(true).open(&path)?;
                let linecnt = ctx.focused_buf().linecnt();
                let len = ctx.focused_buf().len();
                ctx.focused_buf().serialize(&mut f)?;
                write!(ctx.info(), "{path:?} {linecnt}L, {len}B written")?;
                Ok(())
            }
            Command::Edit { path } => {
                ctx.open_buffer(Buffer::open(&path)?);
                Ok(())
            }
            Command::Quit => {
                crate::exit();
                Ok(())
            }
            _ => {
                write!(ctx.warning(), "not yet implemented")?;
                Ok(())
            },
        }
    }
}
