import type { IndirectLighting } from "@wrela/compiler/indirect-query";
import {
  type Camera,
  cross,
  identityMatrix,
  type MeshData,
  normalize,
  type RenderSurface,
  sub,
  type Vec3,
} from "@wrela/model";

export type IndirectSceneFixture = {
  name: string;
  surfaces: RenderSurface[];
  camera: Camera;
  lighting: IndirectLighting;
};

function mesh(positions: Vec3[], indices: number[]): MeshData {
  // Faceted geometric normals match the transport reference; smoothing normals
  // would change the BRDF and confound the lighting-cache comparison.
  const expanded: number[] = [],
    normals: number[] = [];
  for (let i = 0; i < indices.length; i += 3) {
    const a = positions[indices[i]],
      b = positions[indices[i + 1]],
      c = positions[indices[i + 2]];
    const normal = normalize(cross(sub(b, a), sub(c, a)));
    expanded.push(...a, ...b, ...c);
    normals.push(...normal, ...normal, ...normal);
  }
  return {
    positions: new Float32Array(expanded),
    normals: new Float32Array(normals),
    indices: Uint32Array.from({ length: indices.length }, (_, i) => i),
    bounds: {
      min: [0, 1, 2].map((axis) => Math.min(...positions.map((p) => p[axis]))) as Vec3,
      max: [0, 1, 2].map((axis) => Math.max(...positions.map((p) => p[axis]))) as Vec3,
    },
  };
}
function surface(id: string, geometry: MeshData, color: Vec3): RenderSurface {
  return {
    id,
    source: id,
    mesh: geometry,
    matrix: identityMatrix(),
    material: { color, secondary: color, roughness: 1, metallic: 0, pattern: 0, scale: 1, normalStrength: 0 },
  };
}
export function quad(id: string, points: [Vec3, Vec3, Vec3, Vec3], color: Vec3): RenderSurface {
  return surface(id, mesh(points, [0, 1, 2, 0, 2, 3]), color);
}
export function box(id: string, min: Vec3, max: Vec3, color: Vec3): RenderSurface {
  const [x, y, z] = min,
    [X, Y, Z] = max;
  return surface(
    id,
    mesh(
      [
        [x, y, z],
        [X, y, z],
        [X, Y, z],
        [x, Y, z],
        [x, y, Z],
        [X, y, Z],
        [X, Y, Z],
        [x, Y, Z],
      ],
      [
        0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7, 3, 1, 2, 6, 1,
        6, 5,
      ],
    ),
    color,
  );
}

/** Simple diffuse reference: an open front and roof, colored walls, and optional blocker. */
export function indirectBoxFixture(
  options: { ceiling?: boolean; front?: boolean; occluder?: boolean; coloredWalls?: boolean } = {},
): IndirectSceneFixture {
  const white: Vec3 = [0.7, 0.7, 0.7];
  const surfaces = [
    quad(
      "floor",
      [
        [-1, 0, -1],
        [-1, 0, 1],
        [1, 0, 1],
        [1, 0, -1],
      ],
      white,
    ),
    quad(
      "left-wall",
      [
        [-1, 0, 1],
        [-1, 2, 1],
        [-1, 2, -1],
        [-1, 0, -1],
      ],
      options.coloredWalls === false ? white : [0.75, 0.025, 0.015],
    ),
    quad(
      "right-wall",
      [
        [1, 0, -1],
        [1, 2, -1],
        [1, 2, 1],
        [1, 0, 1],
      ],
      options.coloredWalls === false ? white : [0.02, 0.5, 0.055],
    ),
    quad(
      "back-wall",
      [
        [-1, 0, -1],
        [-1, 2, -1],
        [1, 2, -1],
        [1, 0, -1],
      ],
      white,
    ),
  ];
  if (options.ceiling)
    surfaces.push(
      quad(
        "ceiling",
        [
          [-1, 2, -1],
          [-1, 2, 1],
          [1, 2, 1],
          [1, 2, -1],
        ],
        white,
      ),
    );
  if (options.front)
    surfaces.push(
      quad(
        "front-wall",
        [
          [-1, 0, 1],
          [1, 0, 1],
          [1, 2, 1],
          [-1, 2, 1],
        ],
        white,
      ),
    );
  if (options.occluder !== false)
    surfaces.push(box("neutral-block", [-0.25, 0, -0.35], [0.35, 0.85, 0.3], [0.6, 0.6, 0.6]));
  return {
    name: "diffuse-open-box",
    surfaces,
    camera: { position: [0, 1.15, 3.6], target: [0, 0.85, 0], fov: 44 },
    lighting: {
      sunDirection: normalize([0.5, 1, 0.25]),
      sunRadiance: [2.5, 2.3, 2],
      skyRadiance: [0.12, 0.16, 0.22],
    },
  };
}

function rock(id: string, center: Vec3, radii: Vec3, phase: number, color: Vec3): RenderSurface {
  const positions: Vec3[] = [[center[0], center[1] - radii[1], center[2]]];
  const segments = 9;
  for (let ring = 0; ring < 3; ring++) {
    const latitude = ((ring + 1) * Math.PI) / 4;
    for (let segment = 0; segment < segments; segment++) {
      const angle = (segment * Math.PI * 2) / segments + phase + ring * 0.17;
      const wobble = 1 + 0.12 * Math.sin(segment * 3.7 + ring * 1.2 + phase);
      positions.push([
        center[0] + Math.cos(angle) * Math.sin(latitude) * radii[0] * wobble,
        center[1] - Math.cos(latitude) * radii[1],
        center[2] + Math.sin(angle) * Math.sin(latitude) * radii[2] * wobble,
      ]);
    }
  }
  const top = positions.length;
  positions.push([center[0] + 0.12 * radii[0], center[1] + radii[1], center[2]]);
  const indices: number[] = [];
  for (let segment = 0; segment < segments; segment++) {
    const next = (segment + 1) % segments;
    indices.push(0, 1 + segment, 1 + next);
    for (let ring = 0; ring < 2; ring++) {
      const a = 1 + ring * segments + segment,
        b = 1 + ring * segments + next;
      indices.push(a, a + segments, b, b, a + segments, b + segments);
    }
    indices.push(1 + 2 * segments + segment, top, 1 + 2 * segments + next);
  }
  return surface(id, mesh(positions, indices), color);
}

/** A small static rock/ground patch; no claim to capture foliage or a full atmosphere. */
export function indirectAlpineFixture(): IndirectSceneFixture {
  const positions: Vec3[] = [],
    indices: number[] = [],
    divisions = 16;
  const height = (x: number, z: number) =>
    0.1 * Math.sin(x * 0.45) + 0.08 * Math.cos(z * 0.7) + 0.06 * Math.sin((x + z) * 0.8);
  for (let z = 0; z <= divisions; z++)
    for (let x = 0; x <= divisions; x++) {
      const px = (x / divisions) * 12 - 6,
        pz = (z / divisions) * 12 - 6;
      positions.push([px, height(px, pz), pz]);
    }
  for (let z = 0; z < divisions; z++)
    for (let x = 0; x < divisions; x++) {
      const a = z * (divisions + 1) + x,
        b = a + divisions + 1;
      indices.push(a, b, a + 1, a + 1, b, b + 1);
    }
  return {
    name: "diffuse-alpine-rock-ground",
    surfaces: [
      surface("alpine-ground", mesh(positions, indices), [0.22, 0.27, 0.12]),
      rock("large-cool-rock", [-0.65, 0.8, -0.4], [1.3, 1.25, 0.95], 0.4, [0.4, 0.43, 0.47]),
      rock("warm-rock", [1.25, 0.37, 0.35], [0.8, 0.65, 0.65], 1.3, [0.5, 0.38, 0.25]),
      rock("small-rock", [-1.2, 0.19, 1.1], [0.48, 0.38, 0.4], 2, [0.3, 0.32, 0.35]),
    ],
    camera: { position: [4, 2.65, 5.5], target: [0, 0.55, 0], fov: 43 },
    lighting: {
      sunDirection: normalize([-0.7, 0.65, 0.4]),
      sunRadiance: [3.1, 2.7, 2.15],
      skyRadiance: [0.18, 0.25, 0.38],
    },
  };
}
