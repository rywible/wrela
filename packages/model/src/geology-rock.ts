import { z } from "zod";
import type { FieldDefinition, FieldNode } from "./documents";
import { coordinateHash, type Vec3 } from "./math";

/** Bedding is a shared geological frame, not a stack of separate solids. */
export const geologicalRockSchema = z.object({
  seed: z.number().int().min(0).max(65_535),
  layers: z.number().int().min(2).max(8),
  fracture: z.number().finite().min(0).max(1),
  bedding: z
    .object({
      dip: z.number().finite().min(-0.8).max(0.8),
      strike: z.number().finite().min(-Math.PI).max(Math.PI),
    })
    .optional(),
  /** Physical span of terminating joints; tiny features are filtered to the extraction grid. */
  fractureScale: z.number().finite().min(0.1).max(40).optional(),
});
export type GeologicalRock = z.infer<typeof geologicalRockSchema>;

function shape(
  id: string,
  kind: FieldNode["kind"],
  position: Vec3,
  size: Vec3,
  rotation: Vec3 = [0, 0, 0],
): FieldNode {
  return { id, name: id, kind, position, size, rotation, radius: 1, blend: 0, children: [] };
}

/** Connected core, attached buttresses and terminating fracture scars. All
 * pieces overlap the core; bedding cuts terminate near the exposed surface, so
 * neither strata nor weathering can split the mass into floating horizontal rings.
 * Geometry carries broad structure; the material owns sub-grid mineral detail. */
export function buildGeologicalRockNodes(
  size: Vec3,
  source: GeologicalRock,
  minimumFeatureSize = 0,
  protectedRegion?: Pick<FieldDefinition, "root" | "nodes">,
): FieldNode[] {
  const [width, height, depth] = size;
  const random = (index: number, channel: number) =>
    coordinateHash(index, channel, source.seed) / 4_294_967_295;
  const dip = source.bedding?.dip ?? 0.19;
  const strike = source.bedding?.strike ?? -0.28;
  const rotation: Vec3 = [dip * Math.sin(strike), 0, dip * Math.cos(strike)];
  const nodes: FieldNode[] = [];
  const core = shape(
    "geology-core",
    "rock",
    [0, height * 0.37, 0],
    [width * 0.57, height * 0.7, depth * 0.61],
  );
  nodes.push(core);
  const pieces = [core.id];
  for (let index = 0; index < 4; index++) {
    const side = index % 2 ? 1 : -1;
    const upper = index >= 2;
    const piece = shape(
      `geology-buttress-${index}`,
      "rock",
      [
        side * width * (upper ? 0.16 : 0.29),
        height * (upper ? 0.62 : 0.23) + (random(index, 0) - 0.5) * height * 0.08,
        (random(index, 1) - 0.5) * depth * 0.22,
      ],
      [width * (upper ? 0.28 : 0.29), height * (upper ? 0.4 : 0.43), depth * (upper ? 0.4 : 0.48)],
      [rotation[0], (random(index, 2) - 0.5) * 0.18, rotation[2]],
    );
    nodes.push(piece);
    pieces.push(piece.id);
  }
  const mass = shape("geology-mass", "union", [0, 0, 0], [1, 1, 1]);
  mass.children = pieces;
  nodes.push(mass);
  const fractures: string[] = [];
  // Localized, staggered scars follow one inclined bedding family. Their inner
  // tips never approach the core: a fracture exposes a face, not an entire seam.
  const spacing = height / source.layers;
  const jointScale = source.fractureScale ?? width * 0.24;
  const count = source.layers;
  if (source.fracture > 0)
    for (let index = 0; index < count; index++) {
      const front = index % 2 ? 1 : -1;
      const x = (random(index, 4) - 0.5) * width * 0.64;
      const y = height * (0.18 + ((index + random(index, 5) * 0.45) / count) * 0.66);
      const penetration = Math.min(
        depth * 0.12,
        Math.max(minimumFeatureSize, depth * 0.085 * source.fracture),
      );
      const scar = shape(
        `geology-bedding-scar-${index}`,
        "ellipsoid",
        // The cutter center is beyond the full exterior bound and its far end
        // remains outside it. This opens every recess to the exterior instead
        // of trapping a thin shell between two near-tangent noisy rock fields.
        [x, y, front * (depth * 0.5 + penetration * 1.5)],
        [
          Math.min(width * 0.4, Math.max(minimumFeatureSize, jointScale * (0.8 + random(index, 6) * 0.7))),
          Math.max(minimumFeatureSize, spacing * (0.16 + random(index, 7) * 0.09)),
          penetration * 2.5,
        ],
        [0, 0, rotation[2]],
      );
      nodes.push(scar);
      fractures.push(scar.id);
    }
  let body = mass.id;
  if (fractures.length) {
    const scars = shape("geology-fracture-network", "union", [0, 0, 0], [1, 1, 1]);
    scars.children = fractures;
    const carved = shape("geology-weathered-mass", "subtract", [0, 0, 0], [1, 1, 1]);
    let exposedScars = scars.id;
    nodes.push(scars);
    if (protectedRegion) {
      const safeScars = shape("geology-exposed-fractures", "subtract", [0, 0, 0], [1, 1, 1]);
      safeScars.children = [scars.id, protectedRegion.root];
      nodes.push(...protectedRegion.nodes, safeScars);
      exposedScars = safeScars.id;
    }
    carved.children = [mass.id, exposedScars];
    nodes.push(carved);
    body = carved.id;
  }
  const envelope = shape("geology-envelope", "box", [0, height / 2, 0], [width / 2, height / 2, depth / 2]);
  const bounded = shape("geology-bedrock", "intersect", [0, 0, 0], [1, 1, 1]);
  bounded.children = [body, envelope.id];
  nodes.push(envelope, bounded);
  return nodes;
}

/** Reusable above-ground bedrock asset. Formation authoring reuses these exact
 * nodes before subtracting a cave or shelter; render and collision share them. */
export function buildGeologicalRockField(size: Vec3, rock: GeologicalRock, resolution = 56): FieldDefinition {
  const spacing = Math.max(...size) / resolution;
  const nodes = buildGeologicalRockNodes(size, rock, spacing * 1.5);
  return {
    root: nodes[nodes.length - 1].id,
    nodes,
    resolution,
    bounds: {
      min: [-size[0] / 2 - spacing, -spacing, -size[2] / 2 - spacing],
      max: [size[0] / 2 + spacing, size[1] + spacing, size[2] / 2 + spacing],
    },
  };
}
