import {
  type AnalyticQuadric,
  type CompiledRenderProduct,
  contentKey,
  cross,
  type Diagnostic,
  dot,
  type FieldDefinition,
  type MeshData,
  normalize,
  type ObjectDefinition,
  type Quality,
  quadricUnitToLocal,
  sub,
  type Vec3,
} from "@wrela/model";

import { ProductCache } from "./cache";
import { compileField } from "./field";
import { geometryKey, materialBindingKey } from "./products";
import { createRenderProduct, makeDirectRenderProduct } from "./render-products";

type PrimitiveCompilation = {
  products: CompiledRenderProduct[];
  mesh?: MeshData;
  diagnostics?: Diagnostic[];
};
const primitiveProducts = new ProductCache<PrimitiveCompilation>(4 * 1024 * 1024);
export function primitiveCacheMetrics() {
  return primitiveProducts.metrics;
}
export function clearPrimitiveCache() {
  primitiveProducts.clear();
}

/** Recognize only a whole, static leaf. CSG, animation and clipped fields keep extraction. */
export function staticPrimitive(field: FieldDefinition): AnalyticQuadric | null {
  const root = field.nodes.find((node) => node.id === field.root);
  if (!root || (root.kind !== "sphere" && root.kind !== "ellipsoid") || root.children.length) return null;
  if (root.size.some((value) => !Number.isFinite(value) || value <= 0)) return null;
  const radii: Vec3 =
    root.kind === "sphere"
      ? [root.radius, root.radius, root.radius]
      : (root.size.map((v) => Math.max(0.0001, v)) as Vec3);
  const primitive: AnalyticQuadric = {
    center: [...root.position],
    radii,
    rotation: [...root.rotation],
    nodeId: root.id,
    ...(root.material ? { material: root.material } : {}),
  };
  if (
    [...primitive.center, ...primitive.rotation, ...radii].some((v) => !Number.isFinite(v)) ||
    radii.some((v) => v <= 0)
  )
    return null;
  const matrix = quadricUnitToLocal(primitive);
  for (let axis = 0; axis < 3; axis++) {
    const extent = Math.hypot(matrix[axis], matrix[axis + 4], matrix[axis + 8]);
    // A strict guard avoids using a complete primitive for an intentionally clipped field.
    const margin = 1e-6 * Math.max(1, extent, Math.abs(primitive.center[axis]));
    if (
      primitive.center[axis] - extent < field.bounds.min[axis] + margin ||
      primitive.center[axis] + extent > field.bounds.max[axis] - margin
    )
      return null;
  }
  return primitive;
}

/** Latitude/longitude triangles with analytic vertex normals and source provenance.
 * Each unit-sphere triangle lies on a supporting plane n.p=d. The largest
 * radial sagitta is 1-d; multiplying by the ellipsoid operator norm bounds the
 * Hausdorff distance. This is not a ray-depth bound at grazing incidence. */
export function parametricPrimitive(
  primitive: AnalyticQuadric,
  rings: number,
): { mesh: MeshData; surfaceError: number } {
  if (!Number.isInteger(rings) || rings < 4 || rings > 128)
    throw new Error("Primitive rings must be an integer from 4 to 128");
  const segments = rings * 2,
    matrix = quadricUnitToLocal(primitive);
  const unitPositions: Vec3[] = [];
  const positions: number[] = [],
    normals: number[] = [],
    indices: number[] = [];
  const vertex = (unit: Vec3) => {
    unitPositions.push(unit);
    positions.push(
      ...[0, 1, 2].map(
        (r) => matrix[r] * unit[0] + matrix[r + 4] * unit[1] + matrix[r + 8] * unit[2] + matrix[r + 12],
      ),
    );
    // R diag(1/r): each unitToLocal column already carries one radius.
    normals.push(
      ...normalize(
        [0, 1, 2].map(
          (r) =>
            (matrix[r] * unit[0]) / primitive.radii[0] ** 2 +
            (matrix[r + 4] * unit[1]) / primitive.radii[1] ** 2 +
            (matrix[r + 8] * unit[2]) / primitive.radii[2] ** 2,
        ) as Vec3,
      ),
    );
  };
  vertex([0, 1, 0]);
  for (let ring = 1; ring < rings; ring++) {
    const latitude = (ring * Math.PI) / rings;
    for (let segment = 0; segment < segments; segment++) {
      const longitude = (segment * Math.PI * 2) / segments;
      vertex([
        Math.sin(latitude) * Math.cos(longitude),
        Math.cos(latitude),
        Math.sin(latitude) * Math.sin(longitude),
      ]);
    }
  }
  const bottom = positions.length / 3;
  vertex([0, -1, 0]);
  for (let segment = 0; segment < segments; segment++) {
    const next = (segment + 1) % segments;
    indices.push(0, 1 + next, 1 + segment);
    for (let ring = 0; ring < rings - 2; ring++) {
      const a = 1 + ring * segments + segment,
        b = 1 + ring * segments + next,
        c = a + segments,
        d = b + segments;
      indices.push(a, b, c, b, d, c);
    }
    indices.push(bottom, 1 + (rings - 2) * segments + segment, 1 + (rings - 2) * segments + next);
  }
  const extent = [0, 1, 2].map((r) => Math.hypot(matrix[r], matrix[r + 4], matrix[r + 8]));
  let maximumSagitta = 0;
  for (let i = 0; i < indices.length; i += 3) {
    const a = unitPositions[indices[i]],
      b = unitPositions[indices[i + 1]],
      c = unitPositions[indices[i + 2]];
    const normal = normalize(cross(sub(b, a), sub(c, a)));
    maximumSagitta = Math.max(maximumSagitta, 1 - Math.abs(dot(normal, a)));
  }
  const surfaceError = Math.max(...primitive.radii) * maximumSagitta;
  return {
    mesh: {
      positions: new Float32Array(positions),
      normals: new Float32Array(normals),
      indices: new Uint32Array(indices),
      sourceIds: Array(positions.length / 3).fill(primitive.nodeId),
      ...(primitive.material
        ? { materialGroups: [{ material: primitive.material, start: 0, count: indices.length }] }
        : {}),
      bounds: {
        min: primitive.center.map((v, i) => v - extent[i]) as Vec3,
        max: primitive.center.map((v, i) => v + extent[i]) as Vec3,
      },
      // Numeric error remains unknown; the proved real bound lives in product evidence.
      fidelity: { sampleSpacing: [0, 0, 0], maxError: null, unresolvedFeatures: [] },
    },
    surfaceError,
  };
}

export function compilePrimitiveCompilation(doc: ObjectDefinition, quality: Quality): PrimitiveCompilation {
  const cacheKey = `${geometryKey(doc, quality)}:${materialBindingKey(doc)}`;
  const cached = primitiveProducts.get(cacheKey);
  if (cached) return cached;
  const sourceKey = geometryKey(doc, quality),
    extracted = makeDirectRenderProduct(sourceKey);
  const primitive = staticPrimitive(doc.field);
  if (!primitive) {
    const result = { products: [extracted] };
    primitiveProducts.set(cacheKey, result, 512);
    return result;
  }
  compileField(doc.field); // Retain graph/domain validation without sampling an extraction grid.
  let primaryRings = quality === "interactive" ? 12 : quality === "review" ? 24 : 48;
  const requestedError = doc.field.fidelity?.maxError;
  if (requestedError)
    primaryRings = Math.min(
      128,
      Math.max(primaryRings, Math.ceil(Math.PI * Math.sqrt(Math.max(...primitive.radii) / requestedError))),
    );
  const primary = parametricPrimitive(primitive, primaryRings);
  const diagnostics: Diagnostic[] = [];
  if (requestedError && primary.surfaceError > requestedError) {
    const message = `Parametric primitive reaches its 128-ring allocation cap before the requested ${requestedError} metre error; its real-arithmetic Hausdorff bound is ${primary.surfaceError} metres.`;
    if (doc.field.fidelity?.strict) throw new Error(message);
    diagnostics.push({
      severity: "warning",
      code: "field.parametric-error",
      message,
      node: primitive.nodeId,
    });
  }
  if (primary.mesh.fidelity) primary.mesh.fidelity.requestedMaxError = requestedError;
  const geometricEvidence = (maximum: number) => ({
    kind: "real-bound" as const,
    metric: "silhouette" as const,
    maximum,
    domain: "local-space Euclidean Hausdorff distance in metres; not ray depth or pixels",
    numericError: "unknown" as const,
  });
  const direct = createRenderProduct({
    ...extracted,
    algorithmVersion: `parametric-primary-v2-${primaryRings}`,
    errors: [geometricEvidence(primary.surfaceError)],
  });
  const domainKey = contentKey({ primitive, static: true });
  const common = {
    sourceKey,
    formatVersion: 1 as const,
    domainKey,
    fallbackKey: direct.key,
    dependencies: [
      { kind: "geometry" as const, key: sourceKey },
      { kind: "material" as const, key: materialBindingKey(doc) },
    ],
    assumptions: [{ kind: "rigid" as const }, { kind: "opaque" as const }],
  };
  const products: CompiledRenderProduct[] = [
    direct,
    createRenderProduct({
      ...common,
      kind: "analytic-quadric",
      algorithmVersion: "static-quadric-v1",
      primitive,
      assumptions: [...common.assumptions, { kind: "matrix-condition", maximum: 10000 }],
      errors: [
        {
          kind: "real-bound",
          metric: "depth",
          maximum: 0,
          domain: "isolated primitive zero set; numeric ray evaluation excluded",
          numericError: "unknown",
        },
      ],
    }),
  ];
  for (const rings of [6, 12, 24].filter((level) => level < primaryRings)) {
    const { mesh, surfaceError } = parametricPrimitive(primitive, rings);
    products.push(
      createRenderProduct({
        ...common,
        kind: "parametric-mesh",
        mesh,
        algorithmVersion: `parametric-quadric-v2-${rings}`,
        errors: [
          {
            kind: "real-bound",
            metric: "silhouette",
            maximum: surfaceError,
            domain: "local-space Euclidean Hausdorff distance in metres; not ray depth or pixels",
            numericError: "unknown",
          },
        ],
      }),
    );
  }
  const bytes = products.reduce(
    (sum, product) =>
      sum +
      product.byteLength +
      (product.kind === "parametric-mesh"
        ? (product.mesh.sourceIds?.reduce((bytes, id) => bytes + id.length * 2 + 8, 0) ?? 0)
        : 0) +
      512,
    0,
  );
  const result = { products, mesh: primary.mesh, diagnostics };
  const primaryBytes =
    primary.mesh.positions.byteLength +
    primary.mesh.normals.byteLength +
    primary.mesh.indices.byteLength +
    (primary.mesh.sourceIds?.reduce((sum, id) => sum + id.length * 2 + 8, 0) ?? 0);
  primitiveProducts.set(cacheKey, result, bytes + primaryBytes);
  return result;
}

export function compilePrimitiveProducts(doc: ObjectDefinition, quality: Quality): CompiledRenderProduct[] {
  return compilePrimitiveCompilation(doc, quality).products;
}
