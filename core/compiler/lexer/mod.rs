mod comments;
mod cursor;
mod errors;
mod indent;
mod lexer;
mod literals;
mod strings;
mod tokens;

#[cfg(test)]
mod tests;

pub use errors::LexError;
pub use lexer::Lexer;
pub use tokens::{SpannedToken, Token};
