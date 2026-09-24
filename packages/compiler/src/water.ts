import {
  type MeshData,
  normalize,
  type Vec3,
  type WaterDefinition,
  type WaterRenderState,
} from "@wrela/model";

import { channelCoordinates, riverSections } from "./river-channel";
import { reviewRiverChannel } from "./river-review";
import { compileWaterDomain, sampleWaterContact, sampleWaterGrid } from "./water-domain";
import { compileWaterSpectrum, queryWaterSpectrum, sampleWaterSpectrum } from "./water-spectrum";

export { type RiverDiagnostic, reviewRiverChannel } from "./river-review";

const riverSpacing = new WeakMap<MeshData, number>();
/** Conservative maximum triangle edge after subdivision caps; avoids claiming an unrealized error bound. */
export function waterMeshSpacing(mesh: MeshData, fallback: number): number {
  return riverSpacing.get(mesh) ?? fallback;
}

export type WaterSample = {
  height: number;
  normal: Vec3;
  velocity: Vec3;
  /** Distance below the authored still-water level; river edges have zero depth. */
  depth: number;
  wet: boolean;
  shore: number;
};
/** Resolve current advection once for both phase compilation and GPU submission. */
export function resolveWaterWaves(water: WaterDefinition): WaterDefinition["waves"] {
  const velocity = water.flow?.velocity;
  if (!velocity || (velocity[0] === 0 && velocity[1] === 0)) return water.waves;
  return water.waves.map((wave) => ({
    ...wave,
    speed: wave.speed + velocity[0] * Math.cos(wave.direction) + velocity[1] * Math.sin(wave.direction),
  }));
}
type ChannelSample = { depth: number; wet: boolean; shore: number; tangent: [number, number] };
/** Ribbon query uses the same joined boundary as the rendered channel. */
function channelAt(water: WaterDefinition, x: number, z: number): ChannelSample {
  const river = water.flow?.river;
  if (!river) return { depth: Number.POSITIVE_INFINITY, wet: true, shore: 0, tangent: [1, 0] };
  const sections = riverSections(river);
  let best = Number.POSITIVE_INFINITY;
  let result: ChannelSample = { depth: 0, wet: false, shore: 0, tangent: [1, 0] };
  for (let i = 1; i < river.points.length; i++) {
    const a = river.points[i - 1],
      b = river.points[i];
    const dx = b.position[0] - a.position[0],
      dz = b.position[1] - a.position[1];
    const length = Math.hypot(dx, dz);
    const coordinates = channelCoordinates(sections[i - 1], sections[i], [x, z]);
    if (!coordinates) continue;
    const [across, t] = coordinates;
    const halfWidth = (a.width + (b.width - a.width) * t) / 2;
    const normalizedDistance = Math.min(1, Math.abs(across * 2 - 1));
    const edge = halfWidth * (1 - normalizedDistance);
    if (normalizedDistance >= best) continue;
    best = normalizedDistance;
    result = {
      wet: edge >= 0,
      depth: Math.max(0, (a.depth + (b.depth - a.depth) * t) * (1 - normalizedDistance ** 2)),
      shore: edge < 0 ? 0 : 1 - Math.min(1, edge / river.shoreWidth),
      tangent: [dx / length, dz / length],
    };
  }
  return result;
}
/** Analytic height/normal and material velocity share the exact rendered phase convention. */
export function queryWater(
  water: WaterDefinition,
  x: number,
  z: number,
  time: number,
  state?: WaterRenderState,
): WaterSample {
  if (![x, z, time].every(Number.isFinite)) throw new Error("Water coordinates and time must be finite");
  if (water.domain || water.spectrum) {
    const domain = state?.domain ?? compileWaterDomain(water);
    const bed = domain ? sampleWaterContact(domain, x, z) : undefined;
    const fluid = domain && state?.cells ? sampleWaterGrid(domain, state.cells, x, z) : undefined;
    const initial = domain ? sampleWaterGrid(domain, domain.cells, x, z) : undefined;
    const level = fluid?.[0] ?? initial?.[1] ?? water.level;
    const depth = bed ? Math.max(0, level - bed[0]) : Infinity;
    if ((domain && !bed) || depth < 0.003)
      return {
        height: -Number.MAX_VALUE,
        normal: [0, 1, 0],
        velocity: [0, 0, 0],
        depth: 0,
        wet: false,
        shore: 1,
      };
    const spectrum = state?.spectrum ?? compileWaterSpectrum(water);
    const wave = domain
      ? sampleWaterSpectrum(spectrum, x, z, time)
      : queryWaterSpectrum(spectrum, x, z, time);
    const attenuation = Math.min(1, depth / 0.5);
    let dx = 0,
      dz = 0;
    if (domain) {
      const data = state?.cells ?? domain.cells,
        channel = state?.cells ? 0 : 1;
      const h = (px: number, pz: number) => sampleWaterGrid(domain, data, px, pz)?.[channel] ?? level;
      dx = (h(x + domain.spacing[0], z) - h(x - domain.spacing[0], z)) / (2 * domain.spacing[0]);
      dz = (h(x, z + domain.spacing[1]) - h(x, z - domain.spacing[1])) / (2 * domain.spacing[1]);
    }
    return {
      height: level + wave.height * attenuation,
      normal: normalize([-dx - wave.dx * attenuation, 1, -dz - wave.dz * attenuation]),
      velocity: [
        (fluid?.[1] ?? initial?.[2] ?? 0) + (domain ? 0 : wave.velocityX),
        wave.velocity * attenuation,
        (fluid?.[2] ?? initial?.[3] ?? 0) + (domain ? 0 : wave.velocityZ),
      ],
      depth,
      wet: true,
      shore: 1 - attenuation,
    };
  }
  const channel = channelAt(water, x, z);
  // Existing buoyancy consumers accept a height; a dry sample cannot lift a body.
  if (!channel.wet)
    return {
      height: -Number.MAX_VALUE,
      normal: [0, 1, 0],
      velocity: [0, 0, 0],
      depth: 0,
      wet: false,
      shore: 0,
    };
  let height = water.level,
    dx = 0,
    dz = 0,
    dy = 0;
  for (const wave of resolveWaterWaves(water)) {
    const k = (2 * Math.PI) / wave.wavelength,
      c = Math.cos(wave.direction),
      s = Math.sin(wave.direction);
    const phase = k * (c * x + s * z - wave.speed * time) + wave.phase;
    const derivative = wave.amplitude * k * Math.cos(phase);
    height += wave.amplitude * Math.sin(phase);
    dx += derivative * c;
    dz += derivative * s;
    dy -= derivative * wave.speed;
  }
  let current = water.flow?.velocity ?? [0, 0];
  if (water.flow?.river) {
    const speed = Math.hypot(...current) * (1 - channel.shore);
    current = [channel.tangent[0] * speed, channel.tangent[1] * speed];
  }
  return {
    height,
    normal: normalize([-dx, 1, -dz]),
    velocity: [current[0], dy, current[1]],
    depth: channel.depth,
    wet: true,
    shore: channel.shore,
  };
}
/** A bounded channel mesh with authored widths and bank foam, in world coordinates.
 * Miter joins share boundary positions at bends; no procedural bank terrain is implied. */
export function riverWaterMesh(water: WaterDefinition, spacing = 1): MeshData | undefined {
  const river = water.flow?.river;
  if (!river) return;
  if (!Number.isFinite(spacing) || spacing <= 0) throw new Error("River spacing must be positive");
  const failure = reviewRiverChannel(river).find((diagnostic) => diagnostic.severity === "error");
  if (failure) throw new Error(failure.message);
  const sections = riverSections(river);
  const positions: number[] = [],
    normals: number[] = [],
    colors: number[] = [],
    indices: number[] = [];
  const min: Vec3 = [Infinity, water.level, Infinity],
    max: Vec3 = [-Infinity, water.level, -Infinity];
  for (let segment = 1; segment < river.points.length; segment++) {
    const a = river.points[segment - 1],
      b = river.points[segment];
    const dx = b.position[0] - a.position[0],
      dz = b.position[1] - a.position[1],
      length = Math.hypot(dx, dz);
    const columns = Math.min(64, Math.max(4, Math.ceil(Math.max(a.width, b.width) / spacing)));
    const rowBudget = Math.max(1, Math.floor(65536 / ((river.points.length - 1) * (columns + 1))) - 1);
    const rows = Math.min(512, rowBudget, Math.max(1, Math.ceil(length / spacing)));
    const start = positions.length / 3;
    for (let row = 0; row <= rows; row++) {
      const t = row / rows,
        width = a.width + (b.width - a.width) * t;
      for (let column = 0; column <= columns; column++) {
        const side = column / columns - 0.5;
        const startSection = sections[segment - 1],
          endSection = sections[segment];
        const across = column / columns;
        const leftX = startSection.left[0] + (endSection.left[0] - startSection.left[0]) * t;
        const leftZ = startSection.left[1] + (endSection.left[1] - startSection.left[1]) * t;
        const rightX = startSection.right[0] + (endSection.right[0] - startSection.right[0]) * t;
        const rightZ = startSection.right[1] + (endSection.right[1] - startSection.right[1]) * t;
        const x = leftX + (rightX - leftX) * across;
        const z = leftZ + (rightZ - leftZ) * across;
        const edge = (0.5 - Math.abs(side)) * width;
        const foam = river.foam * (1 - Math.min(1, edge / river.shoreWidth));
        positions.push(x, water.level, z);
        normals.push(0, 1, 0);
        colors.push(1 + foam * 2, 1 + foam * 2, 1 + foam * 2);
        min[0] = Math.min(min[0], x);
        min[2] = Math.min(min[2], z);
        max[0] = Math.max(max[0], x);
        max[2] = Math.max(max[2], z);
        if (row < rows && column < columns) {
          const p = start + row * (columns + 1) + column;
          indices.push(p, p + 1, p + columns + 1, p + 1, p + columns + 2, p + columns + 1);
        }
      }
    }
  }
  const mesh: MeshData = {
    positions: new Float32Array(positions),
    normals: new Float32Array(normals),
    colors: new Float32Array(colors),
    indices: new Uint32Array(indices),
    bounds: { min, max },
  };
  let realizedSpacing = 0;
  for (let triangle = 0; triangle < indices.length; triangle += 3) {
    for (let edge = 0; edge < 3; edge++) {
      const a = indices[triangle + edge] * 3,
        b = indices[triangle + ((edge + 1) % 3)] * 3;
      realizedSpacing = Math.max(
        realizedSpacing,
        Math.hypot(positions[a] - positions[b], positions[a + 2] - positions[b + 2]),
      );
    }
  }
  riverSpacing.set(mesh, realizedSpacing);
  return mesh;
}
