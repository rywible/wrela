import { contentKey, idSchema, type Project } from "@wrela/model";
import { z } from "zod";
import { domainBriefSchema, planDomainAuthoring } from "./authoring-domain";
import { type ResultConstraint, reviewVerdict } from "./review-contract";
import { authoringTaskContext, reviewViewsSchema } from "./task-context";
import {
  type EvaluationStore,
  type PacketSettings,
  type ReviewPacket,
  reviewPacketSchema,
} from "./work-evaluation";
import {
  appendWorkEvent,
  attachWorkReview,
  materializeWorkProposal,
  parseWorkSession,
  proposeWork,
} from "./work-session";

export const authoringStudySchema = z.strictObject({
  id: idSchema,
  expectedKey: z.string().min(1),
  brief: domainBriefSchema,
  candidates: z.number().int().min(1).max(4).default(3),
  spread: z.number().min(0).max(0.3).default(0.16),
  views: reviewViewsSchema.optional(),
  budgetSeconds: z.number().int().min(5).max(120).optional(),
});
export type AuthoringStudyRequest = z.input<typeof authoringStudySchema>;
export type StudyCandidate = {
  proposal: string;
  project: Project;
  conditions: z.infer<typeof domainBriefSchema>["conditions"];
  label?: string;
};
export type StudyReview = {
  results: { proposal: string; packet?: ReviewPacket; error?: string }[];
  gallery: string;
};
export type StudyReviewer = (
  baseline: Project,
  candidates: StudyCandidate[],
  constraints: ResultConstraint[],
  settings: PacketSettings,
) => Promise<StudyReview>;

/** Shared bounded search: adapters only provide plans; persistence, review and selection stay domain-independent. */
export async function studyAuthoring(
  store: EvaluationStore,
  workId: string,
  raw: AuthoringStudyRequest,
  review: StudyReviewer,
) {
  const input = authoringStudySchema.parse(raw),
    started = performance.now();
  let work = await store.get(workId);
  if (contentKey(work) !== input.expectedKey) throw Error("Work changed; inspect before searching");
  const candidates: StudyCandidate[] = [],
    plans = [];
  for (let i = 0; i < input.candidates; i++) {
    const offset = i === 0 ? 0 : (i % 2 ? -1 : 1) * input.spread * Math.ceil(i / 2);
    const clamp = (n: number) => Math.min(1, Math.max(0, n));
    const conditions = {
      ...input.brief.conditions,
      exposure: clamp(input.brief.conditions.exposure + offset),
      moisture: clamp(input.brief.conditions.moisture - offset * 0.6),
      variation: clamp(input.brief.conditions.variation + offset * 0.5),
    };
    const proposal = `study-${contentKey([input.id, i])}-${i}`;
    if (work.proposals.some((p) => p.id === proposal))
      throw Error("Study ID already exists; use a new ID for a new search");
    plans.push({
      proposal,
      conditions,
      plan: planDomainAuthoring(work.baseline, { ...input.brief, conditions }, proposal),
    });
  }
  const known = new Set(work.constraints.map((c) => contentKey({ ...c, id: "" })));
  const extra: ResultConstraint[] = [];
  for (const { plan } of plans)
    for (const c of plan.constraints) {
      const key = contentKey({ ...c, id: "" });
      if (!known.has(key)) {
        known.add(key);
        extra.push({ ...c, id: `study-protect-${contentKey({ ...c, id: "" })}` });
      }
    }
  if (extra.length && work.proposals.length)
    throw Error("New preservation requirements need a fresh work session");
  work = parseWorkSession({ ...work, constraints: [...work.constraints, ...extra] });
  for (const { proposal, conditions, plan } of plans) {
    work = proposeWork(work, proposal, plan.operations);
    candidates.push({ proposal, conditions, project: materializeWorkProposal(work, proposal) });
  }
  const context = authoringTaskContext(work.baseline, input.brief.target, work.constraints);
  const defaultViews: PacketSettings["views"] = [
    context.views[1],
    { ...(context.views.find((v) => v.id === "detail") ?? context.views[0]), id: "detail" },
  ].map((v) => ({ ...v, rig: "neutral" as const }));
  defaultViews.push({ ...defaultViews[1], id: "grazing", rig: "grazing" });
  const settings: PacketSettings = {
    target: input.brief.target,
    views: input.views ?? defaultViews,
    width: 640,
    height: 480,
    budgetMs: input.budgetSeconds ? input.budgetSeconds * 1000 : undefined,
  };
  let key = await store.put(work, input.expectedKey);
  let evaluated: StudyReview;
  try {
    evaluated = await review(work.baseline, candidates, work.constraints, settings);
  } catch (error) {
    work = appendWorkEvent(work, {
      id: `study-failure-${crypto.randomUUID()}`,
      at: new Date().toISOString(),
      kind: "failure",
      actor: "study-workflow",
      summary: String(error).slice(0, 1800),
      durationMs: performance.now() - started,
    });
    try {
      await store.put(work, key);
    } catch {
      /* Preserve concurrent author state. */
    }
    throw error;
  }
  if (
    evaluated.results.length !== candidates.length ||
    evaluated.results.some((r, i) => r.proposal !== candidates[i].proposal)
  )
    throw Error("Study review did not account for every candidate");
  const results = [];
  for (const [i, r] of evaluated.results.entries()) {
    let packet: ReviewPacket | undefined;
    if (r.packet) {
      packet = reviewPacketSchema.parse(r.packet);
      if (
        packet.captures.length !== settings.views.length ||
        packet.captures.some((c, i) => c.view !== settings.views[i].id)
      )
        throw Error("Study packet omitted a required view");
      work = attachWorkReview(work, r.proposal, packet.report);
      const reference = await store.saveArtifact("review-packet.json", packet);
      work = appendWorkEvent(work, {
        id: `packet-${crypto.randomUUID()}`,
        at: new Date().toISOString(),
        kind: "review",
        actor: "review-workflow",
        summary: `Study ${input.id}: ${r.proposal}`,
        evidence: [reference, packet.contactSheet],
      });
    } else
      work = appendWorkEvent(work, {
        id: `trial-failure-${crypto.randomUUID()}`,
        at: new Date().toISOString(),
        kind: "failure",
        actor: "study-workflow",
        summary: `${r.proposal}: ${r.error ?? "Missing review"}`.slice(0, 2000),
      });
    results.push({
      proposal: r.proposal,
      conditions: candidates[i].conditions,
      passed:
        !!packet &&
        reviewVerdict(packet.report, work.constraints, work.baselineKey, contentKey(candidates[i].project))
          .passed,
      packet,
      error: r.error,
    });
  }
  const evidence = await store.saveArtifact("authoring-study.json", {
    version: 1,
    request: input,
    settings,
    baselineKey: work.baselineKey,
    candidates: results,
    gallery: evaluated.gallery,
    controlsAreNotArtScores: true,
    limitations: plans[0].plan.limitations,
    explanations: plans[0].plan.explanation,
  });
  work = appendWorkEvent(work, {
    id: `study-${crypto.randomUUID()}`,
    at: new Date().toISOString(),
    kind: "review",
    actor: "study-workflow",
    summary: `${input.brief.domain}: ${input.brief.quality.direction}`.slice(0, 2000),
    durationMs: performance.now() - started,
    evidence: [evidence, evaluated.gallery, ...input.brief.quality.references],
  });
  key = await store.put(work, key);
  const elapsedMs = performance.now() - started;
  return {
    key,
    id: input.id,
    evidence,
    gallery: evaluated.gallery,
    quality: input.brief.quality,
    candidates: results.map(({ packet, ...r }) => ({
      ...r,
      contactSheet: packet?.contactSheet,
      timings: packet?.timings,
    })),
    next: "Inspect the gallery against the quality criteria, then finish a passing proposal. Passing constraints do not imply artistic acceptance.",
    elapsedMs,
    budget: {
      requestedMs: settings.budgetMs ?? null,
      elapsedMs,
      overrunMs: settings.budgetMs ? Math.max(0, elapsedMs - settings.budgetMs) : 0,
      reviewed: results.filter((r) => r.packet).length,
      policy:
        "Complete the first review; stop starting further alternatives after the review budget. Browser startup, storage, and an in-flight review may exceed it.",
    },
  };
}
