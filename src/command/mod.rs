use crate::{buffer::Buffer, render::Ctx};
use std::{error::Error, fs::OpenOptions, io::Read, path::PathBuf, fmt::Display};
mod parser;
pub mod cmdline;


pub trait Command {
    fn exec(self: Box<Self>, ctx: &mut Ctx) -> Result<(), Box<dyn Error>>;
}

/// write to file
struct Write {
    filename: Option<PathBuf>,
}

#[derive(Debug)]
struct WriteCommandError;
impl Display for WriteCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Write command error")
    }
}
impl Error for WriteCommandError { }

impl Command for Write {
    fn exec(self: Box<Self>, ctx: &mut Ctx) -> Result<(), Box<dyn Error>> {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .open(self.filename.as_ref().map(PathBuf::as_path)
                .or_else(|| ctx.focused_buf().path()).ok_or(Box::new(WriteCommandError))?)?;
        ctx.focused_buf().serialize(&mut f)?;
        Ok(())
    }
}

/// Open a file
struct Edit {
    filename: PathBuf,
}

impl Command for Edit {
    fn exec(self: Box<Self>, ctx: &mut Ctx) -> Result<(), Box<dyn Error>> {
        let mut f = OpenOptions::new().read(true).open(&self.filename)?;
        let mut v = vec![];
        f.read_to_end(&mut v)?;
        ctx.open_buffer(Buffer::from_string(String::from_utf8(v)?));
        Ok(())
    }
}
