use super::{DocPos};

pub struct SimpleBuffer {
    data: String,
}

impl SimpleBuffer {
    fn name(&self) -> &str {
        "new simple buffer"
    }

    fn open(file: &std::path::Path) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self::from_string(std::fs::read_to_string(file)?))
    }

    fn from_string(s: String) -> Self {
        let mut data = s;
        if data.is_empty() {
            data = "\n".to_string();
        }
        Self { data }
    }

    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(self.data.as_bytes())
    }

    fn get_lines(&self, lines: std::ops::Range<usize>) -> Vec<&str> {
        self.data.lines().skip(lines.start).take(lines.len()).collect()
    }

    fn delete_char(&mut self, ctx: &mut crate::window::BufCtx) -> char {
        let off = self.to_fileoff(ctx.cursorpos);
        self.data.remove(off)
    }

    fn insert_string(&mut self, ctx: &mut crate::window::BufCtx, s: &str) {
        let off = self.to_fileoff(ctx.cursorpos);
        self.data.insert_str(off, s);
    }

    fn get_off(&self, pos: super::DocPos) -> usize {
        self.to_fileoff(pos)
    }

    fn linecnt(&self) -> usize {
        self.data.lines().count()
    }

    fn end(&self) -> super::DocPos {
        super::DocPos {
            x: self
                .data
                .lines()
                .last()
                .map(str::len)
                .unwrap_or(0),
            y: self.linecnt() - 1,
        }
    }
}

// impl BufferExt for SimpleBuffer {} 

impl SimpleBuffer {
    fn to_fileoff(&self, pos: DocPos) -> usize {
        self.data.lines().take(pos.y).map(str::len).map(|l| l + 1).sum::<usize>() + pos.x
    }
}
