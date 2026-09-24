import { contentKey, idSchema } from "@wrela/model";
import { z } from "zod";
import { capabilityIdSchema, creationRecipeSchema } from "./creation-recipe";
import { editRecipeSchema } from "./edit-recipe";
import { reviewVerdict } from "./review-contract";
import { materializeWorkProposal, parseWorkSession, type WorkSession } from "./work-session";

export const capabilityGapSchema = z.strictObject({
  version: z.literal(1),
  id: idSchema,
  work: idSchema,
  desiredResult: z.string().min(1).max(2000),
  attemptedComposition: z.string().min(1).max(2000),
  evidence: z.array(z.string().min(1)).min(1).max(16),
  solutionLevel: z.enum(["recipe", "operation", "compiler"]),
  reason: z.string().min(1).max(2000),
  engineeringMs: z.number().nonnegative(),
  capability: capabilityIdSchema.optional(),
  status: z.enum(["open", "local", "transferred", "published"]).default("open"),
});
export const toolkitEntrySchema = z.strictObject({
  version: z.literal(1),
  id: idSchema,
  revision: z.string().min(1).max(64),
  title: z.string().min(1).max(120),
  description: z.string().min(1).max(2000),
  tags: z.array(z.string().min(1).max(80)).min(1).max(24),
  level: z.enum(["recipe", "operation", "compiler"]),
  status: z.enum(["experimental", "validated", "approved"]).default("experimental"),
  implementation: z.discriminatedUnion("kind", [
    z.strictObject({ kind: z.literal("creation"), recipe: creationRecipeSchema }),
    z.strictObject({ kind: z.literal("edit"), recipe: editRecipeSchema }),
    z.strictObject({
      kind: z.literal("builtin"),
      capability: capabilityIdSchema,
      sourceKey: z.string().min(1),
      version: z.string().min(1),
    }),
  ]),
  controls: z
    .array(
      z.strictObject({
        name: z.string(),
        units: z.string(),
        min: z.number(),
        max: z.number(),
        description: z.string(),
      }),
    )
    .max(32),
  preserves: z.array(z.string().min(1)).max(32),
  limitations: z.array(z.string().min(1)).min(1).max(32),
  previews: z.array(z.string().min(1)).max(16),
  engineeringMs: z.number().nonnegative().nullable(),
  trials: z
    .array(
      z.strictObject({
        work: idSchema,
        proposal: idSchema,
        family: z.string(),
        sourceKey: z.string(),
        reviewKey: z.string(),
        elapsedMs: z.number().nonnegative(),
        accepted: z.boolean(),
        reviewer: z.string().nullable(),
      }),
    )
    .max(64)
    .default([]),
});
export type ToolkitEntry = z.infer<typeof toolkitEntrySchema>;
export function recordToolkitTrial(
  raw: ToolkitEntry,
  trial: { work: WorkSession; proposal: string; family: string; elapsedMs: number },
): ToolkitEntry {
  const entry = toolkitEntrySchema.parse(raw),
    work = parseWorkSession(trial.work),
    p = work.proposals.find((p) => p.id === trial.proposal);
  if (!p) throw Error("Trial proposal missing");
  const candidate = materializeWorkProposal(work, p.id);
  if (entry.implementation.kind === "builtin") {
    const capability = entry.implementation.capability;
    const present =
      capability === "assembly.revolved-shell"
        ? candidate.documents.some((d) => d.kind === "object" && d.assembly?.parts.some((p) => p.shell))
        : capability === "material.thin-glass"
          ? candidate.documents.some(
              (d) =>
                d.kind === "material" && d.appearance?.family === "glass" && d.appearance.transmission > 0,
            )
          : capability === "assembly.sweep"
            ? candidate.documents.some((d) => d.kind === "object" && d.assembly?.parts.some((p) => !p.shell))
            : false;
    if (!present) throw Error("Candidate does not use the registered built-in capability");
  }
  const review = p.reviews.at(-1);
  if (!review || !reviewVerdict(review, work.constraints, work.baselineKey, p.candidateKey).passed)
    throw Error("Toolkit trial needs source-bound passing review");
  const usage = work.events.some(
    (e) =>
      e.kind === "tool-development" &&
      e.summary === `${entry.id}@${entry.revision}` &&
      e.evidence.includes(p.candidateKey),
  );
  if (!usage) throw Error("Record capability usage against the candidate before qualifying a transfer");
  if (entry.trials.some((t) => t.sourceKey === p.candidateKey))
    throw Error("Duplicate candidate cannot count as a new transfer");
  return toolkitEntrySchema.parse({
    ...entry,
    trials: [
      ...entry.trials,
      {
        work: work.id,
        proposal: p.id,
        family: trial.family,
        sourceKey: p.candidateKey,
        reviewKey: contentKey(review),
        elapsedMs: trial.elapsedMs,
        accepted: p.decision?.verdict === "accepted",
        reviewer: p.decision?.reviewer ?? null,
      },
    ],
  });
}
export function promoteToolkitEntry(raw: ToolkitEntry, status: "validated" | "approved"): ToolkitEntry {
  const entry = toolkitEntrySchema.parse(raw),
    trials = status === "approved" ? entry.trials.filter((t) => t.accepted) : entry.trials;
  if (new Set(trials.map((t) => t.family)).size < 2 || new Set(trials.map((t) => t.sourceKey)).size < 2)
    throw Error("Promotion needs two distinct candidate sources from different asset families");
  if (status === "approved" && !entry.previews.length)
    throw Error("Approved capability needs retained visual examples");
  return { ...entry, status };
}
export function searchAuthoringToolkit(entries: ToolkitEntry[], query: string) {
  const words = query
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
  return entries
    .map((raw) => {
      const entry = toolkitEntrySchema.parse(raw),
        text = `${entry.title} ${entry.description} ${entry.tags.join(" ")}`.toLowerCase();
      return { entry, score: words.reduce((n, w) => n + Number(text.includes(w)), 0) };
    })
    .filter((v) => !words.length || v.score > 0)
    .sort((a, b) => b.score - a.score || a.entry.id.localeCompare(b.entry.id))
    .slice(0, 12)
    .map(({ entry, score }) => ({ ...entry, matchScore: score }));
}

/** Discovery is available in an empty workspace; qualification remains local and evidence based. */
export function builtinAuthoringToolkit(): ToolkitEntry[] {
  return [
    {
      id: "hollow-revolved-shell",
      title: "Hollow revolved shell",
      capability: "assembly.revolved-shell",
      level: "compiler" as const,
      description:
        "Revolve a closed, non-self-intersecting [radius,height] meridian around +Y. The contour includes outer wall, rim and inner wall. Use hero component kind shell with shell.profile and shell.segments; supports bell bodies, reservoirs and curved housings.",
      tags: ["shell", "hollow", "bell", "reservoir", "lathe"],
      controls: [
        {
          name: "segments",
          units: "radial segments",
          min: 12,
          max: 96,
          description: "Angular smoothness and triangle budget",
        },
      ],
      preserves: ["Real internal cavity", "Part identity, sockets and articulation"],
      limitations: [
        "Meridian radii must stay positive; no axis caps or arbitrary holes",
        "No intersecting profile edges",
      ],
    },
    {
      id: "assembly-sweep",
      title: "Editable component construction",
      capability: "assembly.sweep",
      level: "compiler" as const,
      description:
        "Generic hero components rod, beam, plate and ring lower to editable assembly source. Compose with position, rotation in radians, parent and hinge/slider joint. Use components to describe your own design, then iterate with a revised design or targeted operations.",
      tags: ["beam", "rod", "plate", "ring", "joint", "frame"],
      controls: [],
      preserves: ["Named components", "Material ownership", "Parent and joint relationships"],
      limitations: ["Bounded polygonal sweeps; no arbitrary sculpting or boolean subtraction"],
    },
    {
      id: "thin-glass",
      title: "Thin glass enclosure",
      capability: "material.thin-glass",
      level: "compiler" as const,
      description:
        "Hero material glass uses thin premultiplied transmission and IOR-based Fresnel; material.response adjusts transmission and roughness. Rear construction remains visible through the enclosure.",
      tags: ["glass", "transparency", "enclosure"],
      controls: [
        {
          name: "transmission",
          units: "fraction",
          min: 0,
          max: 1,
          description: "Light transmitted through a thin surface",
        },
      ],
      preserves: ["Opaque geometry depth", "Editable optical response"],
      limitations: [
        "No thick refraction, caustics or volumetric absorption",
        "Distance sorting can fail for intersecting transparent surfaces or overlapping water",
      ],
    },
  ].map(({ capability, ...entry }) =>
    toolkitEntrySchema.parse({
      version: 1,
      ...entry,
      revision: "1",
      status: "experimental",
      implementation: {
        kind: "builtin",
        capability,
        version: "1",
        sourceKey: contentKey({ capability, version: 1 }),
      },
      previews: [],
      engineeringMs: null,
      trials: [],
    }),
  );
}
