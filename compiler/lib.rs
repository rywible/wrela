#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::module_inception)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::only_used_in_recursion)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::question_mark)]
#![allow(clippy::redundant_guards)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![allow(clippy::vec_init_then_push)]

pub mod backend;
pub mod diag;
pub mod hir;
pub mod kernel;
pub mod lexer;
pub mod mir;
pub mod parser;
pub mod pir;
pub mod portable;
pub mod query_exec;
pub mod query_plan;
pub mod scene_ir;
