import type { MeshData, RenderSurface, Vec3 } from "@wrela/model";
export const VERTEX_FLOATS = 17;
export const OBJECT_FLOATS = 152;
export const GLOBAL_FLOATS = 144;
export const NOISE_PERIOD = 1024;
/** Reduce the double-precision world origin before uploading shader lattice coordinates. */
export function noiseOrigin(origin: Vec3, frequency: number): Vec3 {
  return origin.map(
    (coordinate) => (((coordinate * frequency) % NOISE_PERIOD) + NOISE_PERIOD) % NOISE_PERIOD,
  ) as Vec3;
}
export function packVertices(mesh: MeshData, skin?: RenderSurface["skin"]): Float32Array {
  const count = mesh.positions.length / 3;
  const out = new Float32Array(count * VERTEX_FLOATS);
  for (let i = 0; i < count; i++) {
    const o = i * VERTEX_FLOATS;
    out.set(mesh.positions.subarray(i * 3, i * 3 + 3), o);
    out.set(mesh.normals.subarray(i * 3, i * 3 + 3), o + 3);
    out.set(mesh.colors?.subarray(i * 3, i * 3 + 3) ?? [1, 1, 1], o + 6);
    out.set(skin?.jointIndices.subarray(i * 4, i * 4 + 4) ?? [0, 0, 0, 0], o + 9);
    out.set(skin?.weights.subarray(i * 4, i * 4 + 4) ?? [1, 0, 0, 0], o + 13);
  }
  return out;
}
export function packSurface(surface: RenderSurface, origin: Vec3 = [0, 0, 0]): Float32Array {
  const out = new Float32Array(OBJECT_FLOATS);
  out.set(surface.matrix, 0);
  out.set([...surface.material.color, surface.material.roughness], 16);
  out.set([...surface.material.secondary, surface.material.metallic], 20);
  out.set(
    [surface.material.pattern, surface.material.scale, surface.material.normalStrength, surface.wind ?? 0],
    24,
  );
  out.set(
    [surface.water ? 1 : 0, surface.selected ? 1 : 0, surface.skin ? 1 : 0, surface.water?.waves.length ?? 0],
    28,
  );
  for (let i = 0; i < 8; i++) {
    const wave = surface.water?.waves[i];
    if (wave)
      out.set([wave.amplitude, wave.wavelength, wave.speed, wave.direction, wave.phase, 0, 0, 0], 32 + i * 8);
  }
  // Level is present even for flat water with no waves.
  out[37] = surface.waterApproximation?.spacing ?? 0;
  out[39] = surface.water?.level ?? 0;
  const material = surface.material;
  const world = material.domain === "world";
  out.set([world ? 1 : 0, Math.min(material.layers?.length ?? 0, 2), 0, 0], 96);
  if (world) {
    for (const [index, frequency] of [1, 2, 7, 21].entries())
      out.set(noiseOrigin(origin, material.scale * frequency), 100 + index * 4);
    const tau = 2 * Math.PI;
    out[116] = (origin[1] * material.scale * 3) % tau;
    out[117] = ((origin[0] * 1.3 + origin[2] * 0.4) * material.scale) % tau;
  }
  for (const [index, layer] of (material.layers ?? []).slice(0, 2).entries()) {
    const offset = 120 + index * 16;
    out.set([...layer.color, layer.roughness], offset);
    out.set([layer.metallic, layer.coverage, layer.slopeBias, layer.noiseScale], offset + 4);
    out[offset + 8] = layer.normalStrength;
    if (world) out.set(noiseOrigin(origin, layer.noiseScale), offset + 12);
  }
  return out;
}
