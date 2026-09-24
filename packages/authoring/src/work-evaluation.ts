import { contentKey, idSchema, type Project } from "@wrela/model";
import { z } from "zod";
import { operationSchema } from "./commands";
import { authoringIntentSchema, planAuthoringIntent } from "./intent";
import { type AuthoringReview, authoringReviewSchema } from "./review-contract";
import { authoringTaskContext, reviewViewsSchema } from "./task-context";
import {
  appendWorkEvent,
  attachWorkReview,
  materializeWorkProposal,
  parseWorkSession,
  proposeWork,
  type WorkSession,
  workHandoff,
} from "./work-session";
export const evaluationRequestSchema = z
  .strictObject({
    expectedKey: z.string(),
    proposal: idSchema,
    target: idSchema,
    operations: z.array(operationSchema).min(1).max(256).optional(),
    intent: authoringIntentSchema.optional(),
    views: reviewViewsSchema.optional(),
    width: z.number().int().min(128).max(1024).default(640),
    height: z.number().int().min(128).max(768).default(480),
  })
  .refine((v) => !(v.operations && v.intent), "Supply operations or intent, not both");
export type EvaluationRequest = z.input<typeof evaluationRequestSchema>;
export type PacketSettings = {
  target: string;
  views: z.infer<typeof reviewViewsSchema>;
  width: number;
  height: number;
  budgetMs?: number;
};
export type ReviewPacket = {
  version: 1;
  report: AuthoringReview;
  contactSheet: string;
  evidence: string[];
  captures: { view: string; baseline: string; candidate: string }[];
  timings: {
    reviewMs: number;
    captureMs: number;
    totalMs: number;
    firstCandidateImageMs: number;
    firstCandidateImageAt?: number;
    cacheHits?: number;
    renderedImages?: number;
  };
};
export const reviewPacketSchema = z.strictObject({
  version: z.literal(1),
  report: authoringReviewSchema,
  contactSheet: z.string().min(1),
  evidence: z.array(z.string()).max(32).default([]),
  captures: z
    .array(z.strictObject({ view: idSchema, baseline: z.string().min(1), candidate: z.string().min(1) }))
    .min(1)
    .max(6),
  timings: z.strictObject({
    reviewMs: z.number().nonnegative(),
    captureMs: z.number().nonnegative(),
    totalMs: z.number().nonnegative(),
    firstCandidateImageMs: z.number().nonnegative(),
    firstCandidateImageAt: z.number().nonnegative().optional(),
    cacheHits: z.number().int().nonnegative().optional(),
    renderedImages: z.number().int().nonnegative().optional(),
  }),
});
export type EvaluationStore = {
  get(id: string): Promise<WorkSession>;
  put(w: WorkSession, key: string | null): Promise<string>;
  saveArtifact(name: string, value: Blob | object): Promise<string>;
};
export async function evaluateWork(
  store: EvaluationStore,
  id: string,
  raw: EvaluationRequest,
  review: (
    baseline: Project,
    candidate: Project,
    work: WorkSession,
    settings: PacketSettings,
  ) => Promise<ReviewPacket>,
) {
  const input = evaluationRequestSchema.parse(raw);
  let work = await store.get(id);
  if (contentKey(work) !== input.expectedKey) throw Error("Work changed; inspect before evaluating");
  if (input.operations || input.intent) {
    const plan = input.intent ? planAuthoringIntent(work.baseline, input.intent) : undefined;
    if (plan) {
      const known = new Set(work.constraints.map((c) => contentKey({ ...c, id: "" })));
      const extra = plan.constraints.filter((c) => !known.has(contentKey({ ...c, id: "" })));
      if (extra.length && work.proposals.length)
        throw Error(
          "New intent constraints require a fresh work session; retain earlier alternatives in their original session",
        );
      work = parseWorkSession({
        ...work,
        constraints: [...work.constraints, ...extra.map((c, i) => ({ ...c, id: `intent-${i}-${c.id}` }))],
      });
    }
    const operations = input.operations ?? plan?.operations;
    if (!operations) throw Error("An edit is required");
    work = proposeWork(work, input.proposal, operations);
  }
  if (work.proposals.find((p) => p.id === input.proposal)?.status !== "proposed")
    throw Error("Only a proposed candidate can be evaluated");
  const candidate = materializeWorkProposal(work, input.proposal),
    context = authoringTaskContext(
      work.baseline.documents.some((d) => d.id === input.target) ? work.baseline : candidate,
      input.target,
      work.constraints,
    );
  const settings = {
    target: input.target,
    views: input.views ?? context.views,
    width: input.width,
    height: input.height,
  };
  let key = await store.put(work, input.expectedKey);
  const started = performance.now();
  try {
    const packet = reviewPacketSchema.parse(await review(work.baseline, candidate, work, settings));
    if (
      packet.captures.length !== settings.views.length ||
      packet.captures.some((c, i) => c.view !== settings.views[i].id)
    )
      throw Error("Review packet is missing a prescribed view");
    work = attachWorkReview(work, input.proposal, packet.report);
    const evidence = await store.saveArtifact("review-packet.json", packet);
    work = appendWorkEvent(work, {
      id: `packet-${crypto.randomUUID()}`,
      at: new Date().toISOString(),
      kind: "review",
      actor: "review-workflow",
      summary: `Review packet for ${input.proposal}`,
      durationMs: performance.now() - started,
      evidence: [evidence, packet.contactSheet],
    });
    key = await store.put(work, key);
    return { key, ...workHandoff(work), packet, evidence };
  } catch (error) {
    const failure = appendWorkEvent(work, {
      id: `failure-${crypto.randomUUID()}`,
      at: new Date().toISOString(),
      kind: "failure",
      actor: "review-workflow",
      summary: String(error).slice(0, 2000),
      durationMs: performance.now() - started,
    });
    // Do not overwrite another author's work while recording an unsuccessful review.
    try {
      await store.put(failure, key);
    } catch {
      /* Original error remains actionable; candidate was already retained. */
    }
    throw error;
  }
}
/** Search only workflow-produced references; stale packets cannot qualify another candidate. */
export async function latestReviewPacket(
  work: WorkSession,
  proposal: string,
  load: (reference: string) => Promise<unknown>,
) {
  const candidate = work.proposals.find((p) => p.id === proposal);
  if (!candidate) throw Error("Unknown proposal");
  for (const event of [...work.events].reverse()) {
    if (event.actor !== "review-workflow" || !event.id.startsWith("packet-")) continue;
    for (const ref of event.evidence) {
      let raw: unknown;
      try {
        raw = await load(ref);
      } catch {
        continue;
      }
      const parsed = reviewPacketSchema.safeParse(raw);
      if (
        parsed.success &&
        parsed.data.report.candidateKey === candidate.candidateKey &&
        parsed.data.report.baselineKey === work.baselineKey &&
        parsed.data.report.constraintsKey === contentKey(work.constraints)
      )
        return parsed.data;
    }
  }
  throw Error("Evaluate this candidate before finishing; a current review packet with images is required");
}
