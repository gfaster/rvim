//! Window organization and heirarchy
//!

use std::sync::Arc;

use crate::{render::Ctx, tui::TermBox, utils::unit_err, TermGrid};

use super::Window;

pub enum Arrange {
    Horizontal,
    Vertical,
}

pub struct Node {
    bounds: TermBox,
    ty: NodeTy,
}

pub enum NodeTy {
    Terminal(Arc<Window>),
    Nonterminal {
        first: Box<Node>,
        second: Box<Node>,
        arrange: Arrange,
    }
}

unit_err!(DoesNotFit: "not enough room");

impl Node {
    pub fn merge(&mut self, _other: Self, _arrange: Arrange) {
        todo!()
    }

    pub fn fit(&mut self, bounds: TermBox) {
        if bounds == self.bounds {
            return;
        }
        self.bounds = bounds;
        match &mut self.ty {
            NodeTy::Terminal(win) => win.get_mut().set_bounds_outer(bounds),
            NodeTy::Nonterminal { first, second, arrange } => {
                let (b1, b2) = match arrange {
                    Arrange::Horizontal => {
                        let start = bounds.xrng().start;
                        let mid = (bounds.xlen() + start) / 2;
                        let end = bounds.xrng().end;
                        (TermBox::from_ranges(start..mid, bounds.yrng()), TermBox::from_ranges(mid..end, bounds.yrng()))
                    },
                    Arrange::Vertical => {
                        let start = bounds.yrng().start;
                        let mid = (bounds.ylen() + start) / 2;
                        let end = bounds.yrng().end;
                        (TermBox::from_ranges(bounds.xrng(), start..mid), TermBox::from_ranges(bounds.xrng(), mid..end))
                    },
                };
                first.fit(b1);
                second.fit(b2);
            },
        }
    }

    pub fn draw(&self, ctx: &Ctx) {
        match &self.ty {
            NodeTy::Terminal(w) => w.get().draw(ctx),
            NodeTy::Nonterminal { first, second, .. } => {
                // draw back to front
                second.draw(ctx);
                first.draw(ctx);
            },
        }
    }
}

impl From<Arc<Window>> for Node {
    fn from(value: Arc<Window>) -> Self {
        let bounds = value.get().outer_bounds();
        Node { bounds, ty: NodeTy::Terminal(value) }
    }
}

