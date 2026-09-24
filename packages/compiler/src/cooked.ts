import {
  contentKey,
  creatureSchema,
  type Document,
  jointSchema,
  type MeshData,
  motionSchema,
  type Project,
  performanceSchema,
  type Quality,
  releaseProject,
  type SurfaceArtifact,
  serializeBufferViews,
  vec3Schema,
} from "@wrela/model";

import { z } from "zod";
import { compileDocument, compilerKey } from "./compile";
import { deserializeCreatureProducts, serializeCreatureProducts } from "./creature-cooked";
import { productKeys } from "./products";
import { deserializeRenderProducts, serializeRenderProducts } from "./render-products";
import { BUNDLE_COMPILER_SOURCE, COMPILER_VERSION, COOK_FORMAT_VERSION, PRODUCT_VERSIONS } from "./versions";

type SerializedMesh = Omit<
  MeshData,
  | "positions"
  | "normals"
  | "indices"
  | "colors"
  | "wind"
  | "thinCoverage"
  | "materialCoordinates"
  | "reliefCoordinates"
  | "reliefNormals"
  | "shoots"
> & {
  positions: number[];
  normals: number[];
  indices: number[];
  colors?: number[];
  wind?: number[];
  materialCoordinates?: number[];
  reliefCoordinates?: number[];
  reliefNormals?: number[];
  shoots?: Omit<NonNullable<MeshData["shoots"]>, "transforms" | "anchors" | "motion" | "skyVisibility"> & {
    transforms: number[];
    anchors: number[];
    motion: number[];
    skyVisibility?: number[];
  };
  thinCoverage?: Omit<NonNullable<MeshData["thinCoverage"]>, "levels" | "uv" | "layer"> & {
    levels: number[][];
    uv: number[];
    layer?: number[];
  };
};
type SerializedArtifact = Record<string, unknown>;
export type CookedProject = {
  format: typeof COOK_FORMAT_VERSION;
  compiler: string;
  compilerSource: string;
  productVersions: typeof PRODUCT_VERSIONS;
  source: string;
  quality: Quality;
  entries: { document: string; source: string; products: ReturnType<typeof productKeys>; artifact: string }[];
  artifacts: Record<string, SerializedArtifact>;
};
export type CompileProvider = (document: Document, quality: Quality) => Promise<SurfaceArtifact | null>;

function serializeMesh(mesh: MeshData): SerializedMesh {
  return serializeBufferViews(mesh) as SerializedMesh;
}

export function serializeArtifact(artifact: SurfaceArtifact): SerializedArtifact {
  if (artifact.kind === "vegetation")
    return { ...artifact, surfaces: artifact.surfaces.map(serializeArtifact) };
  return {
    ...artifact,
    mesh: serializeMesh(artifact.mesh),
    ...(artifact.kind === "surface" && artifact.renderProducts
      ? { renderProducts: serializeRenderProducts(artifact.renderProducts, serializeMesh) }
      : {}),
    ...(artifact.kind === "character"
      ? {
          weights: Array.from(artifact.weights),
          jointIndices: Array.from(artifact.jointIndices),
          ...serializeCreatureProducts(artifact, serializeMesh),
        }
      : { details: artifact.details?.map((detail) => ({ ...detail, mesh: serializeMesh(detail.mesh) })) }),
  };
}
const finite = z.number().finite();
const float32 = z.number().finite().min(-3.4028234663852886e38).max(3.4028234663852886e38);
const bounds = z
  .object({ min: vec3Schema, max: vec3Schema })
  .refine((value) => value.min.every((minimum, axis) => minimum <= value.max[axis]), "Inverted bounds");
const diagnostics = z
  .array(
    z.object({
      severity: z.enum(["warning", "error", "info"]),
      code: z.string().max(128),
      message: z.string().max(4096),
      document: z.string().max(100).optional(),
      node: z.string().max(100).optional(),
    }),
  )
  .max(1024);
const meshSchema = z.strictObject({
  shoots: z
    .object({
      transforms: z.array(float32).max(8192 * 16),
      anchors: z.array(float32).max(8192 * 4),
      motion: z.array(float32).max(8192 * 4),
      skyVisibility: z.array(z.number().finite().min(0).max(1)).max(8192).optional(),
      sourceIds: z.array(z.string().min(1).max(512)).min(1).max(8192),
      templateBounds: bounds,
    })
    .optional(),
  indirectProofs: z.array(float32).max(4_000_000).optional(),
  skyVisibility: z.array(float32).max(4_000_000).optional(),
  positions: z.array(float32).max(750000),
  normals: z.array(float32).max(750000),
  indices: z.array(z.number().int().nonnegative().max(249999)).max(1500000),
  colors: z.array(float32).max(750000).optional(),
  wind: z.array(float32).max(1000000).optional(),
  materialCoordinates: z.array(float32).max(750000).optional(),
  reliefCoordinates: z.array(float32).max(750000).optional(),
  reliefNormals: z.array(float32).max(750000).optional(),
  thinCoverage: z
    .object({
      version: z.literal(1),
      format: z.literal("coverage-normal").optional(),
      layers: z.number().int().min(1).max(256).optional(),
      layer: z.array(z.number().int().min(0).max(255)).max(250000).optional(),
      key: z.string().min(1).max(128),
      width: z
        .number()
        .int()
        .positive()
        .max(512)
        .refine((value) => Number.isInteger(Math.log2(value))),
      height: z
        .number()
        .int()
        .positive()
        .max(512)
        .refine((value) => Number.isInteger(Math.log2(value))),
      levels: z
        .array(z.array(z.number().int().min(0).max(255)).max(4194304))
        .min(1)
        .max(12),
      uv: z.array(float32).max(500000),
    })
    .optional(),
  sourceIds: z.array(z.string().max(512)).max(250000).optional(),
  materialGroups: z
    .array(
      z.object({
        material: z.string().min(1).max(100),
        start: z.number().int().nonnegative(),
        count: z.number().int().nonnegative(),
      }),
    )
    .max(128)
    .optional(),
  bounds,
  fidelity: z
    .object({
      sampleSpacing: vec3Schema,
      requestedMaxError: finite.optional(),
      maxError: finite.nullable(),
      unresolvedFeatures: z.array(z.string().max(100)).max(128),
    })
    .optional(),
});
function deserializeMesh(input: unknown): MeshData {
  const mesh = meshSchema.parse(input),
    count = mesh.positions.length / 3;
  if (
    !Number.isInteger(count) ||
    mesh.normals.length !== mesh.positions.length ||
    mesh.indices.length % 3 ||
    (mesh.indirectProofs && mesh.indirectProofs.length !== count * 4) ||
    (mesh.skyVisibility && mesh.skyVisibility.length !== count * 4) ||
    (mesh.colors && mesh.colors.length !== mesh.positions.length) ||
    (mesh.materialCoordinates && mesh.materialCoordinates.length !== mesh.positions.length) ||
    (mesh.reliefCoordinates && mesh.reliefCoordinates.length !== mesh.positions.length) ||
    (mesh.reliefNormals && mesh.reliefNormals.length !== mesh.positions.length) ||
    !!mesh.reliefCoordinates !== !!mesh.reliefNormals ||
    (mesh.wind && (mesh.wind.length !== count * 4 || mesh.wind.some((v, i) => i % 2 === 1 && v < 0))) ||
    (mesh.sourceIds && mesh.sourceIds.length !== count) ||
    mesh.indices.some((index) => index >= count) ||
    mesh.bounds.min.some((value, axis) => value > mesh.bounds.max[axis])
  )
    throw new Error("Malformed cooked mesh layout");
  if (mesh.thinCoverage) {
    const coverage = mesh.thinCoverage;
    const layers = coverage.layers ?? 1;
    if (
      (layers > 1 && !coverage.layer) ||
      (coverage.layer && (coverage.layer.length !== count || coverage.layer.some((layer) => layer >= layers)))
    )
      throw Error("Malformed coverage layer binding");
    if (coverage.layer)
      for (let triangle = 0; triangle < mesh.indices.length; triangle += 3) {
        const layer = coverage.layer[mesh.indices[triangle]];
        if (
          coverage.layer[mesh.indices[triangle + 1]] !== layer ||
          coverage.layer[mesh.indices[triangle + 2]] !== layer
        )
          throw Error("Coverage layer changes inside a triangle");
      }
    let width = coverage.width,
      height = coverage.height;
    const expectedLevels = Math.floor(Math.log2(Math.max(width, height))) + 1;
    if (coverage.uv.length !== count * 2 || coverage.levels.length !== expectedLevels)
      throw new Error("Malformed cooked thin coverage layout");
    for (const level of coverage.levels) {
      if (
        level.length !==
        width * height * (coverage.layers ?? 1) * (coverage.format === "coverage-normal" ? 4 : 1)
      )
        throw new Error("Malformed cooked thin coverage mip");
      width = Math.max(1, Math.floor(width / 2));
      height = Math.max(1, Math.floor(height / 2));
    }
  }
  if (mesh.materialGroups) {
    let offset = 0;
    for (const group of mesh.materialGroups) {
      if (group.start !== offset || group.count % 3) throw new Error("Malformed cooked material ranges");
      offset += group.count;
    }
    if (offset !== mesh.indices.length) throw new Error("Incomplete cooked material ranges");
  }
  if (mesh.shoots) {
    const { transforms, anchors, motion, sourceIds, templateBounds, skyVisibility } = mesh.shoots;
    if (skyVisibility && skyVisibility.length !== sourceIds.length)
      throw Error("Malformed cooked shoot visibility");
    if (
      transforms.length !== sourceIds.length * 16 ||
      anchors.length !== sourceIds.length * 4 ||
      motion.length !== sourceIds.length * 4 ||
      new Set(sourceIds).size !== sourceIds.length ||
      templateBounds.min.some((v, a) => v > templateBounds.max[a])
    )
      throw Error("Malformed cooked shoot instances");
    for (let i = 0; i < sourceIds.length; i++) {
      const at = i * 16;
      if (
        transforms[at + 3] !== 0 ||
        transforms[at + 7] !== 0 ||
        transforms[at + 11] !== 0 ||
        transforms[at + 15] !== 1 ||
        motion[i * 4] < 0 ||
        motion[i * 4] > 1 ||
        motion[i * 4 + 1] < 0 ||
        motion[i * 4 + 1] > 1 ||
        motion[i * 4 + 3] < 0
      )
        throw Error("Invalid shoot transform or motion");
      const m = transforms.slice(at, at + 16);
      const determinant =
        m[0] * (m[5] * m[10] - m[9] * m[6]) -
        m[4] * (m[1] * m[10] - m[9] * m[2]) +
        m[8] * (m[1] * m[6] - m[5] * m[2]);
      if (!(determinant > 1e-10)) throw Error("Singular or reflected shoot transform");
      for (let corner = 0; corner < 8; corner++)
        for (let axis = 0; axis < 3; axis++) {
          const p = [0, 1, 2].map((a) => templateBounds[corner & (1 << a) ? "max" : "min"][a]);
          const v = m[axis] * p[0] + m[axis + 4] * p[1] + m[axis + 8] * p[2] + m[axis + 12];
          if (v < mesh.bounds.min[axis] - 1e-4 || v > mesh.bounds.max[axis] + 1e-4)
            throw Error("Shoot lies outside cooked bounds");
        }
    }
  }
  for (let index = 0; index < mesh.positions.length; index++) {
    const axis = index % 3,
      value = mesh.positions[index],
      tolerance = Math.max(1e-5, Math.abs(value) * 1e-6);
    const vertexBounds = mesh.shoots?.templateBounds ?? mesh.bounds;
    if (value < vertexBounds.min[axis] - tolerance || value > vertexBounds.max[axis] + tolerance)
      throw new Error("Cooked vertex lies outside its declared bounds");
  }
  return {
    ...mesh,
    positions: new Float32Array(mesh.positions),
    indirectProofs: mesh.indirectProofs ? new Float32Array(mesh.indirectProofs) : undefined,
    skyVisibility: mesh.skyVisibility ? new Float32Array(mesh.skyVisibility) : undefined,
    normals: new Float32Array(mesh.normals),
    indices: new Uint32Array(mesh.indices),
    colors: mesh.colors ? new Float32Array(mesh.colors) : undefined,
    wind: mesh.wind ? new Float32Array(mesh.wind) : undefined,
    shoots: mesh.shoots
      ? {
          ...mesh.shoots,
          transforms: new Float32Array(mesh.shoots.transforms),
          anchors: new Float32Array(mesh.shoots.anchors),
          motion: new Float32Array(mesh.shoots.motion),
          skyVisibility: mesh.shoots.skyVisibility ? new Float32Array(mesh.shoots.skyVisibility) : undefined,
        }
      : undefined,
    materialCoordinates: mesh.materialCoordinates ? new Float32Array(mesh.materialCoordinates) : undefined,
    reliefCoordinates: mesh.reliefCoordinates ? new Float32Array(mesh.reliefCoordinates) : undefined,
    reliefNormals: mesh.reliefNormals ? new Float32Array(mesh.reliefNormals) : undefined,
    thinCoverage: mesh.thinCoverage
      ? {
          ...mesh.thinCoverage,
          levels: mesh.thinCoverage.levels.map((level) => new Uint8Array(level)),
          uv: new Float32Array(mesh.thinCoverage.uv),
          layer: mesh.thinCoverage.layer ? new Uint16Array(mesh.thinCoverage.layer) : undefined,
        }
      : undefined,
  };
}
const surfaceEnvelope = { id: z.string().min(1).max(200), key: z.string().min(1).max(128), diagnostics };
const surfaceSchema = z.object({
  ...surfaceEnvelope,
  kind: z.literal("surface"),
  renderProducts: z.unknown().optional(),
  opaqueVisibility: z
    .object({
      sourceKey: z.string().min(1).max(256),
      interior: z.array(z.object({ center: vec3Schema, radius: z.number().finite().positive() })).max(128),
      exterior: bounds.nullable(),
      evidence: z.literal("real-bound"),
      numericError: z.literal("unknown"),
      rigid: z.literal(true),
      opaque: z.literal(true),
    })
    .optional(),
  mesh: z.unknown(),
  material: z.string().min(1).max(100),
  details: z
    .array(
      z.object({
        label: z.string().max(100),
        vegetation: z
          .object({
            version: z.literal(1),
            kind: z.literal("multiview-crown"),
            key: z.string().max(128),
            sourceKey: z.string().max(128),
            algorithmVersion: z.string().max(64),
            fallback: z.literal("source-mesh"),
            byteLength: z.number().int().nonnegative().max(16777216),
            windEnvelope: z.number().finite().min(0).max(100),
            sourceOrgans: z.array(z.string().max(512)).max(8192),
            clusters: z
              .array(z.object({ bounds, sourceOrgans: z.array(z.string().max(512)).max(8192) }))
              .min(1)
              .max(4)
              .optional(),
            views: z
              .array(
                z.object({
                  direction: vec3Schema,
                  firstIndex: z.number().int().nonnegative(),
                  indexCount: z.number().int().positive(),
                }),
              )
              .min(1)
              .max(64),
            qualification: z.object({
              status: z.enum(["candidate", "qualified"]),
              maximumPixels: z.number().finite().min(0).max(1000),
              maxWind: z.number().finite().min(0).max(10),
              evidence: z.string().max(4096),
            }),
          })
          .optional(),
        mesh: z.unknown(),
        maxProjectedDiameter: z.number().finite().positive().max(10000),
        maxError: z.null(),
      }),
    )
    .max(4)
    .optional(),
});
const characterSchema = z.object({
  ...surfaceEnvelope,
  kind: z.literal("character"),
  mesh: z.unknown(),
  material: z.string().min(1).max(100),
  joints: z.array(jointSchema).min(1).max(64),
  motions: z.array(motionSchema).max(32),
  performance: performanceSchema.optional(),
  jointIndices: z.array(z.number().int().nonnegative().max(63)).max(1000000),
  weights: z.array(z.number().finite().min(0).max(1)).max(1000000),
  creature: creatureSchema.optional(),
});
const vegetationSchema = z.object({
  ...surfaceEnvelope,
  kind: z.literal("vegetation"),
  surfaces: z.array(surfaceSchema).min(2).max(32),
  windResponse: z.number().min(0).max(2),
  maxDisplacement: z.number().finite().nonnegative().max(100),
  bounds,
});
function deserializeDetail(detail: NonNullable<z.infer<typeof surfaceSchema>["details"]>[number]) {
  const mesh = deserializeMesh(detail.mesh),
    product = detail.vegetation;
  if (product) {
    if (
      mesh.thinCoverage?.format !== "coverage-normal" ||
      mesh.thinCoverage.key !== product.key ||
      new Set(product.sourceOrgans).size !== product.sourceOrgans.length
    )
      throw Error("Malformed crown payload identity");
    if (product.clusters) {
      const organs = product.clusters.flatMap((cluster) => cluster.sourceOrgans);
      if (
        new Set(organs).size !== organs.length ||
        organs.length !== product.sourceOrgans.length ||
        organs.some((organ) => !product.sourceOrgans.includes(organ))
      )
        throw Error("Malformed crown cluster ownership");
    }
    const owned = new Set<number>();
    for (const view of product.views) {
      if (
        Math.abs(Math.hypot(...view.direction) - 1) > 1e-4 ||
        view.firstIndex % 3 ||
        view.indexCount % 3 ||
        view.firstIndex + view.indexCount > mesh.indices.length
      )
        throw Error("Malformed crown view range");
      for (let i = view.firstIndex; i < view.firstIndex + view.indexCount; i++) {
        if (owned.has(i)) throw Error("Overlapping crown view ranges");
        owned.add(i);
      }
    }
    if (owned.size !== mesh.indices.length) throw Error("Unowned crown indices");
    const bytes =
      mesh.positions.byteLength +
      mesh.normals.byteLength +
      mesh.indices.byteLength +
      (mesh.colors?.byteLength ?? 0) +
      mesh.thinCoverage.uv.byteLength +
      (mesh.thinCoverage.layer?.byteLength ?? 0) +
      mesh.thinCoverage.levels.reduce((sum, level) => sum + level.byteLength, 0);
    if (bytes !== product.byteLength) throw Error("Incorrect crown byte ownership");
  }
  return { ...detail, mesh };
}
export function deserializeArtifact(input: unknown): SurfaceArtifact {
  const value = z.discriminatedUnion("kind", [surfaceSchema, characterSchema, vegetationSchema]).parse(input);
  if (value.kind === "vegetation")
    return {
      ...value,
      surfaces: value.surfaces.map((surface) => ({
        ...surface,
        mesh: deserializeMesh(surface.mesh),
        renderProducts:
          surface.renderProducts === undefined
            ? undefined
            : deserializeRenderProducts(surface.renderProducts, deserializeMesh),
        details: surface.details?.map(deserializeDetail),
      })),
    };
  const mesh = deserializeMesh(value.mesh);
  if (value.kind === "surface")
    return {
      ...value,
      mesh,
      renderProducts:
        value.renderProducts === undefined
          ? undefined
          : deserializeRenderProducts(value.renderProducts, deserializeMesh),
      details: value.details?.map(deserializeDetail),
    };
  const count = mesh.positions.length / 3;
  if (
    value.weights.length !== count * 4 ||
    value.jointIndices.length !== count * 4 ||
    value.jointIndices.some((index) => index >= value.joints.length)
  )
    throw new Error("Malformed cooked skin binding");
  for (let vertex = 0; vertex < count; vertex++) {
    const sum = value.weights.slice(vertex * 4, vertex * 4 + 4).reduce((a, b) => a + b, 0);
    if (Math.abs(sum - 1) > 1e-4) throw new Error("Cooked skin weights must sum to one");
  }
  const character = {
    ...value,
    mesh,
    weights: new Float32Array(value.weights),
    jointIndices: new Uint16Array(value.jointIndices),
  };
  return { ...character, ...deserializeCreatureProducts(input, character, deserializeMesh) };
}

/** Cooking is an explicit build step. Semantic source remains the authority;
 * baked triangles are replaceable delivery products with checked identities. */
export function cookProject(
  input: Project,
  quality: Quality = "export",
  compilerSource = BUNDLE_COMPILER_SOURCE ?? COMPILER_VERSION,
): CookedProject {
  const project = releaseProject(input);
  return assembleCook(
    project,
    quality,
    compilerSource,
    project.documents.map((document) => compileDocument(document, quality)),
  );
}
export async function cookProjectAsync(
  input: Project,
  quality: Quality,
  compile: CompileProvider,
  compilerSource = BUNDLE_COMPILER_SOURCE ?? COMPILER_VERSION,
): Promise<CookedProject> {
  const project = releaseProject(input);
  const artifacts = await Promise.all(
    project.documents.map((document) =>
      ["object", "character", "vegetation"].includes(document.kind)
        ? compile(document, quality)
        : Promise.resolve(null),
    ),
  );
  return assembleCook(project, quality, compilerSource, artifacts);
}
function assembleCook(
  project: Project,
  quality: Quality,
  compilerSource: string,
  compiled: (SurfaceArtifact | null)[],
): CookedProject {
  const entries: CookedProject["entries"] = [],
    artifacts: CookedProject["artifacts"] = {};
  for (let index = 0; index < project.documents.length; index++) {
    const document = project.documents[index],
      artifact = compiled[index];
    if (["object", "character", "vegetation"].includes(document.kind) && !artifact)
      throw new Error(`Compiler did not produce ${document.id}`);
    if (!artifact) continue;
    if (artifact.id !== document.id || artifact.key !== compilerKey(document, quality))
      throw new Error(`Compiler returned incompatible product ${document.id}`);
    const serialized = serializeArtifact(artifact),
      key = contentKey(serialized);
    artifacts[key] = serialized;
    entries.push({
      document: document.id,
      source: contentKey(document),
      products: productKeys(document, quality),
      artifact: key,
    });
  }
  return {
    format: COOK_FORMAT_VERSION,
    compiler: COMPILER_VERSION,
    compilerSource,
    productVersions: PRODUCT_VERSIONS,
    source: contentKey(project),
    quality,
    entries,
    artifacts,
  };
}
export function loadCookedProject(
  input: unknown,
  project: Project,
  compilerSource?: string,
): Map<string, SurfaceArtifact> {
  const bundle = z
    .object({
      format: z.literal(COOK_FORMAT_VERSION),
      compiler: z.literal(COMPILER_VERSION),
      compilerSource: z.string().min(1).max(128),
      productVersions: z.record(z.string(), z.string()),
      source: z.string(),
      quality: z.enum(["interactive", "review", "export"]),
      entries: z
        .array(
          z.object({
            document: z.string().max(100),
            source: z.string(),
            products: z.record(z.string(), z.string()),
            artifact: z.string(),
          }),
        )
        .max(100_000),
      artifacts: z.record(z.string(), z.unknown()),
    })
    .parse(input);
  if (
    bundle.source !== contentKey(releaseProject(project)) ||
    contentKey(bundle.productVersions) !== contentKey(PRODUCT_VERSIONS) ||
    (compilerSource && compilerSource !== bundle.compilerSource)
  )
    throw new Error("Cooked project source or compiler is incompatible");
  if (Object.keys(bundle.artifacts).length > 100_000) throw new Error("Cooked artifact count exceeds budget");
  const documents = new Map(project.documents.map((document) => [document.id, document])),
    result = new Map<string, SurfaceArtifact>();
  for (const entry of bundle.entries) {
    const document = documents.get(entry.document),
      serialized = bundle.artifacts[entry.artifact];
    if (
      !document ||
      result.has(entry.document) ||
      entry.source !== contentKey(document) ||
      entry.artifact !== contentKey(serialized) ||
      contentKey(entry.products) !== contentKey(productKeys(document, bundle.quality))
    )
      throw new Error("Cooked product identity mismatch");
    const artifact = deserializeArtifact(serialized),
      expectedKind = document.kind === "object" ? "surface" : document.kind;
    if (
      artifact.id !== document.id ||
      artifact.kind !== expectedKind ||
      artifact.key !== compilerKey(document, bundle.quality)
    )
      throw new Error("Cooked artifact does not match its source definition");
    if (
      artifact.kind === "surface" &&
      artifact.renderProducts?.some(
        (product) => product.sourceKey !== productKeys(document, bundle.quality).geometry,
      )
    )
      throw new Error("Cooked render product source mismatch");
    if (
      artifact.kind === "surface" &&
      artifact.opaqueVisibility &&
      artifact.opaqueVisibility.sourceKey !== productKeys(document, bundle.quality).geometry
    )
      throw new Error("Cooked visibility facts source mismatch");
    result.set(entry.document, artifact);
  }
  for (const document of project.documents)
    if (["object", "character", "vegetation"].includes(document.kind) && !result.has(document.id))
      throw new Error(`Cooked project is missing required product ${document.id}`);
  return result;
}
export function createCookedCompiler(
  bundle: CookedProject,
  project: Project,
  options: { compilerSource?: string; fallback?: CompileProvider } = {},
): CompileProvider {
  const products = loadCookedProject(bundle, project, options.compilerSource ?? BUNDLE_COMPILER_SOURCE);
  const sources = new Map(project.documents.map((document) => [document.id, contentKey(document)]));
  return async (document, quality) => {
    if (quality === bundle.quality && sources.get(document.id) === contentKey(document)) {
      const artifact = products.get(document.id);
      if (artifact) return artifact;
      if (!["object", "character", "vegetation"].includes(document.kind)) return null;
    }
    if (options.fallback) return options.fallback(document, quality);
    throw new Error(`No compatible cooked product for ${document.id} at ${quality} quality`);
  };
}
