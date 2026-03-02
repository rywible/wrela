#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 2 ]]; then
  echo "usage: $0 <perf_metrics_json> <report_path>" >&2
  exit 2
fi

METRICS_PATH="$1"
REPORT_PATH="$2"
SUMMARY_PATH="$(dirname "$REPORT_PATH")/summary.md"

mkdir -p "$(dirname "$REPORT_PATH")"

if [[ ! -f "$METRICS_PATH" ]]; then
  echo "perf metrics file not found: ${METRICS_PATH}" >&2
  exit 1
fi

analysis_json="$(node - "$METRICS_PATH" <<'NODE'
const fs = require("node:fs");

const [metricsPath] = process.argv.slice(2);

const fail = (message) => {
  console.error(message);
  process.exit(1);
};

let metrics;
try {
  metrics = JSON.parse(fs.readFileSync(metricsPath, "utf8"));
} catch (error) {
  fail(`invalid perf metrics json ${metricsPath}: ${error instanceof Error ? error.message : String(error)}`);
}

const readThreshold = (name, fallback) => {
  const raw = process.env[name];
  if (typeof raw !== "string" || raw.trim().length === 0) {
    return fallback;
  }
  const parsed = Number(raw);
  return Number.isFinite(parsed) ? parsed : fallback;
};

const readAtPath = (value, dottedPath) => {
  return dottedPath.split(".").reduce((acc, key) => {
    if (acc === null || typeof acc !== "object" || !Object.prototype.hasOwnProperty.call(acc, key)) {
      return undefined;
    }
    return acc[key];
  }, value);
};

const normalizeNumber = (value) => {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string" && value.trim().length > 0) {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return Number.NaN;
};

const readNumberByPaths = (paths) => {
  for (const path of paths) {
    const value = readAtPath(metrics, path);
    const numeric = normalizeNumber(value);
    if (Number.isFinite(numeric)) {
      return { value: numeric, source_path: path };
    }
  }
  return { value: Number.NaN, source_path: null };
};

const requiredProfile = {
  required_actor_count: readThreshold("WRELA_PERF_REQUIRED_ACTOR_COUNT", 32),
  min_fps: readThreshold("WRELA_PERF_MIN_FPS", 60),
};

const thresholds = {
  max_p95_frame_ms: readThreshold("WRELA_PERF_MAX_P95_FRAME_MS", 16.7),
  max_animation_eval_ms: readThreshold("WRELA_PERF_MAX_ANIM_EVAL_MS", 2.5),
  max_skinning_upload_ms: readThreshold("WRELA_PERF_MAX_SKINNING_UPLOAD_MS", 1.5),
};

const actorCountReading = readNumberByPaths([
  "actor_count",
  "actors",
  "active_actor_count",
  "scene.actor_count",
  "scene.actors",
  "workload.actor_count",
  "workload.actors",
  "profile.actor_count",
  "metrics.actor_count",
]);
const fpsReading = readNumberByPaths([
  "fps",
  "avg_fps",
  "average_fps",
  "mean_fps",
  "frame_rate_fps",
  "frame_rate",
  "render_fps",
  "metrics.fps",
  "metrics.avg_fps",
  "summary.fps",
]);
const p95FrameReading = readNumberByPaths([
  "p95_frame_ms",
  "frame_p95_ms",
  "frame_ms_p95",
  "metrics.p95_frame_ms",
  "metrics.frame_p95_ms",
  "timings.p95_frame_ms",
  "timings.frame_p95_ms",
  "frame.p95_ms",
  "render.p95_frame_ms",
  "render.frame_p95_ms",
]);
const animationEvalReading = readNumberByPaths([
  "animation_eval_ms",
  "anim_eval_ms",
  "animation_evaluation_ms",
  "metrics.animation_eval_ms",
  "metrics.anim_eval_ms",
  "timings.animation_eval_ms",
  "timings.anim_eval_ms",
  "animation.eval_ms",
]);
const skinningUploadReading = readNumberByPaths([
  "skinning_upload_ms",
  "pose_upload_ms",
  "skinning_pose_upload_ms",
  "metrics.skinning_upload_ms",
  "metrics.pose_upload_ms",
  "timings.skinning_upload_ms",
  "timings.pose_upload_ms",
  "skinning.upload_ms",
]);

let effectiveFpsValue = fpsReading.value;
let effectiveFpsSource = fpsReading.source_path;
if (!Number.isFinite(effectiveFpsValue) && Number.isFinite(p95FrameReading.value) && p95FrameReading.value > 0) {
  effectiveFpsValue = 1000 / p95FrameReading.value;
  effectiveFpsSource = "derived_from_p95_frame_ms";
}

const makeFailure = (metric, reason, observed, threshold, sourcePath) => ({
  metric,
  reason,
  observed: Number.isFinite(observed) ? Number(Number(observed).toFixed(6)) : null,
  threshold,
  source_path: sourcePath,
});

const attribution = [];
const failures = [];

const evaluateEqual = (metric, observed, expected, sourcePath, missingReason, mismatchReason) => {
  if (!Number.isFinite(observed)) {
    attribution.push({
      metric,
      comparator: "eq",
      required: expected,
      observed: null,
      source_path: sourcePath,
      passed: false,
      reason: missingReason,
    });
    failures.push(makeFailure(metric, missingReason, observed, expected, sourcePath));
    return;
  }
  const passed = observed === expected;
  attribution.push({
    metric,
    comparator: "eq",
    required: expected,
    observed: observed,
    source_path: sourcePath,
    passed,
    reason: passed ? "within_threshold" : mismatchReason,
  });
  if (!passed) {
    failures.push(makeFailure(metric, mismatchReason, observed, expected, sourcePath));
  }
};

const evaluateGte = (metric, observed, minimum, sourcePath) => {
  if (!Number.isFinite(observed)) {
    attribution.push({
      metric,
      comparator: "gte",
      required: minimum,
      observed: null,
      source_path: sourcePath,
      passed: false,
      reason: "missing_or_non_numeric",
    });
    failures.push(makeFailure(metric, "missing_or_non_numeric", observed, minimum, sourcePath));
    return;
  }
  const roundedObserved = Number(observed.toFixed(6));
  const passed = observed >= minimum;
  attribution.push({
    metric,
    comparator: "gte",
    required: minimum,
    observed: roundedObserved,
    source_path: sourcePath,
    passed,
    reason: passed ? "within_threshold" : "below_required_min",
  });
  if (!passed) {
    failures.push(makeFailure(metric, "below_required_min", observed, minimum, sourcePath));
  }
};

const evaluateLte = (metric, observed, maximum, sourcePath) => {
  if (!Number.isFinite(observed)) {
    attribution.push({
      metric,
      comparator: "lte",
      required: maximum,
      observed: null,
      source_path: sourcePath,
      passed: false,
      reason: "missing_or_non_numeric",
    });
    failures.push(makeFailure(metric, "missing_or_non_numeric", observed, maximum, sourcePath));
    return;
  }
  const roundedObserved = Number(observed.toFixed(6));
  const passed = observed <= maximum;
  attribution.push({
    metric,
    comparator: "lte",
    required: maximum,
    observed: roundedObserved,
    source_path: sourcePath,
    passed,
    reason: passed ? "within_threshold" : "above_allowed_max",
  });
  if (!passed) {
    failures.push(makeFailure(metric, "above_allowed_max", observed, maximum, sourcePath));
  }
};

evaluateEqual(
  "actor_count",
  actorCountReading.value,
  requiredProfile.required_actor_count,
  actorCountReading.source_path,
  "missing_or_non_numeric",
  "actor_count_mismatch",
);
evaluateGte("fps", effectiveFpsValue, requiredProfile.min_fps, effectiveFpsSource);
evaluateLte("p95_frame_ms", p95FrameReading.value, thresholds.max_p95_frame_ms, p95FrameReading.source_path);
evaluateLte(
  "animation_eval_ms",
  animationEvalReading.value,
  thresholds.max_animation_eval_ms,
  animationEvalReading.source_path,
);
evaluateLte(
  "skinning_upload_ms",
  skinningUploadReading.value,
  thresholds.max_skinning_upload_ms,
  skinningUploadReading.source_path,
);

const payload = {
  schema_version: 3,
  kind: "animation-perf-cert-report-v3",
  metrics_path: metricsPath,
  status: failures.length === 0 ? "pass" : "fail",
  required_profile: requiredProfile,
  thresholds,
  observed: {
    actor_count: Number.isFinite(actorCountReading.value) ? actorCountReading.value : null,
    fps: Number.isFinite(effectiveFpsValue) ? Number(effectiveFpsValue.toFixed(6)) : null,
    p95_frame_ms: Number.isFinite(p95FrameReading.value) ? Number(p95FrameReading.value.toFixed(6)) : null,
    animation_eval_ms: Number.isFinite(animationEvalReading.value) ? Number(animationEvalReading.value.toFixed(6)) : null,
    skinning_upload_ms: Number.isFinite(skinningUploadReading.value)
      ? Number(skinningUploadReading.value.toFixed(6))
      : null,
  },
  sources: {
    actor_count: actorCountReading.source_path,
    fps: effectiveFpsSource,
    p95_frame_ms: p95FrameReading.source_path,
    animation_eval_ms: animationEvalReading.source_path,
    skinning_upload_ms: skinningUploadReading.source_path,
  },
  attribution,
  failing_metrics: failures,
};

process.stdout.write(JSON.stringify(payload));
NODE
)"

status="$(node -e 'const p=JSON.parse(process.argv[1]); process.stdout.write(p.status);' "$analysis_json")"
fps_value="$(node -e 'const p=JSON.parse(process.argv[1]); process.stdout.write(String((p.observed || {}).fps));' "$analysis_json")"
actors_value="$(node -e 'const p=JSON.parse(process.argv[1]); process.stdout.write(String((p.observed || {}).actor_count));' "$analysis_json")"
failure_count="$(node -e 'const p=JSON.parse(process.argv[1]); process.stdout.write(String(Array.isArray(p.failing_metrics) ? p.failing_metrics.length : 0));' "$analysis_json")"

printf '%s\n' "$analysis_json" | node -e '
const fs = require("node:fs");
const payload = JSON.parse(fs.readFileSync(0, "utf8"));
const reportPath = process.argv[1];
fs.writeFileSync(reportPath, JSON.stringify(payload, null, 2));
' "$REPORT_PATH"

cat > "$SUMMARY_PATH" <<SUMMARY
# Animation Perf Cert Summary

status: ${status}
actor_count: ${actors_value}
fps: ${fps_value}
failing_metrics: ${failure_count}
report: ${REPORT_PATH}
SUMMARY

if [[ "$status" == "pass" ]]; then
  exit 0
fi

echo "perf cert failed: 60fps/32-actor animation performance contract not met" >&2
exit 1
