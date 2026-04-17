#![allow(unused_assignments)]
#![allow(unused_imports)]

use crate::hir::{
    Arg, BinaryOp, Body, ClassRole, Expr, FieldBounds, FieldClass, FieldDefault, FieldExpr,
    FieldGraph, FieldMetadata, FieldPrimitive, FieldSupport, Function, FunctionKind, FunctionLane,
    FunctionRole, Idx, InterfaceMethodKind, Literal, Module, Pattern, RegionItemMetadata, Shape,
    ShapeExpr, ShapeGraph, ShapeLeaf, Stmt, TypeRef, UnaryOp, Visibility, body_key,
};
use crate::portable::{
    PortableBuiltinAtom, PortableBuiltinType, builtin_record, is_builtin_record_name,
};
use miette::{Diagnostic, SourceSpan};
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

mod async_effects;
mod calls;
mod conformance;
mod context;
mod expr;
mod stmt;
#[cfg(test)]
mod tests;
mod types;

use async_effects::{
    build_call_chain, build_call_maps, build_func_labels, check_async_actor_usage,
    check_body_async_usage, check_expr_async_usage, check_stmt_async_usage,
    collect_direct_await_and_sync_calls, visit_expr_for_async, visit_stmt_for_async,
};
use calls::{
    build_type_subst, check_class_init_args, error_type, infer_list, infer_map, literal_type,
    requires_named_args, resolve_type_args, substitute_type, type_from_ref, type_from_ref_in_ctx,
    type_from_ref_with_params,
};
use conformance::{
    actor_type_for_detach_target, binary_from_assign, binary_op_label, binary_result,
    callee_error_span, check_type_param_bounds, class_subst, collection_method_sig,
    instantiate_method_params, instantiate_method_ret, interface_method_matches,
    interface_type_compatible, is_assignable, is_identity_primitive, is_integer_like, is_known,
    is_matrix_type, is_numeric, is_result_type, is_scalar_numeric, is_stored_boolean_named,
    is_vector_or_matrix_type, is_vector_type, numeric_result, same_matrix_kind, same_vector_kind,
    span_from_option_range, span_from_range, type_label, type_satisfies_bound, types_known,
    unary_op_label, unary_result, valid_binary, valid_equality_operands, valid_unary,
};
use context::{
    ClassIndex, ClassSig, EnumIndex, EnumSig, FunctionIndex, FunctionSig, InterfaceIndex,
    InterfaceMethodSig, InterfaceSig, MethodSig, TypeContext, body_contains_forbidden_world_return,
    builtin_function_is_portable, builtin_functions, check_function, check_interface_conformance,
    is_pool_of_call, is_pool_of_member, pool_of_class_name, query_authored_return_type,
    query_compat_builtin_functions, query_compat_builtin_params,
    stmt_contains_forbidden_world_return, supports_structural_value_type,
};
use expr::{
    arg_matches_componentwise_shape, batch_item_arg_name, batch_item_type,
    builtin_allows_positional_args, call_named_arg_value, capture_kind_label, detail_tier_type,
    direct_capture_target_name, dispatch_backend_type, expected_capture_label,
    expected_family_query_capture_label, expected_legacy_query_capture_label,
    family_query_call_name, family_query_params, family_query_return_type, field_capture_type,
    infer_call_arg_type, infer_capture_builtin, infer_capture_kind_for_query_arg,
    infer_componentwise_binary_builtin, infer_componentwise_ternary_builtin,
    infer_componentwise_unary_builtin, infer_compute_builtin_call, infer_exact_builtin_call,
    infer_expr, infer_family_query_member_builtin, infer_legacy_query_builtin,
    infer_math_builtin_call, infer_scalar_cast_builtin, infer_scene_backend_builtin,
    is_region_capture_type, is_same_vector_kind, is_scalar_numeric_type, is_vector_like_type,
    is_vector_only_type, legacy_query_contract_candidates, legacy_query_surface,
    push_math_builtin_arg_mismatch, query_family_object, region_capture_type,
    same_vector_like_kind, scalar_item_param, scene_domain_type, shape_capture_type,
    validate_family_query_capture_argument, validate_region_capture_argument,
    validate_scene_domain_argument, vector_component_type,
};
use stmt::{
    AssertEqualityMode, MatchCoverage, bind_pattern, check_assert_approx, check_assert_expr,
    check_stmt, match_case_span, match_missing_variants,
};

use types::{
    is_portable_named_data_type_name, portable_builtin_type_to_type, portable_named_field_type,
    portable_named_type, validate_bounds_clause_type, validate_support_clause_type,
};

pub use types::{
    FunctionTypeInfo, Type, TypeError, TypeInfo, WrapperOperandConstant, check_module,
    check_module_with_info, classify_wrapper_operand_constant,
};
