#!/usr/bin/env bash
set -euo pipefail

mkdir -p "${HF_HOME:-/data/hf}"
mkdir -p "${HUGGINGFACE_HUB_CACHE:-/data/hf/hub}"
mkdir -p "${TORCH_HOME:-/data/torch}"
mkdir -p "${HY3D_CACHE_PATH:-/data/gradio_cache}"

read -r -a EXTRA_ARGS <<< "${HY3D_EXTRA_ARGS:-}"

exec python /opt/Hunyuan3D-2.1/api_server.py \
  --host 0.0.0.0 \
  --port "${PORT:-8081}" \
  --device cuda \
  --model_path "${HY3D_MODEL_PATH:-tencent/Hunyuan3D-2.1}" \
  --cache-path "${HY3D_CACHE_PATH:-/data/gradio_cache}" \
  "${EXTRA_ARGS[@]}"
