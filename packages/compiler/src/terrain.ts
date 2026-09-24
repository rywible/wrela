import {
  clamp,
  coordinateHash,
  MAX_WORLD_COORDINATE,
  type MeshData,
  normalize,
  type TerrainDefinition,
  type Vec3,
} from "@wrela/model";

import { compileGeologicalCorridors, geologicalCorridorHeight } from "./geology-corridors";
import {
  compileGeologicalLandforms,
  geologicalBankTint,
  geologicalLandformHeight,
  geologicalSurfaceHeight,
} from "./geology-height";

const smooth = (t: number) => t * t * t * (t * (t * 6 - 15) + 10);
const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
/** Coordinate-hashed value noise: request ordering and patch boundaries do not
 * affect samples. CPU doubles retain detail before conversion to local f32. */
export function terrainNoise(x: number, z: number, seed: number): number {
  const ix = Math.floor(x),
    iz = Math.floor(z),
    tx = smooth(x - ix),
    tz = smooth(z - iz);
  const at = (a: number, b: number) => (coordinateHash(a, b, seed) / 4294967295) * 2 - 1;
  return lerp(lerp(at(ix, iz), at(ix + 1, iz), tx), lerp(at(ix, iz + 1), at(ix + 1, iz + 1), tx), tz);
}
export function terrainHeight(terrain: TerrainDefinition, x: number, z: number): number {
  return sampleTerrainHeight(terrain, x, z, (xx, zz) =>
    terrainBaseHeight(terrain, xx, zz, (height, px, pz) =>
      geologicalLandformHeight(terrain.geology, height, px, pz),
    ),
  );
}
/** Reuse compiled landform bounds for a synchronous batch; rebuild after source edits. */
export function createTerrainSampler(terrain: TerrainDefinition) {
  const landforms = compileGeologicalLandforms(terrain.geology);
  const corridors = compileGeologicalCorridors(terrain.geology);
  // Normal and erosion stencils revisit nearby coordinates. Exact numeric keys
  // avoid quantization and the hard cap bounds memory for maximum-size patches.
  const columns = new Map<number, Map<number, number>>();
  let entries = 0;
  const base = (x: number, z: number) => {
    if (!terrain.geology?.erosion.strength) return terrainBaseHeight(terrain, x, z, landforms);
    const column = columns.get(x),
      cached = column?.get(z);
    if (cached !== undefined) return cached;
    const value = terrainBaseHeight(terrain, x, z, landforms);
    if (entries >= 32_768) {
      columns.clear();
      entries = 0;
    }
    const target = columns.get(x) ?? new Map<number, number>();
    target.set(z, value);
    columns.set(x, target);
    entries++;
    return value;
  };
  const height = (x: number, z: number) => sampleTerrainHeight(terrain, x, z, base, corridors);
  return { height, normal: (x: number, z: number) => sampleTerrainNormal(height, x, z) };
}
function sampleTerrainHeight(
  terrain: TerrainDefinition,
  x: number,
  z: number,
  base: (x: number, z: number) => number,
  corridors = (height: number, px: number, pz: number) =>
    geologicalCorridorHeight(terrain.geology, height, px, pz),
): number {
  if (
    !Number.isFinite(x) ||
    !Number.isFinite(z) ||
    Math.abs(x) > MAX_WORLD_COORDINATE ||
    Math.abs(z) > MAX_WORLD_COORDINATE
  )
    throw new Error("Terrain coordinates must lie within ±1,000,000,000 metres");
  const h = geologicalSurfaceHeight(
    terrain.geology,
    (xx, zz) =>
      base(
        clamp(xx, -MAX_WORLD_COORDINATE, MAX_WORLD_COORDINATE),
        clamp(zz, -MAX_WORLD_COORDINATE, MAX_WORLD_COORDINATE),
      ),
    x,
    z,
  );
  return applyInterventions(terrain, corridors(h, x, z), x, z);
}
function terrainBaseHeight(
  terrain: TerrainDefinition,
  x: number,
  z: number,
  landforms: (height: number, x: number, z: number) => number,
): number {
  let sum = 0,
    weight = 1,
    weights = 0,
    frequency = terrain.frequency;
  for (let octave = 0; octave < terrain.octaves; octave++) {
    sum += terrainNoise(x * frequency, z * frequency, terrain.seed + octave * 1013) * weight;
    weights += weight;
    weight *= 0.5;
    frequency *= 2;
  }
  return landforms(terrain.baseHeight + (terrain.amplitude * sum) / weights, x, z);
}
function applyInterventions(terrain: TerrainDefinition, initial: number, x: number, z: number): number {
  let h = initial;
  for (const edit of terrain.interventions) {
    const distance = Math.hypot(x - edit.center[0], z - edit.center[1]) / edit.radius;
    if (distance >= 1 || edit.kind === "clearing") continue;
    const influence = 1 - smooth(clamp(distance, 0, 1));
    if (edit.kind === "flatten")
      h = lerp(h, edit.targetHeight, influence * clamp(Math.abs(edit.strength), 0, 1));
    else if (edit.kind === "raise") h += Math.abs(edit.strength) * influence;
    else if (edit.kind === "lower" || edit.kind === "valley") h -= Math.abs(edit.strength) * influence;
    else if (edit.kind === "river")
      h = lerp(h, Math.min(h, edit.targetHeight - Math.abs(edit.strength)), influence);
  }
  return h;
}
export function terrainNormal(terrain: TerrainDefinition, x: number, z: number): Vec3 {
  return sampleTerrainNormal((xx, zz) => terrainHeight(terrain, xx, zz), x, z);
}
function sampleTerrainNormal(height: (x: number, z: number) => number, x: number, z: number): Vec3 {
  if (
    !Number.isFinite(x) ||
    !Number.isFinite(z) ||
    Math.abs(x) > MAX_WORLD_COORDINATE ||
    Math.abs(z) > MAX_WORLD_COORDINATE
  )
    throw new Error("Terrain normal coordinates lie outside the supported domain");
  const e = 0.05,
    left = Math.max(-MAX_WORLD_COORDINATE, x - e),
    right = Math.min(MAX_WORLD_COORDINATE, x + e),
    north = Math.max(-MAX_WORLD_COORDINATE, z - e),
    south = Math.min(MAX_WORLD_COORDINATE, z + e);
  return normalize([
    (height(left, z) - height(right, z)) / (right - left),
    1,
    (height(x, north) - height(x, south)) / (south - north),
  ]);
}
export type TerrainStitch = { north?: boolean; east?: boolean; south?: boolean; west?: boolean };
/** Local x/z positions, absolute height. True edge flags match a 2:1 coarser
 * neighbor. Odd vertices lie on its straight edges; degenerate triangles are
 * harmless. North is minimum z. Shared boundary normals interpolate likewise. */
export function generateTerrainPatch(
  terrain: TerrainDefinition,
  x: number,
  z: number,
  size: number,
  resolution: number,
  stitch: TerrainStitch = {},
): MeshData {
  if (
    !Number.isFinite(size) ||
    size <= 0 ||
    !Number.isInteger(resolution) ||
    resolution < 2 ||
    resolution > 256
  )
    throw new Error("Terrain patch requires positive size and 2–256 cells");
  if (Object.values(stitch).some(Boolean) && resolution % 2 !== 0)
    throw new Error("Stitched patches require an even resolution");
  const sampler = createTerrainSampler(terrain);
  const row = resolution + 1,
    positions = new Float32Array(row * row * 3),
    normals = new Float32Array(positions.length),
    colors = terrain.geology?.bankWetness ? new Float32Array(positions.length) : undefined,
    indices = new Uint32Array(resolution * resolution * 6),
    step = size / resolution;
  let minY = Infinity,
    maxY = -Infinity;
  for (let j = 0; j <= resolution; j++)
    for (let i = 0; i <= resolution; i++) {
      const k = (j * row + i) * 3,
        xx = x + i * step,
        zz = z + j * step;
      let h = sampler.height(xx, zz),
        n = sampler.normal(xx, zz);
      const horizontal = ((j === 0 && stitch.north) || (j === resolution && stitch.south)) && i % 2 === 1;
      const vertical = ((i === 0 && stitch.west) || (i === resolution && stitch.east)) && j % 2 === 1;
      if (horizontal || vertical) {
        const dx = horizontal ? step : 0,
          dz = vertical ? step : 0;
        h = (sampler.height(xx - dx, zz - dz) + sampler.height(xx + dx, zz + dz)) / 2;
        const a = sampler.normal(xx - dx, zz - dz),
          b = sampler.normal(xx + dx, zz + dz);
        n = normalize([a[0] + b[0], a[1] + b[1], a[2] + b[2]]);
      }
      positions.set([i * step, h, j * step], k);
      normals.set(n, k);
      if (colors) {
        const tint = geologicalBankTint(terrain.geology, xx, zz);
        colors.set([tint * 0.98, tint, tint * 0.96], k);
      }
      minY = Math.min(minY, h);
      maxY = Math.max(maxY, h);
    }
  let t = 0;
  for (let j = 0; j < resolution; j++)
    for (let i = 0; i < resolution; i++) {
      const a = j * row + i,
        b = a + 1,
        c = a + row,
        d = c + 1;
      indices.set([a, c, b, b, c, d], t);
      t += 6;
    }
  return { positions, normals, colors, indices, bounds: { min: [0, minY, 0], max: [size, maxY, size] } };
}
export { queryWater, type WaterSample } from "./water";
