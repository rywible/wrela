import { type CompiledRenderProduct, contentKey, type MeshData, type RenderDependency } from "@wrela/model";

import { z } from "zod";
import { PRODUCT_VERSIONS } from "./versions";

type ProductInput<T = CompiledRenderProduct> = T extends CompiledRenderProduct
  ? Omit<T, "key" | "byteLength">
  : never;
export function renderProductBuffers(product: CompiledRenderProduct): ArrayBuffer[] {
  if (product.kind !== "parametric-mesh") return [];
  const mesh = product.mesh;
  return [
    ...new Set([
      mesh.positions.buffer,
      mesh.normals.buffer,
      mesh.indices.buffer,
      ...(mesh.colors ? [mesh.colors.buffer] : []),
      ...(mesh.wind ? [mesh.wind.buffer] : []),
      ...(mesh.reliefCoordinates ? [mesh.reliefCoordinates.buffer] : []),
      ...(mesh.reliefNormals ? [mesh.reliefNormals.buffer] : []),
      ...(mesh.thinCoverage
        ? [
            mesh.thinCoverage.uv.buffer,
            ...(mesh.thinCoverage.layer ? [mesh.thinCoverage.layer.buffer] : []),
            ...mesh.thinCoverage.levels.map((level) => level.buffer),
          ]
        : []),
    ]),
  ] as ArrayBuffer[];
}
function meshIdentity(mesh: MeshData): unknown {
  return {
    ...mesh,
    positions: Array.from(mesh.positions),
    normals: Array.from(mesh.normals),
    indices: Array.from(mesh.indices),
    colors: mesh.colors ? Array.from(mesh.colors) : undefined,
    wind: mesh.wind ? Array.from(mesh.wind) : undefined,
    reliefCoordinates: mesh.reliefCoordinates ? Array.from(mesh.reliefCoordinates) : undefined,
    reliefNormals: mesh.reliefNormals ? Array.from(mesh.reliefNormals) : undefined,
    thinCoverage: mesh.thinCoverage
      ? {
          ...mesh.thinCoverage,
          uv: Array.from(mesh.thinCoverage.uv),
          layer: mesh.thinCoverage.layer ? Array.from(mesh.thinCoverage.layer) : undefined,
          levels: mesh.thinCoverage.levels.map((level) => Array.from(level)),
        }
      : undefined,
  };
}
export function renderProductKey(
  product: Omit<CompiledRenderProduct, "key"> | CompiledRenderProduct,
): string {
  const { key: _key, ...value } = product as CompiledRenderProduct;
  return contentKey(value.kind === "parametric-mesh" ? { ...value, mesh: meshIdentity(value.mesh) } : value);
}
export function createRenderProduct(input: ProductInput): CompiledRenderProduct {
  const product = { ...input, byteLength: 0, key: "" } as CompiledRenderProduct;
  product.byteLength = renderProductBuffers(product).reduce((sum, buffer) => sum + buffer.byteLength, 0);
  product.key = renderProductKey(product);
  return product;
}
export function makeDirectRenderProduct(sourceKey: string): CompiledRenderProduct {
  return createRenderProduct({
    kind: "direct-mesh",
    sourceKey,
    algorithmVersion: PRODUCT_VERSIONS.render,
    formatVersion: 1,
    domainKey: sourceKey,
    assumptions: [],
    errors: [{ kind: "unknown", reason: "Extracted mesh fidelity is not a certified geometric bound" }],
    fallbackKey: null,
    dependencies: [{ kind: "geometry", key: sourceKey }],
  });
}
/** Exact dependency identities, including transitive product references. This does
 * not treat a small edit as proof that an old domain remains valid. */
export function invalidatedRenderProducts(
  products: readonly CompiledRenderProduct[],
  changed: readonly RenderDependency[],
): string[] {
  const changes = new Set(changed.map(({ kind, key }) => `${kind}:${key}`)),
    invalid = new Set<string>();
  let progress = true;
  while (progress) {
    progress = false;
    for (const product of products) {
      if (
        !invalid.has(product.key) &&
        product.dependencies.some(
          ({ kind, key }) => changes.has(`${kind}:${key}`) || (kind === "product" && invalid.has(key)),
        )
      ) {
        invalid.add(product.key);
        progress = true;
      }
    }
  }
  return products.filter((product) => invalid.has(product.key)).map((product) => product.key);
}
const id = z.string().min(1).max(256),
  nonnegative = z.number().finite().nonnegative();
const metric = z.enum([
  "depth",
  "silhouette",
  "normal-angle",
  "linear-radiance",
  "transmittance",
  "temporal",
]);
const evidence = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("numeric-bound"), metric, maximum: nonnegative, domain: id }),
  z.object({
    kind: z.literal("real-bound"),
    metric,
    maximum: nonnegative,
    domain: id,
    numericError: z.literal("unknown"),
  }),
  z.object({
    kind: z.literal("measured"),
    metric,
    rms: nonnegative,
    maximum: nonnegative,
    domain: id,
    reference: id,
    samples: z.number().int().positive().max(1e9),
  }),
  z.object({ kind: z.literal("unknown"), reason: z.string().min(1).max(4096) }),
]);
const assumption = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("rigid") }),
  z.object({ kind: z.literal("opaque") }),
  z.object({ kind: z.literal("roughness-range"), minimum: nonnegative.max(1), maximum: nonnegative.max(1) }),
  z.object({
    kind: z.literal("carrier-relation"),
    phaseBasisKey: id,
    relation: z.enum(["coherent", "independent"]),
  }),
  z.object({ kind: z.literal("complete-periods"), minimum: z.number().int().nonnegative() }),
  z.object({ kind: z.literal("pose-revision"), revision: id }),
  z.object({
    kind: z.literal("finite-source"),
    geometry: z.literal("sphere"),
    angularRadius: nonnegative.max(Math.PI),
  }),
  z.object({ kind: z.literal("matrix-condition"), maximum: z.number().finite().min(1) }),
]);
const vector = z.tuple([z.number().finite(), z.number().finite(), z.number().finite()]);
const envelope = {
  key: id,
  sourceKey: id,
  algorithmVersion: id,
  formatVersion: z.literal(1),
  domainKey: id,
  assumptions: z.array(assumption).max(32),
  errors: z.array(evidence).max(32),
  byteLength: z
    .number()
    .int()
    .nonnegative()
    .max(64 * 1024 * 1024),
  fallbackKey: id.nullable(),
  dependencies: z
    .array(
      z.object({
        kind: z.enum([
          "geometry",
          "material",
          "binding",
          "motion",
          "water",
          "lighting",
          "atmosphere",
          "product",
        ]),
        key: id,
      }),
    )
    .max(64),
};
const productSchema = z.discriminatedUnion("kind", [
  z.object({ ...envelope, kind: z.literal("direct-mesh") }),
  z.object({
    ...envelope,
    kind: z.literal("analytic-quadric"),
    primitive: z.object({
      center: vector,
      radii: z.tuple([
        z.number().finite().positive(),
        z.number().finite().positive(),
        z.number().finite().positive(),
      ]),
      rotation: vector,
      nodeId: id,
      material: id.optional(),
    }),
  }),
  z.object({ ...envelope, kind: z.literal("parametric-mesh"), mesh: z.unknown() }),
]);
export function serializeRenderProducts(
  products: readonly CompiledRenderProduct[],
  serializeMesh: (mesh: MeshData) => unknown,
): unknown[] {
  return products.map((product) =>
    product.kind === "parametric-mesh" ? { ...product, mesh: serializeMesh(product.mesh) } : product,
  );
}
export function deserializeRenderProducts(
  input: unknown,
  deserializeMesh: (value: unknown) => MeshData,
): CompiledRenderProduct[] {
  const values = z.array(productSchema).min(1).max(16).parse(input);
  // Check declared and actual serialized sizes before materializing any typed
  // payload. A forged declaration must not bypass the allocation budget.
  const budget = 64 * 1024 * 1024;
  if (values.reduce((total, value) => total + value.byteLength, 0) > budget)
    throw new Error("Render product bytes exceed budget");
  let serializedBytes = 0;
  for (const value of values) {
    let bytes = 0;
    if (value.kind === "parametric-mesh") {
      if (!value.mesh || typeof value.mesh !== "object") throw new Error("Malformed cooked render mesh");
      const mesh = value.mesh as Record<string, unknown>;
      for (const name of [
        "positions",
        "normals",
        "indices",
        "colors",
        "wind",
        "reliefCoordinates",
        "reliefNormals",
      ] as const) {
        const array = mesh[name];
        if (!["positions", "normals", "indices"].includes(name) && array === undefined) continue;
        if (
          !Array.isArray(array) ||
          array.length > (name === "indices" ? 1500000 : name === "wind" ? 1000000 : 750000)
        )
          throw new Error("Malformed cooked render mesh arrays");
        bytes += array.length * 4;
      }
      if (mesh.thinCoverage !== undefined) {
        if (!mesh.thinCoverage || typeof mesh.thinCoverage !== "object")
          throw new Error("Malformed cooked render coverage");
        const coverage = mesh.thinCoverage as Record<string, unknown>;
        if (
          !Array.isArray(coverage.uv) ||
          coverage.uv.length > 500000 ||
          !Array.isArray(coverage.levels) ||
          coverage.levels.length > 10
        )
          throw new Error("Malformed cooked render coverage arrays");
        bytes += coverage.uv.length * 4;
        for (const level of coverage.levels) {
          if (!Array.isArray(level) || level.length > 512 * 512)
            throw new Error("Malformed cooked render coverage level");
          bytes += level.length;
        }
      }
    }
    serializedBytes += bytes;
    if (serializedBytes > budget) throw new Error("Render product bytes exceed budget");
    if (bytes !== value.byteLength) throw new Error("Render product byte ownership mismatch");
  }
  const products: CompiledRenderProduct[] = values.map((value) =>
    value.kind === "parametric-mesh" ? { ...value, mesh: deserializeMesh(value.mesh) } : value,
  );
  const keys = new Map(products.map((product) => [product.key, product]));
  if (
    keys.size !== products.length ||
    products.filter((product) => product.kind === "direct-mesh").length !== 1
  )
    throw new Error("Render products require unique identities and one direct fallback");
  const owned = new Set<ArrayBuffer>();
  for (const product of products) {
    const buffers = renderProductBuffers(product);
    for (const buffer of buffers) owned.add(buffer);
    if (
      product.key !== renderProductKey(product) ||
      product.byteLength !== buffers.reduce((sum, buffer) => sum + buffer.byteLength, 0)
    )
      throw new Error("Render product identity or byte ownership mismatch");
    if (product.errors.some((error) => error.kind === "measured" && error.rms > error.maximum))
      throw new Error("Measured RMS cannot exceed measured maximum");
    if (
      product.assumptions.some(
        (assumption) => assumption.kind === "roughness-range" && assumption.minimum > assumption.maximum,
      )
    )
      throw new Error("Invalid render product roughness domain");
    if (product.kind === "direct-mesh" ? product.fallbackKey !== null : !product.fallbackKey)
      throw new Error("Render product fallback is missing");
    const seen = new Set([product.key]);
    let fallback = product.fallbackKey;
    while (fallback !== null) {
      const next = keys.get(fallback);
      if (!next || seen.has(fallback)) throw new Error("Render product fallback is missing or cyclic");
      seen.add(fallback);
      fallback = next.fallbackKey;
    }
    if (product.dependencies.some((dependency) => dependency.kind === "product" && !keys.has(dependency.key)))
      throw new Error("Render product dependency is missing");
  }
  const complete = new Set<string>();
  const visit = (key: string, active: Set<string>) => {
    if (complete.has(key)) return;
    if (active.has(key)) throw new Error("Render product dependency cycle");
    active.add(key);
    for (const dependency of keys.get(key)?.dependencies ?? [])
      if (dependency.kind === "product") visit(dependency.key, active);
    active.delete(key);
    complete.add(key);
  };
  for (const product of products) visit(product.key, new Set());
  if ([...owned].reduce((sum, buffer) => sum + buffer.byteLength, 0) > 64 * 1024 * 1024)
    throw new Error("Render product bytes exceed budget");
  return products;
}
