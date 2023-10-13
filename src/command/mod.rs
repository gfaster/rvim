use crate::prelude::*;
use crate::{buffer::Buffer, render::Ctx};
use std::{error::Error, fmt::Display, fs::OpenOptions, io::Read, path::PathBuf};
pub mod cmdline;
mod parser;

pub enum Command {
    Write { path: Option<PathBuf> },
    Edit { path: PathBuf },
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
    pub fn exec(&self, ctx: &mut Ctx) -> Result<(), Box<dyn Error>> {
        match self {
            Command::Write { path } => {
                let mut f = OpenOptions::new().write(true).create(true).open(
                    path.as_ref()
                        .map(PathBuf::as_path)
                        .or_else(|| ctx.focused_buf().path())
                        .ok_or(Box::new(WriteCommandError))?,
                )?;
                ctx.focused_buf().serialize(&mut f)?;
                Ok(())
            }
            Command::Edit { path } => {
                ctx.open_buffer(Buffer::open(path)?);
                Ok(())
            }
            Command::Quit => {
                crate::exit();
                Ok(())
            }
        }
    }
}
