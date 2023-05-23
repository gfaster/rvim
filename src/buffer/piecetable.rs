use crate::buffer::DocPos;
use crate::window::BufCtx;
use std::ffi::OsStr;
use std::io::{Write, ErrorKind};
use std::ops::Range;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
enum PTType {
    Add,
    Orig,
}

// This is linewise, not characterwise
#[derive(Debug, Clone, Copy)]
struct PieceEntry {
    which: PTType,
    start: usize,
    len: usize,
}

/// Piece Table Buffer
pub struct PTBuffer {
    name: String,
    path: Option<PathBuf>,
    orig: Vec<String>,
    add: Vec<String>,
    table: Vec<PieceEntry>,
}

impl PTBuffer {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_ref().map(PathBuf::as_path)
    }

    pub fn open(file: &Path) -> Result<Self, std::io::Error> {
        let s = std::fs::read_to_string(file)?;
        let mut res = Self::from_string(s);
        res.path = Some(file.canonicalize()?);
        res.name = file.file_name().map(OsStr::to_str).flatten().map(str::to_string)
            .ok_or_else(|| std::io::Error::from(ErrorKind::InvalidInput))?;
        Ok(res)
    }

    pub fn from_string(s: String) -> Self {
        let name = "new buffer".to_string();
        let mut orig: Vec<_> = s.lines().map(str::to_string).collect();
        if orig.is_empty() {
            orig.push("".to_string());
        }
        let add = Vec::new();
        let table = vec![PieceEntry {
            which: PTType::Orig,
            start: 0,
            len: orig.len(),
        }];
        Self {
            path: None,
            name,
            orig,
            add,
            table,
        }
    }

    pub fn delete_char(&mut self, _ctx: &mut BufCtx) -> char {
        todo!()
    }

    pub fn insert_string(&mut self, ctx: &mut BufCtx, s: &str) {
        let pos = ctx.cursorpos; // since this is just insertion, we always replace one line
        let (prev, tidx, testartln) = self.get_line(pos);
        let te = self.table[tidx];
        // eprintln!("prev: {prev:?}  tidx: {tidx:?}  start: {testartln:?}");
        let mut new = prev.to_string();
        new.replace_range(pos.x..pos.x, s);
        let addv = new.split('\n').map(str::to_string).collect::<Vec<_>>();

        if addv.len() > 1 {
            ctx.cursorpos.x = s.lines().last().unwrap().len();
        } else {
            ctx.cursorpos.x = s.len() + pos.x;
        }
        ctx.cursorpos.y += addv.len() - 1;

        let addstart = self.add.len();
        self.add.extend(addv.into_iter());
        let addlen = self.add.len() - addstart;
        self.table.remove(tidx);

        // the insertion position is before the end of the chunk
        if pos.y + 1 < testartln + te.len {
            self.table.insert(
                tidx,
                PieceEntry {
                    which: te.which,
                    start: te.start + (pos.y + 1 - testartln),
                    len: te.len - (pos.y + 1 - testartln),
                },
            )
        }

        // new stuffs
        self.table.insert(
            tidx,
            PieceEntry {
                which: PTType::Add,
                start: addstart,
                len: addlen,
            },
        );

        // the insertion position is past the beginning of the chunk, so reinsert for those lines
        if pos.y > testartln {
            self.table.insert(
                tidx,
                PieceEntry {
                    which: te.which,
                    start: te.start,
                    len: pos.y - testartln,
                },
            )
        }

        // eprintln!("Inserted {s:?} at {pos:?}\norig: {:?}\nnew: {:?}\ntable: {:?}\n", &self.orig, &self.add, &self.table);
    }

    pub fn get_off(&self, _pos: DocPos) -> usize {
        todo!()
    }

    pub fn get_lines(&self, lines: Range<usize>) -> Vec<&str> {
        let (tidx, start) = self.table_idx(DocPos {
            x: 0,
            y: lines.start,
        });
        let extra = lines.start - start;
        self.lines_fwd_internal(tidx)
            .skip(extra)
            .take(lines.len())
            .map(String::as_ref)
            .collect()
    }

    pub fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        for line in self.lines_fwd_internal(0) {
            writeln!(writer, "{}", line)?;
        }
        Ok(())
    }

    pub fn linecnt(&self) -> usize {
        self.table.iter().map(|te| te.len).sum()
    }

    pub fn end(&self) -> DocPos {
        let y = self.linecnt() - 1;
        let x = self.get_line(DocPos { x: 0, y }).0.len();
        DocPos { x, y }
    }
}

impl PTBuffer {
    fn match_table(&self, which: &PTType) -> &[String] {
        match which {
            PTType::Add => &self.add,
            PTType::Orig => &self.orig,
        }
    }

    /// Iterator over lines starting at table table entry tidx
    fn lines_fwd_internal(&self, tidx: usize) -> impl Iterator<Item = &String> {
        self.table[tidx..]
            .iter()
            .flat_map(|te| self.match_table(&te.which)[te.start..].iter().take(te.len))
    }

    /// Iterator over reverse-order lines starting at table entry tidx
    fn lines_bck_internal(&self, tidx: usize) -> impl Iterator<Item = &String> {
        self.table[..tidx].iter().rev().flat_map(|te| {
            self.match_table(&te.which)[te.start..]
                .iter()
                .rev()
                .take(te.len)
        })
    }

    /// get the table idx and line at pos
    ///
    /// Return (line, tidx, te start line)
    fn get_line(&self, pos: DocPos) -> (&str, usize, usize) {
        let (tidx, first) = self.table_idx(pos);
        let te = &self.table[tidx];
        let rem = pos.y - first;
        let line = &self.match_table(&te.which)[te.start + rem];

        let truefirst = self.table[..tidx].iter().map(|te| te.len).sum();
        assert!(
            (truefirst..(truefirst + te.len)).contains(&pos.y),
            "{:?} does not contain {pos:?}",
            self.table[tidx]
        );

        (line, tidx, first)
    }

    /// returns the table idx and start line of entry for pos
    ///
    /// Returns: (table index, te start line)
    fn table_idx(&self, pos: DocPos) -> (usize, usize) {
        let mut line = 0;
        let tidx = self
            .table
            .iter()
            .enumerate()
            .take_while(|x| {
                if line + x.1.len <= pos.y {
                    line += x.1.len;
                    true
                } else {
                    false
                }
            })
            .map(|(i, _)| i + 1)
            .last()
            .unwrap_or(0);

        let truefirst = self.table[..tidx].iter().map(|te| te.len).sum();
        assert!(
            (truefirst..(truefirst + self.table[tidx].len)).contains(&pos.y),
            "{:?} does not contain {pos:?}",
            self.table[tidx]
        );

        (tidx, line)
    }
}
