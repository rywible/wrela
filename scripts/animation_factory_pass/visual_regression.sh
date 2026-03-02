#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 3 ]]; then
  echo "usage: $0 <baseline_file> <candidate_file> <report_path>" >&2
  exit 2
fi

BASELINE_PATH="$1"
CANDIDATE_PATH="$2"
REPORT_PATH="$3"
SUMMARY_PATH="$(dirname "$REPORT_PATH")/summary.md"

mkdir -p "$(dirname "$REPORT_PATH")"

if [[ ! -f "$BASELINE_PATH" ]]; then
  echo "baseline file not found: ${BASELINE_PATH}" >&2
  exit 1
fi
if [[ ! -f "$CANDIDATE_PATH" ]]; then
  echo "candidate file not found: ${CANDIDATE_PATH}" >&2
  exit 1
fi

analysis_json="$(node - "$BASELINE_PATH" "$CANDIDATE_PATH" <<'NODE'
const fs = require("node:fs");
const crypto = require("node:crypto");

const [baselinePath, candidatePath] = process.argv.slice(2);
const baseline = fs.readFileSync(baselinePath);
const candidate = fs.readFileSync(candidatePath);

const readThreshold = (name, fallback) => {
  const raw = process.env[name];
  if (typeof raw !== "string" || raw.trim().length === 0) {
    return fallback;
  }
  const parsed = Number(raw);
  return Number.isFinite(parsed) ? parsed : fallback;
};

const perceptualThreshold = readThreshold("WRELA_VISUAL_PERCEPTUAL_THRESHOLD", 0.995);
const poseMaeThreshold = readThreshold("WRELA_VISUAL_POSE_MAE_THRESHOLD", 0.015);
const poseMaxErrorThreshold = readThreshold("WRELA_VISUAL_POSE_MAX_ERROR_THRESHOLD", 0.08);

const histogram = (buf) => {
  const h = new Array(256).fill(0);
  for (const b of buf) h[b] += 1;
  return h;
};

const hb = histogram(baseline);
const hc = histogram(candidate);
const total = Math.max(baseline.length, candidate.length, 1);
let l1 = 0;
for (let i = 0; i < 256; i += 1) {
  l1 += Math.abs(hb[i] - hc[i]);
}
const normalizedDiff = l1 / (2 * total);
const perceptualScore = Number((1 - normalizedDiff).toFixed(6));
const perceptualPassed = perceptualScore >= perceptualThreshold;

const parseJson = (buf) => {
  try {
    return JSON.parse(buf.toString("utf8"));
  } catch (_) {
    return null;
  }
};

const findString = (value, paths) => {
  for (const path of paths) {
    let current = value;
    let found = true;
    for (const segment of path) {
      if (current === null || typeof current !== "object" || !Object.prototype.hasOwnProperty.call(current, segment)) {
        found = false;
        break;
      }
      current = current[segment];
    }
    if (found && typeof current === "string" && current.trim().length > 0) {
      return current.trim();
    }
  }
  return null;
};

const collectNumbers = (value, sink, limit) => {
  if (sink.length >= limit) return;
  if (typeof value === "number" && Number.isFinite(value)) {
    sink.push(value);
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) {
      collectNumbers(item, sink, limit);
      if (sink.length >= limit) return;
    }
    return;
  }
  if (value && typeof value === "object") {
    for (const [key, nested] of Object.entries(value)) {
      if (key === "timestamp_ms" || key === "frame_index" || key === "tick") {
        continue;
      }
      collectNumbers(nested, sink, limit);
      if (sink.length >= limit) return;
    }
  }
};

const extractPoseVector = (value) => {
  if (!value || typeof value !== "object") {
    return null;
  }
  const candidatePaths = [
    ["pose_vector"],
    ["pose_values"],
    ["pose_samples"],
    ["pose_sample"],
    ["animation", "pose_vector"],
    ["animation", "pose_values"],
    ["animation", "pose_samples"],
    ["animation", "pose_sample"],
    ["pose", "vector"],
    ["pose", "values"],
    ["pose", "joints"],
    ["animation", "pose", "vector"],
    ["animation", "pose", "values"],
    ["animation", "pose", "joints"],
  ];
  for (const path of candidatePaths) {
    let current = value;
    let found = true;
    for (const segment of path) {
      if (current === null || typeof current !== "object" || !Object.prototype.hasOwnProperty.call(current, segment)) {
        found = false;
        break;
      }
      current = current[segment];
    }
    if (!found) continue;
    const numbers = [];
    collectNumbers(current, numbers, 2048);
    if (numbers.length >= 8) {
      return numbers;
    }
  }
  return null;
};

const baselineJson = parseJson(baseline);
const candidateJson = parseJson(candidate);

const baselinePoseHash = findString(baselineJson, [
  ["pose_hash"],
  ["pose_parity_hash"],
  ["animation", "pose_hash"],
  ["animation", "pose_parity_hash"],
  ["pose", "hash"],
  ["animation", "pose", "hash"],
]);
const candidatePoseHash = findString(candidateJson, [
  ["pose_hash"],
  ["pose_parity_hash"],
  ["animation", "pose_hash"],
  ["animation", "pose_parity_hash"],
  ["pose", "hash"],
  ["animation", "pose", "hash"],
]);

let poseParityMode = "not_provided";
let poseParityPassed = true;
let poseParityReason = "no structured pose parity fields provided; perceptual gate only";
let poseMae = null;
let poseMaxError = null;

if (baselinePoseHash && candidatePoseHash) {
  poseParityMode = "hash";
  poseParityPassed = baselinePoseHash === candidatePoseHash;
  poseParityReason = poseParityPassed
    ? "pose hashes match"
    : "pose hashes differ";
} else {
  const baselineVector = extractPoseVector(baselineJson);
  const candidateVector = extractPoseVector(candidateJson);
  if (baselineVector && candidateVector) {
    poseParityMode = "vector";
    const n = Math.min(baselineVector.length, candidateVector.length);
    if (n > 0) {
      let absTotal = 0;
      let maxAbs = 0;
      for (let i = 0; i < n; i += 1) {
        const delta = Math.abs(baselineVector[i] - candidateVector[i]);
        absTotal += delta;
        if (delta > maxAbs) {
          maxAbs = delta;
        }
      }
      poseMae = Number((absTotal / n).toFixed(6));
      poseMaxError = Number(maxAbs.toFixed(6));
      poseParityPassed = poseMae <= poseMaeThreshold && poseMaxError <= poseMaxErrorThreshold;
      poseParityReason = poseParityPassed
        ? "pose vectors within tolerance"
        : "pose vectors exceeded tolerance";
    } else {
      poseParityPassed = false;
      poseParityReason = "pose vectors were present but empty";
    }
  }
}

const byteHashBaseline = crypto.createHash("sha256").update(baseline).digest("hex");
const byteHashCandidate = crypto.createHash("sha256").update(candidate).digest("hex");

const passed = perceptualPassed && poseParityPassed;

const payload = {
  schema_version: 3,
  kind: "animation-visual-regression-report-v3",
  baseline: baselinePath,
  candidate: candidatePath,
  status: passed ? "pass" : "fail",
  perceptual_metric: "byte-histogram-l1-similarity",
  perceptual_threshold: perceptualThreshold,
  perceptual_score: perceptualScore,
  pose_parity: {
    mode: poseParityMode,
    passed: poseParityPassed,
    reason: poseParityReason,
    mae: poseMae,
    mae_threshold: poseMaeThreshold,
    max_error: poseMaxError,
    max_error_threshold: poseMaxErrorThreshold,
    baseline_pose_hash: baselinePoseHash,
    candidate_pose_hash: candidatePoseHash,
  },
  attribution: [
    {
      gate: "perceptual",
      passed: perceptualPassed,
      observed: perceptualScore,
      threshold: perceptualThreshold,
    },
    {
      gate: "pose_parity",
      passed: poseParityPassed,
      mode: poseParityMode,
      reason: poseParityReason,
      observed_mae: poseMae,
      threshold_mae: poseMaeThreshold,
      observed_max_error: poseMaxError,
      threshold_max_error: poseMaxErrorThreshold,
    },
  ],
  byte_hashes: {
    baseline_sha256: byteHashBaseline,
    candidate_sha256: byteHashCandidate,
  },
  byte_length_delta: candidate.length - baseline.length,
};

process.stdout.write(JSON.stringify(payload));
NODE
)"

status="$(node -e 'const p=JSON.parse(process.argv[1]); process.stdout.write(p.status);' "$analysis_json")"
perceptual_score="$(node -e 'const p=JSON.parse(process.argv[1]); process.stdout.write(String(p.perceptual_score));' "$analysis_json")"
pose_mode="$(node -e 'const p=JSON.parse(process.argv[1]); process.stdout.write(String((p.pose_parity||{}).mode || "unknown"));' "$analysis_json")"
pose_passed="$(node -e 'const p=JSON.parse(process.argv[1]); process.stdout.write(String((p.pose_parity||{}).passed === true));' "$analysis_json")"

printf '%s\n' "$analysis_json" | node -e '
const fs = require("node:fs");
const payload = JSON.parse(fs.readFileSync(0, "utf8"));
const reportPath = process.argv[1];
fs.writeFileSync(reportPath, JSON.stringify(payload, null, 2));
' "$REPORT_PATH"

cat > "$SUMMARY_PATH" <<SUMMARY
# Animation Visual Regression Summary

status: ${status}
perceptual_score: ${perceptual_score}
pose_parity_mode: ${pose_mode}
pose_parity_passed: ${pose_passed}
report: ${REPORT_PATH}
SUMMARY

if [[ "$status" == "pass" ]]; then
  exit 0
fi

echo "visual regression failed: perceptual threshold and/or pose parity contract unmet" >&2
exit 1
