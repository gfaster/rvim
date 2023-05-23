use crate::term;

use super::{Command, parser};



pub enum CommandLineInput {
    Append(char),
    Delete,
}

pub enum CommandType {
    Ex
}

pub struct CommandLine {
    buf: String,
}

impl CommandLine {
    pub fn render(&self) {
        let (w,h) = terminal_size::terminal_size().unwrap();
        term::goto(term::TermPos { x: 0, y: h.0 as u32 - 1 });
        print!("\x1b[0m{: <width$}", self.buf, width=w.0 as usize);
        term::flush();
    }

    pub fn input(&mut self, input: CommandLineInput) {
        match input {
            CommandLineInput::Append(c) => {
                self.buf.push(c);
            },
            CommandLineInput::Delete => {
                self.buf.pop();
            },
        };
        self.render();
    }

    pub fn complete(&mut self) -> Option<Box<dyn Command>> {
        let out = parser::parse_command(&self.buf);
        self.clear();
        out
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }

    pub fn new() -> Self {
        Self { buf: String::new() }
    }
}

impl Default for CommandLine {
    fn default() -> Self {
        Self::new()
    }
}

