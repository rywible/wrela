import { contentKey, idSchema } from "@wrela/model";
import { z } from "zod";
import type { StudyCandidate, StudyReviewer } from "./authoring-study";
import { buildHeroDesign, heroCreationOperations, heroDesignSchema } from "./hero-design";
import { type ResultConstraint, reviewVerdict } from "./review-contract";
import { authoringTaskContext, reviewViewsSchema } from "./task-context";
import { type EvaluationStore, type PacketSettings, reviewPacketSchema } from "./work-evaluation";
import {
  appendWorkEvent,
  attachWorkReview,
  materializeWorkProposal,
  parseWorkSession,
  proposeWork,
} from "./work-session";

export const heroStudySchema = z
  .strictObject({
    expectedKey: z.string(),
    id: idSchema,
    designs: z.array(heroDesignSchema).min(1).max(4),
    maxTriangles: z.number().int().min(100).max(80000).default(30000),
    budgetSeconds: z.number().min(5).max(120).default(30),
    views: reviewViewsSchema.optional(),
    width: z.number().int().min(128).max(1024).default(640),
    height: z.number().int().min(128).max(768).default(480),
    qualityTarget: z
      .strictObject({
        references: z.array(z.string().min(1)).max(8).default([]),
        criteria: z.array(z.string().min(1).max(300)).min(1).max(12),
        direction: z.string().min(1).max(2000),
      })
      .optional(),
  })
  .refine(
    (v) => new Set(v.designs.map((d) => d.id)).size === 1,
    "Alternatives must create the same asset identity",
  );
export async function authorHeroStudy(
  store: EvaluationStore,
  id: string,
  raw: z.input<typeof heroStudySchema>,
  review: StudyReviewer,
) {
  const input = heroStudySchema.parse(raw),
    started = performance.now();
  let work = await store.get(id);
  if (contentKey(work) !== input.expectedKey) throw Error("Work changed before hero creation");
  const target = input.designs[0].id;
  const constraint: ResultConstraint = {
    kind: "geometry",
    id: `hero-geometry-${target}`,
    target,
    minTriangles: 1,
    maxTriangles: input.maxTriangles,
    minimumExtent: [0.005, 0.005, 0.005],
    maximumExtent: [50, 50, 50],
  };
  const existing = work.constraints.find((c) => c.id === constraint.id);
  if (existing && contentKey(existing) !== contentKey(constraint))
    throw Error("Hero geometry budget is pinned for this work session");
  if (!existing) {
    if (work.proposals.length) throw Error("Start a new work session to add hero creation constraints");
    work = parseWorkSession({ ...work, constraints: [...work.constraints, constraint] });
  }
  const candidates: StudyCandidate[] = input.designs.map((design, i) => {
    const proposal = `${input.id}-${i}`;
    work = proposeWork(work, proposal, heroCreationOperations(work.baseline, design));
    return {
      proposal,
      project: materializeWorkProposal(work, proposal),
      conditions: { exposure: 0, moisture: 0, maturity: design.age, variation: 0 },
      label: `${proposal} · ${design.name} · ${design.stage}`,
    };
  });
  const defaultViews = heroReviewViews(
    candidates[0].project,
    target,
    input.designs.some((d) => d.stage === "production") ? "production" : "construction",
  );
  const settings: PacketSettings = {
    target,
    width: input.width,
    height: input.height,
    budgetMs: input.budgetSeconds * 1000,
    views: input.views ?? defaultViews,
  };
  let key = await store.put(work, input.expectedKey);
  try {
    const reviewed = await review(work.baseline, candidates, work.constraints, settings);
    if (
      reviewed.results.length !== candidates.length ||
      reviewed.results.some((r, i) => r.proposal !== candidates[i].proposal)
    )
      throw Error("Hero review omitted a candidate");
    const results = [];
    for (const [i, result] of reviewed.results.entries()) {
      let passed = false;
      if (result.packet) {
        const packet = reviewPacketSchema.parse(result.packet);
        if (
          packet.captures.length !== settings.views.length ||
          packet.captures.some((c, i) => c.view !== settings.views[i].id)
        )
          throw Error("Hero review omitted required views");
        work = attachWorkReview(work, result.proposal, packet.report);
        passed = reviewVerdict(
          packet.report,
          work.constraints,
          work.baselineKey,
          contentKey(candidates[i].project),
        ).passed;
        const evidence = await store.saveArtifact("review-packet.json", packet);
        const designEvidence = await store.saveArtifact("hero-design.json", {
          version: 1,
          design: input.designs[i],
          capabilities: buildHeroDesign(input.designs[i]).capabilities,
          candidateKey: packet.report.candidateKey,
        });
        work = appendWorkEvent(work, {
          id: `hero-review-${crypto.randomUUID()}`,
          at: new Date().toISOString(),
          kind: "review",
          actor: "review-workflow",
          summary: `${result.proposal}: ${input.designs[i].intent}`.slice(0, 2000),
          evidence: [evidence, packet.contactSheet, designEvidence],
        });
      } else
        work = appendWorkEvent(work, {
          id: `hero-failure-${crypto.randomUUID()}`,
          at: new Date().toISOString(),
          kind: "failure",
          actor: "hero-workflow",
          summary: `${result.proposal}: ${result.error ?? "Missing review"}`.slice(0, 2000),
        });
      results.push({
        proposal: result.proposal,
        passed,
        error: result.error,
        contactSheet: result.packet?.contactSheet,
        timings: result.packet?.timings,
      });
    }
    const evidence = await store.saveArtifact("hero-study.json", {
      version: 1,
      request: input,
      settings,
      results,
      gallery: reviewed.gallery,
      artisticAcceptance: null,
    });
    work = appendWorkEvent(work, {
      id: `hero-${crypto.randomUUID()}`,
      at: new Date().toISOString(),
      kind: "review",
      actor: "hero-workflow",
      summary: "From-scratch construction alternatives; visual selection required",
      evidence: [evidence, reviewed.gallery],
      durationMs: performance.now() - started,
    });
    key = await store.put(work, key);
    return {
      key,
      gallery: reviewed.gallery,
      candidates: results,
      evidence,
      target,
      elapsedMs: performance.now() - started,
      quality: input.qualityTarget ?? {
        direction: work.brief,
        references: [],
        criteria: [
          "Distinct silhouette",
          "Plausible connected construction",
          "Coherent material response",
          "Readable at gameplay distance",
          "Stable under motion and grazing light",
        ],
      },
      next: "Inspect silhouette and construction; branch a candidate with iterate, or finish a passing proposal. Source checks do not certify art quality.",
    };
  } catch (error) {
    const failed = appendWorkEvent(work, {
      id: `hero-error-${crypto.randomUUID()}`,
      at: new Date().toISOString(),
      kind: "failure",
      actor: "hero-workflow",
      summary: String(error).slice(0, 2000),
    });
    try {
      await store.put(failed, key);
    } catch {
      /* Preserve concurrent changes. */
    }
    throw error;
  }
}

export function heroReviewViews(
  project: Parameters<typeof authoringTaskContext>[0],
  target: string,
  stage: string,
): PacketSettings["views"] {
  const context = authoringTaskContext(project, target),
    angle = context.views[1];
  const defaultViews: PacketSettings["views"] = [
    { ...angle, rig: "neutral" },
    { ...context.views[0], id: "clay", rig: "neutral", mode: "clay" },
    { ...angle, id: "grazing", rig: "grazing" },
  ];
  if (stage === "production")
    defaultViews.push(
      { ...(context.views.find((v) => v.id === "detail") ?? angle), id: "detail", rig: "neutral" },
      { ...angle, id: "motion", tick: 60, rig: "neutral" },
      {
        ...angle,
        id: "gameplay",
        rig: "neutral",
        camera: {
          ...angle.camera,
          position: angle.camera.position.map(
            (v, i) => angle.camera.target[i] + (v - angle.camera.target[i]) * 3,
          ) as [number, number, number],
        },
      },
    );
  return defaultViews;
}
