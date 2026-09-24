/** Maximum complete lighting responses in an ordinary unresolved water footprint. */
export const WATER_HIGHLIGHT_SAMPLE_BUDGET = 64;
const THRESHOLDS = [0.03, 0.25, 0.5, 1, 2] as const;
type Triple = [number, number, number];

/** Bounded anisotropic quadrature; saturation is an estimate, never a certificate. */
export function waterHighlightCounts(variation: readonly number[], roughness: number): Triple {
  const width = Math.max(roughness * roughness, 0.0004);
  const requested = variation.map(
    (change) => 2 ** THRESHOLDS.filter((threshold) => change / width > threshold).length,
  );
  const counts: Triple = [1, 1, 1];
  for (let step = 0; step < Math.log2(WATER_HIGHLIGHT_SAMPLE_BUDGET); step++) {
    if (counts[0] * counts[1] * counts[2] * 2 > WATER_HIGHLIGHT_SAMPLE_BUDGET) break;
    let best = -1;
    let score = -1;
    for (let axis = 0; axis < 3; axis++) {
      const priority = (variation[axis] * variation[axis]) / counts[axis];
      if (counts[axis] < requested[axis] && priority > score) {
        best = axis;
        score = priority;
      }
    }
    if (best < 0) break;
    counts[best] *= 2;
  }
  return counts;
}

/** Emit the same budget and per-axis requests used by the CPU accuracy checks. */
export function createWaterBudgetWGSL(): string {
  return `
const WATER_MAX_HIGHLIGHT_SAMPLES:u32=${WATER_HIGHLIGHT_SAMPLE_BUDGET}u;
fn waterHighlightCounts(variation:vec3f,rough:f32)->vec3u {
  let ratio=variation/max(rough*rough,0.0004);var requested=vec3u(1u);
  for(var axis=0u;axis<3u;axis++) {
    ${THRESHOLDS.map((threshold, index) => `if(ratio[axis]>${threshold.toFixed(2)}) {requested[axis]=${2 ** (index + 1)}u;}`).join("\n    ")}
  }
  var counts=vec3u(1u);
  for(var allocation=0u;allocation<${Math.log2(WATER_HIGHLIGHT_SAMPLE_BUDGET)}u;allocation++) {
    if(counts.x*counts.y*counts.z*2u>WATER_MAX_HIGHLIGHT_SAMPLES) {break;}
    var best=3u;var score=-1.0;
    for(var axis=0u;axis<3u;axis++) {
      let priority=variation[axis]*variation[axis]/f32(counts[axis]);
      if(counts[axis]<requested[axis]&&priority>score) {best=axis;score=priority;}
    }
    if(best==3u) {break;}counts[best]*=2u;
  }
  return counts;
}`;
}
