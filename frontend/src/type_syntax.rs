//! Action domains for the shared type grammar and its prospective depth budget.
use crate::token::TokenId;

pub struct TypeDepth {
    pub(crate) current: usize,
    pub(crate) maximum: usize,
}
impl TypeDepth {
    pub fn unlimited() -> Self {
        Self::bounded(usize::MAX)
    }
    pub fn bounded(maximum: usize) -> Self {
        Self {
            current: 0,
            maximum,
        }
    }
    pub fn enter(&mut self) -> Result<(), &'static str> {
        self.current += 1;
        if self.current > self.maximum {
            Err("prospective depth")
        } else {
            Ok(())
        }
    }
    pub fn check_next(&self) -> Result<(), &'static str> {
        if self.current.saturating_add(1) > self.maximum {
            Err("prospective depth")
        } else {
            Ok(())
        }
    }
    pub fn leave(&mut self) {
        self.current -= 1;
    }
}

pub mod construction {
    use super::TokenId;
    use crate::syntax::{self, Delimited, Node, Separated, TypeKind};
    pub type Type = syntax::Type;
    pub type Path = syntax::Path;
    pub type List = Separated<Type>;
    pub type Arguments = Delimited<List>;
    pub fn named(l: usize, path: Path, arguments: Option<Arguments>, r: usize) -> Type {
        Node::new(l, r, TypeKind::Named { path, arguments })
    }
    pub fn tuple(l: usize, open: TokenId, contents: List, close: TokenId, r: usize) -> Type {
        Node::new(
            l,
            r,
            TypeKind::Tuple(Delimited {
                open,
                contents,
                close,
            }),
        )
    }
    pub fn array(l: usize, open: TokenId, ty: Type, close: TokenId, r: usize) -> Type {
        Node::new(
            l,
            r,
            TypeKind::Array(Delimited {
                open,
                contents: Box::new(ty),
                close,
            }),
        )
    }
    pub fn arguments(open: TokenId, contents: List, close: TokenId) -> Arguments {
        Delimited {
            open,
            contents,
            close,
        }
    }
    pub fn path(first: TokenId, rest: Vec<(TokenId, TokenId)>) -> Path {
        let mut path = Path {
            segments: vec![first],
            separators: vec![],
        };
        for (separator, next) in rest {
            path.separators.push(separator);
            path.segments.push(next);
        }
        path
    }
    pub fn empty_list() -> List {
        List::default()
    }
    pub fn restore_order(list: List) -> List {
        list.restore_order()
    }
    pub fn push_reversed(item: Type, comma: Option<TokenId>, rest: List) -> List {
        List::push_reversed(item, comma, rest)
    }
}

/// No syntax trees: only token-path tails, the generated LR stack and explicit policy
/// bookkeeping are needed to recognize a prospective type argument list.
pub mod recognition {
    use super::TokenId;
    pub type Type = ();
    pub type Path = ();
    pub type List = ();
    pub type Arguments = ();
    pub fn named(_: usize, _: Path, _: Option<Arguments>, _: usize) {}
    pub fn tuple(_: usize, _: TokenId, _: List, _: TokenId, _: usize) {}
    pub fn array(_: usize, _: TokenId, _: Type, _: TokenId, _: usize) {}
    pub fn arguments(_: TokenId, _: List, _: TokenId) {}
    pub fn path(_: TokenId, _: Vec<(TokenId, TokenId)>) {}
    pub fn empty_list() {}
    pub fn restore_order(_: List) {}
    pub fn push_reversed(_: Type, _: Option<TokenId>, _: List) {}
}
