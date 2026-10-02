//! Diagnostics for the wrela compiler: spans and source files, the registry of diagnostic codes
//! (each with a `wrela explain` text), and the text and JSON renderings.

mod chars;
pub mod codes;
mod diagnostic;
mod json;
mod render;
mod source;

pub use chars::{display_name, is_visible};
pub use codes::{Code, CodeInfo};
pub use diagnostic::{Diagnostic, Edit, Help, Label, Severity};
pub use json::{JSON_VERSION, to_json};
pub use render::{render, render_all};
pub use source::{FileId, LineCol, SourceFile, SourceMap, SourceTooLarge, Span};
