import type { AuthoringSession } from "@wrela/authoring";
import { contentKey, type Project, parseProject, VIEW_MODES } from "@wrela/model";

import { z } from "zod";

const vector = z.tuple([z.number().finite(), z.number().finite(), z.number().finite()]);
export const candidatePreviewSchema = z
  .object({
    subject: z.string().min(1).max(100),
    stageId: z.string().max(100).optional(),
    camera: z.object({ position: vector, target: vector, fov: z.number().min(15).max(100) }),
    motion: z.string().min(1).max(100).optional(),
    tick: z.number().int().min(0).max(600).default(0),
    ticks: z.array(z.number().int().min(0).max(600)).min(1).max(8).optional(),
    channel: z.enum(VIEW_MODES).default("beauty"),
    hideGroom: z.boolean().default(false),
    overlays: z
      .array(z.enum(["rig", "colliders"]))
      .max(2)
      .default([]),
    width: z.number().int().min(128).max(512).default(320),
    height: z.number().int().min(128).max(512).default(240),
    timeoutMs: z.number().int().min(100).max(30000).default(30000),
  })
  .strict()
  .refine(
    (value) => Math.hypot(...value.camera.position.map((x, i) => x - value.camera.target[i])) > 0.01,
    "Camera position and target must differ",
  )
  .refine(
    (value) => (value.ticks?.length ?? 1) * value.width * value.height <= 1048576,
    "Pose sequence exceeds the one-megapixel image budget per version",
  )
  .refine(
    (value) =>
      !value.ticks || value.ticks.every((tick, index, ticks) => index === 0 || tick > ticks[index - 1]),
    "Sequence ticks must be unique and increasing",
  );
export type CandidatePreviewOptions = z.input<typeof candidatePreviewSchema>;
export type CandidatePreviewSettings = z.output<typeof candidatePreviewSchema>;
export type CandidatePreviewImage = { blob: Blob; measurements?: unknown; diagnostics?: unknown };
export type CandidatePreviewResource = {
  render(
    project: Project,
    settings: CandidatePreviewSettings,
    signal: AbortSignal,
  ): Promise<CandidatePreviewImage>;
  dispose(): void;
};

export function candidatePreviewProjects(session: AuthoringSession, id: string) {
  const candidate = session.inspectCandidate(id);
  if (!candidate || candidate.status !== "proposed")
    throw Error("Preview requires a retained, unadopted candidate");
  // Revalidates original revision/read/write requirements before any resource allocation.
  session.preview(candidate.batch);
  const current = session.getSnapshot();
  const baseline = structuredClone(current.project);
  const documents = baseline.documents.filter(
    (document) => !candidate.changes.some((change) => change.id === document.id),
  );
  for (const change of [...candidate.changes].sort((a, b) => (a.afterIndex ?? 0) - (b.afterIndex ?? 0))) {
    if (change.after)
      documents.splice(change.afterIndex ?? documents.length, 0, structuredClone(change.after));
  }
  const proposal = parseProject({ ...baseline, documents });
  return {
    baseline,
    proposal,
    candidate,
    acceptedRevision: current.revision,
    acceptedKey: contentKey(current.project),
  };
}

/** Isolated A/B evaluation: accepted source and its runtime are never inputs to the rendering resource. */
export async function previewCreatureCandidate(
  session: AuthoringSession,
  id: string,
  input: CandidatePreviewOptions,
  createResource: (signal: AbortSignal) => Promise<CandidatePreviewResource>,
  externalSignal?: AbortSignal,
) {
  const settings = candidatePreviewSchema.parse(input);
  const source = candidatePreviewProjects(session, id);
  for (const project of [source.baseline, source.proposal]) {
    const character = project.documents.find((document) => document.id === settings.subject);
    if (character?.kind !== "character")
      throw Error("Candidate preview subject must remain a character in both versions");
    if (settings.motion && !character.motions.some((motion) => motion.id === settings.motion))
      throw Error("A matched preview motion must exist in both versions");
    if (
      settings.stageId &&
      !project.documents.some((document) => document.id === settings.stageId && document.kind === "stage")
    )
      throw Error("A matched preview stage must exist in both versions");
  }
  const abort = new AbortController();
  const cancel = () => abort.abort(externalSignal?.reason ?? new Error("Candidate preview cancelled"));
  externalSignal?.addEventListener("abort", cancel, { once: true });
  if (externalSignal?.aborted) cancel();
  const timeout = setTimeout(
    () => abort.abort(new Error("Candidate preview exceeded its time budget")),
    settings.timeoutMs,
  );
  let resource: CandidatePreviewResource | undefined;
  const aborted = new Promise<never>((_, reject) => {
    if (abort.signal.aborted) reject(abort.signal.reason);
    else abort.signal.addEventListener("abort", () => reject(abort.signal.reason), { once: true });
  });
  const assertCurrent = () => {
    abort.signal.throwIfAborted();
    const current = session.getSnapshot();
    if (current.revision !== source.acceptedRevision || contentKey(current.project) !== source.acceptedKey)
      throw Error("Accepted source changed during candidate preview; inspect and propose again");
    const candidate = session.inspectCandidate(id);
    if (candidate?.status !== "proposed" || candidate.fingerprint !== source.candidate.fingerprint)
      throw Error("Candidate changed during preview");
  };
  try {
    assertCurrent();
    const created = createResource(abort.signal).then((value) => {
      if (abort.signal.aborted) {
        value.dispose();
        throw abort.signal.reason;
      }
      resource = value;
      return value;
    });
    const activeResource = await Promise.race([created, aborted]);
    assertCurrent();
    const frames: { tick: number; baseline: CandidatePreviewImage; candidate: CandidatePreviewImage }[] = [];
    for (const tick of settings.ticks ?? [settings.tick]) {
      const frameSettings = { ...structuredClone(settings), tick };
      const baseline = await Promise.race([
        activeResource.render(structuredClone(source.baseline), structuredClone(frameSettings), abort.signal),
        aborted,
      ]);
      assertCurrent();
      const candidate = await Promise.race([
        activeResource.render(structuredClone(source.proposal), structuredClone(frameSettings), abort.signal),
        aborted,
      ]);
      assertCurrent();
      frames.push({ tick, baseline, candidate });
    }
    return {
      baseline: frames[0].baseline,
      candidate: frames[0].candidate,
      frames,
      metadata: {
        candidateId: id,
        candidateFingerprint: source.candidate.fingerprint,
        baseRevision: source.candidate.sourceRevision,
        acceptedRevision: source.acceptedRevision,
        baselineKey: source.acceptedKey,
        candidateKey: contentKey(source.proposal),
        settings,
        runtimeTick: (settings.ticks?.[0] ?? settings.tick) + (settings.motion ? 1 : 0),
        runtimeTicks: (settings.ticks ?? [settings.tick]).map((tick) => tick + (settings.motion ? 1 : 0)),
        quality: "interactive" as const,
        renderer: "low" as const,
        channel: settings.channel,
        overlays: settings.overlays,
        visualApproval: "not-reviewed" as const,
      },
    };
  } finally {
    clearTimeout(timeout);
    externalSignal?.removeEventListener("abort", cancel);
    abort.abort(new Error("Candidate preview finished"));
    resource?.dispose();
  }
}
