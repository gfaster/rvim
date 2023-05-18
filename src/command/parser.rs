use super::{Command, Edit, Write};

fn parse_command(s: &str) -> Option<Box<dyn Command>> {
    let args = s.split_whitespace().collect::<Vec<_>>();
    Some(match *args.first()? {
        "w" | "write" => Box::new(Write {
            filename: args.get(1)?.into(),
        }),
        "e" | "edit" => Box::new(Edit {
            filename: args.get(1)?.into(),
        }),
        _ => None?,
    })
}
