import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import {
  createHeroWorkspace,
  createWorkSession,
  domainQuality,
  materializeWorkProposal,
} from "@wrela/authoring";
import { contentKey } from "@wrela/model";
import { invokeAuthoringAgent } from "./authoring-agent";
import { withAuthoringReviewSession } from "./authoring-review-session";
import { WorkFileStore } from "./authoring-work-store";
import { WorkspaceBridge } from "./bridge";
import { distribution, sourceManifest } from "./evidence";
import { expeditionLantern, hangingBell } from "./fixtures/hero-authoring";

export async function measureHeroLatency(destination: string, suiteDirectory: string, repetitions = 5) {
  const output = resolve(destination),
    suite = resolve(suiteDirectory);
  await mkdir(output, { recursive: true });
  const source = await sourceManifest("hero-authoring-latency");
  await Bun.write(join(output, "source-manifest.json"), JSON.stringify(source, null, 2));
  const rows: {
    domain: string;
    mode: string;
    repetition: number;
    elapsedMs: number;
    startupMs: number;
    candidateKey: string;
    images: string[];
    candidates: unknown;
  }[] = [];
  async function sample(domain: string, mode: string, repetition: number, startupMs = 0) {
    const workspace = join(output, `${mode}-${domain}-${repetition}`),
      bridge = new WorkspaceBridge(workspace),
      store = new WorkFileStore(workspace);
    const baseline = ["wood", "vegetation"].includes(domain)
      ? await Bun.file(join(suite, domain, "baseline.json")).json()
      : createHeroWorkspace();
    await bridge.initialize();
    await bridge.save(baseline, null);
    const work = createWorkSession(baseline, { id: "latency", brief: `Scripted ${domain} latency fixture` });
    await store.put(work, null);
    const isHero = ["lantern", "bell"].includes(domain),
      design = domain === "lantern" ? expeditionLantern() : hangingBell();
    design.stage = "production";
    design.seed = (design.seed ?? 73) + repetition;
    design.age = 0.45 + repetition * 0.035;
    const input = isHero
      ? {
          expectedKey: contentKey(work),
          id: "candidate",
          designs: [design],
          width: 640,
          height: 480,
          budgetSeconds: 60,
        }
      : {
          expectedKey: contentKey(work),
          id: "candidate",
          candidates: 3,
          budgetSeconds: 60,
          brief: {
            domain: domain === "wood" ? "timber" : "vegetation",
            target: domain === "wood" ? "benchmark-gate" : "study-tree",
            conditions: {
              exposure: 0.75 + repetition * 0.015,
              moisture: 0.4,
              maturity: 0.85,
              variation: 0.55,
            },
            quality: domainQuality(domain === "wood" ? "timber" : "vegetation"),
          },
        };
    const start = performance.now(),
      result = await invokeAuthoringAgent({
        workspace,
        work: work.id,
        action: isHero ? "hero" : "study",
        input,
      });
    const elapsedMs = performance.now() - start,
      current = await store.get(work.id),
      candidateKey = contentKey(materializeWorkProposal(current, current.proposals[0].id));
    const candidates = result.result.candidates as { passed: boolean }[];
    if (!candidates.length || candidates.some((c) => !c.passed))
      throw Error("Measured candidate did not pass");
    const row = {
      domain,
      mode,
      repetition,
      elapsedMs,
      startupMs,
      candidateKey,
      images: result.images,
      candidates: result.result.candidates,
    };
    rows.push(row);
    console.log(JSON.stringify({ domain, mode, repetition, elapsedMs, startupMs }));
  }
  for (let r = 0; r < repetitions; r++)
    for (const domain of ["wood", "vegetation", "lantern", "bell"]) {
      const started = performance.now();
      await withAuthoringReviewSession(() => sample(domain, "cold", r, performance.now() - started));
    }
  const warmStarted = performance.now();
  let warmStartupMs = 0;
  await withAuthoringReviewSession(async () => {
    warmStartupMs = performance.now() - warmStarted;
    for (let r = 0; r < repetitions; r++)
      for (const domain of ["wood", "vegetation", "lantern", "bell"]) await sample(domain, "warm", r);
  });
  for (const cold of rows.filter((r) => r.mode === "cold")) {
    const warm = rows.find(
      (r) => r.mode === "warm" && r.domain === cold.domain && r.repetition === cold.repetition,
    );
    if (warm?.candidateKey !== cold.candidateKey) throw Error("Warm source differs from paired cold source");
  }
  const summary = ["wood", "vegetation", "lantern", "bell"].map((domain) => ({
    domain,
    coldToolMs: distribution(
      rows.filter((r) => r.domain === domain && r.mode === "cold").map((r) => r.elapsedMs),
    ),
    coldIncludingStartupMs: distribution(
      rows.filter((r) => r.domain === domain && r.mode === "cold").map((r) => r.elapsedMs + r.startupMs),
    ),
    warmToolMs: distribution(
      rows.filter((r) => r.domain === domain && r.mode === "warm").map((r) => r.elapsedMs),
    ),
  }));
  const report = {
    sourceFingerprint: source.sourceFingerprint,
    repetitions,
    warmStartupMs,
    summary,
    rows,
    scope:
      "Paired deterministic scripted tool execution; five unique inputs per domain. Known designs, not fresh-agent creation or artistic qualification. Cold means a fresh browser, not a purged driver or disk cache. Browser shutdown and setup of persisted work excluded from tool timing.",
  };
  await Bun.write(join(output, "report.json"), JSON.stringify(report, null, 2));
  return { output, warmStartupMs, summary };
}
if (import.meta.main)
  console.log(
    JSON.stringify(
      await measureHeroLatency(process.argv[2], process.argv[3], Number(process.argv[4] ?? 5)),
      null,
      2,
    ),
  );
