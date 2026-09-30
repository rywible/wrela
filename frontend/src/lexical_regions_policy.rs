// Lexical regions intentionally survive malformed syntax. These rules classify
// angles before parsing; they do not admit declarations or define any type form.
// Ordered conditions map an immutable current region to a structural effect.
use TokenKind::*;
use Event::{Boundary, Token};
use Transition::{Enter, Keep, Leave, Replace};

const ROOT_REGION: Region = Region {
    delimiter: None,
    contents: Contents::Block,
    type_context: false,
    list_types: false,
    declaration_name: false,
    parameters_pending: false,
    head: None,
};

lexical_region_policy! { (event, region, previous, nested) {
    Boundary => Replace(Region {
        type_context: region.list_types,
        declaration_name: false,
        ..region
    }),
    Token(Fn | Record | Enum) => Replace(Region {
        head: Some(match event {
            Token(Fn) => DeclarationHead::Function,
            Token(Record) => DeclarationHead::Record,
            _ => DeclarationHead::Enum,
        }),
        declaration_name: true,
        parameters_pending: matches!(event, Token(Fn)),
        type_context: false,
        ..region
    }),
    Token(If | For | Every | Match | Else) => Replace(Region {
        head: Some(DeclarationHead::Control),
        type_context: false,
        ..region
    }),
    Token(Ident) if region.declaration_name => Replace(Region {
        declaration_name: false,
        type_context: true,
        ..region
    }),
    Token(Colon) => Replace(Region {
        type_context: region.contents != Contents::FieldValues,
        ..region
    }),
    Token(Arrow) => Replace(Region { type_context: true, ..region }),
    Token(Where) => Replace(Region {
        type_context: true,
        list_types: true,
        ..region
    }),
    Token(Let | Var | Equal | Semicolon) => Replace(Region {
        type_context: false,
        declaration_name: false,
        ..region
    }),
    Token(Comma) => Replace(Region { type_context: region.list_types, ..region }),
    Token(LParen | LBracket) => {
        let delimiter = match event { Token(kind) => kind, Boundary => unreachable!() };
        let parameters = delimiter == LParen && region.parameters_pending;
        let list_types = !parameters
            && (region.contents == Contents::EnumVariants || region.type_context);
        Enter {
            parent: if parameters {
                Region { parameters_pending: false, type_context: false, declaration_name: false, ..region }
            } else { region },
            child: Region {
                delimiter: Some(delimiter),
                type_context: list_types,
                list_types,
                ..ROOT_REGION
            },
        }
    },
    Token(LBrace) => Enter {
        parent: Region { head: None, type_context: false, declaration_name: false, ..region },
        child: Region {
            delimiter: Some(LBrace),
            contents: match region.head {
                Some(DeclarationHead::Record) => Contents::RecordFields,
                Some(DeclarationHead::Enum) => Contents::EnumVariants,
                Some(DeclarationHead::Function | DeclarationHead::Control) => Contents::Block,
                None if previous == Some(Ident) => Contents::FieldValues,
                None => Contents::Block,
            },
            ..ROOT_REGION
        },
    },
    Token(RParen) if nested && region.delimiter == Some(LParen) => Leave,
    Token(RBracket) if nested && region.delimiter == Some(LBracket) => Leave,
    Token(RBrace) if nested && region.delimiter == Some(LBrace) => Leave,
    _ => Keep,
}}
