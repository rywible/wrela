import { contentKey, idSchema, type Project, parseProject, projectSchema } from "@wrela/model";

import { z } from "zod";
import { batchSchema, type EditBatch, parseEditBatch } from "./commands";
import {
  type AuthoringReview,
  authoringReviewSchema,
  resultConstraintSchema,
  reviewVerdict,
} from "./review-contract";
import { AuthoringSession } from "./session";

const workEventSchema = z.strictObject({
  id: idSchema,
  at: z.string().datetime(),
  kind: z.enum([
    "request",
    "inspect",
    "propose",
    "review",
    "feedback",
    "adopt",
    "failure",
    "handoff",
    "tool-development",
  ]),
  summary: z.string().min(1).max(2000),
  actor: z.string().min(1).max(200),
  durationMs: z.number().finite().nonnegative().nullable().default(null),
  tokens: z.number().int().nonnegative().nullable().default(null),
  humanInterventions: z.number().int().nonnegative().nullable().default(null),
  evidence: z.array(z.string().max(1000)).max(32).default([]),
});
const proposalSchema = z.strictObject({
  id: idSchema,
  baselineKey: z.string(),
  candidateKey: z.string(),
  batch: batchSchema,
  reviews: z.array(authoringReviewSchema).max(32),
  status: z.enum(["proposed", "adopted", "rejected"]),
  decision: z
    .strictObject({
      reviewer: z.string().min(1).max(200),
      verdict: z.enum(["accepted", "rejected"]),
      reason: z.string().min(1).max(2000),
      candidateKey: z.string(),
      at: z.string().datetime(),
    })
    .optional(),
});
export const workReceiptSchema = z.strictObject({
  revision: z.number().int().nonnegative(),
  changed: z.array(idSchema),
  key: z.string(),
  transactionId: idSchema,
  diff: z.array(
    z.strictObject({
      id: idSchema,
      action: z.enum(["create", "delete", "update"]),
      properties: z.array(z.string()),
    }),
  ),
});
export const workSessionSchema = z.strictObject({
  version: z.literal(2),
  id: idSchema,
  brief: z.string().min(1).max(8000),
  createdAt: z.string().datetime(),
  baseline: projectSchema,
  baselineKey: z.string(),
  constraints: z.array(resultConstraintSchema).max(64),
  proposals: z.array(proposalSchema).max(32),
  events: z.array(workEventSchema).max(1024),
  publication: z
    .strictObject({
      requestKey: z.string(),
      proposal: idSchema,
      expectedRevision: z.number().int().nonnegative(),
      receipt: workReceiptSchema,
      state: z.enum(["pending", "committed"]),
    })
    .optional(),
});
export type WorkSession = z.infer<typeof workSessionSchema>;
const trusted = new WeakSet<object>();
const frozen = new WeakSet<object>();
function freeze<T>(value: T): T {
  if (value && typeof value === "object" && !frozen.has(value)) {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
    frozen.add(value);
  }
  return value;
}
function accept(work: WorkSession): WorkSession {
  freeze(work);
  trusted.add(work);
  return work;
}
// Each immutable baseline has one indexed session. Preview copies changed documents only.
const sessions = new WeakMap<Project, AuthoringSession>();
function baselineSession(work: WorkSession) {
  let session = sessions.get(work.baseline);
  if (!session) {
    session = new AuthoringSession(work.baseline);
    sessions.set(work.baseline, session);
  }
  return session;
}
function candidate(work: WorkSession, batch: EditBatch) {
  const preview = baselineSession(work).preview(batch);
  const changes = new Map(preview.changes.map((change) => [change.id, change.after]));
  const documents = work.baseline.documents.flatMap((doc) => {
    if (!changes.has(doc.id)) return [doc];
    const replacement = changes.get(doc.id);
    return replacement ? [replacement] : [];
  });
  const existing = new Set(work.baseline.documents.map((doc) => doc.id));
  for (const change of preview.changes)
    if (change.after && !existing.has(change.id)) documents.push(change.after);
  return { ...work.baseline, documents };
}
export function createWorkSession(
  project: Project,
  input: { id: string; brief: string; constraints?: z.input<typeof resultConstraintSchema>[] },
): WorkSession {
  const baseline = parseProject(project);
  return parseWorkSession({
    version: 2,
    ...input,
    createdAt: new Date().toISOString(),
    baseline,
    baselineKey: contentKey(baseline),
    constraints: input.constraints ?? [],
    proposals: [],
    events: [
      {
        id: `request-${crypto.randomUUID()}`,
        at: new Date().toISOString(),
        kind: "request",
        actor: "author",
        summary: input.brief.slice(0, 2000),
      },
    ],
  });
}
/** Decode and validate the record, without executing every alternative. Candidate identity is
 * checked on materialization. Untrusted imports use verifyWorkSession before being retained. */
export function parseWorkSession(input: unknown): WorkSession {
  if (input && typeof input === "object" && trusted.has(input)) return input as WorkSession;
  const legacy = z
    .object({ version: z.literal(1), proposals: z.array(z.object({ project: projectSchema }).passthrough()) })
    .safeParse(input);
  if (legacy.success) {
    for (const proposal of legacy.data.proposals)
      if (contentKey(proposal.project) !== proposal.candidateKey)
        throw Error("Proposal content does not match its identity");
    input = {
      ...(input as object),
      version: 2,
      proposals: legacy.data.proposals.map(({ project: _project, ...proposal }) => proposal),
    };
  }
  if (input && typeof input === "object" && "baseline" in input) parseProject(input.baseline);
  const work = workSessionSchema.parse(input);
  for (const p of (input as WorkSession).proposals) parseEditBatch(p.batch);
  parseProject(work.baseline);
  if (work.baselineKey !== contentKey(work.baseline))
    throw Error("Work baseline content does not match its identity");
  if (
    new Set(work.constraints.map((c) => c.id)).size !== work.constraints.length ||
    new Set(work.proposals.map((p) => p.id)).size !== work.proposals.length ||
    new Set(work.events.map((e) => e.id)).size !== work.events.length
  )
    throw Error("Duplicate work identities");
  for (const p of work.proposals) {
    if (p.baselineKey !== work.baselineKey) throw Error("Proposal baseline does not match");
    for (const review of p.reviews) reviewVerdict(review, work.constraints, work.baselineKey, p.candidateKey);
    if (p.decision && p.decision.candidateKey !== p.candidateKey)
      throw Error("Decision belongs to another candidate");
  }
  if (work.publication) {
    const publication = work.publication;
    const p = work.proposals.find((p) => p.id === publication.proposal);
    if (!p || p.candidateKey !== work.publication.receipt.key || p.status === "rejected")
      throw Error("Invalid publication identity");
  }
  return accept(work);
}
export function materializeWorkProposal(input: WorkSession, id: string): Project {
  const work = parseWorkSession(input),
    proposal = work.proposals.find((p) => p.id === id);
  if (!proposal) throw Error("Unknown proposal");
  const project = candidate(work, proposal.batch);
  if (contentKey(project) !== proposal.candidateKey)
    throw Error("Proposal content does not match its operations");
  return freeze(project);
}
export function verifyWorkSession(input: unknown) {
  const work = parseWorkSession(input);
  for (const p of work.proposals) materializeWorkProposal(work, p.id);
  return work;
}
export function proposeWork(
  input: WorkSession,
  id: string,
  operations: EditBatch["operations"],
  actor = "author",
) {
  const work = parseWorkSession(input);
  if (work.proposals.some((p) => p.id === id)) throw Error("Proposal ID already exists; use a new revision");
  if (work.proposals.length >= 32) throw Error("A work session supports up to 32 alternatives");
  const batch = parseEditBatch({
    expectedRevision: 0,
    label: id,
    actor,
    intent: work.brief.slice(0, 1000),
    operations,
  });
  const project = candidate(work, batch),
    candidateKey = contentKey(project);
  if (candidateKey === work.baselineKey) throw Error("Proposal makes no change");
  const proposal = proposalSchema.parse({
    id,
    baselineKey: work.baselineKey,
    candidateKey,
    batch,
    reviews: [],
    status: "proposed",
  });
  return accept({ ...work, proposals: [...work.proposals, proposal] });
}
export function appendWorkEvent(input: WorkSession, value: z.input<typeof workEventSchema>) {
  const work = parseWorkSession(input),
    event = workEventSchema.parse(value),
    old = work.events.find((e) => e.id === event.id);
  if (old) {
    if (contentKey(old) !== contentKey(event)) throw Error("Event identity has conflicting content");
    return work;
  }
  if (work.events.length >= 1024) throw Error("Work event limit reached");
  return accept({ ...work, events: [...work.events, event] });
}
function updateProposal(
  input: WorkSession,
  id: string,
  update: (p: WorkSession["proposals"][number]) => WorkSession["proposals"][number],
) {
  const work = parseWorkSession(input),
    p = work.proposals.find((p) => p.id === id);
  if (!p || p.status !== "proposed") throw Error("Action requires a proposed candidate");
  const next = proposalSchema.parse(update(p));
  return accept({ ...work, proposals: work.proposals.map((p) => (p.id === id ? next : p)) });
}
export function attachWorkReview(input: WorkSession, proposal: string, report: AuthoringReview) {
  const work = parseWorkSession(input);
  materializeWorkProposal(work, proposal);
  return updateProposal(work, proposal, (p) => {
    reviewVerdict(report, work.constraints, work.baselineKey, p.candidateKey);
    return { ...p, reviews: [...p.reviews, authoringReviewSchema.parse(report)] };
  });
}
export function decideWork(
  work: WorkSession,
  proposal: string,
  decision: { reviewer: string; verdict: "accepted" | "rejected"; reason: string },
) {
  return updateProposal(work, proposal, (p) => ({
    ...p,
    status: decision.verdict === "rejected" ? "rejected" : p.status,
    decision: { ...decision, candidateKey: p.candidateKey, at: new Date().toISOString() },
  }));
}
export function adoptionBatch(input: WorkSession, proposal: string, requireArtReview = false) {
  const work = parseWorkSession(input),
    p = work.proposals.find((p) => p.id === proposal);
  if (!p || p.status !== "proposed") throw Error("Adoption requires a proposed candidate");
  materializeWorkProposal(work, proposal);
  const review = p.reviews.at(-1);
  if (
    work.constraints.length &&
    (!review || !reviewVerdict(review, work.constraints, work.baselineKey, p.candidateKey).passed)
  )
    throw Error("Every declared result constraint must pass before adoption");
  if (requireArtReview && p.decision?.verdict !== "accepted")
    throw Error("Artistic review is required for this adoption");
  return p.batch;
}
export function adoptWork(
  input: WorkSession,
  proposal: string,
  session: AuthoringSession,
  requireArtReview = false,
) {
  const work = parseWorkSession(input),
    batch = adoptionBatch(work, proposal, requireArtReview);
  if (contentKey(session.getSnapshot().project) !== work.baselineKey)
    throw Error("Accepted source changed; start a new work session and review again");
  const result = session.apply({ ...batch, expectedRevision: session.getSnapshot().revision });
  return { work: updateProposal(work, proposal, (p) => ({ ...p, status: "adopted" })), result };
}
export function workHandoff(input: WorkSession) {
  const work = parseWorkSession(input);
  return {
    id: work.id,
    brief: work.brief,
    baselineKey: work.baselineKey,
    constraints: work.constraints,
    publication: work.publication,
    proposals: work.proposals.map((p) => ({
      id: p.id,
      sourceKey: p.candidateKey,
      status: p.status,
      decision: p.decision,
      constraints:
        p.reviews.at(-1)?.results.map(({ id, status, reason }) => ({ id, status, reason })) ?? "unreviewed",
    })),
    recentEvents: work.events.slice(-12),
    eventCount: work.events.length,
    next: "Inspect feedback, select a proposal by ID, and retrieve relevant source paths. Failed and unmeasured constraints block adoption. Retry a pending publication to recover its receipt.",
  };
}
