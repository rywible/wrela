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

pub mod acceleration;
pub mod artifact_contract;
pub mod artifact_key;
pub mod artifact_layout;
pub mod artifact_store;
pub mod backend;
pub mod collision_contract;
pub mod collision_exec;
pub mod collision_plan;
pub mod diag;
pub mod engine_frame;
pub mod execution_policy;
pub mod gpu_runtime;
pub mod hir;
pub mod kernel;
pub mod lexer;
pub mod mir;
pub mod parser;
pub mod perf_target;
pub mod pir;
pub mod portable;
pub mod presentation_binding;
pub mod presentation_contract;
pub mod presentation_exec;
pub mod presentation_plan;
pub mod query_contract;
pub mod query_exec;
pub mod query_plan;
pub mod query_program_spine;
pub mod query_solver;
pub mod scene_ir;
pub mod semantic_evidence;
pub mod state_advance;
pub mod time_semantics;
pub mod world_identity;
