#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: $0 <instance-name> [zone]" >&2
  exit 1
fi

INSTANCE="$1"
ZONE="${2:-${GCP_ZONE:-us-central1-a}}"
PROJECT="${GCP_PROJECT:-$(gcloud config get-value project 2>/dev/null)}"

if [[ -z "${PROJECT}" ]]; then
  echo "Missing project. Set GCP_PROJECT or run: gcloud config set project <id>" >&2
  exit 1
fi

gcloud compute instances start "${INSTANCE}" --project "${PROJECT}" --zone "${ZONE}"
