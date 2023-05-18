use crate::{buffer::Buffer, render::Ctx};
use std::{error::Error, fs::OpenOptions, io::Read, path::PathBuf};
mod parser;

/// The command buffer serves two purposes - entering commands and displaying errors/feedback
///
/// I think I'm going to want to separate out a sort of "render" trait to draw this.
///
/// For now, I'm going to make it only one line
struct CommandBuffer {
    msg: Option<String>,
    content: Option<String>,
}

impl CommandBuffer {
    fn log<'a>(&mut self, msg: impl AsRef<&'a str>) {
        self.msg = msg.as_ref().lines().last().map(|s| s.to_string());
    }
}

trait Command {
    fn exec(self, ctx: &mut Ctx) -> Result<(), Box<dyn Error>>;
}

/// write to file
struct Write {
    filename: PathBuf,
}

impl Command for Write {
    fn exec(self, ctx: &mut Ctx) -> Result<(), Box<dyn Error>> {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .open(self.filename)?;
        ctx.focused_buf().serialize(&mut f)?;
        Ok(())
    }
}

/// Open a file
struct Edit {
    filename: PathBuf,
}

impl Command for Edit {
    fn exec(self, ctx: &mut Ctx) -> Result<(), Box<dyn Error>> {
        let mut f = OpenOptions::new().read(true).open(self.filename)?;
        let mut v = vec![];
        f.read_to_end(&mut v)?;
        ctx.open_buffer(Buffer::from_string(String::from_utf8(v)?));
        Ok(())
    }
}
