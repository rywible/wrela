import { CLOUD_FORMATION_FRAME_OFFSET, CLOUD_FRAME_FLOATS } from "./cloud-formations";
/** History applies to the low-cloud advected field. Shape/light tolerances are
 * measured temporal approximations; the shader separately bounds parallax and
 * rejects contours. Hard changes and coordinate discontinuities start fresh. */
export function canReuseCloudHistory(previous: Float32Array, current: Float32Array): boolean {
  if (
    previous.length !== CLOUD_FRAME_FLOATS ||
    current.length !== CLOUD_FRAME_FLOATS ||
    !previous.every(Number.isFinite) ||
    !current.every(Number.isFinite)
  )
    return false;
  const close = (indices: number[], tolerance: number) =>
    indices.every((i) => Math.abs(current[i] - previous[i]) <= tolerance);
  const dot = (offset: number) =>
    current[offset] * previous[offset] +
    current[offset + 1] * previous[offset + 1] +
    current[offset + 2] * previous[offset + 2];
  const drift = Math.hypot(
    current[37] * current[39] - previous[37] * previous[39],
    current[38] * current[39] - previous[38] * previous[39],
  );
  return (
    current[36] > 0.001 &&
    current
      .subarray(CLOUD_FORMATION_FRAME_OFFSET)
      .every((value, i) =>
        i === 1 || i === 4
          ? Math.abs(value - previous[CLOUD_FORMATION_FRAME_OFFSET + i]) <= 0.01
          : value === previous[CLOUD_FORMATION_FRAME_OFFSET + i],
      ) &&
    previous[36] > 0.001 &&
    close([12, 13, 14, 28, 29, 32, 33, 34], 0.00001) &&
    close([0, 1, 2], 80) &&
    drift <= 80 &&
    // Upper clouds advect faster than the low-cloud depth represented by the
    // history moments. A large seek/wind step must redraw that mixed depth.
    (Math.max(
      previous[50],
      current[50],
      previous[CLOUD_FORMATION_FRAME_OFFSET + 4],
      current[CLOUD_FORMATION_FRAME_OFFSET + 4],
    ) <= 0.001 ||
      drift <= 4) &&
    close([4, 5, 6, 40, 41, 42], 0.01) &&
    close([7, 8, 9, 10, 43], 0.03) &&
    close([36, 44, 48, 49, 51], 0.025) &&
    close([50], 0.01) &&
    close([52, 53, 54], 20) &&
    close([56, 57], 0.01) &&
    current[44] > 0.5 === previous[44] > 0.5 &&
    dot(24) > 0.98 &&
    dot(16) > 0.98
  );
}

/** Low-angle illumination changes rapidly with solar elevation. An old sunset
 * sample gets only one reuse; stable light still permits the longer motion cache. */
export function cloudHistoryMaxAge(previous: Float32Array, current: Float32Array): number {
  // Background coverage now opens/closes entire weather groups. Reusing that
  // boundary for three frames leaves old cloud edges behind during a transition.
  if (
    [1, 4].some(
      (offset) =>
        Math.abs(
          current[CLOUD_FORMATION_FRAME_OFFSET + offset] - previous[CLOUD_FORMATION_FRAME_OFFSET + offset],
        ) > 0.0001,
    )
  )
    return 1;
  if (
    Math.min(previous[5], current[5]) < 0.2 &&
    Math.max(previous[5], current[5]) > -0.18 &&
    [4, 5, 6].some((i) => Math.abs(previous[i] - current[i]) > 0.0001)
  )
    return 1;
  const changedLightingOrShape = [
    4,
    5,
    6,
    7,
    8,
    9,
    10,
    36,
    40,
    41,
    42,
    43,
    44,
    48,
    49,
    50,
    51,
    52,
    53,
    54,
    56,
    57,
    CLOUD_FORMATION_FRAME_OFFSET + 1,
  ].some((i) => Math.abs(previous[i] - current[i]) > 0.0001);
  return changedLightingOrShape ? 3 : 7;
}
