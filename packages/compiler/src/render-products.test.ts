import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { type CompiledSurface, contentKey, type MeshData } from "@wrela/model";
import { cookProject, deserializeArtifact, loadCookedProject, serializeArtifact } from "./cooked";
import { artifactTransfers } from "./index";
import { appearanceKey, geometryKey, renderSourceKey } from "./products";
import {
  createRenderProduct,
  deserializeRenderProducts,
  invalidatedRenderProducts,
  makeDirectRenderProduct,
} from "./render-products";

function fixture(): CompiledSurface {
  const mesh: MeshData = {
    positions: new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]),
    normals: new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1]),
    indices: new Uint32Array([0, 1, 2]),
    bounds: { min: [0, 0, 0], max: [1, 1, 0] },
  };
  const direct = makeDirectRenderProduct("shape");
  const parameter = createRenderProduct({
    kind: "parametric-mesh",
    mesh: structuredClone(mesh),
    sourceKey: "shape",
    algorithmVersion: "parametric-1",
    formatVersion: 1,
    domainKey: "unit-sphere",
    assumptions: [{ kind: "rigid" }],
    errors: [
      { kind: "real-bound", metric: "depth", maximum: 0.1, domain: "unit-sphere", numericError: "unknown" },
    ],
    fallbackKey: direct.key,
    dependencies: [{ kind: "geometry", key: "shape" }],
  });
  return {
    kind: "surface",
    id: "ball",
    key: "artifact",
    material: "stone",
    mesh,
    diagnostics: [],
    renderProducts: [direct, parameter],
  };
}
test("render products survive JSON cooking and worker transfer with owned byte accounting", () => {
  const source = fixture(),
    serialized = JSON.parse(JSON.stringify(serializeArtifact(source))),
    loaded = deserializeArtifact(serialized);
  expect(loaded.kind).toBe("surface");
  if (loaded.kind !== "surface") throw new Error("Missing surface");
  expect(serializeArtifact(loaded)).toEqual(serialized);
  const product = loaded.renderProducts?.[1];
  if (product?.kind !== "parametric-mesh") throw new Error("Missing mesh product");
  expect(product.byteLength).toBe(84);
  expect(artifactTransfers(loaded)).toContain(product.mesh.positions.buffer as ArrayBuffer);
  const transfer = artifactTransfers(loaded),
    copied = structuredClone(loaded, { transfer });
  expect(product.mesh.positions.byteLength).toBe(0);
  expect(copied.renderProducts?.[1].byteLength).toBe(84);
  expect(copied.mesh.positions.length).toBe(9);
});
test("invalid product bytes, format, domain and dangling fallback are rejected", () => {
  const surface = fixture(),
    original = serializeArtifact(surface);
  for (const change of [
    (product: Record<string, unknown>) => {
      product.byteLength = 1;
    },
    (product: Record<string, unknown>) => {
      product.formatVersion = 2;
    },
    (product: Record<string, unknown>) => {
      product.errors = [{ kind: "measured", metric: "depth", rms: 0, maximum: 0, domain: "x" }];
    },
  ]) {
    const damaged = structuredClone(original) as { renderProducts: Record<string, unknown>[] };
    change(damaged.renderProducts[1]);
    expect(() => deserializeArtifact(damaged)).toThrow();
  }
  const product = surface.renderProducts?.[1];
  if (!product) throw new Error("Missing product");
  surface.renderProducts = [
    makeDirectRenderProduct("shape"),
    createRenderProduct({ ...product, fallbackKey: "absent" }),
  ];
  expect(() => deserializeArtifact(serializeArtifact(surface))).toThrow("fallback");
});
test("dependency edits invalidate only dependent products, including transitive response products", () => {
  const direct = makeDirectRenderProduct("shape");
  const appearance = createRenderProduct({
    ...direct,
    kind: "analytic-quadric",
    primitive: { center: [0, 0, 0], radii: [1, 1, 1], rotation: [0, 0, 0], nodeId: "ball" },
    fallbackKey: direct.key,
    dependencies: [{ kind: "material", key: "old-material" }],
  });
  const derived = createRenderProduct({
    ...appearance,
    domainKey: "second-domain",
    dependencies: [{ kind: "product", key: appearance.key }],
  });
  expect(
    invalidatedRenderProducts([direct, appearance, derived], [{ kind: "material", key: "old-material" }]),
  ).toEqual([appearance.key, derived.key]);
  expect(
    invalidatedRenderProducts([direct, appearance, derived], [{ kind: "lighting", key: "new-light" }]),
  ).toEqual([]);
});
test("render geometry and appearance keys isolate source edits", () => {
  const project = referenceProject(),
    object = project.documents.find((doc) => doc.kind === "object"),
    material = project.documents.find((doc) => doc.kind === "material");
  if (!object || object.kind !== "object" || !material || material.kind !== "material")
    throw new Error("Missing fixtures");
  const rebound = structuredClone(object);
  rebound.material = "another";
  expect(geometryKey(rebound)).toBe(geometryKey(object));
  expect(renderSourceKey(rebound)).toBe(renderSourceKey(object));
  expect(appearanceKey(rebound)).not.toBe(appearanceKey(object));
  expect(appearanceKey({ ...material, roughness: material.roughness * 0.5 })).not.toBe(
    appearanceKey(material),
  );
  expect(appearanceKey({ ...material, name: "Rename" })).toBe(appearanceKey(material));
});
test("cooked manifests cannot pass completeness by omitting a required identity", () => {
  const project = referenceProject(),
    bundle = cookProject(project, "interactive");
  bundle.entries.pop();
  expect(() => loadCookedProject(bundle, project)).toThrow("missing required product");
});
test("product identities include evidence and runtime domains", () => {
  const product = makeDirectRenderProduct("shape");
  expect(createRenderProduct({ ...product, domainKey: "new-domain" }).key).not.toBe(product.key);
  expect(
    createRenderProduct({ ...product, errors: [{ kind: "unknown", reason: "different evidence" }] }).key,
  ).not.toBe(product.key);
  expect(contentKey(product)).not.toBe("");
});

test("cooked render product allocation rejects excessive declared or serialized bytes before decoding", () => {
  const source = fixture();
  const serialized = serializeArtifact(source) as { renderProducts: Record<string, unknown>[] };
  let allocations = 0;
  const decode = (_value: unknown): MeshData => {
    allocations++;
    return source.mesh;
  };
  const direct = serialized.renderProducts[0];
  const product = serialized.renderProducts[1];
  expect(() =>
    deserializeRenderProducts(
      [direct, { ...product, byteLength: 40 * 1024 * 1024 }, { ...product, byteLength: 40 * 1024 * 1024 }],
      decode,
    ),
  ).toThrow("budget");
  expect(allocations).toBe(0);
  const oversized = {
    ...product,
    byteLength: 0,
    mesh: {
      positions: new Array(750000),
      normals: new Array(750000),
      indices: new Array(1500000),
      colors: new Array(750000),
    },
  };
  expect(() => deserializeRenderProducts([direct, ...Array(15).fill(oversized)], decode)).toThrow(
    "byte ownership",
  );
  expect(allocations).toBe(0);
  expect(() =>
    deserializeRenderProducts(
      [direct, { ...oversized, mesh: { ...oversized.mesh, positions: new Array(750001) } }],
      decode,
    ),
  ).toThrow("arrays");
  expect(allocations).toBe(0);
});

test("parametric foliage motion participates in ownership, identity and cooking budgets", () => {
  const surface = fixture();
  const original = surface.renderProducts?.[1];
  if (original?.kind !== "parametric-mesh") throw new Error("Missing mesh product");
  const wind = new Float32Array([0, 0.1, 0, 0.02, 1, 0.2, 1, 0.03, 2, 0.3, 2, 0.04]);
  const product = createRenderProduct({ ...original, mesh: { ...original.mesh, wind } });
  if (!surface.renderProducts) throw new Error("Missing products");
  surface.renderProducts[1] = product;
  expect(product.byteLength).toBe(original.byteLength + wind.byteLength);
  expect(product.key).not.toBe(original.key);
  const loaded = deserializeArtifact(JSON.parse(JSON.stringify(serializeArtifact(surface))));
  if (loaded.kind !== "surface") throw new Error("Missing loaded surface");
  const cooked = loaded.renderProducts?.[1];
  if (cooked?.kind !== "parametric-mesh") throw new Error("Missing cooked parametric mesh");
  expect(cooked.mesh.wind).toEqual(wind);
  expect(artifactTransfers(loaded)).toContain(cooked.mesh.wind?.buffer as ArrayBuffer);
});

test("parametric products preserve physical relief frames and thin coverage through cooking and transfer", () => {
  const source = fixture(),
    original = source.renderProducts?.[1];
  if (original?.kind !== "parametric-mesh") throw new Error("Missing product");
  const mesh: MeshData = {
    ...original.mesh,
    reliefCoordinates: original.mesh.positions.slice(),
    reliefNormals: original.mesh.normals.slice(),
    thinCoverage: {
      version: 1,
      key: "leaf-mask",
      width: 2,
      height: 2,
      uv: new Float32Array([0, 0, 1, 0, 0, 1]),
      levels: [new Uint8Array([255, 255, 0, 0]), new Uint8Array([128])],
    },
  };
  const product = createRenderProduct({ ...original, mesh });
  expect(product.byteLength).toBe(original.byteLength + 72 + 24 + 5);
  if (!source.renderProducts) throw new Error("Missing products");
  source.renderProducts[1] = product;
  const loaded = deserializeArtifact(JSON.parse(JSON.stringify(serializeArtifact(source))));
  if (loaded.kind !== "surface" || loaded.renderProducts?.[1].kind !== "parametric-mesh")
    throw new Error("Missing loaded product");
  const actual = loaded.renderProducts[1].mesh;
  expect(actual.reliefCoordinates).toEqual(mesh.reliefCoordinates);
  expect(actual.reliefNormals).toEqual(mesh.reliefNormals);
  expect(actual.thinCoverage).toEqual(mesh.thinCoverage);
  const transferred = artifactTransfers(loaded);
  expect(transferred).toContain(actual.reliefCoordinates?.buffer as ArrayBuffer);
  expect(transferred).toContain(actual.reliefNormals?.buffer as ArrayBuffer);
  expect(transferred).toContain(actual.thinCoverage?.uv.buffer as ArrayBuffer);
  expect(transferred).toContain(actual.thinCoverage?.levels[0].buffer as ArrayBuffer);
  if (!mesh.reliefCoordinates) throw new Error("Missing relief reference coordinates");
  const changed = { ...mesh, reliefCoordinates: mesh.reliefCoordinates.slice() };
  changed.reliefCoordinates[0] += 0.01;
  expect(createRenderProduct({ ...original, mesh: changed }).key).not.toBe(product.key);
  let allocations = 0;
  const serialized = serializeArtifact(source) as { renderProducts: Record<string, unknown>[] };
  const malformed = structuredClone(serialized.renderProducts);
  const payload = malformed[1].mesh as { thinCoverage: { levels: number[][] } };
  payload.thinCoverage.levels.push(new Array(512 * 512 + 1));
  expect(() =>
    deserializeRenderProducts(malformed, () => {
      allocations++;
      return mesh;
    }),
  ).toThrow("coverage level");
  expect(allocations).toBe(0);
});
