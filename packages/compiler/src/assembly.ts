import {
  ASSEMBLY_MAX_TRIANGLES,
  type AssemblyDefinition,
  type AssemblyPart,
  type AssemblyProfile,
  add,
  assemblyPartPoint,
  assemblyPathFrames,
  assemblySchema,
  type CompiledSurface,
  contentKey,
  cross,
  type Diagnostic,
  dot,
  type MeshData,
  normalize,
  type ObjectDefinition,
  type Quality,
  revolvedShellRings,
  scale,
  sub,
  type Vec3,
} from "@wrela/model";

import { assemblyModuleObstructs } from "./assembly-clearance";
import { productKeys } from "./products";
import { COMPILER_VERSION } from "./versions";

type Point2 = [number, number];
/** Beveling cuts profile corners without changing its outer dimensions. */
export function assemblyProfilePoints(
  profile: AssemblyProfile,
  bevel: number,
  cornerCuts?: number[],
): Point2[] {
  const points: Point2[] =
    profile.kind === "rectangle"
      ? [
          [-profile.width / 2, -profile.height / 2],
          [profile.width / 2, -profile.height / 2],
          [profile.width / 2, profile.height / 2],
          [-profile.width / 2, profile.height / 2],
        ]
      : profile.kind === "circle"
        ? Array.from({ length: profile.segments }, (_, i) => [
            Math.cos((i * Math.PI * 2) / profile.segments) * profile.radius,
            Math.sin((i * Math.PI * 2) / profile.segments) * profile.radius,
          ])
        : profile.points.map((p) => [...p]);
  const area = points.reduce((sum, a, i) => {
    const b = points[(i + 1) % points.length];
    return sum + a[0] * b[1] - a[1] * b[0];
  }, 0);
  if (area < 0) points.reverse();
  if (bevel <= 0 || profile.kind === "circle") return points;
  return points.flatMap((p, i) => {
    const prev = points[(i + points.length - 1) % points.length],
      next = points[(i + 1) % points.length];
    const a = Math.hypot(prev[0] - p[0], prev[1] - p[1]),
      b = Math.hypot(next[0] - p[0], next[1] - p[1]);
    const cut = Math.min(cornerCuts?.[i] ?? bevel, a * 0.45, b * 0.45);
    return [
      [p[0] + ((prev[0] - p[0]) * cut) / a, p[1] + ((prev[1] - p[1]) * cut) / a],
      [p[0] + ((next[0] - p[0]) * cut) / b, p[1] + ((next[1] - p[1]) * cut) / b],
    ] as Point2[];
  });
}
function sweepRings(part: AssemblyPart): Vec3[][] {
  const profile = assemblyProfilePoints(part.profile, part.bevel);
  const worn =
    !!part.edgeWear && part.bevel > 0 && part.profile.kind === "rectangle" && part.path.length === 2;
  const length = Math.hypot(...sub(part.path[1], part.path[0]));
  const count = worn ? Math.min(64, Math.max(2, Math.ceil(length / 0.08) + 1)) : part.path.length;
  const path = worn
    ? Array.from({ length: count }, (_, i) =>
        add(part.path[0], scale(sub(part.path[1], part.path[0]), i / (count - 1))),
      )
    : part.path;
  const frames = assemblyPathFrames({ ...part, path });
  const rings = frames.map((frame, i) => {
    const arc = (length * i) / (count - 1);
    // Recess only corners, leaving broad faces and socket positions intact.
    // End support is unchanged so cap transitions remain watertight.
    const cuts =
      worn && i > 0 && i < count - 1
        ? [0, 1, 2, 3].map((corner) => {
            const phase = part.wear.seed * 1.73 + corner * 13.17;
            const chip = Math.max(0, Math.sin(arc * 18 + phase) * Math.sin(arc * 7 + phase * 0.37) - 0.35);
            return part.bevel + (part.edgeWear ?? 0) * 0.012 * chip;
          })
        : undefined;
    const section = cuts ? assemblyProfilePoints(part.profile, part.bevel, cuts) : profile;
    return section.map((p) => add(frame.origin, add(scale(frame.axes[0], p[0]), scale(frame.axes[1], p[1]))));
  });
  if (!part.endBevel) return rings;
  const center: Point2 = [
    profile.reduce((sum, p) => sum + p[0], 0) / profile.length,
    profile.reduce((sum, p) => sum + p[1], 0) / profile.length,
  ];
  const inward = profile.map((p, i) => {
    const next = profile[(i + 1) % profile.length],
      length = Math.hypot(next[0] - p[0], next[1] - p[1]);
    return [-(next[1] - p[1]) / length, (next[0] - p[0]) / length] as Point2;
  });
  const radius = Math.min(
    ...profile.map((p, i) => (center[0] - p[0]) * inward[i][0] + (center[1] - p[1]) * inward[i][1]),
  );
  const bevel = Math.min(
    part.endBevel,
    radius * 0.45,
    Math.hypot(...sub(part.path[1], part.path[0])) * 0.2,
    Math.hypot(...sub(part.path.at(-1) as Vec3, part.path.at(-2) as Vec3)) * 0.2,
  );
  const inset = profile.map((p, i) => {
    const a = inward[(i + profile.length - 1) % profile.length],
      b = inward[i],
      factor = bevel / Math.max(1e-6, 1 + a[0] * b[0] + a[1] * b[1]);
    return [p[0] + (a[0] + b[0]) * factor, p[1] + (a[1] + b[1]) * factor] as Point2;
  });
  const cap = (i: number) =>
    inset.map((p) =>
      add(frames[i].origin, add(scale(frames[i].axes[0], p[0]), scale(frames[i].axes[1], p[1]))),
    );
  const first = rings[0].map((p) => add(p, scale(frames[0].axes[2], bevel))),
    last = rings.at(-1)?.map((p) => add(p, scale(frames.at(-1)?.axes[2] as Vec3, -bevel))) ?? [];
  return [cap(0), first, ...rings.slice(1, -1), last, cap(frames.length - 1)];
}
/** Geometry is generated directly, preserving dimensions independently of field resolution. */
export function compileAssemblyMesh(
  source: AssemblyDefinition,
  fallbackMaterial: string,
  values: Readonly<Record<string, number>> = {},
): { mesh: MeshData; diagnostics: Diagnostic[] } {
  const assembly = assemblySchema.parse(source);
  const positions: number[] = [],
    normals: number[] = [],
    indices: number[] = [],
    colors: number[] = [],
    sourceIds: string[] = [],
    materialCoordinates: number[] = [];
  const materialGroups: NonNullable<MeshData["materialGroups"]> = [],
    diagnostics: Diagnostic[] = [];
  const min: Vec3 = [Infinity, Infinity, Infinity],
    max: Vec3 = [-Infinity, -Infinity, -Infinity];
  const wearFrames = new Map(
    assembly.parts.map((part) => {
      const origin = assemblyPartPoint(assembly, part, [0, 0, 0], values);
      const axes = [0, 1, 2].map((axis) => {
        const p: Vec3 = [0, 0, 0];
        p[axis] = 1;
        return sub(assemblyPartPoint(assembly, part, p, values), origin);
      });
      return [part.id, { origin, axes, pathFrames: assemblyPathFrames(part) }] as const;
    }),
  );
  const triangle = (a: Vec3, b: Vec3, c: Vec3, part: AssemblyPart, repeat: number, smooth?: Vec3[]) => {
    if (indices.length >= ASSEMBLY_MAX_TRIANGLES * 3)
      throw new Error("Assembly exceeds 80,000 triangle budget; reduce repeats or sweep subdivisions");
    const normal = normalize(cross(sub(b, a), sub(c, a)));
    for (const [corner, p] of [a, b, c].entries()) {
      indices.push(positions.length / 3);
      positions.push(...p);
      normals.push(...(smooth?.[corner] ?? normal));
      sourceIds.push(part.id);
      // Deterministic local wear remains attached to its articulated part.
      const frame = wearFrames.get(part.id);
      if (!frame) throw new Error("Missing assembly wear frame");
      const local = frame.axes.map((axis) => dot(axis, sub(p, frame.origin)));
      // The existing rigid material channel carries the sweep frame, with Y
      // measured along the member. End faces naturally cut across growth rings.
      const rest = local.map((v, axis) => v - part.repeat.offset[axis] * repeat) as Vec3;
      const frames = frame.pathFrames;
      let nearest = Infinity,
        arc = 0,
        coordinates: Vec3 = [0, 0, 0];
      for (let segment = 0; segment + 1 < part.path.length; segment++) {
        const delta = sub(part.path[segment + 1], part.path[segment]);
        const length = Math.hypot(...delta),
          tangent = scale(delta, 1 / length);
        const t = Math.max(0, Math.min(length, dot(sub(rest, part.path[segment]), tangent)));
        const radial = sub(rest, add(part.path[segment], scale(tangent, t)));
        const distance = dot(radial, radial);
        if (distance < nearest) {
          nearest = distance;
          coordinates = [dot(radial, frames[segment].axes[0]), arc + t, dot(radial, frames[segment].axes[1])];
        }
        arc += length;
      }
      materialCoordinates.push(...coordinates.map((v, axis) => v + (part.grainOffset?.[axis] ?? 0)));
      const noise =
        Math.sin(
          (local[0] * 12.9898 + local[1] * 78.233 + local[2] * 37.719 + part.wear.seed) * part.wear.scale,
        ) * 43758.5453;
      const shade = 1 - part.wear.amount * (0.15 + (noise - Math.floor(noise)) * 0.45);
      colors.push(shade, shade, shade);
      for (let axis = 0; axis < 3; axis++) {
        min[axis] = Math.min(min[axis], p[axis]);
        max[axis] = Math.max(max[axis], p[axis]);
      }
    }
  };
  for (const part of assembly.parts) {
    const rings = part.shell ? revolvedShellRings(part.shell) : sweepRings(part),
      start = indices.length;
    for (let repeat = 0; repeat < part.repeat.count; repeat++) {
      const moduleStart = positions.length;
      const transformed = rings.map((ring) =>
        ring.map((p) => assemblyPartPoint(assembly, part, p, values, repeat)),
      );
      const partMin: Vec3 = [Infinity, Infinity, Infinity],
        partMax: Vec3 = [-Infinity, -Infinity, -Infinity];
      for (const ring of transformed)
        for (const p of ring)
          for (let axis = 0; axis < 3; axis++) {
            partMin[axis] = Math.min(partMin[axis], p[axis]);
            partMax[axis] = Math.max(partMax[axis], p[axis]);
          }
      for (let ring = 0; ring < transformed.length - (part.shell ? 0 : 1); ring++)
        for (let i = 0; i < transformed[ring].length; i++) {
          const next = (i + 1) % transformed[ring].length;
          const a = transformed[ring][i],
            b = transformed[ring][next],
            c = transformed[(ring + 1) % transformed.length][next],
            d = transformed[(ring + 1) % transformed.length][i];
          if (part.shell) {
            const profileNormal = (edge: number): [number, number] => {
              const p = rings[(edge + rings.length) % rings.length][0],
                q = rings[(edge + 1 + rings.length) % rings.length][0];
              const dr = q[0] - p[0],
                dy = q[1] - p[1],
                length = Math.hypot(dr, dy);
              return [dy / length, -dr / length];
            };
            const face = profileNormal(ring);
            const vertexNormal = (profile: number, angleIndex: number, neighbor: number): Vec3 => {
              const other = profileNormal(neighbor),
                blend = face[0] * other[0] + face[1] * other[1] > 0.75;
              const radial = blend ? face[0] + other[0] : face[0],
                vertical = blend ? face[1] + other[1] : face[1];
              const point = rings[profile][angleIndex],
                radius = Math.hypot(point[0], point[2]);
              const local = normalize([(radial * point[0]) / radius, vertical, (radial * point[2]) / radius]);
              const frame = wearFrames.get(part.id)!;
              return normalize(
                add(
                  add(scale(frame.axes[0], local[0]), scale(frame.axes[1], local[1])),
                  scale(frame.axes[2], local[2]),
                ),
              );
            };
            const upper = (ring + 1) % rings.length,
              na = vertexNormal(ring, i, ring - 1),
              nb = vertexNormal(ring, next, ring - 1),
              nc = vertexNormal(upper, next, ring + 1),
              nd = vertexNormal(upper, i, ring + 1);
            triangle(a, b, c, part, repeat, [na, nb, nc]);
            triangle(a, c, d, part, repeat, [na, nc, nd]);
          } else {
            triangle(a, b, c, part, repeat);
            triangle(a, c, d, part, repeat);
          }
        }
      for (const [ring, reverse] of part.shell
        ? []
        : ([
            [transformed[0], true],
            [transformed[transformed.length - 1], false],
          ] as const)) {
        const center = scale(
          ring.reduce((sum, p) => add(sum, p), [0, 0, 0] as Vec3),
          1 / ring.length,
        );
        for (let i = 0; i < ring.length; i++) {
          const next = (i + 1) % ring.length;
          triangle(center, ring[reverse ? next : i], ring[reverse ? i : next], part, repeat);
        }
      }
      for (const clearance of assembly.clearances)
        if (
          partMin.every(
            (v, axis) => v < clearance.max[axis] - 1e-6 && partMax[axis] > clearance.min[axis] + 1e-6,
          ) &&
          assemblyModuleObstructs(positions, moduleStart, positions.length, clearance)
        )
          diagnostics.push({
            severity: "warning",
            code: "assembly.clearance-overlap",
            node: part.id,
            message: `${part.name} module ${repeat + 1} obstructs ${clearance.name} in the evaluated pose; its closed sweep intersects the reserved volume.`,
          });
    }
    materialGroups.push({
      material: part.material ?? fallbackMaterial,
      start,
      count: indices.length - start,
    });
  }
  return {
    mesh: {
      positions: new Float32Array(positions),
      normals: new Float32Array(normals),
      indices: new Uint32Array(indices),
      colors: new Float32Array(colors),
      sourceIds,
      materialCoordinates: new Float32Array(materialCoordinates),
      materialGroups,
      bounds: { min, max },
    },
    diagnostics,
  };
}
export function compileAssembly(doc: ObjectDefinition, quality: Quality = "review"): CompiledSurface {
  if (!doc.assembly) throw new Error("Object has no assembly source");
  const compiled = compileAssemblyMesh(doc.assembly, doc.material);
  return {
    kind: "surface",
    id: doc.id,
    key: contentKey({
      compiler: COMPILER_VERSION,
      id: doc.id,
      kind: doc.kind,
      quality,
      ...productKeys(doc, quality),
    }),
    material: doc.material,
    mesh: compiled.mesh,
    diagnostics: compiled.diagnostics.map((d) => ({ ...d, document: doc.id })),
  };
}
