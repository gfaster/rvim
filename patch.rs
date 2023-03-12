struct Modification {
    /// The characters replaced in original string.
    /// Always based off last modification.
    orig: Range<usize>,

    /// New string
    new: String,
}

impl Modification {
    fn delta(&self) -> isize {
        self.orig.len() as isize - self.new.len() as isize
    }
}

/// dynamic string, made to be modified.
/// Stores modification history within it
struct DynStr {
    base: String,
    qmod: VecDeque<Modification>,
}

impl DynStr {
    fn add_mod(&mut self, r: Range<usize>, n: String) {
        self.qmod.push_back(Modification { orig: r, new: n });
    }

    fn undo(&mut self) {
        self.qmod.pop_back();
    }

    /// absorb changes leaving khist modifications
    fn fold_in(&mut self, khist: usize) {
        while self.qmod.len() > khist {
            if let Some(change) = self.qmod.pop_front() {
                self.base.replace_range(change.orig, change.new.as_str());
            } else {
                break;
            };
        }
    }

    /// gets the effective string from effective range
    fn substring(&self, win: Range<usize>) {
        let _start = self.qmod.iter().fold(win.start as isize, |acc, x| {
            if x.orig.end < win.start {
                acc + x.delta()
            } else {
                acc
            }
        }) as usize;
        let _end = self.qmod.iter().fold(win.end as isize, |acc, x| {
            if x.orig.end < win.end {
                acc + x.delta()
            } else {
                acc
            }
        }) as usize;

        todo!()
    }
}
