import {
  clamp,
  coordinateHash,
  MAX_WORLD_COORDINATE,
  type MeshData,
  normalize,
  type TerrainDefinition,
  type Vec3,
  type WaterDefinition,
} from "@wrela/model";

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
  if (
    !Number.isFinite(x) ||
    !Number.isFinite(z) ||
    Math.abs(x) > MAX_WORLD_COORDINATE ||
    Math.abs(z) > MAX_WORLD_COORDINATE
  )
    throw new Error("Terrain coordinates must lie within ±1,000,000,000 metres");
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
  let h = terrain.baseHeight + (terrain.amplitude * sum) / weights;
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
    (terrainHeight(terrain, left, z) - terrainHeight(terrain, right, z)) / (right - left),
    1,
    (terrainHeight(terrain, x, north) - terrainHeight(terrain, x, south)) / (south - north),
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
  const row = resolution + 1,
    positions = new Float32Array(row * row * 3),
    normals = new Float32Array(positions.length),
    indices = new Uint32Array(resolution * resolution * 6),
    step = size / resolution;
  let minY = Infinity,
    maxY = -Infinity;
  for (let j = 0; j <= resolution; j++)
    for (let i = 0; i <= resolution; i++) {
      const k = (j * row + i) * 3,
        xx = x + i * step,
        zz = z + j * step;
      let h = terrainHeight(terrain, xx, zz),
        n = terrainNormal(terrain, xx, zz);
      const horizontal = ((j === 0 && stitch.north) || (j === resolution && stitch.south)) && i % 2 === 1;
      const vertical = ((i === 0 && stitch.west) || (i === resolution && stitch.east)) && j % 2 === 1;
      if (horizontal || vertical) {
        const dx = horizontal ? step : 0,
          dz = vertical ? step : 0;
        h = (terrainHeight(terrain, xx - dx, zz - dz) + terrainHeight(terrain, xx + dx, zz + dz)) / 2;
        const a = terrainNormal(terrain, xx - dx, zz - dz),
          b = terrainNormal(terrain, xx + dx, zz + dz);
        n = normalize([a[0] + b[0], a[1] + b[1], a[2] + b[2]]);
      }
      positions.set([i * step, h, j * step], k);
      normals.set(n, k);
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
  return { positions, normals, indices, bounds: { min: [0, minY, 0], max: [size, maxY, size] } };
}
export type WaterSample = { height: number; normal: Vec3; velocity: Vec3 };
/** speed is metres/second. This exact phase convention is mirrored in WGSL. */
export function queryWater(water: WaterDefinition, x: number, z: number, time: number): WaterSample {
  let height = water.level,
    dx = 0,
    dz = 0,
    dy = 0;
  for (const wave of water.waves) {
    const k = (2 * Math.PI) / wave.wavelength,
      c = Math.cos(wave.direction),
      s = Math.sin(wave.direction),
      phase = k * (c * x + s * z - wave.speed * time) + wave.phase;
    const derivative = wave.amplitude * k * Math.cos(phase);
    height += wave.amplitude * Math.sin(phase);
    dx += derivative * c;
    dz += derivative * s;
    dy -= derivative * wave.speed;
  }
  return { height, normal: normalize([-dx, 1, -dz]), velocity: [0, dy, 0] };
}
