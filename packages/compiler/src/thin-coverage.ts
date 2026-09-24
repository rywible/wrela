import { contentKey, type ThinCoverage } from "@wrela/model";

import { botanicalRandom } from "./botanical-branch";

export type NeedleCoverageSource = {
  seed: number;
  count: number;
  width: number;
  length: number;
  shootLength: number;
  spread: number;
  loss?: number;
};
export type CoverageTile = Omit<ThinCoverage, "uv">;

/** Explicit area averages, including the last one-pixel level. No alpha-test
 * coverage rescaling: the field stores actual expected coverage at every scale. */
export function coverageMipChain(base: Uint8Array, width: number, height: number): Uint8Array[] {
  const levels = [base];
  // Keep unquantized averages between levels. Repeated R8 rounding otherwise
  // accumulates visible opacity drift in sparse needles at the smallest mips.
  let previous = Float64Array.from(base);
  while (width > 1 || height > 1) {
    const nextWidth = Math.max(1, width >> 1),
      nextHeight = Math.max(1, height >> 1);
    const next = new Float64Array(nextWidth * nextHeight);
    for (let y = 0; y < nextHeight; y++)
      for (let x = 0; x < nextWidth; x++) {
        let sum = 0,
          count = 0;
        for (let dy = 0; dy < Math.min(2, height); dy++)
          for (let dx = 0; dx < Math.min(2, width); dx++) {
            sum += previous[(y * 2 + dy) * width + x * 2 + dx];
            count++;
          }
        next[y * nextWidth + x] = sum / count;
      }
    levels.push(Uint8Array.from(next, Math.round));
    previous = next;
    width = nextWidth;
    height = nextHeight;
  }
  return levels;
}

type Point = [number, number];
export type CoverageTriangle = [Point, Point, Point];
/** Rasterize a semantic tapered needle into a supersampled scalar field. */
function triangle(mask: Uint8Array, width: number, height: number, a: Point, b: Point, c: Point) {
  const edge = (a: Point, b: Point, x: number, y: number) =>
    (x - a[0]) * (b[1] - a[1]) - (y - a[1]) * (b[0] - a[0]);
  const sign = Math.sign(edge(a, b, c[0], c[1]));
  const minX = Math.max(0, Math.floor(Math.min(a[0], b[0], c[0]) * width)),
    maxX = Math.min(width - 1, Math.ceil(Math.max(a[0], b[0], c[0]) * width));
  const minY = Math.max(0, Math.floor(Math.min(a[1], b[1], c[1]) * height)),
    maxY = Math.min(height - 1, Math.ceil(Math.max(a[1], b[1], c[1]) * height));
  for (let y = minY; y <= maxY; y++)
    for (let x = minX; x <= maxX; x++) {
      const u = (x + 0.5) / width,
        v = (y + 0.5) / height;
      if (edge(a, b, u, v) * sign >= 0 && edge(b, c, u, v) * sign >= 0 && edge(c, a, u, v) * sign >= 0)
        mask[y * width + x] = 255;
    }
}

/** Generate one deterministic needle spray from botanical dimensions. The proxy
 * can be coarse; the shape and projected coverage live in this sampled field. */
export function needleCoverageTile(source: NeedleCoverageSource, width = 128, height = 256): CoverageTile {
  const scale = 4,
    mask = new Uint8Array(width * height * scale * scale);
  for (const [a, b, c] of needleCoverageTriangles(source))
    triangle(mask, width * scale, height * scale, a, b, c);
  const base = new Uint8Array(width * height);
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++) {
      let sum = 0;
      for (let dy = 0; dy < scale; dy++)
        for (let dx = 0; dx < scale; dx++) sum += mask[(y * scale + dy) * width * scale + x * scale + dx];
      base[y * width + x] = Math.round(sum / (scale * scale));
    }
  return {
    version: 1,
    key: contentKey({ algorithm: "semantic-needle-coverage-2", source, width, height }),
    width,
    height,
    levels: coverageMipChain(base, width, height),
  };
}

/** Exact semantic triangles retained for supersampled reference renders. */
export function needleCoverageTriangles(source: NeedleCoverageSource): CoverageTriangle[] {
  const triangles: CoverageTriangle[] = [];
  const random = (key: string) => botanicalRandom(source.seed, key);
  const span = source.spread * 2;
  for (let index = 0; index < source.count; index++) {
    if (random(`loss${index}`) < (source.loss ?? 0)) continue;
    const t = 0.07 + ((index + 0.5) / source.count) * 0.84;
    const side = index % 2 ? -1 : 1;
    const length = source.length * (0.78 + random(`length${index}`) * 0.35);
    const sideways = length * (0.72 + random(`angle${index}`) * 0.2);
    const forward = length * (0.3 + random(`forward${index}`) * 0.22);
    const x = 0.5 + (random(`origin${index}`) - 0.5) * 0.03;
    const diagonal = Math.hypot(sideways, forward);
    const perpendicular: Point = [
      ((-forward / diagonal) * source.width * 0.5) / span,
      (((side * sideways) / diagonal) * source.width * 0.5) / source.shootLength,
    ];
    triangles.push([
      [x - perpendicular[0], t - perpendicular[1]],
      [x + perpendicular[0], t + perpendicular[1]],
      [x + (side * sideways) / span, Math.min(0.99, t + forward / source.shootLength)],
    ]);
  }
  return triangles;
}

/** Independent needle groups share one binding, including a half-retained cohort.
 * The retained layer stores expected area; explicit needles remain the anatomy reference. */
export function needleCoverageModules(source: NeedleCoverageSource): CoverageTile {
  const tiles = Array.from({ length: 4 }, (_, variant) =>
    needleCoverageTile({ ...source, seed: source.seed + variant * 7919 }),
  );
  const layers = 8;
  const levels = tiles[0].levels.map((base, mip) => {
    const packed = new Uint8Array(base.length * layers);
    for (let retained = 0; retained < 2; retained++)
      for (let variant = 0; variant < 4; variant++) {
        const tile = tiles[variant].levels[mip],
          offset = (retained * 4 + variant) * base.length;
        for (let i = 0; i < tile.length; i++) packed[offset + i] = Math.round(tile[i] * (retained ? 0.5 : 1));
      }
    return packed;
  });
  return { ...tiles[0], layers, levels, key: contentKey({ algorithm: "needle-modules-1", source }) };
}
