#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ZONE="${GCP_ZONE:-us-central1-a}"
X86_INSTANCE="${X86_INSTANCE:-wrela-perf-x86-4c}"
ARM_INSTANCE="${ARM_INSTANCE:-wrela-perf-arm-4c}"

"${ROOT}/scripts/perf/gcp_sync_branch_and_run.sh" "${X86_INSTANCE}" "${ZONE}"
"${ROOT}/scripts/perf/gcp_sync_branch_and_run.sh" "${ARM_INSTANCE}" "${ZONE}"
