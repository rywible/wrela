/** Conventional current-frame Hi-Z control. Each texel stores the maximum
 * (farthest) opaque depth over its footprint, with background/invalid input
 * treated as uncovered. This CPU reference defines the reduction/query contract
 * for GPU comparisons; it is not a claimed runtime optimization. */
export type DepthLevel = { width: number; height: number; depths: Float32Array };
export function buildHiZ(width: number, height: number, depths: Float32Array): DepthLevel[] {
  if (
    !Number.isInteger(width) ||
    !Number.isInteger(height) ||
    width < 1 ||
    height < 1 ||
    width * height !== depths.length ||
    depths.length > 16_777_216
  )
    throw new Error("Invalid Hi-Z extent");
  const base = Float32Array.from(depths, (value) =>
    Number.isFinite(value) && value >= 0 && value <= 1 ? value : 1,
  );
  const levels: DepthLevel[] = [{ width, height, depths: base }];
  while (width > 1 || height > 1) {
    const previous = levels[levels.length - 1];
    width = Math.ceil(width / 2);
    height = Math.ceil(height / 2);
    const next = new Float32Array(width * height);
    for (let y = 0; y < height; y++)
      for (let x = 0; x < width; x++) {
        let depth = 0;
        for (let dy = 0; dy < 2; dy++)
          for (let dx = 0; dx < 2; dx++) {
            const px = 2 * x + dx,
              py = 2 * y + dy;
            if (px < previous.width && py < previous.height)
              depth = Math.max(depth, previous.depths[py * previous.width + px]);
          }
        next[y * width + x] = depth;
      }
    levels.push({ width, height, depths: next });
  }
  return levels;
}

/** Inclusive integer pixel bounds must conservatively cover the candidate.
 * No history/reprojection is implied: a stale pyramid cannot authorize a cull. */
export function hiddenByHiZ(
  levels: DepthLevel[],
  pixels: { x0: number; y0: number; x1: number; y1: number },
  nearestDepth: number,
  depthMargin = 1e-5,
): boolean {
  const base = levels[0];
  const { x0, y0, x1, y1 } = pixels;
  if (
    !base ||
    ![x0, y0, x1, y1].every(Number.isInteger) ||
    !Number.isFinite(nearestDepth) ||
    !Number.isFinite(depthMargin) ||
    depthMargin < 0 ||
    nearestDepth < 0 ||
    nearestDepth > 1 ||
    x0 < 0 ||
    y0 < 0 ||
    x1 >= base.width ||
    y1 >= base.height ||
    x1 < x0 ||
    y1 < y0
  )
    return false;
  // A coarse query may include extra pixels, which only weakens occlusion.
  const levelIndex = Math.min(
    levels.length - 1,
    Math.max(0, Math.floor(Math.log2(Math.max(x1 - x0 + 1, y1 - y0 + 1)))),
  );
  const level = levels[levelIndex],
    scale = 2 ** levelIndex;
  for (let y = Math.floor(y0 / scale); y <= Math.floor(y1 / scale); y++)
    for (let x = Math.floor(x0 / scale); x <= Math.floor(x1 / scale); x++)
      if (!(nearestDepth > level.depths[y * level.width + x] + depthMargin)) return false;
  return true;
}
