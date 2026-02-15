#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PIPELINE="$ROOT/wrela-v2/src/application/check_pipeline.wr"
COMMAND="$ROOT/wrela-v2/src/application/check_command.wr"
CONTRACT="$ROOT/wrela-v2/tests/contract/check_pipeline_contract_test.wr"
MIR_CONTRACT="$ROOT/wrela-v2/tests/contract/mir_pipeline_contract_test.wr"
BACKEND_CONTRACT="$ROOT/wrela-v2/tests/contract/backend_object_contract_test.wr"
LINKER_CONTRACT="$ROOT/wrela-v2/tests/contract/linker_executable_contract_test.wr"
PARSER="$ROOT/wrela-v2/src/domain/frontend/parser.wr"
TYPECHECK="$ROOT/wrela-v2/src/domain/frontend/typecheck.wr"
TOKEN="$ROOT/wrela-v2/src/domain/frontend/token.wr"
MIR_IR="$ROOT/wrela-v2/src/domain/mir/ir.wr"
MIR_LOWER="$ROOT/wrela-v2/src/domain/mir/lower.wr"
MIR_VALIDATE="$ROOT/wrela-v2/src/domain/mir/validate.wr"
MIR_REWRITE="$ROOT/wrela-v2/src/domain/mir/rewrite.wr"
BACKEND_OBJECT_IR="$ROOT/wrela-v2/src/domain/backend/object_ir.wr"
BACKEND_EMIT_OBJECT="$ROOT/wrela-v2/src/domain/backend/emit_object.wr"
BACKEND_OBJECT_VALIDATE="$ROOT/wrela-v2/src/domain/backend/object_validate.wr"
BACKEND_OBJECT_SERIALIZE="$ROOT/wrela-v2/src/domain/backend/object_serialize.wr"
LINKER_INPUT_CONTRACT="$ROOT/wrela-v2/src/domain/linker/input_contract.wr"
LINKER_IR="$ROOT/wrela-v2/src/domain/linker/link_ir.wr"
LINKER_RESOLVE_SYMBOLS="$ROOT/wrela-v2/src/domain/linker/resolve_symbols.wr"
LINKER_APPLY_RELOCATIONS="$ROOT/wrela-v2/src/domain/linker/apply_relocations.wr"
LINKER_EMIT_EXECUTABLE="$ROOT/wrela-v2/src/domain/linker/emit_executable.wr"
LINKER_VALIDATE_EXECUTABLE="$ROOT/wrela-v2/src/domain/linker/validate_executable.wr"
PLATFORM_CONTRACTS="$ROOT/wrela-v2/src/domain/platform/contracts.wr"
PLATFORM_PORTS="$ROOT/wrela-v2/src/application/composition/platform_ports.wr"
PLATFORM_RUNTIME="$ROOT/wrela-v2/src/application/runtime/platform_runtime.wr"
DARWIN_ADAPTER="$ROOT/wrela-v2/src/infrastructure/platform/adapters/darwin_kqueue_adapter.wr"
LINUX_IO_URING_ADAPTER="$ROOT/wrela-v2/src/infrastructure/platform/adapters/linux_io_uring_adapter.wr"
LINUX_EPOLL_ADAPTER="$ROOT/wrela-v2/src/infrastructure/platform/adapters/linux_epoll_adapter.wr"
PLATFORM_CONTRACT_TEST="$ROOT/wrela-v2/tests/contract/platform_adapter_contract_test.wr"
LINUX_READINESS_PARITY_TEST="$ROOT/wrela-v2/tests/parity/linux_readiness_architecture_test.wr"
MIR_FIXTURE_DIR="$ROOT/wrela-v2/fixtures/mir"
LINKER_MULTI_OBJECT_FIXTURE_DIR="$ROOT/wrela-v2/fixtures/linker_multi_object"

if [[ ! -f "$PIPELINE" ]]; then
  echo "missing v2 check pipeline module: $PIPELINE" >&2
  exit 1
fi

if [[ ! -f "$COMMAND" ]]; then
  echo "missing v2 check command module: $COMMAND" >&2
  exit 1
fi

if [[ ! -f "$CONTRACT" ]]; then
  echo "missing v2 check pipeline contract: $CONTRACT" >&2
  exit 1
fi

if [[ ! -f "$MIR_CONTRACT" ]]; then
  echo "missing v2 mir pipeline contract: $MIR_CONTRACT" >&2
  exit 1
fi

if [[ ! -f "$BACKEND_CONTRACT" ]]; then
  echo "missing v2 backend object contract: $BACKEND_CONTRACT" >&2
  exit 1
fi

if [[ ! -f "$LINKER_CONTRACT" ]]; then
  echo "missing v2 linker executable contract: $LINKER_CONTRACT" >&2
  exit 1
fi

if [[ ! -f "$PARSER" ]]; then
  echo "missing v2 parser module: $PARSER" >&2
  exit 1
fi

if [[ ! -f "$TYPECHECK" ]]; then
  echo "missing v2 typecheck module: $TYPECHECK" >&2
  exit 1
fi

if [[ ! -f "$TOKEN" ]]; then
  echo "missing v2 token module: $TOKEN" >&2
  exit 1
fi

if [[ ! -f "$MIR_IR" || ! -f "$MIR_LOWER" || ! -f "$MIR_VALIDATE" || ! -f "$MIR_REWRITE" ]]; then
  echo "missing v2 mir modules (ir/lower/validate/rewrite)" >&2
  exit 1
fi

if [[ ! -f "$BACKEND_OBJECT_IR" || ! -f "$BACKEND_EMIT_OBJECT" || ! -f "$BACKEND_OBJECT_VALIDATE" || ! -f "$BACKEND_OBJECT_SERIALIZE" ]]; then
  echo "missing v2 backend modules (object_ir/emit_object/object_validate/object_serialize)" >&2
  exit 1
fi

if [[ ! -f "$LINKER_INPUT_CONTRACT" ]]; then
  echo "missing v2 linker input contract module: $LINKER_INPUT_CONTRACT" >&2
  exit 1
fi

if [[ ! -f "$LINKER_IR" || ! -f "$LINKER_RESOLVE_SYMBOLS" || ! -f "$LINKER_APPLY_RELOCATIONS" || ! -f "$LINKER_EMIT_EXECUTABLE" || ! -f "$LINKER_VALIDATE_EXECUTABLE" ]]; then
  echo "missing v2 linker modules (link_ir/resolve_symbols/apply_relocations/emit_executable/validate_executable)" >&2
  exit 1
fi

if [[ ! -f "$PLATFORM_CONTRACTS" || ! -f "$PLATFORM_PORTS" || ! -f "$PLATFORM_RUNTIME" || ! -f "$DARWIN_ADAPTER" || ! -f "$LINUX_IO_URING_ADAPTER" || ! -f "$LINUX_EPOLL_ADAPTER" ]]; then
  echo "missing v2 platform modules (contracts/composition/runtime/adapters)" >&2
  exit 1
fi

if [[ ! -f "$PLATFORM_CONTRACT_TEST" || ! -f "$LINUX_READINESS_PARITY_TEST" ]]; then
  echo "missing v2 platform adapter contract/parity tests" >&2
  exit 1
fi

if [[ ! -d "$MIR_FIXTURE_DIR" ]]; then
  echo "missing v2 mir fixture directory: $MIR_FIXTURE_DIR" >&2
  exit 1
fi

if [[ ! -d "$LINKER_MULTI_OBJECT_FIXTURE_DIR" ]]; then
  echo "missing v2 linker multi-object fixture directory: $LINKER_MULTI_OBJECT_FIXTURE_DIR" >&2
  exit 1
fi

required_pipeline_symbols=(
  "resolve_check_input_path"
  "load_source_stage"
  "make_error_report"
  "make_ok_report"
  "run_check_pipeline"
)
for symbol in "${required_pipeline_symbols[@]}"; do
  if ! rg -n "$symbol" "$PIPELINE" >/dev/null; then
    echo "v2 check pipeline bootstrap violation: missing '$symbol' in $PIPELINE" >&2
    exit 1
  fi
done

if ! rg -n "use get_source_tokens from domain/frontend/lexer|use parse_tokens from domain/frontend/parser|use check_program from domain/frontend/typecheck" "$PIPELINE" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: native frontend stage imports missing in $PIPELINE" >&2
  exit 1
fi

if ! rg -n "use try_to_lower_program_to_mir from domain/mir/lower|use validate_mir_program from domain/mir/validate|use create_rewritten_mir_program from domain/mir/rewrite" "$PIPELINE" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: mir stage imports missing in $PIPELINE" >&2
  exit 1
fi

if ! rg -n "create_mir_program|create_mir_function|create_mir_block|create_mir_binary_instruction|create_mir_return_instruction" "$MIR_IR" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: mir IR constructors missing in $MIR_IR" >&2
  exit 1
fi

if ! rg -n "create_object_program_maps|create_object_section_maps|create_object_symbol_maps|get_object_signature_text" "$BACKEND_OBJECT_IR" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: backend object IR constructors missing in $BACKEND_OBJECT_IR" >&2
  exit 1
fi

if ! rg -n "emit_object_from_mir_program|create_backend_emit_result_maps" "$BACKEND_EMIT_OBJECT" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: backend emitter symbols missing in $BACKEND_EMIT_OBJECT" >&2
  exit 1
fi

if ! rg -n "validate_object_program|create_object_validate_result_maps" "$BACKEND_OBJECT_VALIDATE" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: backend validator symbols missing in $BACKEND_OBJECT_VALIDATE" >&2
  exit 1
fi

if ! rg -n "create_serialized_object_program_maps|get_serialized_object_byte_lists|get_serialized_object_text" "$BACKEND_OBJECT_SERIALIZE" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: backend serializer symbols missing in $BACKEND_OBJECT_SERIALIZE" >&2
  exit 1
fi

if ! rg -n "create_linker_input_maps|create_linker_input_from_object_program|create_linker_input_from_object_program_lists|get_linker_input_entry_symbol_text" "$LINKER_INPUT_CONTRACT" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: linker input contract symbols missing in $LINKER_INPUT_CONTRACT" >&2
  exit 1
fi

if ! rg -n "create_linker_image_maps|get_linker_image_signature_text|get_linker_image_byte_lists|get_linker_image_entry_offset_number" "$LINKER_IR" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: linker IR symbols missing in $LINKER_IR" >&2
  exit 1
fi

if ! rg -n "try_to_resolve_linker_symbols_from_input|create_linker_symbol_resolution_result_maps" "$LINKER_RESOLVE_SYMBOLS" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: linker symbol resolution symbols missing in $LINKER_RESOLVE_SYMBOLS" >&2
  exit 1
fi

if ! rg -n "try_to_apply_linker_relocations|create_linker_relocation_apply_result_maps" "$LINKER_APPLY_RELOCATIONS" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: linker relocation symbols missing in $LINKER_APPLY_RELOCATIONS" >&2
  exit 1
fi

if ! rg -n "try_to_emit_linker_executable_from_input|create_linker_executable_emit_result_maps" "$LINKER_EMIT_EXECUTABLE" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: linker emit symbols missing in $LINKER_EMIT_EXECUTABLE" >&2
  exit 1
fi

if ! rg -n "adapter_identity_name|host_family_name" "$PLATFORM_CONTRACTS" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: platform contract identity fields missing in $PLATFORM_CONTRACTS" >&2
  exit 1
fi

if ! rg -n "ops: Map|get_platform_operation_value_or_empty_text|get_platform_operation_presence_flag" "$PLATFORM_CONTRACTS" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: platform operation contract fields/helpers missing in $PLATFORM_CONTRACTS" >&2
  exit 1
fi

if ! rg -n "get_required_platform_operation_text|run_reactor_port_lifecycle|run_clock_port_now_ns|run_clock_port_sleep_ms|try_to_run_fs_write_text|try_to_run_fs_read_text|try_to_run_fs_read_metadata|try_to_run_process_port|try_to_get_process_port_cwd" "$PLATFORM_RUNTIME" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: platform runtime facade symbols missing in $PLATFORM_RUNTIME" >&2
  exit 1
fi

if ! rg -n "from infrastructure/platform/adapters/darwin_kqueue_adapter|from infrastructure/platform/adapters/linux_io_uring_adapter|from infrastructure/platform/adapters/linux_epoll_adapter" "$PLATFORM_PORTS" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: platform composition must import concrete adapters in $PLATFORM_PORTS" >&2
  exit 1
fi

if rg -n "reactor_port=ReactorPort\(\)|clock_port=ClockPort\(\)|fs_port=FsPort\(\)|process_port=ProcessPort\(\)" "$PLATFORM_PORTS" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: platform composition uses placeholder-only raw port constructors in $PLATFORM_PORTS" >&2
  exit 1
fi

if ! rg -n "get_darwin_kqueue_reactor_port|get_darwin_clock_port|get_darwin_fs_port|get_darwin_process_port|get_darwin_kqueue_reactor_ops|get_darwin_clock_ops|get_darwin_fs_ops|get_darwin_process_ops|create_reactor_handle|register_reactor_token|try_fs_read_text|try_process_run" "$DARWIN_ADAPTER" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: darwin adapter mapping symbols missing in $DARWIN_ADAPTER" >&2
  exit 1
fi

if ! rg -n "get_linux_io_uring_reactor_port|get_linux_clock_port|get_linux_fs_port|get_linux_process_port|get_linux_io_uring_reactor_ops|get_linux_clock_ops|get_linux_fs_ops|get_linux_process_ops|create_reactor_handle|register_reactor_token|try_fs_read_text|try_process_run" "$LINUX_IO_URING_ADAPTER" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: linux io_uring adapter mapping symbols missing in $LINUX_IO_URING_ADAPTER" >&2
  exit 1
fi

if ! rg -n "get_linux_epoll_reactor_port|get_linux_epoll_clock_port|get_linux_epoll_fs_port|get_linux_epoll_process_port|get_linux_epoll_reactor_ops|get_linux_epoll_clock_ops|get_linux_epoll_fs_ops|get_linux_epoll_process_ops|create_reactor_handle|register_reactor_token|try_fs_read_text|try_process_run" "$LINUX_EPOLL_ADAPTER" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: linux epoll adapter mapping symbols missing in $LINUX_EPOLL_ADAPTER" >&2
  exit 1
fi

if ! rg -n "validate_linker_executable_image|create_linker_executable_validate_result_maps" "$LINKER_VALIDATE_EXECUTABLE" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: linker validate symbols missing in $LINKER_VALIDATE_EXECUTABLE" >&2
  exit 1
fi

if ! rg -n "try_to_lower_program_to_mir|try_to_lower_expression_node_to_mir" "$MIR_LOWER" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: mir lowering symbols missing in $MIR_LOWER" >&2
  exit 1
fi

if ! rg -n "validate_mir_program" "$MIR_VALIDATE" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: mir validator missing in $MIR_VALIDATE" >&2
  exit 1
fi

if ! rg -n "create_rewritten_mir_program|create_rewritten_mir_instruction" "$MIR_REWRITE" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: mir rewrite symbols missing in $MIR_REWRITE" >&2
  exit 1
fi

if ! rg -n "create_program_node|create_integer_literal_node|create_boolean_literal_node|create_binary_expression_node" "$PARSER" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: parser must build structured AST nodes in $PARSER" >&2
  exit 1
fi

if ! rg -n "get_program_return_expression_node|get_binary_left_node|get_binary_right_node|get_ast_binary_operator_text" "$TYPECHECK" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: typechecker must traverse AST accessors in $TYPECHECK" >&2
  exit 1
fi

if ! rg -n "create_frontend_token|get_frontend_token_kind_text|get_frontend_token_at_index" "$TOKEN" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: token payload helpers missing in $TOKEN" >&2
  exit 1
fi

if ! rg -n "run_check_pipeline|try_to_run_shadow_v1_check" "$COMMAND" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: required command wiring missing in $COMMAND" >&2
  exit 1
fi

if ! rg -n "test_check_pipeline_requires_input_path|test_check_pipeline_lex_rejects_whitespace_only_source|test_check_pipeline_lex_rejects_tab_indentation|test_check_pipeline_type_rejects_integer_plus_boolean_literal|test_check_pipeline_parse_rejects_trailing_operator|test_check_pipeline_parse_rejects_unexpected_program_shape|test_check_pipeline_reports_ok_for_simple_program|test_check_pipeline_reports_ok_for_parenthesized_program|test_check_pipeline_resolves_directory_to_src_main" "$CONTRACT" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: required contract scenarios missing in $CONTRACT" >&2
  exit 1
fi

if ! rg -n "E-LEX-TAB|E-LEX-EMPTY|E-PARSE-UNEXPECTED_TOKEN|E-PARSE-TRAILING_OPERATOR|E-TYPE-ARITH_NON_INTEGER" "$CONTRACT" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: diagnostic code coverage assertions missing in $CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_mir_lowering_accepts_precedence_program|test_mir_lowering_accepts_parenthesized_program|test_mir_validator_accepts_lowered_subset_program|test_mir_rewrite_is_deterministic_and_idempotent" "$MIR_CONTRACT" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: required mir contract scenarios missing in $MIR_CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_mir_corpus_50_case_parity_locked|test_mir_corpus_50_case_rewrite_idempotent" "$MIR_CONTRACT" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: mir corpus parity hooks missing in $MIR_CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_backend_object_emission_accepts_precedence_program|test_backend_object_emission_accepts_parenthesized_program|test_backend_object_serializer_is_byte_identical_across_repeated_runs|test_backend_object_validator_rejects_corrupt_fixture|test_backend_linker_input_contract_handshake_is_stable" "$BACKEND_CONTRACT" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: backend object contract scenarios missing in $BACKEND_CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_linker_executable_links_precedence_program|test_linker_executable_links_parenthesized_program|test_linker_executable_links_multi_object_programs|test_linker_executable_is_deterministic_for_repeated_link|test_linker_executable_is_deterministic_for_repeated_multi_object_link|test_linker_executable_fails_when_entry_symbol_missing|test_linker_executable_fails_for_duplicate_symbol_name_multi_object|test_linker_executable_fails_closed_for_unknown_relocation_kind|test_linker_executable_fails_for_unresolved_symbol_multi_object|test_linker_executable_validator_rejects_corrupt_fixture" "$LINKER_CONTRACT" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: linker executable contract scenarios missing in $LINKER_CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_platform_adapter_contract_linux_io_uring_path|test_platform_adapter_contract_linux_epoll_fallback_path|test_platform_adapter_contract_darwin_path|test_platform_adapter_contract_runtime_facade_executes_representative_ops|test_platform_adapter_contract_runtime_facade_fails_closed_on_missing_operation|adapter_identity_name" "$PLATFORM_CONTRACT_TEST" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: platform adapter contract scenarios/identity assertions missing in $PLATFORM_CONTRACT_TEST" >&2
  exit 1
fi

if ! rg -n "test_linux_uses_io_uring_when_preferred|test_linux_uses_epoll_when_io_uring_not_preferred|test_darwin_uses_kqueue_for_host_development|adapter_identity_name" "$LINUX_READINESS_PARITY_TEST" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: linux readiness parity scenarios/identity assertions missing in $LINUX_READINESS_PARITY_TEST" >&2
  exit 1
fi

wr_case_count="$(find "$MIR_FIXTURE_DIR" -maxdepth 1 -type f -name 'case_*.wr' | wc -l | tr -d ' ')"
expected_case_count="$(find "$MIR_FIXTURE_DIR" -maxdepth 1 -type f -name 'case_*.expected.txt' | wc -l | tr -d ' ')"
if [[ "$wr_case_count" != "50" || "$expected_case_count" != "50" ]]; then
  echo "v2 check pipeline bootstrap violation: expected 50 mir corpus source files and 50 expectation files in $MIR_FIXTURE_DIR" >&2
  echo "  found source=$wr_case_count expected=$expected_case_count" >&2
  exit 1
fi

linker_case_count="$(find "$LINKER_MULTI_OBJECT_FIXTURE_DIR" -maxdepth 1 -type f -name 'case_*.txt' | wc -l | tr -d ' ')"
if [[ "$linker_case_count" != "3" ]]; then
  echo "v2 check pipeline bootstrap violation: expected 3 linker multi-object fixture files in $LINKER_MULTI_OBJECT_FIXTURE_DIR" >&2
  echo "  found fixtures=$linker_case_count" >&2
  exit 1
fi

if rg -n "expression_bytes|get_frontend_parser_starts_with_header_flag|get_frontend_boolean_literal_flag" "$PARSER" "$TYPECHECK" "$PIPELINE" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: brittle byte-pattern frontend helpers are still present" >&2
  exit 1
fi

if rg -n "fake_object_success|skip_backend_emit|mock_emit_success" "$BACKEND_EMIT_OBJECT" "$BACKEND_OBJECT_VALIDATE" "$BACKEND_OBJECT_SERIALIZE" "$BACKEND_CONTRACT" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: backend object bypass/fake-success patterns detected" >&2
  exit 1
fi

if rg -n "fake_link_success|skip_linker|skip_multi_object|first_object_only|ignore_symbol_collision|external_linker|delegate_to_ld|delegate_to_clang" "$LINKER_IR" "$LINKER_RESOLVE_SYMBOLS" "$LINKER_APPLY_RELOCATIONS" "$LINKER_EMIT_EXECUTABLE" "$LINKER_VALIDATE_EXECUTABLE" "$LINKER_CONTRACT" >/dev/null; then
  echo "v2 check pipeline bootstrap violation: linker bypass/delegation patterns detected" >&2
  exit 1
fi

if rg -n "from infrastructure/platform/adapters/" "$ROOT/wrela-v2/src/domain" "$ROOT/wrela-v2/src/application" --glob '!**/composition/platform_ports.wr' --glob '!**/runtime/platform_runtime.wr' >/dev/null; then
  echo "v2 check pipeline bootstrap violation: platform adapter imports leaked into core/app layers outside composition/platform_ports.wr and application/runtime/platform_runtime.wr" >&2
  exit 1
fi

if rg -n "from host/|from runtime/reactor" "$ROOT/wrela-v2/src/domain" "$ROOT/wrela-v2/src/application" --glob '!**/composition/platform_ports.wr' --glob '!**/runtime/platform_runtime.wr' >/dev/null; then
  echo "v2 check pipeline bootstrap violation: host/runtime primitive wrapper imports leaked into core/app layers outside approved runtime facade/composition boundaries" >&2
  exit 1
fi

echo "v2 check pipeline bootstrap check passed"
