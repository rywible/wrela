import { link, mkdir, realpath, unlink } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { latestReviewPacket, materializeWorkProposal, reviewVerdict, workHandoff } from "@wrela/authoring";
import { contentKey, idSchema } from "@wrela/model";
import { z } from "zod";
import { readAuthoringTrace, summarizeAuthoringTrace } from "./authoring-trace";
import { WorkFileStore } from "./authoring-work-store";

const schema = z.strictObject({
  expectedKey: z.string(),
  proposal: idSchema,
  requireArtReview: z.boolean().default(false),
  benchmarkRequest: z.string().optional(),
  tokens: z.number().int().nonnegative().nullable().default(null),
  humanInterventions: z.number().int().nonnegative().nullable().default(null),
});
export async function finishAuthoringWork(
  workspace: string,
  id: string,
  raw: unknown,
  adopt: (input: { expectedKey: string; proposal: string; requireArtReview: boolean }) => Promise<unknown>,
) {
  const input = schema.parse(raw),
    store = new WorkFileStore(workspace);
  let work = await store.get(id);
  if (contentKey(work) !== input.expectedKey) throw Error("Work changed; inspect before finishing");
  const packet = await latestReviewPacket(work, input.proposal, (ref) => Bun.file(ref).json());
  if (!reviewVerdict(packet.report, work.constraints, work.baselineKey, packet.report.candidateKey).passed)
    throw Error("Every constraint must pass before finishing");
  let output = join(resolve(workspace), ".wrela", "exports", `${id}-${packet.report.candidateKey}`),
    submission: string | undefined;
  if (input.benchmarkRequest) {
    const request = await Bun.file(input.benchmarkRequest).json();
    if ((await realpath(request.workspace)) !== (await realpath(workspace)) || request.task.workId !== id)
      throw Error("Benchmark request belongs to another workspace or work session");
    if (
      resolve(request.output) !== dirname(resolve(input.benchmarkRequest)) ||
      resolve(request.submission) !== join(resolve(request.output), "submission.json")
    )
      throw Error("Invalid benchmark output location");
    output = resolve(request.output);
    submission = request.submission;
    if (await Bun.file(submission!).exists())
      throw Error("Benchmark submission already exists; it cannot be replaced");
  }
  // Prove the retained evidence can be exported before changing published source.
  await store.backup(id);
  const prior = work.proposals.find((p) => p.id === input.proposal);
  let publication: unknown;
  if (prior?.status !== "adopted") publication = await adopt(input);
  if (
    publication &&
    typeof publication === "object" &&
    "receiptPending" in publication &&
    publication.receiptPending
  ) {
    return {
      published: "outcome" in publication && publication.outcome === "committed",
      accepted: true,
      packagingPending: true,
      receiptPending: true,
      publication,
      key: contentKey(await store.get(id)),
      output,
      reason: "Source was accepted; retry finish to recover durable publication before packaging.",
    };
  }
  work = await store.get(id);
  const published =
    work.proposals.find((p) => p.id === input.proposal)?.status === "adopted" ||
    work.publication?.state === "committed";
  if (!published) return { published: false, publication, packagingPending: true };
  try {
    const project = materializeWorkProposal(work, input.proposal),
      bundle = await store.backup(id);
    await mkdir(output, { recursive: true });
    const trace = await readAuthoringTrace(workspace),
      now = Date.now(),
      timing = summarizeAuthoringTrace(trace, Date.parse(work.createdAt), now);
    await Bun.write(join(output, "final.json"), JSON.stringify(project, null, 2));
    await Bun.write(
      join(output, "handoff.json"),
      JSON.stringify({ sourceKey: contentKey(project), work: workHandoff(work), timing, packet }, null, 2),
    );
    await Bun.write(join(output, "handoff-bundle.json"), JSON.stringify(bundle));
    await Bun.write(join(output, "authoring-trace.json"), JSON.stringify(trace, null, 2));
    const artifacts: { path: string; kind: "source" | "image" | "review" | "handoff" | "transcript" }[] = [
      { path: "final.json", kind: "source" },
      { path: "handoff.json", kind: "handoff" },
      { path: "handoff-bundle.json", kind: "handoff" },
      { path: "authoring-trace.json", kind: "transcript" },
    ];
    for (const capture of packet.captures) {
      const name = `${capture.view}-final.png`;
      await Bun.write(join(output, name), Bun.file(capture.candidate));
      artifacts.push({ path: name, kind: "image" });
    }
    await Bun.write(join(output, "contact-sheet.png"), Bun.file(packet.contactSheet));
    artifacts.push({ path: "contact-sheet.png", kind: "review" });
    if (submission) {
      const temporary = join(output, `submission-${crypto.randomUUID()}.tmp`);
      await Bun.write(
        temporary,
        JSON.stringify(
          {
            tokens: input.tokens,
            humanInterventions: input.humanInterventions,
            constraintsPassed: true,
            artifacts,
            stages: [{ name: "review-packet-tool-runtime", durationMs: packet.timings.totalMs }],
            reason:
              "Automatic reviewed handoff; no artistic acceptance inferred. Token/intervention counts remain unknown unless supplied by an instrumented host.",
          },
          null,
          2,
        ),
      );
      try {
        await link(temporary, submission);
      } finally {
        await unlink(temporary);
      }
    }
    return {
      published: true,
      packagingPending: false,
      key: contentKey(work),
      output,
      submission,
      artifacts,
      timing,
      publication,
    };
  } catch (error) {
    return {
      published: true,
      packagingPending: true,
      key: contentKey(work),
      output,
      reason: String(error),
      next: "Retry finish with the returned work key; source is already committed.",
      publication,
    };
  }
}
