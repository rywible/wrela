import type { MeshData, RenderSurface, Vec3 } from "@wrela/model";

import { CLOUD_FORMATION_FLOATS, CLOUD_FORMATION_GLOBAL_OFFSET } from "./cloud-formations";
import {
  CREATURE_MATERIAL_FLOATS,
  CREATURE_MATERIAL_OFFSET,
  packCreatureMaterial,
} from "./creature-material";
import { packSurfaceAppearance, SURFACE_APPEARANCE_FLOATS } from "./surface-appearance";

const DEFAULT_CREATURE_MATERIAL = packCreatureMaterial();
export const VERTEX_FLOATS = 23;
export const RELIEF_VERTEX_FLOATS = 6;
export const WATER_GLINT_OFFSET = CREATURE_MATERIAL_OFFSET + CREATURE_MATERIAL_FLOATS;
export const SURFACE_APPEARANCE_OFFSET = WATER_GLINT_OFFSET + 16;
export const RELIEF_APPEARANCE_OFFSET = SURFACE_APPEARANCE_OFFSET + SURFACE_APPEARANCE_FLOATS;
export const WATER_FIELD_OFFSET = RELIEF_APPEARANCE_OFFSET + 20;
export const LOCAL_LIGHT_OFFSET = WATER_FIELD_OFFSET + 4;
export const RADIANCE_LIGHT_OFFSET = LOCAL_LIGHT_OFFSET + 4;
export const EMISSION_OFFSET = RADIANCE_LIGHT_OFFSET + 8;
export const RADIANCE_WEIGHTS_OFFSET = EMISSION_OFFSET + 4;
export const OBJECT_FLOATS = RADIANCE_WEIGHTS_OFFSET + 4;
export const GLOBAL_FLOATS = CLOUD_FORMATION_GLOBAL_OFFSET + CLOUD_FORMATION_FLOATS;
export const NOISE_PERIOD = 1024;
/** Reduce the double-precision world origin before uploading shader lattice coordinates. */
export function noiseOrigin(origin: Vec3, frequency: number): Vec3 {
  return origin.map(
    (coordinate) => (((coordinate * frequency) % NOISE_PERIOD) + NOISE_PERIOD) % NOISE_PERIOD,
  ) as Vec3;
}
export function packVertices(mesh: MeshData, skin?: RenderSurface["skin"]): Float32Array {
  if (
    mesh.materialCoordinates &&
    (skin ||
      mesh.materialCoordinates.length !== mesh.positions.length ||
      !mesh.materialCoordinates.every(Number.isFinite))
  )
    throw Error("Invalid rigid material coordinates");
  if (skin && mesh.thinCoverage?.layer) throw Error("Layered coverage does not support skinned meshes");
  if (
    mesh.skyVisibility &&
    (skin ||
      mesh.indirectProofs ||
      mesh.skyVisibility.length !== (mesh.positions.length / 3) * 4 ||
      !mesh.skyVisibility.every(Number.isFinite))
  )
    throw Error("Invalid rigid sky visibility stream");
  if (
    mesh.indirectProofs &&
    (skin ||
      mesh.indirectProofs.length !== (mesh.positions.length / 3) * 4 ||
      !mesh.indirectProofs.every(Number.isFinite))
  )
    throw Error("Invalid static lighting proof stream");
  if (
    mesh.radianceProbes &&
    (skin ||
      mesh.wind ||
      mesh.radianceProbes.length !== (mesh.positions.length / 3) * 4 ||
      !mesh.radianceProbes.every(
        (v, i, values) =>
          Number.isFinite(v) &&
          v >= 0 &&
          (i % 2 === 0
            ? v < 132096 && (v < 513 || v >= 1024) && (Number.isInteger(v) || Number.isInteger(v - 0.5))
            : values[i - 1] >= 1024
              ? v <= 65536 && Number.isInteger(v)
              : v < 513 && (Number.isInteger(v) || (v >= 1 && v % 1 >= 0.125 && v % 1 <= 0.375))),
      ))
  )
    throw Error("Invalid radiance sample stream");
  const count = mesh.positions.length / 3;
  const out = new Float32Array(count * VERTEX_FLOATS);
  for (let i = 0; i < count; i++) {
    const o = i * VERTEX_FLOATS;
    out.set(mesh.positions.subarray(i * 3, i * 3 + 3), o);
    out.set(mesh.normals.subarray(i * 3, i * 3 + 3), o + 3);
    out.set(mesh.colors?.subarray(i * 3, i * 3 + 3) ?? [1, 1, 1], o + 6);
    out.set(
      skin?.jointIndices.subarray(i * 4, i * 4 + 4) ??
        mesh.materialCoordinates?.subarray(i * 3, i * 3 + 3) ?? [0, 0, 0, 0],
      o + 9,
    );
    if (!skin) out[o + 12] = mesh.thinCoverage?.layer?.[i] ?? 0;
    out.set(
      skin?.weights.subarray(i * 4, i * 4 + 4) ??
        mesh.indirectProofs?.subarray(i * 4, i * 4 + 4) ??
        mesh.skyVisibility?.subarray(i * 4, i * 4 + 4) ?? [1, 0, 0, 0],
      o + 13,
    );
    out.set(
      mesh.wind?.subarray(i * 4, i * 4 + 4) ??
        mesh.radianceProbes?.subarray(i * 4, i * 4 + 4) ?? [0, 0, 0, 0],
      o + 17,
    );
    out.set(mesh.thinCoverage?.uv.subarray(i * 2, i * 2 + 2) ?? [0, 0], o + 21);
  }
  return out;
}

/** Only realized relief needs a second immutable source stream. Ordinary meshes
 * reuse the main vertex buffer for source position and normal attributes. */
export function packReliefSourceVertices(mesh: MeshData): Float32Array | undefined {
  if (!mesh.reliefCoordinates && !mesh.reliefNormals) return undefined;
  if (
    !mesh.reliefCoordinates ||
    !mesh.reliefNormals ||
    mesh.reliefCoordinates.length !== mesh.positions.length ||
    mesh.reliefNormals.length !== mesh.normals.length
  )
    throw new Error("Malformed relief source vertex attributes");
  const count = mesh.positions.length / 3;
  const out = new Float32Array(count * RELIEF_VERTEX_FLOATS);
  for (let i = 0; i < count; i++) {
    out.set(mesh.reliefCoordinates.subarray(i * 3, i * 3 + 3), i * RELIEF_VERTEX_FLOATS);
    out.set(mesh.reliefNormals.subarray(i * 3, i * 3 + 3), i * RELIEF_VERTEX_FLOATS + 3);
  }
  return out;
}
export function packSurface(surface: RenderSurface, origin: Vec3 = [0, 0, 0]): Float32Array {
  const out = new Float32Array(OBJECT_FLOATS);
  out.set(surface.radianceProbes ?? [0, 0, 0, 0], RADIANCE_LIGHT_OFFSET);
  out[RADIANCE_LIGHT_OFFSET + 4] = surface.radianceWeight ?? 0;
  out[RADIANCE_LIGHT_OFFSET + 5] =
    surface.mesh.radianceProbes &&
    (!surface.selectedRenderProduct || surface.selectedRenderProduct.kind === "direct-mesh")
      ? 1
      : surface.radianceWeights
        ? 2
        : 0;
  if (surface.radianceWeights) out.set(surface.radianceWeights, RADIANCE_WEIGHTS_OFFSET);
  // Smooth, rough rigid carriers can interpolate low-frequency reflections.
  // Sharp, textured, coated and moving materials keep per-pixel evaluation.
  out[RADIANCE_LIGHT_OFFSET + 6] =
    Number(
      out[RADIANCE_LIGHT_OFFSET + 5] === 1 &&
        !surface.skin &&
        !surface.wind &&
        !surface.material.appearance &&
        !surface.material.creature &&
        !surface.material.layers?.length &&
        surface.material.pattern === 0 &&
        surface.material.normalStrength === 0 &&
        (surface.material.roughness >= 0.75 || surface.mesh.radianceFlatNormals),
    ) * (surface.material.roughness >= 0.75 ? 2 : 1);
  out[RADIANCE_LIGHT_OFFSET + 7] = Number(!!surface.mesh.radianceFlatNormals);
  if (surface.material.emission)
    out.set([...surface.material.emission.color, surface.material.emission.intensity], EMISSION_OFFSET);
  out[WATER_FIELD_OFFSET] = surface.waterState ? 1 : 0;
  out[WATER_FIELD_OFFSET + 1] = surface.waterEffect === undefined ? 0 : 1;
  out[WATER_FIELD_OFFSET + 2] = surface.waterEffect ?? 0;
  out[WATER_FIELD_OFFSET + 3] = surface.waterContact ? 1 : 0;
  out[LOCAL_LIGHT_OFFSET] = surface.localLightMask ?? 255;
  out[LOCAL_LIGHT_OFFSET + 1] = surface.staticIndirectReceiver ? 1 : 0;
  out[LOCAL_LIGHT_OFFSET + 2] = surface.indirectSurfaceChart ?? 0;
  out[LOCAL_LIGHT_OFFSET + 3] = surface.mesh.indirectProofs
    ? 1
    : surface.mesh.skyVisibility &&
        (!surface.selectedRenderProduct || surface.selectedRenderProduct.kind === "direct-mesh")
      ? 2 + (surface.skyVisibilityWeight ?? 1)
      : 0;
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
  const water = surface.water;
  const phases = surface.waterPhases,
    glints = phases?.glints;
  if (
    glints &&
    water &&
    phases.algorithmVersion === 2 &&
    phases.carriers.length === 2 &&
    glints.indices[0] === 0 &&
    glints.indices[1] === 1 &&
    water.waves.length === 2
  ) {
    const valid = phases.carriers.every((carrier, index) => {
      const wave = water.waves[index],
        k = (2 * Math.PI) / wave.wavelength;
      return (
        carrier.amplitude === wave.amplitude &&
        Math.abs(carrier.spatial[0] - k * Math.cos(wave.direction)) < 1e-10 &&
        Math.abs(carrier.spatial[1] - k * Math.sin(wave.direction)) < 1e-10 &&
        Math.abs(carrier.temporal + k * wave.speed) < 1e-10
      );
    });
    if (valid) {
      phases.carriers.forEach((c, i) => {
        out.set([...c.slope, ...c.spatial], WATER_GLINT_OFFSET + i * 4);
      });
      out.set([phases.carriers[0].temporal, phases.carriers[1].temporal, 1, 0], WATER_GLINT_OFFSET + 8);
      out.set(glints.inverseSlope, WATER_GLINT_OFFSET + 12);
    }
  }
  // Level is present even for flat water with no waves.
  out[37] = surface.waterApproximation?.spacing ?? 0;
  out[38] =
    surface.waterAppearance?.quality === "low" ? 1 : surface.waterAppearance?.quality === "high" ? 3 : 2;
  const shutter = surface.waterAppearance?.shutterSeconds ?? 1 / 60;
  out[45] = Number.isFinite(shutter) && shutter >= 0 && shutter <= 4 ? shutter : 0;
  out[46] =
    surface.waterAppearance?.mode === "reference"
      ? 1
      : surface.waterAppearance?.mode === "direct"
        ? 2
        : surface.waterAppearance?.mode === "regular"
          ? 3
          : 0;
  out[39] = surface.water?.level ?? 0;
  const material = surface.material;
  const world = material.domain === "world";
  out.set(
    [
      world ? 1 : surface.mesh.materialCoordinates && !surface.skin ? 2 : 0,
      Math.min(material.layers?.length ?? 0, 2),
      surface.creatureInspection ? 1 : 0,
      surface.mesh.thinCoverage?.format === "coverage-normal" ? 2 : surface.mesh.thinCoverage ? 1 : 0,
    ],
    96,
  );
  if (world) {
    for (const [index, frequency] of [1, 2, 7, 21].entries())
      out.set(noiseOrigin(origin, material.scale * frequency), 100 + index * 4);
    const tau = 2 * Math.PI;
    out[116] = (origin[1] * material.scale * 3) % tau;
    out[117] = ((origin[0] * 1.3 + origin[2] * 0.4) * material.scale) % tau;
    out[118] = ((origin[0] * material.scale) % 1) * tau;
    out[119] = ((origin[2] * material.scale) % 1) * tau;
  }
  for (const [index, layer] of (material.layers ?? []).slice(0, 2).entries()) {
    const offset = 120 + index * 16;
    out.set([...layer.color, layer.roughness], offset);
    out.set([layer.metallic, layer.coverage, layer.slopeBias, layer.noiseScale], offset + 4);
    out[offset + 8] = layer.normalStrength;
    if (world) out.set(noiseOrigin(origin, layer.noiseScale), offset + 12);
  }
  out.set(
    material.creature ? packCreatureMaterial(material.creature) : DEFAULT_CREATURE_MATERIAL,
    CREATURE_MATERIAL_OFFSET,
  );
  if (material.appearance)
    out.set(packSurfaceAppearance(material.appearance, origin, world), SURFACE_APPEARANCE_OFFSET);
  const relief = surface.reliefAppearance;
  if (relief) {
    const { recipe } = relief;
    out.set([recipe.kind === "bark" ? 1 : 2, recipe.amplitude, recipe.scale, 0], RELIEF_APPEARANCE_OFFSET);
    // Two exactly representable halves avoid NaN bit patterns in uniform signatures.
    out[RELIEF_APPEARANCE_OFFSET + 3] = recipe.seed & 65535;
    out.set([...recipe.direction, recipe.seed >>> 16], RELIEF_APPEARANCE_OFFSET + 4);
    out.set([...relief.geometryWeights, recipe.seed % (2 * Math.PI)], RELIEF_APPEARANCE_OFFSET + 8);
    out.set([...relief.residualWeights, 0], RELIEF_APPEARANCE_OFFSET + 12);
    out.set([...relief.slopeVariance, 0], RELIEF_APPEARANCE_OFFSET + 16);
  }
  return out;
}
