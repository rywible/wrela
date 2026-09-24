import {
  add,
  type Bounds,
  type CompiledVegetation,
  contentKey,
  cross,
  type MeshData,
  normalize,
  type Quality,
  random01,
  scale,
  sub,
  thinCoverageBytes,
  type Vec3,
  type VegetationDefinition,
  vegetationMotionEnvelope,
  vegetationWindResponse,
} from "@wrela/model";

import { botanicalMeshes } from "./botanical-mesh";
import { ProductCache } from "./cache";
import { compileShootInstances } from "./shoot-instances";

export { type BotanicalBranch, type BotanicalStructure, botanicalStructure } from "./botanical-structure";

import { geometryKey, materialBindingKey } from "./products";
import { COMPILER_VERSION } from "./surface";

type Builder = {
  positions: number[];
  normals: number[];
  indices: number[];
  colors: number[];
  sourceIds: string[];
};
const builder = (): Builder => ({ positions: [], normals: [], indices: [], colors: [], sourceIds: [] });
function tube(
  mesh: Builder,
  a: Vec3,
  b: Vec3,
  ra: number,
  rb: number,
  sides: number,
  source: string,
  color: Vec3,
): void {
  const axis = normalize(sub(b, a)),
    u = normalize(cross(axis, Math.abs(axis[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0])),
    v = cross(axis, u),
    start = mesh.positions.length / 3,
    slope = (ra - rb) / (Math.hypot(...sub(b, a)) || 1);
  for (let end = 0; end < 2; end++)
    for (let i = 0; i < sides; i++) {
      const angle = (i / sides) * Math.PI * 2,
        radial = add(scale(u, Math.cos(angle)), scale(v, Math.sin(angle))),
        p = add(end ? b : a, scale(radial, end ? rb : ra));
      mesh.positions.push(...p);
      mesh.normals.push(...normalize(add(radial, scale(axis, slope))));
      mesh.colors.push(...color);
      mesh.sourceIds.push(source);
    }
  for (let i = 0; i < sides; i++) {
    const j = (i + 1) % sides;
    mesh.indices.push(
      start + i,
      start + j,
      start + sides + i,
      start + j,
      start + sides + j,
      start + sides + i,
    );
  }
  // Flat caps own vertices so trunk ends retain a flat normal.
  for (let end = 0; end < 2; end++) {
    const offset = mesh.positions.length / 3,
      p = end ? b : a,
      n = scale(axis, end ? 1 : -1);
    mesh.positions.push(...p);
    mesh.normals.push(...n);
    mesh.colors.push(...color);
    mesh.sourceIds.push(source);
    for (let i = 0; i < sides; i++) {
      const angle = (i / sides) * Math.PI * 2,
        radial = add(scale(u, Math.cos(angle)), scale(v, Math.sin(angle)));
      mesh.positions.push(...add(p, scale(radial, end ? rb : ra)));
      mesh.normals.push(...n);
      mesh.colors.push(...color);
      mesh.sourceIds.push(source);
    }
    for (let i = 0; i < sides; i++) {
      const j = (i + 1) % sides;
      mesh.indices.push(offset, offset + 1 + (end ? i : j), offset + 1 + (end ? j : i));
    }
  }
}
/** Overlapping tapered whorls create a continuous conifer silhouette. Lobed
 * skirts and displaced tips preserve procedural variation at every quality. */
function foliage(
  mesh: Builder,
  center: Vec3,
  radius: number,
  height: number,
  sides: number,
  rings: number,
  source: string,
  color: Vec3,
  phase: number,
): void {
  const start = mesh.positions.length / 3;
  for (let j = 0; j <= rings; j++) {
    const t = j / rings,
      profile = (1 - t) ** 1.05;
    for (let i = 0; i <= sides; i++) {
      const phi = (i / sides) * Math.PI * 2 + phase;
      const lobe = 1 + 0.1 * Math.sin(phi * 5 + phase) + 0.045 * Math.sin(phi * 9 - phase);
      const r = radius * profile * lobe;
      const y =
        center[1] + height * t - (1 - t) ** 4 * height * 0.065 * (0.5 + 0.5 * Math.sin(phi * 5 + phase));
      mesh.positions.push(
        center[0] + Math.cos(phi) * r + t * height * 0.045 * Math.cos(phase),
        y,
        center[2] + Math.sin(phi) * r + t * height * 0.045 * Math.sin(phase),
      );
      mesh.normals.push(...normalize([Math.cos(phi), (radius / height) * 1.05, Math.sin(phi)]));
      const tint = 0.82 + 0.18 * t;
      mesh.colors.push(color[0] * tint, color[1] * tint, color[2] * tint);
      mesh.sourceIds.push(source);
    }
  }
  for (let j = 0; j < rings; j++)
    for (let i = 0; i < sides; i++) {
      const a = start + j * (sides + 1) + i,
        b = a + 1,
        c = a + sides + 1,
        d = c + 1;
      mesh.indices.push(a, c, b);
      if (j < rings - 1) mesh.indices.push(b, c, d);
    }
  const cap = mesh.positions.length / 3;
  mesh.positions.push(...center);
  mesh.normals.push(0, -1, 0);
  mesh.colors.push(...color.map((v) => v * 0.7));
  mesh.sourceIds.push(source);
  for (let i = 0; i < sides; i++) {
    const a = start + i,
      b = a + 1;
    const va = mesh.positions.slice(a * 3, a * 3 + 3),
      vb = mesh.positions.slice(b * 3, b * 3 + 3),
      first = mesh.positions.length / 3;
    mesh.positions.push(...va, ...vb);
    mesh.normals.push(0, -1, 0, 0, -1, 0);
    mesh.colors.push(...color.map((v) => v * 0.7), ...color.map((v) => v * 0.7));
    mesh.sourceIds.push(source, source);
    mesh.indices.push(cap, first, first + 1);
  }
}
function finish(b: Builder): MeshData {
  const bounds: Bounds = { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] };
  for (let i = 0; i < b.positions.length; i++) {
    const axis = i % 3;
    bounds.min[axis] = Math.min(bounds.min[axis], b.positions[i]);
    bounds.max[axis] = Math.max(bounds.max[axis], b.positions[i]);
  }
  return {
    positions: new Float32Array(b.positions),
    normals: new Float32Array(b.normals),
    indices: new Uint32Array(b.indices),
    colors: new Float32Array(b.colors),
    sourceIds: b.sourceIds,
    bounds,
  };
}
export function vegetationGeometrySource(doc: VegetationDefinition, quality: Quality): unknown {
  return {
    compiler: COMPILER_VERSION,
    id: doc.id,
    quality,
    geometry: geometryKey(doc, quality),
    material: materialBindingKey(doc),
  };
}
const vegetationProducts = new ProductCache<CompiledVegetation>(64 * 1024 * 1024);
export function compileVegetation(
  doc: VegetationDefinition,
  quality: Quality = "review",
): CompiledVegetation {
  const geometry = geometryKey(doc, quality);
  let product = vegetationProducts.get(geometry);
  if (!product) {
    product = buildVegetation(doc, quality);
    const distant = buildVegetation(doc, quality, true);
    product.maxDisplacement = Math.max(product.maxDisplacement, distant.maxDisplacement);
    product.surfaces = product.surfaces.map((surface) => {
      const coarse = distant.surfaces.find((candidate) => candidate.id === surface.id)?.mesh;
      if (!coarse) return surface;
      if (coarse.indices.length >= surface.mesh.indices.length) return surface;
      const bounds: Bounds = {
        min: surface.mesh.bounds.min.map((value, axis) => Math.min(value, coarse.bounds.min[axis])) as Vec3,
        max: surface.mesh.bounds.max.map((value, axis) => Math.max(value, coarse.bounds.max[axis])) as Vec3,
      };
      return {
        ...surface,
        mesh: { ...surface.mesh, bounds },
        details: [{ label: "distant-vegetation", mesh: coarse, maxProjectedDiameter: 96, maxError: null }],
      };
    });
    product.bounds = {
      min: product.bounds.min.map((value, axis) => Math.min(value, distant.bounds.min[axis])) as Vec3,
      max: product.bounds.max.map((value, axis) => Math.max(value, distant.bounds.max[axis])) as Vec3,
    };
    const shared = doc.botanical?.conifer?.architecture?.sharedShoots
      ? compileShootInstances(doc)
      : undefined;
    if (shared) product.surfaces = [...product.surfaces.filter((s) => !s.id.endsWith("-foliage")), ...shared];
    vegetationProducts.set(
      geometry,
      product,
      product.surfaces.reduce((sum, surface) => {
        return (
          sum +
          [surface.mesh, ...(surface.details ?? []).map((detail) => detail.mesh)].reduce(
            (bytes, mesh) =>
              bytes +
              mesh.positions.byteLength +
              mesh.normals.byteLength +
              mesh.indices.byteLength +
              (mesh.colors?.byteLength ?? 0) +
              (mesh.wind?.byteLength ?? 0) +
              (mesh.shoots
                ? (mesh.shoots.skyVisibility?.byteLength ?? 0) +
                  mesh.shoots.transforms.byteLength +
                  mesh.shoots.anchors.byteLength +
                  mesh.shoots.motion.byteLength +
                  mesh.shoots.sourceIds.reduce((n, id) => n + id.length * 2 + 8, 0)
                : 0) +
              (mesh.materialCoordinates?.byteLength ?? 0) +
              thinCoverageBytes(mesh.thinCoverage) +
              (mesh.sourceIds?.reduce((metadata, id) => metadata + id.length * 2 + 8, 0) ?? 0),
            0,
          )
        );
      }, 0),
    );
  }
  const key = contentKey(vegetationGeometrySource(doc, quality));
  return {
    ...product,
    id: doc.id,
    key,
    windResponse: vegetationWindResponse(doc),
    surfaces: product.surfaces.map((surface) => {
      const foliage = surface.id.endsWith("-foliage");
      return {
        ...surface,
        id: doc.id + surface.id.slice(product.id.length),
        key: key + surface.id.slice(product.id.length),
        material: foliage ? doc.material : doc.trunkMaterial,
        mesh:
          product.id === doc.id
            ? surface.mesh
            : {
                ...surface.mesh,
                sourceIds: surface.mesh.sourceIds?.map((id) => doc.id + id.slice(product.id.length)),
                shoots: surface.mesh.shoots
                  ? {
                      ...surface.mesh.shoots,
                      sourceIds: surface.mesh.shoots.sourceIds.map(
                        (id) => doc.id + id.slice(product.id.length),
                      ),
                    }
                  : undefined,
              },
        details:
          product.id === doc.id
            ? surface.details
            : surface.details?.map((detail) => ({
                ...detail,
                mesh: {
                  ...detail.mesh,
                  sourceIds: detail.mesh.sourceIds?.map((id) => doc.id + id.slice(product.id.length)),
                  shoots: detail.mesh.shoots
                    ? {
                        ...detail.mesh.shoots,
                        sourceIds: detail.mesh.shoots.sourceIds.map(
                          (id) => doc.id + id.slice(product.id.length),
                        ),
                      }
                    : undefined,
                },
              })),
      };
    }),
  };
}
function buildVegetation(
  doc: VegetationDefinition,
  quality: Quality = "review",
  distant = false,
): CompiledVegetation {
  if (doc.botanical) return buildBotanicalVegetation(doc, quality, distant);
  const trunk = builder(),
    leaves = builder(),
    sides = distant ? 6 : quality === "interactive" ? 6 : quality === "review" ? 10 : 14,
    rings = distant ? 2 : quality === "interactive" ? 4 : quality === "review" ? 6 : 10,
    seed = doc.seed;
  const random = (id: number) => random01(seed + id * 7919),
    height = doc.height * (1 + (random(0) - 0.5) * doc.variation * 0.3),
    trunkRadius = Math.max(0.04, height * 0.028),
    lean: Vec3 = [
      (random(1) - 0.5) * height * doc.variation * 0.12,
      height,
      (random(2) - 0.5) * height * doc.variation * 0.12,
    ];
  tube(trunk, [0, 0, 0], lean, trunkRadius, trunkRadius * 0.14, sides, doc.id, [1, 1, 1]);
  for (let i = 0; i < doc.branches; i++) {
    const t = 0.22 + ((i + 0.5) / doc.branches) * 0.7,
      angle = i * 2.399963229728653 + random(i + 3) * doc.variation * 0.6,
      length = doc.radius * (1 - t) * 1.5 * (0.85 + random(i + 100) * 0.3),
      origin = scale(lean, t),
      end: Vec3 = [
        origin[0] + Math.cos(angle) * length,
        origin[1] + height * 0.08 * (0.5 + random(i + 200)),
        origin[2] + Math.sin(angle) * length,
      ];
    tube(trunk, origin, end, trunkRadius * (1 - t) * 0.55, 0.012, sides, doc.id, [0.85, 0.85, 0.85]);
    const center = add(origin, [
      Math.cos(angle) * length * 0.12,
      -height * 0.035,
      Math.sin(angle) * length * 0.12,
    ]);
    const radius = doc.radius * (1 - t) ** 0.8 * (1.05 + random(i + 400) * 0.18);
    const crownHeight = height * (0.18 + (1 - t) * 0.08);
    const shade = 0.86 + random(i + 300) * 0.14;
    foliage(leaves, center, radius, crownHeight, sides * 2, rings, doc.id, [shade, shade, shade], angle);
  }
  foliage(
    leaves,
    scale(lean, 0.84),
    doc.radius * 0.24,
    height * 0.2,
    sides * 2,
    rings,
    doc.id,
    [0.96, 1, 0.92],
    random(800) * Math.PI * 2,
  );
  const trunkMesh = finish(trunk),
    leafMesh = finish(leaves),
    key = contentKey(vegetationGeometrySource(doc, quality)),
    bounds: Bounds = { min: [0, 0, 0], max: [0, 0, 0] };
  for (let a = 0; a < 3; a++) {
    bounds.min[a] = Math.min(trunkMesh.bounds.min[a], leafMesh.bounds.min[a]);
    bounds.max[a] = Math.max(trunkMesh.bounds.max[a], leafMesh.bounds.max[a]);
  }
  const maxDisplacement = Math.min(Math.max(bounds.max[1], 0) ** 2 * 0.012, 0.6) * 2;
  for (let axis = 0; axis < 3; axis++) {
    bounds.min[axis] -= maxDisplacement;
    bounds.max[axis] += maxDisplacement;
  }
  return {
    kind: "vegetation",
    id: doc.id,
    key,
    surfaces: [
      {
        kind: "surface" as const,
        id: `${doc.id}-trunk`,
        key: `${key}-trunk`,
        mesh: trunkMesh,
        material: doc.trunkMaterial,
        diagnostics: [],
      },
      {
        kind: "surface" as const,
        id: `${doc.id}-foliage`,
        key: `${key}-foliage`,
        mesh: leafMesh,
        material: doc.material,
        diagnostics: [],
      },
    ],
    windResponse: vegetationWindResponse(doc),
    maxDisplacement,
    bounds,
    diagnostics: [],
  };
}

function buildBotanicalVegetation(
  doc: VegetationDefinition,
  quality: Quality,
  distant: boolean,
): CompiledVegetation {
  const mesh = botanicalMeshes(doc, quality, distant);
  const key = contentKey(vegetationGeometrySource(doc, quality));
  const bounds: Bounds = {
    min: mesh.trunk.bounds.min.map((v, axis) => Math.min(v, mesh.foliage.bounds.min[axis])) as Vec3,
    max: mesh.trunk.bounds.max.map((v, axis) => Math.max(v, mesh.foliage.bounds.max[axis])) as Vec3,
  };
  const maxDisplacement =
    Math.min(Math.max(bounds.max[1], 0) ** 2 * 0.012, 0.6) * 2 +
    Math.max(vegetationMotionEnvelope(mesh.trunk.wind), vegetationMotionEnvelope(mesh.foliage.wind));
  for (let axis = 0; axis < 3; axis++) {
    bounds.min[axis] -= maxDisplacement;
    bounds.max[axis] += maxDisplacement;
  }
  return {
    kind: "vegetation",
    id: doc.id,
    key,
    windResponse: vegetationWindResponse(doc),
    maxDisplacement,
    bounds,
    surfaces: [
      {
        kind: "surface" as const,
        id: `${doc.id}-trunk`,
        key: `${key}-trunk`,
        mesh: mesh.trunk,
        material: doc.trunkMaterial,
        diagnostics: [],
      },
      {
        kind: "surface" as const,
        id: `${doc.id}-foliage`,
        key: `${key}-foliage`,
        mesh: mesh.foliage,
        material: doc.material,
        diagnostics: [],
      },
    ],
    diagnostics: mesh.truncated
      ? [
          {
            severity: "warning",
            code: "BOTANICAL_BUDGET",
            message: "Plant exceeds its bounded branch or leaf budget; reduce hierarchy or cluster density.",
            document: doc.id,
          },
        ]
      : [],
  };
}
