import {
  contentKey,
  type Document,
  jointSchema,
  type MeshData,
  motionSchema,
  type Project,
  parseProject,
  type Quality,
  type SurfaceArtifact,
  vec3Schema,
} from "@wrela/model";
import { z } from "zod";
import { compileDocument, compilerKey } from "./index";
import { productKeys } from "./products";
import { BUNDLE_COMPILER_SOURCE, COMPILER_VERSION, COOK_FORMAT_VERSION, PRODUCT_VERSIONS } from "./versions";

type SerializedMesh = Omit<MeshData, "positions" | "normals" | "indices" | "colors"> & {
  positions: number[];
  normals: number[];
  indices: number[];
  colors?: number[];
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
  return {
    ...mesh,
    positions: Array.from(mesh.positions),
    normals: Array.from(mesh.normals),
    indices: Array.from(mesh.indices),
    colors: mesh.colors ? Array.from(mesh.colors) : undefined,
  };
}
export function serializeArtifact(artifact: SurfaceArtifact): SerializedArtifact {
  if (artifact.kind === "vegetation")
    return { ...artifact, surfaces: artifact.surfaces.map(serializeArtifact) };
  return {
    ...artifact,
    mesh: serializeMesh(artifact.mesh),
    ...(artifact.kind === "character"
      ? {
          weights: Array.from(artifact.weights),
          jointIndices: Array.from(artifact.jointIndices),
        }
      : { details: artifact.details?.map((detail) => ({ ...detail, mesh: serializeMesh(detail.mesh) })) }),
  };
}
const finite = z.number().finite();
const float32 = z.number().finite().min(-3.4028234663852886e38).max(3.4028234663852886e38);
const bounds = z.object({ min: vec3Schema, max: vec3Schema });
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
const meshSchema = z.object({
  positions: z.array(float32).max(750000),
  normals: z.array(float32).max(750000),
  indices: z.array(z.number().int().nonnegative().max(249999)).max(1500000),
  colors: z.array(float32).max(750000).optional(),
  sourceIds: z.array(z.string().max(100)).max(250000).optional(),
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
    (mesh.colors && mesh.colors.length !== mesh.positions.length) ||
    (mesh.sourceIds && mesh.sourceIds.length !== count) ||
    mesh.indices.some((index) => index >= count) ||
    mesh.bounds.min.some((value, axis) => value > mesh.bounds.max[axis])
  )
    throw new Error("Malformed cooked mesh layout");
  if (mesh.materialGroups) {
    let offset = 0;
    for (const group of mesh.materialGroups) {
      if (group.start !== offset || group.count % 3) throw new Error("Malformed cooked material ranges");
      offset += group.count;
    }
    if (offset !== mesh.indices.length) throw new Error("Incomplete cooked material ranges");
  }
  for (let index = 0; index < mesh.positions.length; index++) {
    const axis = index % 3,
      value = mesh.positions[index],
      tolerance = Math.max(1e-5, Math.abs(value) * 1e-6);
    if (value < mesh.bounds.min[axis] - tolerance || value > mesh.bounds.max[axis] + tolerance)
      throw new Error("Cooked vertex lies outside its declared bounds");
  }
  return {
    ...mesh,
    positions: new Float32Array(mesh.positions),
    normals: new Float32Array(mesh.normals),
    indices: new Uint32Array(mesh.indices),
    colors: mesh.colors ? new Float32Array(mesh.colors) : undefined,
  };
}
const surfaceEnvelope = { id: z.string().min(1).max(200), key: z.string().min(1).max(128), diagnostics };
const surfaceSchema = z.object({
  ...surfaceEnvelope,
  kind: z.literal("surface"),
  mesh: z.unknown(),
  material: z.string().min(1).max(100),
  details: z
    .array(
      z.object({
        label: z.string().max(100),
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
  jointIndices: z.array(z.number().int().nonnegative().max(63)).max(1000000),
  weights: z.array(z.number().finite().min(0).max(1)).max(1000000),
});
const vegetationSchema = z.object({
  ...surfaceEnvelope,
  kind: z.literal("vegetation"),
  surfaces: z.array(surfaceSchema).length(2),
  windResponse: z.number().min(0).max(2),
  maxDisplacement: z.number().finite().nonnegative().max(100),
  bounds,
});
export function deserializeArtifact(input: unknown): SurfaceArtifact {
  const value = z.discriminatedUnion("kind", [surfaceSchema, characterSchema, vegetationSchema]).parse(input);
  if (value.kind === "vegetation")
    return {
      ...value,
      surfaces: value.surfaces.map((surface) => ({
        ...surface,
        mesh: deserializeMesh(surface.mesh),
        details: surface.details?.map((detail) => ({ ...detail, mesh: deserializeMesh(detail.mesh) })),
      })),
    };
  const mesh = deserializeMesh(value.mesh);
  if (value.kind === "surface")
    return {
      ...value,
      mesh,
      details: value.details?.map((detail) => ({ ...detail, mesh: deserializeMesh(detail.mesh) })),
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
  return {
    ...value,
    mesh,
    weights: new Float32Array(value.weights),
    jointIndices: new Uint16Array(value.jointIndices),
  };
}

/** Cooking is an explicit build step. Semantic source remains the authority;
 * baked triangles are replaceable delivery products with checked identities. */
export function cookProject(
  input: Project,
  quality: Quality = "export",
  compilerSource = BUNDLE_COMPILER_SOURCE ?? COMPILER_VERSION,
): CookedProject {
  const project = parseProject(input);
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
  const project = parseProject(input);
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
        .max(256),
      artifacts: z.record(z.string(), z.unknown()),
    })
    .parse(input);
  if (
    bundle.source !== contentKey(project) ||
    contentKey(bundle.productVersions) !== contentKey(PRODUCT_VERSIONS) ||
    (compilerSource && compilerSource !== bundle.compilerSource)
  )
    throw new Error("Cooked project source or compiler is incompatible");
  if (Object.keys(bundle.artifacts).length > 256) throw new Error("Cooked artifact count exceeds budget");
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
    result.set(entry.document, artifact);
  }
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
