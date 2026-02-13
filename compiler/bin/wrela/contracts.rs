pub const EXIT_USAGE: i32 = 1;
pub const EXIT_PARSE: i32 = 2;
pub const EXIT_TYPE: i32 = 3;
pub const EXIT_OK: i32 = 0;
pub const EXIT_CODEGEN: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Pretty,
    Json,
}
