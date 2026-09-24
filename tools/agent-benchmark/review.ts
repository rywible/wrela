import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import {
  type BenchmarkAttempt,
  type BenchmarkJudgement,
  benchmarkAttemptSchema,
  benchmarkJudgementSchema,
  benchmarkSuiteSchema,
  summarizeBenchmark,
} from "./protocol";
import { hashBenchmarkArtifact } from "./runner";

export async function readAttempts(directory: string) {
  const root = resolve(directory),
    attempts: BenchmarkAttempt[] = [];
  for (const path of new Bun.Glob("attempts/*/attempt.json").scanSync(root))
    attempts.push(benchmarkAttemptSchema.parse(await Bun.file(join(root, path)).json()));
  return attempts;
}
const html = (s: string) =>
  s.replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c] ?? c,
  );
export async function createBlindReview(directory: string) {
  const root = resolve(directory),
    output = join(root, "blind"),
    attempts = await readAttempts(root);
  const suite = benchmarkSuiteSchema.parse(await Bun.file(join(root, "suite.json")).json());
  await mkdir(output, { recursive: true });
  if (await Bun.file(join(root, "blind-mapping.json")).exists())
    throw Error("Blind mapping already exists; keep review identities stable");
  const eligible = attempts.filter((a) => a.execution === "agent" && a.status === "completed");
  const ordered = eligible
    .map((a) => ({ a, order: crypto.randomUUID() }))
    .sort((a, b) => a.order.localeCompare(b.order));
  const mapping: Record<string, string> = {},
    cards: string[] = [];
  for (let index = 0; index < ordered.length; index++) {
    const attempt = ordered[index].a,
      id = `sample-${index + 1}`,
      images: string[] = [];
    mapping[id] = attempt.id;
    for (const [n, artifact] of attempt.artifacts.filter((a) => a.kind === "image").entries()) {
      const dir = join(root, "attempts", attempt.id),
        verified = await hashBenchmarkArtifact(dir, artifact.path);
      if (verified.sha256 !== artifact.sha256) throw Error("Attempt artifact changed after submission");
      const file = Bun.file(join(dir, artifact.path));
      const name = `${id}-${n}.png`;
      await Bun.write(join(output, name), file);
      images.push(name);
    }
    cards.push(
      `<article><h2>${id}</h2><p>Task: ${html(attempt.task)}</p><p>${html(suite.tasks.find((t) => t.id === attempt.task)?.brief ?? "")}</p>${images.map((name) => `<img src="${name}" alt="${id} review view">`).join("")}<p>First record visual quality and brief adherence without identities or timing. Assess editable source in a separate second phase.</p></article>`,
    );
  }
  await Bun.write(join(root, "blind-mapping.json"), JSON.stringify(mapping, null, 2));
  await Bun.write(
    join(output, "index.html"),
    `<!doctype html><meta charset="utf-8"><title>Authoring review</title><style>body{font:16px system-ui;margin:32px;background:#18201e;color:#edf2ee}article{margin:32px 0}img{max-width:46%;margin:1%}</style><h1>Blind authoring review</h1><p>Judge against the briefs before opening engine identities or costs. Empty galleries establish no acceptance.</p>${cards.join("")}`,
  );
  return { output, samples: ordered.length };
}
export async function reportBenchmark(directory: string) {
  const root = resolve(directory),
    suite = benchmarkSuiteSchema.parse(await Bun.file(join(root, "suite.json")).json());
  const mapping = await Bun.file(join(root, "blind-mapping.json"))
    .json()
    .catch(() => ({}));
  const judgements: BenchmarkJudgement[] = [];
  for (const file of new Bun.Glob("judgements/*.json").scanSync(root))
    judgements.push(benchmarkJudgementSchema.parse(await Bun.file(join(root, file)).json()));
  const attempts = await readAttempts(root);
  for (const attempt of attempts)
    for (const artifact of attempt.artifacts) {
      if (
        (await hashBenchmarkArtifact(join(root, "attempts", attempt.id), artifact.path)).sha256 !==
        artifact.sha256
      )
        throw Error("Attempt artifact changed after submission");
    }
  const summary = summarizeBenchmark(suite, attempts, judgements, mapping);
  const complete =
    summary.every(
      (s) =>
        s.attempted === s.expected &&
        !s.pending &&
        !s.unavailable &&
        !s.unknownTokenRuns &&
        !s.unknownInterventionRuns,
    ) &&
    attempts
      .filter((a) => a.execution === "agent" && a.status === "completed")
      .every((a) => a.constraintsPassed !== null && judgements.some((j) => mapping[j.blindId] === a.id));
  const report = {
    suite: suite.id,
    summary,
    attempts,
    judgements,
    comparisonEstablished: complete,
    limitations: [
      "Artistic acceptance requires independent blind review.",
      "Unknown token/intervention costs are not zero.",
      "Scripted smoke tests are excluded from agent productivity.",
      "Compare asset tasks with Blender and scene/gameplay tasks with Unreal; browser delivery is a separate axis.",
    ],
  };
  await Bun.write(join(root, "report.json"), JSON.stringify(report, null, 2));
  return report;
}
