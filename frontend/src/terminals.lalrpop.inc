extern {
 type Location=usize;
 type Error=&'static str;
 enum Tok {
 "ident"=>Tok::Ident(<TokenId>), "number"=>Tok::Number(<TokenId>), "string"=>Tok::String(<TokenId>),
 "fn"=>Tok::Fn(<TokenId>), "record"=>Tok::Record(<TokenId>), "enum"=>Tok::Enum(<TokenId>), "where"=>Tok::Where(<TokenId>), "let"=>Tok::Let(<TokenId>), "var"=>Tok::Var(<TokenId>), "mut"=>Tok::Mut(<TokenId>), "return"=>Tok::Return(<TokenId>), "if"=>Tok::If(<TokenId>), "else"=>Tok::Else(<TokenId>), "match"=>Tok::Match(<TokenId>), "for"=>Tok::For(<TokenId>), "every"=>Tok::Every(<TokenId>), "in"=>Tok::In(<TokenId>), "true"=>Tok::True(<TokenId>), "false"=>Tok::False(<TokenId>),
 "("=>Tok::LParen(<TokenId>), ")"=>Tok::RParen(<TokenId>), "["=>Tok::LBracket(<TokenId>), "]"=>Tok::RBracket(<TokenId>), "{"=>Tok::LBrace(<TokenId>), "}"=>Tok::RBrace(<TokenId>), ","=>Tok::Comma(<TokenId>), ":"=>Tok::Colon(<TokenId>), ";"=>Tok::Semicolon(<TokenId>), "."=>Tok::Dot(<TokenId>), "::"=>Tok::PathSeparator(<TokenId>), "->"=>Tok::Arrow(<TokenId>), "=>"=>Tok::FatArrow(<TokenId>), "?"=>Tok::Question(<TokenId>),
 "+"=>Tok::Plus(<TokenId>), "-"=>Tok::Minus(<TokenId>), "*"=>Tok::Star(<TokenId>), "/"=>Tok::Slash(<TokenId>), "%"=>Tok::Percent(<TokenId>), "!"=>Tok::Bang(<TokenId>), "="=>Tok::Equal(<TokenId>), "=="=>Tok::EqualEqual(<TokenId>), "!="=>Tok::BangEqual(<TokenId>), "<"=>Tok::Less(<TokenId>), "<="=>Tok::LessEqual(<TokenId>), ">"=>Tok::Greater(<TokenId>), ">="=>Tok::GreaterEqual(<TokenId>), "&&"=>Tok::AndAnd(<TokenId>), "||"=>Tok::OrOr(<TokenId>), "generic<"=>Tok::GenericOpen(<TokenId>), "generic>"=>Tok::GenericClose(<TokenId>), "type<"=>Tok::TypeOpen(<TokenId>), "type>"=>Tok::TypeClose(<TokenId>), "newline"=>Tok::Newline(<TokenId>)
 }
}
