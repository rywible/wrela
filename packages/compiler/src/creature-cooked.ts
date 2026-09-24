import { type CompiledCharacter, type MeshData, materialSchema, vec3Schema } from "@wrela/model";

import { z } from "zod";

type MeshWriter = (mesh: MeshData) => unknown;
type MeshReader = (value: unknown) => MeshData;
const id = z.string().min(1).max(200);
const finite = z.number().finite();
const nonnegative = finite.nonnegative();
const vertex = z.number().int().min(0).max(249999);
const integers = z.array(vertex).max(250000);
const floats = z.array(finite).max(1000000);
const coordinate = z.object({
  region: id,
  chart: id,
  chartRevision: z.number().int().nonnegative(),
  coordinates: vec3Schema,
  id: id.optional(),
  layer: id.optional(),
});
const diagnostic = z.object({
  severity: z.enum(["warning", "error", "info"]),
  code: z.string().max(128),
  message: z.string().max(4096),
  document: id.optional(),
  node: id.optional(),
});
const corrective = z.object({
  id,
  region: id,
  joint: id,
  axis: z.enum(["x", "y", "z"]),
  angle: finite,
  vertices: integers,
  displacements: floats,
});
const guide = z.object({
  rootProjection: z
    .object({
      domain: z.literal("compiled-body"),
      chartPosition: vec3Schema,
      sourceNode: id.optional(),
      distance: finite,
      interval: z.tuple([finite, finite]).optional(),
    })
    .optional(),
  representation: z.enum(["tufts", "ribbons"]).optional(),
  ribbonThickness: finite.min(0.001).max(1).optional(),
  widthDirection: vec3Schema.optional(),
  root: coordinate.extend({ id, layer: id }),
  points: z.array(vec3Schema).min(2).max(128),
  normal: vec3Schema,
  width: nonnegative,
  taper: finite.min(0).max(1),
  material: id,
  rootColor: vec3Schema,
  tipColor: vec3Schema,
  stiffness: nonnegative,
  damping: nonnegative,
});
const detail = z.object({
  label: z.string().max(100),
  mesh: z.unknown(),
  jointIndices: integers,
  weights: floats,
  maxProjectedDiameter: finite.positive(),
  maxError: z.null(),
  correctives: z.array(corrective).max(256).optional(),
  groomGuideIndices: integers.optional(),
});
const productsSchema = z.object({
  creatureSourceKey: id.optional(),
  creatureCoordinates: z.array(coordinate.nullable()).max(250000).optional(),
  creatureRegions: z.array(id.nullable()).max(250000).optional(),
  creatureCorrectives: z.array(corrective).max(256).optional(),
  creatureAnchors: z
    .array(
      z.object({
        id,
        status: z.enum(["resolved", "invalid", "ambiguous"]),
        position: vec3Schema.optional(),
        normal: vec3Schema.optional(),
        residual: nonnegative.nullable(),
        confidence: finite.min(0).max(1),
        diagnostics: z.array(diagnostic).max(128),
      }),
    )
    .max(1024)
    .optional(),
  creatureMaterials: z.array(materialSchema).max(256).optional(),
  creatureGroomMaterialSources: z.record(id, id).optional(),
  creatureBodyVertexCount: z.number().int().nonnegative().max(250000).optional(),
  creatureGroomDetail: z.string().min(1).max(100).optional(),
  creatureGroom: z
    .object({
      key: id,
      guides: z.array(guide).max(8192),
      representation: z.enum(["opaque-tufts", "opaque-ribbons", "mixed-opaque"]),
      diagnostics: z.array(diagnostic).max(1024),
      details: z
        .array(
          z.object({
            label: z.string().min(1).max(100),
            mesh: z.unknown(),
            fidelity: z
              .object({
                scope: z.literal("rest-guide-envelope"),
                sourceGuides: nonnegative,
                retainedGuides: nonnegative,
                tipBoundsError: vec3Schema.nullable(),
                coverageError: z.null(),
                drawGroups: nonnegative,
                realizations: z
                  .array(z.enum(["tufts", "ribbons"]))
                  .max(2)
                  .optional(),
                maxGuideInterpolationError: nonnegative.optional(),
              })
              .optional(),
            vertexGuideIndices: integers,
            guideIds: z.array(id).max(8192),
            cost: z.object({
              vertices: nonnegative,
              triangles: nonnegative,
              bytes: nonnegative,
              guides: nonnegative,
            }),
            maxError: z.null(),
          }),
        )
        .max(6),
    })
    .optional(),
  creatureDetails: z.array(detail).max(6).optional(),
});

export function serializeCreatureProducts(artifact: CompiledCharacter, mesh: MeshWriter) {
  return {
    creatureSourceKey: artifact.creatureSourceKey,
    creatureCoordinates: artifact.creatureCoordinates,
    creatureRegions: artifact.creatureRegions,
    creatureAnchors: artifact.creatureAnchors,
    creatureMaterials: artifact.creatureMaterials,
    creatureGroomMaterialSources: artifact.creatureGroomMaterialSources,
    creatureBodyVertexCount: artifact.creatureBodyVertexCount,
    creatureGroomDetail: artifact.creatureGroomDetail,
    creatureCorrectives: artifact.creatureCorrectives?.map((c) => ({
      ...c,
      vertices: Array.from(c.vertices),
      displacements: Array.from(c.displacements),
    })),
    creatureGroom: artifact.creatureGroom && {
      ...artifact.creatureGroom,
      details: artifact.creatureGroom.details.map((d) => ({
        ...d,
        mesh: mesh(d.mesh),
        vertexGuideIndices: Array.from(d.vertexGuideIndices),
      })),
    },
    creatureDetails: artifact.creatureDetails?.map((d) => ({
      ...d,
      mesh: mesh(d.mesh),
      jointIndices: Array.from(d.jointIndices),
      weights: Array.from(d.weights),
      groomGuideIndices: d.groomGuideIndices ? Array.from(d.groomGuideIndices) : undefined,
      correctives: d.correctives?.map((c) => ({
        ...c,
        vertices: Array.from(c.vertices),
        displacements: Array.from(c.displacements),
      })),
    })),
  };
}

export function deserializeCreatureProducts(input: unknown, base: CompiledCharacter, mesh: MeshReader) {
  const value = productsSchema.parse(input),
    count = base.mesh.positions.length / 3;
  if (!base.creature && Object.values(value).some((v) => v !== undefined))
    throw new Error("Cooked creature products require anatomical source");
  const regions = new Set(base.creature?.regions.map((r) => r.id));
  const charts = new Map(base.creature?.charts.map((c) => [c.id, c]));
  const joints = new Set(base.joints.map((j) => j.id));
  if (value.creatureCoordinates && value.creatureCoordinates.length !== count)
    throw new Error("Cooked creature correspondence length mismatch");
  if (value.creatureRegions && value.creatureRegions.length !== count)
    throw new Error("Cooked creature region length mismatch");
  if (value.creatureRegions?.some((r) => r !== null && !regions.has(r)))
    throw new Error("Cooked creature region is missing from source");
  for (const c of value.creatureCoordinates ?? [])
    if (c) {
      const chart = charts.get(c.chart);
      if (!chart || chart.region !== c.region || chart.revision !== c.chartRevision)
        throw new Error("Cooked creature correspondence is stale");
    }
  if ((value.creatureBodyVertexCount ?? 0) > count) throw new Error("Cooked creature body range is invalid");
  const readCorrective = (c: z.infer<typeof corrective>, vertexCount: number) => {
    if (
      !regions.has(c.region) ||
      !joints.has(c.joint) ||
      c.displacements.length !== c.vertices.length * 3 ||
      c.vertices.some((v) => v >= vertexCount)
    )
      throw new Error("Malformed cooked creature corrective");
    return { ...c, vertices: new Uint32Array(c.vertices), displacements: new Float32Array(c.displacements) };
  };
  const correctives = value.creatureCorrectives?.map((c) => readCorrective(c, count));
  const groom = value.creatureGroom && {
    ...value.creatureGroom,
    details: value.creatureGroom.details.map((d) => {
      const m = mesh(d.mesh);
      if (
        d.vertexGuideIndices.length !== m.positions.length / 3 ||
        d.vertexGuideIndices.some((i) => i >= value.creatureGroom!.guides.length)
      )
        throw new Error("Malformed cooked groom guide binding");
      return { ...d, mesh: m, vertexGuideIndices: new Uint32Array(d.vertexGuideIndices) };
    }),
  };
  if (groom)
    for (const g of groom.guides) {
      const chart = charts.get(g.root.chart);
      if (!chart || chart.region !== g.root.region || chart.revision !== g.root.chartRevision)
        throw new Error("Cooked groom root has stale correspondence");
    }
  const details = value.creatureDetails?.map((d) => {
    const m = mesh(d.mesh),
      n = m.positions.length / 3;
    if (
      d.jointIndices.length !== n * 4 ||
      d.weights.length !== n * 4 ||
      d.jointIndices.some((i) => i >= base.joints.length) ||
      d.weights.some((w) => w < 0 || w > 1)
    )
      throw new Error("Malformed cooked creature detail binding");
    for (let i = 0; i < n; i++) {
      const sum = d.weights.slice(i * 4, i * 4 + 4).reduce((a, b) => a + b, 0);
      if (Math.abs(sum - 1) > 1e-4) throw new Error("Cooked creature detail weights must sum to one");
    }
    if (
      d.groomGuideIndices &&
      (d.groomGuideIndices.length !== n - (value.creatureBodyVertexCount ?? n) ||
        d.groomGuideIndices.some((i) => i >= (groom?.guides.length ?? 0)))
    )
      throw new Error("Malformed cooked detail groom mapping");
    return {
      ...d,
      mesh: m,
      jointIndices: new Uint16Array(d.jointIndices),
      weights: new Float32Array(d.weights),
      correctives: d.correctives?.map((c) => readCorrective(c, n)),
      groomGuideIndices: d.groomGuideIndices ? new Uint32Array(d.groomGuideIndices) : undefined,
    };
  });
  return { ...value, creatureCorrectives: correctives, creatureGroom: groom, creatureDetails: details };
}
