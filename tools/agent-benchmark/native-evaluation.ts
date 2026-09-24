import { join } from "node:path";
import { z } from "zod";
import type { BenchmarkTask } from "./protocol";

const reportSchema = z.strictObject({
  version: z.literal(1),
  suiteKey: z.string(),
  task: z.string(),
  evaluator: z.string().min(1),
  sources: z.array(z.strictObject({ path: z.string(), sha256: z.string().regex(/^[a-f0-9]{64}$/) })).min(1),
  checks: z.array(
    z.strictObject({
      id: z.string(),
      status: z.enum(["passed", "failed", "unmeasured"]),
      scope: z.string().min(1),
      measurements: z.record(z.string(), z.union([z.number().finite(), z.string(), z.boolean(), z.null()])),
      evidence: z.array(z.string()),
    }),
  ),
});
export function validateNativeEvaluation(
  input: unknown,
  suiteKey: string,
  task: BenchmarkTask,
  sources: { path: string; sha256: string }[],
) {
  const report = reportSchema.parse(input);
  if (report.suiteKey !== suiteKey || report.task !== task.id)
    throw Error("Native review belongs to another task");
  const keys = new Map(sources.map((s) => [s.path, s.sha256]));
  if (
    new Set(report.sources.map((s) => s.path)).size !== sources.length ||
    report.sources.length !== sources.length ||
    report.sources.some((s) => keys.get(s.path) !== s.sha256)
  )
    throw Error("Native review belongs to different submitted source bytes");
  const expected = new Set(task.constraints.map((c) => c.id));
  if (
    report.checks.length !== expected.size ||
    new Set(report.checks.map((c) => c.id)).size !== expected.size ||
    report.checks.some((c) => !expected.has(c.id))
  )
    throw Error("Native review must measure every registered constraint exactly once");
  return { report, passed: report.checks.every((c) => c.status === "passed") };
}

/** Run an operator-provided evaluator after the author exits; never execute an agent's claimed verifier. */
export async function evaluateNative(
  command: string[],
  directory: string,
  suiteKey: string,
  task: BenchmarkTask,
  sources: { path: string; sha256: string }[],
  seconds: number,
) {
  const output = join(directory, "native-review.json"),
    request = join(directory, "native-evaluation-request.json");
  await Bun.write(
    request,
    JSON.stringify({ version: 1, suiteKey, task, sources, output, directory }, null, 2),
  );
  const child = Bun.spawn([...command, request], {
    stdout: Bun.file(join(directory, "native-evaluation.log")),
    stderr: Bun.file(join(directory, "native-evaluation-errors.log")),
  });
  let timeout = false;
  const timer = setTimeout(() => {
    timeout = true;
    child.kill("SIGKILL");
  }, seconds * 1000);
  let code: number;
  try {
    code = await child.exited;
  } finally {
    clearTimeout(timer);
  }
  if (timeout || code !== 0)
    throw Error(timeout ? "Native evaluator exceeded its budget" : `Native evaluator exited ${code}`);
  return validateNativeEvaluation(await Bun.file(output).json(), suiteKey, task, sources);
}
