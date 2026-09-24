import { contentKey, idSchema } from "@wrela/model";
import { z } from "zod";
import { authoringConditionsSchema, domainFor, domainQuality } from "./authoring-domain";
import { matchAuthoringExemplar } from "./authoring-exemplars";
import { type StudyReviewer, studyAuthoring } from "./authoring-study";
import type { EvaluationStore } from "./work-evaluation";

export const authoringJobSchema = z.strictObject({
  id: idSchema.optional(),
  target: idSchema.optional(),
  expectedKey: z.string().optional(),
  brief: z.string().min(1).max(2000).optional(),
  exemplar: idSchema.optional(),
  conditions: authoringConditionsSchema.partial().optional(),
  budgetSeconds: z.union([z.literal(15), z.literal(30), z.literal(60)]).default(30),
  candidates: z.number().int().min(1).max(4).default(3),
});
/** One task-ready request resolves context and a visible curated target before bounded review.
 * Selection is a recommendation by authored-condition distance, not a learned visual quality score. */
export async function authoringJob(
  store: EvaluationStore,
  id: string,
  raw: z.input<typeof authoringJobSchema>,
  review: StudyReviewer,
) {
  const input = authoringJobSchema.parse(raw),
    work = await store.get(id),
    start = performance.now();
  if (input.expectedKey && input.expectedKey !== contentKey(work))
    throw Error("Work changed before authoring job");
  const target =
    input.target ??
    work.constraints.find((c) => domainFor(work.baseline, c.target))?.target ??
    work.baseline.entry;
  const domain = domainFor(work.baseline, target);
  if (!domain) throw Error("No supported subject; supply an assembly or botanical target");
  const brief = input.brief ?? work.brief,
    exemplar = matchAuthoringExemplar(domain, brief, input.exemplar);
  const conditions = { ...exemplar.conditions, ...input.conditions };
  const result = await studyAuthoring(
    store,
    id,
    {
      id: input.id ?? `job-${crypto.randomUUID()}`,
      expectedKey: contentKey(work),
      brief: {
        domain,
        target,
        conditions,
        quality: {
          ...domainQuality(domain),
          direction: brief.slice(0, 1200),
          referenceNotes: `${exemplar.title}: ${exemplar.rationale} ${exemplar.watchFor.join(". ")}. Curated reference intent; independent art approval pending.`,
        },
      },
      candidates: input.candidates,
      budgetSeconds: input.budgetSeconds,
    },
    review,
  );
  const ranked = result.candidates
    .filter((c) => c.passed)
    .map((c) => ({
      proposal: c.proposal,
      distance: Object.keys(conditions).reduce(
        (sum, k) =>
          sum + (c.conditions[k as keyof typeof conditions] - conditions[k as keyof typeof conditions]) ** 2,
        0,
      ),
    }))
    .sort((a, b) => a.distance - b.distance);
  return {
    ...result,
    exemplar: { id: exemplar.id, title: exemplar.title, approval: exemplar.approval },
    recommendation: ranked[0]?.proposal ?? null,
    ranking: {
      method: "distance-to-authored-conditions",
      visualScore: null,
      reason: "All passing candidates remain visible for visual selection. No automatic artistic acceptance.",
    },
    finish: ranked.length ? { expectedKey: result.key, proposal: ranked[0].proposal } : null,
    elapsedMs: performance.now() - start,
  };
}
