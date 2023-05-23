use crate::{buffer::Buffer, render::Ctx};
use std::{error::Error, fs::OpenOptions, io::Read, path::PathBuf};
mod parser;
pub mod cmdline;


pub trait Command {
    fn exec(&mut self, ctx: &mut Ctx) -> Result<(), Box<dyn Error>>;
}

/// write to file
struct Write {
    filename: PathBuf,
}

impl Command for Write {
    fn exec(&mut self, ctx: &mut Ctx) -> Result<(), Box<dyn Error>> {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&self.filename)?;
        ctx.focused_buf().serialize(&mut f)?;
        Ok(())
    }
}

/// Open a file
struct Edit {
    filename: PathBuf,
}

impl Command for Edit {
    fn exec(&mut self, ctx: &mut Ctx) -> Result<(), Box<dyn Error>> {
        let mut f = OpenOptions::new().read(true).open(&self.filename)?;
        let mut v = vec![];
        f.read_to_end(&mut v)?;
        ctx.open_buffer(Buffer::from_string(String::from_utf8(v)?));
        Ok(())
    }
}
