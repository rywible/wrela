//! Storage for the declarative, error-tolerant lexical region policy.
use crate::token::TokenKind;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Contents {
    Block,
    RecordFields,
    EnumVariants,
    FieldValues,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum DeclarationHead {
    Function,
    Record,
    Enum,
    Control,
}
#[derive(Clone, Copy)]
struct Region {
    delimiter: Option<TokenKind>,
    contents: Contents,
    type_context: bool,
    list_types: bool,
    declaration_name: bool,
    parameters_pending: bool,
    head: Option<DeclarationHead>,
}
#[derive(Clone, Copy)]
enum Event {
    Token(TokenKind),
    Boundary,
}
enum Transition {
    Keep,
    Replace(Region),
    Enter { parent: Region, child: Region },
    Leave,
}

// Generate the recognizer directly from ordered immutable transition rules.
// The macro supplies no token decisions or region defaults of its own.
macro_rules! lexical_region_policy {
    (($event:ident, $region:ident, $previous:ident, $nested:ident) {
        $($pattern:pat $(if $guard:expr)? => $effect:expr),+ $(,)?
    }) => {
        fn transition($event: Event, $region: Region, $previous: Option<TokenKind>, $nested: bool) -> Transition {
            match $event { $($pattern $(if $guard)? => $effect),+ }
        }
    };
}
include!("lexical_regions_policy.rs");

pub(crate) struct LexicalRegions {
    frames: Vec<Region>,
}
impl LexicalRegions {
    pub(crate) fn new() -> Self {
        Self {
            frames: vec![ROOT_REGION],
        }
    }
    pub(crate) fn expects_type_angles(&self) -> bool {
        self.frames.last().unwrap().type_context
    }
    pub(crate) fn boundary(&mut self) {
        self.apply(Event::Boundary, None);
    }
    pub(crate) fn observe(&mut self, kind: TokenKind, previous: Option<TokenKind>) {
        self.apply(Event::Token(kind), previous);
    }
    fn apply(&mut self, event: Event, previous: Option<TokenKind>) {
        match transition(
            event,
            *self.frames.last().unwrap(),
            previous,
            self.frames.len() > 1,
        ) {
            Transition::Keep => {}
            Transition::Replace(region) => *self.frames.last_mut().unwrap() = region,
            Transition::Enter { parent, child } => {
                *self.frames.last_mut().unwrap() = parent;
                self.frames.push(child);
            }
            Transition::Leave => {
                self.frames.pop();
            }
        }
    }
}
