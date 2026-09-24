import { appendFile, mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import { z } from "zod";

const traceSchema = z
  .strictObject({
    version: z.literal(1),
    id: z.string(),
    command: z.string(),
    startedAt: z.number().nonnegative(),
    endedAt: z.number().nonnegative(),
    durationMs: z.number().nonnegative(),
    ok: z.boolean(),
    error: z.string().optional(),
    firstCandidateImageAt: z.number().nonnegative().optional(),
    imagesAvailableAt: z.number().nonnegative().optional(),
  })
  .refine((v) => v.endedAt >= v.startedAt);
export type AuthoringTrace = z.infer<typeof traceSchema>;
export async function traceAuthoring<T>(args: string[], run: () => Promise<T>): Promise<T> {
  const startedAt = Date.now(),
    start = performance.now();
  let ok = false,
    error: string | undefined,
    firstCandidateImageAt: number | undefined;
  let imagesAvailableAt: number | undefined;
  try {
    const result = await run();
    const packetTime = (
      result as { work?: { packet?: { timings?: { firstCandidateImageAt?: number } } } } | null
    )?.work?.packet?.timings?.firstCandidateImageAt;
    if (typeof packetTime === "number" && Number.isFinite(packetTime)) firstCandidateImageAt = packetTime;
    const trials = (
      result as { work?: { candidates?: { timings?: { firstCandidateImageAt?: number } }[] } } | null
    )?.work?.candidates;
    const times =
      trials?.flatMap((c) =>
        typeof c.timings?.firstCandidateImageAt === "number" ? [c.timings.firstCandidateImageAt] : [],
      ) ?? [];
    if (times.length) firstCandidateImageAt = Math.min(...times);
    if (
      (result as { work?: { gallery?: string; packet?: unknown } } | null)?.work?.gallery ||
      (result as { work?: { packet?: unknown } } | null)?.work?.packet
    )
      imagesAvailableAt = Date.now();
    ok = true;
    return result;
  } catch (e) {
    error = String(e).slice(0, 1200);
    throw e;
  } finally {
    if (args[0])
      try {
        const dir = join(resolve(args[0]), ".wrela");
        await mkdir(dir, { recursive: true });
        const path = join(dir, "authoring-trace.jsonl");
        if (Bun.file(path).size < 16 * 1024 * 1024 || !(await Bun.file(path).exists()))
          await appendFile(
            path,
            JSON.stringify({
              version: 1,
              id: crypto.randomUUID(),
              command: args[1] === "work" ? `work.${args[2] ?? "list"}` : (args[1] ?? "inspect"),
              startedAt,
              endedAt: Date.now(),
              durationMs: performance.now() - start,
              ok,
              ...(firstCandidateImageAt !== undefined ? { firstCandidateImageAt } : {}),
              ...(imagesAvailableAt !== undefined ? { imagesAvailableAt } : {}),
              ...(error ? { error } : {}),
            } satisfies AuthoringTrace) + "\n",
          );
      } catch {
        /* Observability cannot turn a committed source transaction into a reported failure. */
      }
  }
}
export async function readAuthoringTrace(workspace: string): Promise<AuthoringTrace[]> {
  const file = Bun.file(join(resolve(workspace), ".wrela", "authoring-trace.jsonl"));
  if (!(await file.exists())) return [];
  if (file.size > 16 * 1024 * 1024) throw Error("Authoring trace exceeds 16 MiB");
  return (await file.text())
    .split("\n")
    .filter(Boolean)
    .map((line) => traceSchema.parse(JSON.parse(line)));
}
export function summarizeAuthoringTrace(rows: AuthoringTrace[], startedAt: number, endedAt: number) {
  const matching = rows.filter(
    (r) =>
      r.startedAt >= startedAt && r.endedAt <= endedAt && Number.isFinite(r.durationMs) && r.durationMs >= 0,
  );
  const spans = matching.map((r) => [r.startedAt, r.endedAt]).sort((a, b) => a[0] - b[0]);
  let covered = 0,
    last = startedAt;
  for (const [start, end] of spans) {
    covered += Math.max(0, end - Math.max(start, last));
    last = Math.max(last, end);
  }
  const discovery = matching.filter((r) =>
    /context|discover|inspect|fields|catalog|working-set/.test(r.command),
  );
  const imageTimes = matching.flatMap((r) =>
    r.firstCandidateImageAt !== undefined &&
    r.firstCandidateImageAt >= startedAt &&
    r.firstCandidateImageAt <= endedAt
      ? [r.firstCandidateImageAt]
      : [],
  );
  return {
    toolCalls: matching.length,
    failedCalls: matching.filter((r) => !r.ok).length,
    discoveryCalls: discovery.length,
    discoveryMs: discovery.reduce((n, r) => n + r.durationMs, 0),
    firstCandidateImageMs: imageTimes.length ? Math.min(...imageTimes) - startedAt : null,
    ...(matching.some((r) => r.imagesAvailableAt !== undefined)
      ? {
          firstReviewDeliveredMs:
            Math.min(
              ...matching.flatMap((r) => (r.imagesAvailableAt !== undefined ? [r.imagesAvailableAt] : [])),
            ) - startedAt,
        }
      : {}),
    toolMs: matching.reduce((n, r) => n + r.durationMs, 0),
    observedToolWallMs: covered,
    unattributedMs: Math.max(0, endedAt - startedAt - covered),
    modelMs: null,
    scope:
      "CLI spans only. Unattributed time includes model latency, image inspection, manual shell work, orchestration and idle time; it is not measured reasoning time.",
  };
}
