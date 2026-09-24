import { contentKey, idSchema, type Project, type Vec3 } from "@wrela/model";
import { z } from "zod";
import type { Operation } from "./commands";
import { operationSchema } from "./commands";
import { sourceProperty } from "./discovery";
import { heroDesignSchema, heroRevisionOperations } from "./hero-design";
import { heroReviewViews } from "./hero-study";
import { reviewVerdict } from "./review-contract";
import { authoringTaskContext, reviewViewsSchema } from "./task-context";
import {
  type EvaluationStore,
  evaluateWork,
  type PacketSettings,
  type ReviewPacket,
} from "./work-evaluation";
import { appendWorkEvent, materializeWorkProposal, type WorkSession } from "./work-session";

const path = z.array(z.union([z.string().min(1), z.number().int().nonnegative()])).max(8);
export const authoringPinSchema = z.strictObject({
  target: idSchema,
  path,
  reason: z.string().min(1).max(300),
});
const repairBase = { target: idSchema };
export const visualRepairSchema = z.discriminatedUnion("kind", [
  z.strictObject({
    ...repairBase,
    kind: z.literal("timber.grain-origin"),
    parts: z.array(idSchema).min(1).max(128),
    amount: z.number().min(0).max(1),
    seed: z.number().int().default(73),
  }),
  z.strictObject({
    ...repairBase,
    kind: z.literal("assembly.edge-wear"),
    parts: z.array(idSchema).min(1).max(128),
    amount: z.number().min(0).max(1),
  }),
  z.strictObject({
    ...repairBase,
    kind: z.literal("vegetation.branch-gap"),
    branches: z.array(idSchema).min(1).max(64),
    bare: z.boolean().default(true),
    lengthScale: z.number().min(0.05).max(2).default(1),
  }),
  z.strictObject({
    ...repairBase,
    kind: z.literal("material.response"),
    roughness: z.number().min(0.04).max(1).optional(),
    transmission: z.number().min(0).max(1).optional(),
    weathering: z.number().min(0).max(1).optional(),
  }),
]);
export type VisualRepair = z.input<typeof visualRepairSchema>;
/** Each repair only emits the properties named by its semantic operation. */
export function planVisualRepair(project: Project, raw: VisualRepair): Operation[] {
  const repair = visualRepairSchema.parse(raw),
    doc = project.documents.find((d) => d.id === repair.target),
    operations: Operation[] = [];
  if (!doc) throw Error("Repair target missing");
  const set = (path: (string | number)[], value: unknown) =>
    operations.push({ kind: "document.set", target: doc.id, path, value });
  if (repair.kind === "timber.grain-origin" || repair.kind === "assembly.edge-wear") {
    if (doc.kind !== "object" || !doc.assembly) throw Error("Repair needs an assembly");
    for (const id of repair.parts) {
      const index = doc.assembly.parts.findIndex((p) => p.id === id);
      if (index < 0) throw Error(`Unknown part ${id}`);
      const part = doc.assembly.parts[index];
      if (repair.kind === "assembly.edge-wear") {
        if (part.shell || part.profile.kind !== "rectangle" || part.path.length !== 2 || part.bevel <= 0)
          throw Error("Edge wear needs a beveled straight rectangular part");
        set(["assembly", "parts", index, "edgeWear"], repair.amount);
      } else {
        const material = project.documents.find((d) => d.id === (part.material ?? doc.material));
        if (material?.kind !== "material" || material.appearance?.detail.kind !== "wood")
          throw Error("Grain repair requires a wood material");
        const random = (axis: number) =>
          Number.parseInt(contentKey([id, repair.seed, axis]).slice(0, 6), 16) / 0xffffff;
        set(["assembly", "parts", index, "grainOffset"], [
          (random(0) - 0.5) * 0.35 * repair.amount,
          random(1) * 7 * repair.amount,
          (random(2) - 0.5) * 0.35 * repair.amount,
        ] as Vec3);
      }
    }
  } else if (repair.kind === "vegetation.branch-gap") {
    if (doc.kind !== "vegetation" || !doc.botanical || doc.botanical.development)
      throw Error("Branch repair needs authored non-developmental vegetation");
    const edits = structuredClone(doc.botanical.branchEdits);
    for (const branch of repair.branches) {
      const old = edits.find((e) => e.branch === branch);
      if (old) {
        old.bare = repair.bare;
        old.lengthScale = repair.lengthScale;
      } else edits.push({ branch, bare: repair.bare, lengthScale: repair.lengthScale, bend: [0, 0, 0] });
    }
    set(["botanical", "branchEdits"], edits);
  } else {
    if (doc.kind !== "material") throw Error("Response repair needs a material target");
    if (repair.roughness !== undefined) set(["roughness"], repair.roughness);
    for (const key of ["transmission", "weathering"] as const)
      if (repair[key] !== undefined) {
        if (!doc.appearance) throw Error("Material has no appearance source");
        set(["appearance", key], repair[key]);
      }
    if (!operations.length) throw Error("Specify a material response change");
  }
  return operations;
}
export const iterationRequestSchema = z
  .strictObject({
    expectedKey: z.string(),
    proposal: idSchema,
    target: idSchema,
    baseProposal: idSchema.optional(),
    repair: visualRepairSchema.optional(),
    operations: z.array(operationSchema).min(1).max(256).optional(),
    design: heroDesignSchema.optional(),
    combine: z.strictObject({ proposal: idSchema, materials: z.array(idSchema).min(1).max(16) }).optional(),
    pins: z.array(authoringPinSchema).max(64).default([]),
    critique: z.string().max(2000).default(""),
    views: reviewViewsSchema.optional(),
    width: z.number().int().min(128).max(1024).default(640),
    height: z.number().int().min(128).max(768).default(480),
  })
  .superRefine((v, c) => {
    if ([v.repair, v.operations, v.design, v.combine].filter(Boolean).length !== 1)
      c.addIssue({
        code: "custom",
        message: "Choose one creation, repair, operation batch or material combination",
      });
  });
export type IterationRequest = z.input<typeof iterationRequestSchema>;
export type PacketReviewer = (
  baseline: Project,
  candidate: Project,
  work: WorkSession,
  settings: PacketSettings,
) => Promise<ReviewPacket>;
export function planAuthoringIteration(work: WorkSession, raw: IterationRequest) {
  const input = iterationRequestSchema.parse(raw);
  if (contentKey(work) !== input.expectedKey) throw Error("Work changed before iteration");
  const base = input.baseProposal ? materializeWorkProposal(work, input.baseProposal) : work.baseline;
  const prior = input.baseProposal
    ? work.proposals.find((p) => p.id === input.baseProposal)!.batch.operations
    : [];
  let operations =
    input.operations ??
    (input.repair
      ? planVisualRepair(base, input.repair)
      : input.design
        ? heroRevisionOperations(base, input.design)
        : []);
  if (input.combine) {
    const donor = materializeWorkProposal(work, input.combine.proposal);
    operations = input.combine.materials.flatMap((id) => {
      const from = donor.documents.find((d) => d.id === id),
        to = base.documents.find((d) => d.id === id);
      if (from?.kind !== "material" || to?.kind !== "material")
        throw Error("Combination requires the same material identity in both branches");
      for (const key of ["appearance", "emission"] as const)
        if (to[key] !== undefined && from[key] === undefined)
          throw Error(
            `Material combination cannot remove ${key}; use an explicit neutral response in the donor`,
          );
      return Object.entries(from)
        .filter(
          ([key, value]) =>
            !["id", "kind", "schemaVersion", "generated", "dependencies"].includes(key) &&
            contentKey(value) !== contentKey((to as unknown as Record<string, unknown>)[key] ?? null),
        )
        .map(([key, value]): Operation => ({ kind: "document.set", target: id, path: [key], value }));
    });
  }
  return { input, base, operations: [...prior, ...operations] };
}
export async function iterateAuthoring(
  store: EvaluationStore,
  id: string,
  raw: IterationRequest,
  review: PacketReviewer,
) {
  const work = await store.get(id),
    { input, base, operations } = planAuthoringIteration(work, raw);
  // Use the transaction engine for source validation and pin checks before persisting anything.
  const { proposeWork } = await import("./work-session");
  const proposed = proposeWork(work, input.proposal, operations),
    candidate = materializeWorkProposal(proposed, input.proposal);
  for (const pin of input.pins) {
    const before = base.documents.find((d) => d.id === pin.target),
      after = candidate.documents.find((d) => d.id === pin.target);
    if (
      !before ||
      !after ||
      contentKey(sourceProperty(before, pin.path)) !== contentKey(sourceProperty(after, pin.path))
    )
      throw Error(`Pinned decision changed: ${pin.reason}`);
  }
  const context = authoringTaskContext(candidate, input.target),
    angle = context.views[1];
  const views =
    input.views ??
    (input.design
      ? heroReviewViews(candidate, input.target, input.design.stage)
      : [
          { ...angle, rig: "neutral" as const },
          {
            ...(context.views.find((v) => v.id === "detail") ?? angle),
            id: "detail",
            rig: "neutral" as const,
          },
          { ...angle, id: "grazing", rig: "grazing" as const },
        ]);
  const result = await evaluateWork(
    store,
    id,
    {
      expectedKey: input.expectedKey,
      proposal: input.proposal,
      target: input.target,
      operations,
      views,
      width: input.width,
      height: input.height,
    },
    review,
  );
  const current = await store.get(id);
  const evidence = await store.saveArtifact("iteration.json", {
    version: 1,
    request: input,
    baseKey: contentKey(base),
    candidateKey: contentKey(candidate),
    pinsVerified: input.pins,
    stage: input.design?.stage ?? "refinement",
    capabilities: input.design ? [{ id: "hero-construction", version: "hero-construction-1" }] : [],
    artisticAcceptance: null,
  });
  const updated = appendWorkEvent(current, {
    id: `iteration-${crypto.randomUUID()}`,
    at: new Date().toISOString(),
    kind: "feedback",
    actor: "iteration-workflow",
    summary: input.critique || input.design?.intent || "Targeted visual iteration",
    evidence: [evidence],
  });
  return {
    ...result,
    key: await store.put(updated, contentKey(current)),
    gallery: result.packet.contactSheet,
    proposal: input.proposal,
    passed: reviewVerdict(
      result.packet.report,
      updated.constraints,
      updated.baselineKey,
      contentKey(candidate),
    ).passed,
    evidence,
    baseProposal: input.baseProposal ?? null,
  };
}
