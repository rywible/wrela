import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import {
  FRONTIER_QUALITY_COORDINATES,
  type FrontierCandidate,
  type FrontierPolicy,
  frontierProductKey,
} from "@wrela/examples/render-frontier";
import { contentKey } from "@wrela/model/math";
import { z } from "zod";

const text = z.string().min(1),
  nonnegative = z.number().finite().nonnegative();
const metric = z.enum([
  "depth",
  "silhouette",
  "normal-angle",
  "linear-radiance",
  "transmittance",
  "temporal",
]);
const error = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("unknown"), reason: text }),
  z.strictObject({ kind: z.literal("numeric-bound"), metric, maximum: nonnegative, domain: text }),
  z.strictObject({
    kind: z.literal("real-bound"),
    metric,
    maximum: nonnegative,
    domain: text,
    numericError: z.literal("unknown"),
  }),
  z.strictObject({
    kind: z.literal("measured"),
    metric,
    rms: nonnegative,
    maximum: nonnegative,
    domain: text,
    reference: text,
    samples: z.number().int().positive(),
  }),
  z.strictObject({
    kind: z.literal("measured-coverage"),
    rms: nonnegative,
    maximum: nonnegative,
    domain: text,
    reference: text,
    samples: z.number().int().positive(),
  }),
]);
const product = z.object({ key: text, sourceKey: text, algorithmVersion: text, domainKey: text });
export const frontierPolicySchema = z.strictObject({
  budgets: z.partialRecord(z.enum(FRONTIER_QUALITY_COORDINATES), nonnegative),
  costAxes: z
    .array(z.enum(["gpuMs", "cpuMs", "ownedBytes"]))
    .min(1)
    .max(3),
});
export const frontierCandidateSchema = z.strictObject({
  id: text,
  label: text,
  products: z.array(product).max(256),
  domain: z.strictObject({
    sourceKey: text,
    fixture: text,
    cameraKey: text,
    lightingKey: text,
    motionKey: text,
    width: z.number().int(),
    height: z.number().int(),
    adapter: text,
    browser: text,
    kernelVersion: text,
    referenceKey: text,
    errorDomain: text,
    cpuScope: z.enum(["preparation", "render-p95"]),
    memoryScope: z.enum(["selected-payload", "renderer-retained"]),
  }),
  quality: z
    .array(
      z.strictObject({
        coordinate: z.enum(FRONTIER_QUALITY_COORDINATES),
        units: z.enum(["linear-radiance", "pixels", "fractional-coverage", "linear-radiance-difference"]),
        evidence: error,
        uncertainty: nonnegative.nullable(),
      }),
    )
    .max(16),
  cost: z.strictObject({
    observation: z
      .strictObject({
        productKey: text,
        adapter: text,
        browser: text,
        kernelVersion: text,
        fixture: text,
        gpuP50Ms: nonnegative,
        gpuP95Ms: nonnegative,
        preparationMs: nonnegative,
      })
      .nullable(),
    cpuMs: nonnegative.nullable(),
    ownedBytes: nonnegative.nullable(),
    samples: z.number().int().nonnegative(),
    uncertainty: z.strictObject({
      gpuMs: nonnegative.nullable(),
      cpuMs: nonnegative.nullable(),
      ownedBytes: nonnegative.nullable(),
    }),
  }),
  provenance: z
    .array(z.strictObject({ artifact: text, pointer: text }))
    .min(1)
    .max(32),
});
export const frontierEvidenceBundleSchema = z.strictObject({
  schemaVersion: z.literal(1),
  policy: frontierPolicySchema,
  candidates: z.array(frontierCandidateSchema).min(1).max(128),
  notes: z.array(z.string()).default([]),
});
export type FrontierEvidenceBundle = z.infer<typeof frontierEvidenceBundleSchema>;

function pointerValue(value: unknown, pointer: string): unknown {
  if (pointer === "#") return value;
  if (!pointer.startsWith("/"))
    throw Error("Evidence pointers must be JSON pointers or # for the whole document");
  for (const raw of pointer.slice(1).split("/")) {
    const part = raw.replace(/~1/g, "/").replace(/~0/g, "~");
    if (!value || typeof value !== "object" || !Object.hasOwn(value, part))
      throw Error(`Evidence pointer ${pointer} does not exist`);
    value = (value as Record<string, unknown>)[part];
  }
  return value;
}
export async function verifyFrontierProvenance(candidates: FrontierCandidate[], base: string) {
  const artifacts = new Map<string, { json: unknown; sha256: string; bytes: number }>();
  for (const candidate of candidates)
    for (const source of candidate.provenance) {
      const path = resolve(base, source.artifact);
      let artifact = artifacts.get(path);
      if (!artifact) {
        const bytes = await readFile(path);
        if (bytes.byteLength > 64 * 1024 * 1024) throw Error("Evidence file exceeds 64 MiB budget");
        artifact = {
          json: JSON.parse(bytes.toString("utf8")),
          sha256: new Bun.CryptoHasher("sha256").update(bytes).digest("hex"),
          bytes: bytes.byteLength,
        };
        artifacts.set(path, artifact);
      }
      pointerValue(artifact.json, source.pointer);
    }
  return [...artifacts].map(([path, value]) => ({ path, sha256: value.sha256, bytes: value.bytes }));
}

const reliefReportSchema = z.object({
  reports: z.array(
    z.object({
      choice: z.enum(["automatic", "forced-near"]),
      variant: text,
      shot: text,
      camera: z.unknown(),
      geometry: z.array(
        z.object({
          selected: z.object({ key: text }).optional(),
          candidates: z
            .array(
              product.extend({
                kind: text,
                errors: z.array(error),
                projectedGeometryErrors: z.array(z.object({ metric: text, maximumPixels: nonnegative })),
              }),
            )
            .optional(),
        }),
      ),
      timing: z.object({
        preparationMs: nonnegative,
        gpuP50Ms: nonnegative.nullable(),
        gpuP95Ms: nonnegative.nullable(),
        cpuP95Ms: nonnegative,
        gpuSamples: z.number().int().nonnegative(),
      }),
      measurements: z.object({
        adapter: text,
        gpuBytes: nonnegative,
        outputResolution: z.tuple([z.number().int().positive(), z.number().int().positive()]),
      }),
      completeness: z.object({ complete: z.boolean() }),
      failures: z.array(z.string()),
    }),
  ),
});

/** Native matched relief reports retain useful costs while missing image/uncertainty evidence stays unknown. */
export async function readReliefFrontierEvidence(
  path: string,
  policy: FrontierPolicy,
): Promise<FrontierEvidenceBundle> {
  const absolute = resolve(path),
    source = await readFile(absolute, "utf8"),
    report = reliefReportSchema.parse(JSON.parse(source));
  const captureKey = contentKey(source);
  const authoredPath = resolve(dirname(absolute), "source-relief.json");
  const authored = JSON.parse(await readFile(authoredPath, "utf8"));
  const choices = report.reports
    .map((value, index) => ({ value, index }))
    .filter(({ value }) => value.variant === "physical-relief" && value.shot === "far-clay");
  if (
    !choices.some(({ value }) => value.choice === "forced-near") ||
    !choices.some(({ value }) => value.choice === "automatic")
  )
    throw Error("Relief evidence requires matched far-clay automatic and forced-near captures");
  const candidates: FrontierCandidate[] = choices.map(({ value, index }) => {
    if (!value.completeness.complete || value.failures.length)
      throw Error("Incomplete relief evidence cannot enter a frontier report");
    const selected = value.geometry.map((surface) => {
      const selected = surface.candidates?.find((candidate) => candidate.key === surface.selected?.key);
      if (!selected) throw Error("Selected compiled product metadata is missing");
      return selected;
    });
    const products = selected.map(({ key, sourceKey, algorithmVersion, domainKey }) => ({
      key,
      sourceKey,
      algorithmVersion,
      domainKey,
    }));
    const cameraKey = contentKey(value.camera),
      errorDomain = contentKey({ captureKey, cameraKey, shot: value.shot });
    const referenceKey = contentKey(
      value.geometry
        .flatMap((surface) =>
          (surface.candidates ?? [])
            .filter((candidate) => candidate.kind === "direct-mesh")
            .map((candidate) => candidate.key),
        )
        .sort(),
    );
    const projected = selected.map(
      (product) =>
        product.projectedGeometryErrors.find((error) => error.metric === "silhouette")?.maximumPixels,
    );
    const numeric = selected.every((product) =>
      product.errors.some((error) => error.kind === "numeric-bound" && error.metric === "silhouette"),
    );
    const domain = {
      sourceKey: contentKey(authored),
      fixture: "surface-relief-far-clay",
      cameraKey,
      lightingKey: contentKey(
        authored.documents?.filter((document: { kind: string }) =>
          ["environment", "lighting", "stage"].includes(document.kind),
        ),
      ),
      motionKey: "static-time-zero",
      width: value.measurements.outputResolution[0],
      height: value.measurements.outputResolution[1],
      adapter: value.measurements.adapter,
      browser: `same-capture-browser-${captureKey}`,
      kernelVersion: `same-capture-kernel-${captureKey}`,
      referenceKey,
      errorDomain,
      cpuScope: "render-p95" as const,
      memoryScope: "renderer-retained" as const,
    };
    return {
      id: value.choice,
      label: `Relief ${value.choice}`,
      products,
      domain,
      quality: [
        {
          coordinate: "silhouette-max-pixels",
          units: "pixels",
          evidence: projected.every((value) => value !== undefined)
            ? numeric
              ? {
                  kind: "numeric-bound",
                  metric: "silhouette",
                  maximum: Math.max(...(projected as number[])),
                  domain: errorDomain,
                }
              : {
                  kind: "real-bound",
                  metric: "silhouette",
                  maximum: Math.max(...(projected as number[])),
                  domain: errorDomain,
                  numericError: "unknown",
                }
            : { kind: "unknown", reason: "Projected geometry bound is unavailable" },
          uncertainty: numeric ? 0 : null,
        },
      ],
      cost: {
        observation:
          value.timing.gpuP50Ms === null || value.timing.gpuP95Ms === null
            ? null
            : {
                productKey: frontierProductKey(products),
                adapter: domain.adapter,
                browser: domain.browser,
                kernelVersion: domain.kernelVersion,
                fixture: domain.fixture,
                gpuP50Ms: value.timing.gpuP50Ms,
                gpuP95Ms: value.timing.gpuP95Ms,
                preparationMs: value.timing.preparationMs,
              },
        cpuMs: value.timing.cpuP95Ms,
        ownedBytes: value.measurements.gpuBytes,
        samples: value.timing.gpuSamples,
        uncertainty: { gpuMs: null, cpuMs: null, ownedBytes: 0 },
      },
      provenance: [
        { artifact: absolute, pointer: `/reports/${index}` },
        { artifact: authoredPath, pointer: "#" },
      ],
    };
  });
  return {
    schemaVersion: 1,
    policy,
    candidates,
    notes: [
      "Imported actual matched far-clay automatic/forced-near relief captures. Source and selected compiled products remain linked to their retained artifacts.",
      "Old reports did not record browser/kernel version; identities are confined to this single capture artifact and cannot compare across runs.",
      "GPU/CPU point observations are retained, but timing uncertainty and linear radiance/coverage/temporal evidence are absent. They remain inadmissible where required. A projected real-arithmetic bound retains unknown numeric error.",
      "Renderer retained GPU bytes include cached resources; selecting a smaller geometry does not imply allocations were released. Near identity bounds concern the realized reference, not ideal authored relief.",
    ],
  };
}
