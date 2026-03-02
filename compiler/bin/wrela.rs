#![allow(clippy::large_enum_variant)]
#![allow(clippy::too_many_arguments)]

#[path = "wrela/mod.rs"]
mod wrela_cmd;

fn main() {
    let spec = wrela_cmd::cli_args::parse(std::env::args().skip(1).collect());
    wrela_cmd::command_handlers::execute(spec);
}
