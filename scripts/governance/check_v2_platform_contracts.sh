#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONTRACT="$ROOT/wrela-v2/tests/contract/platform_adapter_contract_test.wr"
ARCH="$ROOT/wrela-v2/tests/parity/linux_readiness_architecture_test.wr"
RUNTIME_CONTRACT="$ROOT/wrela-v2/tests/contract/platform_runtime_contract_test.wr"
RUNTIME_EVENT_LOOP_CONTRACT="$ROOT/wrela-v2/tests/contract/runtime_event_loop_contract_test.wr"
RUNTIME_CONCURRENCY_CONTRACT="$ROOT/wrela-v2/tests/contract/runtime_concurrency_contract_test.wr"
RUNTIME_SCHEDULER_CONTRACT="$ROOT/wrela-v2/tests/contract/runtime_scheduler_contract_test.wr"
RUNTIME_CANCELLATION_CONTRACT="$ROOT/wrela-v2/tests/contract/runtime_cancellation_contract_test.wr"
RUNTIME_DETERMINISM_CONTRACT="$ROOT/wrela-v2/tests/contract/runtime_determinism_contract_test.wr"
CONFORMANCE_MODULE="$ROOT/wrela-v2/src/application/runtime/platform_conformance.wr"
LINUX_BOUNDARY_MODULE="$ROOT/wrela-v2/src/domain/platform/linux_sys_boundary.wr"
EVENT_LOOP_RUNTIME_MODULE="$ROOT/wrela-v2/src/application/runtime/event_loop_runtime.wr"
CONCURRENCY_RUNTIME_MODULE="$ROOT/wrela-v2/src/application/runtime/concurrency_runtime.wr"
SCHEDULER_RUNTIME_MODULE="$ROOT/wrela-v2/src/application/runtime/scheduler_runtime.wr"
CANCELLATION_RUNTIME_MODULE="$ROOT/wrela-v2/src/application/runtime/cancellation_runtime.wr"
RUNTIME_FIXTURES_DIR="$ROOT/wrela-v2/tests/fixtures/runtime"

if [[ ! -f "$CONTRACT" ]]; then
  echo "missing platform adapter contract file: $CONTRACT" >&2
  exit 1
fi

if [[ ! -f "$ARCH" ]]; then
  echo "missing linux-readiness architecture file: $ARCH" >&2
  exit 1
fi

if [[ ! -f "$RUNTIME_CONTRACT" ]]; then
  echo "missing platform runtime contract file: $RUNTIME_CONTRACT" >&2
  exit 1
fi

if [[ ! -f "$RUNTIME_EVENT_LOOP_CONTRACT" ]]; then
  echo "missing runtime event loop contract file: $RUNTIME_EVENT_LOOP_CONTRACT" >&2
  exit 1
fi

if [[ ! -f "$RUNTIME_CONCURRENCY_CONTRACT" ]]; then
  echo "missing runtime concurrency contract file: $RUNTIME_CONCURRENCY_CONTRACT" >&2
  exit 1
fi

if [[ ! -f "$RUNTIME_SCHEDULER_CONTRACT" ]]; then
  echo "missing runtime scheduler contract file: $RUNTIME_SCHEDULER_CONTRACT" >&2
  exit 1
fi

if [[ ! -f "$RUNTIME_CANCELLATION_CONTRACT" ]]; then
  echo "missing runtime cancellation contract file: $RUNTIME_CANCELLATION_CONTRACT" >&2
  exit 1
fi

if [[ ! -f "$RUNTIME_DETERMINISM_CONTRACT" ]]; then
  echo "missing runtime determinism contract file: $RUNTIME_DETERMINISM_CONTRACT" >&2
  exit 1
fi

if [[ ! -f "$CONFORMANCE_MODULE" ]]; then
  echo "missing platform conformance module: $CONFORMANCE_MODULE" >&2
  exit 1
fi

if [[ ! -f "$LINUX_BOUNDARY_MODULE" ]]; then
  echo "missing linux syscall boundary module: $LINUX_BOUNDARY_MODULE" >&2
  exit 1
fi

if [[ ! -f "$EVENT_LOOP_RUNTIME_MODULE" ]]; then
  echo "missing event loop runtime module: $EVENT_LOOP_RUNTIME_MODULE" >&2
  exit 1
fi

if [[ ! -f "$CONCURRENCY_RUNTIME_MODULE" ]]; then
  echo "missing concurrency runtime module: $CONCURRENCY_RUNTIME_MODULE" >&2
  exit 1
fi

if [[ ! -f "$SCHEDULER_RUNTIME_MODULE" ]]; then
  echo "missing scheduler runtime module: $SCHEDULER_RUNTIME_MODULE" >&2
  exit 1
fi

if [[ ! -f "$CANCELLATION_RUNTIME_MODULE" ]]; then
  echo "missing cancellation runtime module: $CANCELLATION_RUNTIME_MODULE" >&2
  exit 1
fi

if [[ ! -d "$RUNTIME_FIXTURES_DIR" ]]; then
  echo "missing runtime fixtures dir: $RUNTIME_FIXTURES_DIR" >&2
  exit 1
fi

if ! rg -n "test_platform_adapter_contract_linux_io_uring_path|test_platform_adapter_contract_linux_epoll_fallback_path|test_platform_adapter_contract_darwin_path|test_platform_adapter_contract_rejects_unknown_platform|test_platform_adapter_contract_linux_sys_boundary_model_is_present" "$CONTRACT" >/dev/null; then
  echo "missing required platform adapter contract scenarios in $CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_linux_uses_io_uring_when_preferred|test_linux_uses_epoll_when_io_uring_not_preferred|test_darwin_uses_kqueue_for_host_development|test_unsupported_platform_is_rejected|test_normalize_os_name_supports_ostype_prefixes|test_linux_syscall_boundary_groups_exist" "$ARCH" >/dev/null; then
  echo "missing required linux-readiness architecture scenarios in $ARCH" >&2
  exit 1
fi

if ! rg -n "test_platform_runtime_contract_linux_io_uring_conformance|test_platform_runtime_contract_linux_epoll_conformance|test_platform_runtime_contract_darwin_conformance|test_platform_runtime_contract_rejects_unknown_target|test_platform_runtime_contract_missing_operation_fails_closed" "$RUNTIME_CONTRACT" >/dev/null; then
  echo "missing required runtime conformance scenarios in $RUNTIME_CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_platform_runtime_contract_missing_thread_operation_fails_closed" "$RUNTIME_CONTRACT" >/dev/null; then
  echo "missing required thread-op conformance scenario in $RUNTIME_CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_runtime_event_loop_contract_linux_io_uring_probe_succeeds|test_runtime_event_loop_contract_linux_epoll_probe_succeeds|test_runtime_event_loop_contract_darwin_probe_succeeds|test_runtime_event_loop_contract_bounded_loop_succeeds|test_runtime_event_loop_contract_rejects_invalid_loop_inputs|test_runtime_event_loop_contract_missing_operation_fails_closed|test_runtime_event_loop_contract_unsupported_adapter_identity_fails_closed|test_runtime_event_loop_contract_repeated_probe_structure_is_stable" "$RUNTIME_EVENT_LOOP_CONTRACT" >/dev/null; then
  echo "missing required event-loop runtime scenarios in $RUNTIME_EVENT_LOOP_CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_runtime_concurrency_contract_linux_io_uring_thread_and_wait_lifecycle|test_runtime_concurrency_contract_linux_epoll_thread_and_wait_lifecycle|test_runtime_concurrency_contract_darwin_thread_and_wait_lifecycle|test_runtime_concurrency_contract_missing_thread_op_fails_closed|test_runtime_concurrency_contract_unsupported_thread_adapter_fails_closed" "$RUNTIME_CONCURRENCY_CONTRACT" >/dev/null; then
  echo "missing required concurrency runtime scenarios in $RUNTIME_CONCURRENCY_CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_runtime_scheduler_contract_fifo_and_bounded_cycles|test_runtime_scheduler_contract_rejects_invalid_max_cycles|test_runtime_scheduler_contract_fails_closed_on_starvation" "$RUNTIME_SCHEDULER_CONTRACT" >/dev/null; then
  echo "missing required scheduler runtime scenarios in $RUNTIME_SCHEDULER_CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_runtime_cancellation_contract_token_state_transitions|test_runtime_cancellation_contract_shutdown_order_is_deterministic|test_runtime_cancellation_contract_cancellable_event_loop_honors_cancel_token" "$RUNTIME_CANCELLATION_CONTRACT" >/dev/null; then
  echo "missing required cancellation runtime scenarios in $RUNTIME_CANCELLATION_CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_runtime_determinism_contract_scheduler_summary_is_stable|test_runtime_determinism_contract_event_loop_summary_is_stable|test_runtime_determinism_contract_probe_structure_is_stable" "$RUNTIME_DETERMINISM_CONTRACT" >/dev/null; then
  echo "missing required runtime determinism scenarios in $RUNTIME_DETERMINISM_CONTRACT" >&2
  exit 1
fi

if ! rg -n "try_to_validate_platform_ports_conformance|try_to_create_and_validate_platform_ports" "$CONFORMANCE_MODULE" >/dev/null; then
  echo "missing required conformance symbols in $CONFORMANCE_MODULE" >&2
  exit 1
fi

if ! rg -n "fd_path|memory|time|process|sync|reactor|try_to_validate_linux_sys_boundary_map" "$LINUX_BOUNDARY_MODULE" >/dev/null; then
  echo "missing required linux syscall boundary symbols in $LINUX_BOUNDARY_MODULE" >&2
  exit 1
fi

if ! rg -n "domain/platform/linux_sys_boundary|try_to_validate_platform_ports_conformance" "$CONTRACT" >/dev/null; then
  echo "missing required linux-boundary/conformance references in $CONTRACT" >&2
  exit 1
fi

if ! rg -n "try_to_run_bounded_event_loop|try_to_run_runtime_probe_sequence" "$EVENT_LOOP_RUNTIME_MODULE" >/dev/null; then
  echo "missing required event-loop runtime symbols in $EVENT_LOOP_RUNTIME_MODULE" >&2
  exit 1
fi

if ! rg -n "try_to_run_thread_lifecycle|try_to_run_wait_lifecycle" "$CONCURRENCY_RUNTIME_MODULE" >/dev/null; then
  echo "missing required concurrency runtime symbols in $CONCURRENCY_RUNTIME_MODULE" >&2
  exit 1
fi

if ! rg -n "try_to_run_scheduler_queue|scheduler starvation: max_cycles exceeded" "$SCHEDULER_RUNTIME_MODULE" >/dev/null; then
  echo "missing required scheduler runtime symbols in $SCHEDULER_RUNTIME_MODULE" >&2
  exit 1
fi

if ! rg -n "create_cancellation_token|mark_cancellation_token_cancelled|try_to_shutdown_runtime" "$CANCELLATION_RUNTIME_MODULE" >/dev/null; then
  echo "missing required cancellation runtime symbols in $CANCELLATION_RUNTIME_MODULE" >&2
  exit 1
fi

if ! rg -n "seed" "$RUNTIME_FIXTURES_DIR" >/dev/null; then
  echo "missing runtime fixture seed in $RUNTIME_FIXTURES_DIR" >&2
  exit 1
fi

echo "v2 platform contracts check passed"
