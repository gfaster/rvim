use crate::term;

use super::Command;



pub enum CommandLineInput {
    Append(char),
    Delete,
    Exec
}

pub enum CommandType {
    Ex
}

pub struct CommandLine {
    buf: String,
}

impl CommandLine {
    pub fn render(&self) {
        term::goto(term::TermPos { x: 0, y: terminal_size::terminal_size().unwrap().1.0 as u32 - 1 });
        print!("\x1b[0m{}", self.buf);
        term::flush();
    }

    pub fn input(&mut self, input: CommandLineInput) -> Option<Box<dyn Command>> {
        let out;
        match input {
            CommandLineInput::Append(c) => {
                self.buf.push(c);
                out = None;
            },
            CommandLineInput::Delete => {
                self.buf.pop();
                out = None;
            },
            CommandLineInput::Exec => {
                todo!()
            },
        };
        self.render();

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

