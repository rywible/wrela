import { type ObjectDefinition, referenceProject } from "@wrela/model";
import { AuthoringSession } from "../packages/authoring/src/session";

/** Reproduce the architecture review's accepted production-sized source workload.
 * CPU acceptance latency only: compilation, subscribers and visible frames are separate. */
export function benchmarkAuthoring(definitions = 200, samples = 20) {
  if (
    !Number.isInteger(definitions) ||
    definitions < 20 ||
    definitions > 256 ||
    !Number.isInteger(samples) ||
    samples < 1 ||
    samples > 1000
  )
    throw Error("Expected 20–256 definitions and 1–1000 samples");
  const project = referenceProject();
  const template = project.documents.find(
    (document): document is ObjectDefinition => document.kind === "object",
  );
  if (!template) throw Error("Missing object fixture");
  const primitive =
    template.field.nodes.find((node) => node.kind === "ellipsoid" || node.kind === "sphere") ??
    template.field.nodes[0];
  for (let i = 0; project.documents.length < definitions; i++) {
    const document = structuredClone(template);
    document.id = `benchmark-${i}`;
    const leaves = Array.from({ length: 125 }, (_, index) => ({
      ...structuredClone(primitive),
      id: `p${index}`,
      kind: "sphere" as const,
      children: [],
    }));
    const group = (id: string, children: string[]) => ({
      ...structuredClone(primitive),
      id,
      kind: "union" as const,
      children,
    });
    document.field.nodes = [
      group("root", ["g1", "g2"]),
      group(
        "g1",
        leaves.slice(0, 63).map((node) => node.id),
      ),
      group(
        "g2",
        leaves.slice(63).map((node) => node.id),
      ),
      ...leaves,
    ];
    document.field.root = "root";
    project.documents.push(document);
  }
  const session = new AuthoringSession(project),
    timings: number[] = [];
  const initial = session.getSnapshot().project;
  for (let i = 0; i < samples + 1; i++) {
    const start = performance.now();
    const result = session.apply({
      expectedRevision: i,
      operations: [{ kind: "document.rename", target: "benchmark-0", name: `Edit ${i}` }],
    });
    if (!result.key) throw Error("Source key was not produced");
    const elapsed = performance.now() - start;
    if (i > 0) timings.push(elapsed);
  }
  const sorted = [...timings].sort((a, b) => a - b),
    percentile = (fraction: number) =>
      sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * fraction) - 1)];
  return {
    scenario: "authoring-200-definitions-128-node-fields-v1",
    definitions,
    sourceBytes: new TextEncoder().encode(JSON.stringify(project)).length,
    samples,
    warmupEdits: 1,
    acceptedMs: { p50: percentile(0.5), p95: percentile(0.95), max: sorted.at(-1) },
    unchangedDocumentsShared: session
      .getSnapshot()
      .project.documents.filter(
        (document) => initial.documents.find((old) => old.id === document.id) === document,
      ).length,
    history: session.historyUsage(),
  };
}
if (import.meta.main) {
  const report = benchmarkAuthoring();
  console.log(JSON.stringify(report, null, 2));
  const maximumP95 = Number(process.env.WRELA_AUTHORING_MAX_P95_MS ?? 50);
  if (!Number.isFinite(maximumP95) || maximumP95 <= 0) throw Error("Invalid authoring latency budget");
  if (report.acceptedMs.p95 > maximumP95) throw Error(`Authoring p95 exceeds ${maximumP95} ms budget`);
}
