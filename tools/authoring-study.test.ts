import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  AuthoringSession,
  authoringJob,
  createWorkSession,
  decideWork,
  domainQuality,
  instantiateEditRecipe,
  materializeWorkProposal,
  planDomainAuthoring,
  promoteStudyRecipe,
  type StudyReviewer,
  studyAuthoring,
} from "@wrela/authoring";
import { contentKey } from "@wrela/model";
import { reviewAuthoring, reviewStage } from "@wrela/review";
import { authoringAgentTools, invokeAuthoringAgent } from "./authoring-agent";
import { WorkFileStore } from "./authoring-work-store";
import { WorkspaceBridge } from "./bridge";
import { transferTimber, transferTree } from "./fixtures/authoring-transfer";

const conditions = { exposure: 0.75, moisture: 0.4, maturity: 0.8, variation: 0.5 };
function apply(project: ReturnType<typeof transferTimber>, plan: ReturnType<typeof planDomainAuthoring>) {
  const s = new AuthoringSession(project);
  s.apply({ expectedRevision: 0, operations: plan.operations });
  return s.getSnapshot().project;
}
for (const domain of ["timber", "vegetation"] as const)
  test(`${domain} semantic conditions transfer to a second shape while preserving authored structure`, async () => {
    for (const heldout of [false, true]) {
      const p = domain === "timber" ? transferTimber(heldout) : transferTree(heldout);
      const plan = planDomainAuthoring(
        p,
        { domain, target: p.entry, conditions, quality: domainQuality(domain) },
        "transfer",
      );
      const q = apply(p, plan);
      expect(contentKey(q)).not.toBe(contentKey(p));
      const r = await reviewAuthoring(p, q, plan.constraints);
      expect(r.results.every((r) => r.status === "passed")).toBe(true);
      const same = p.documents.filter((d) => d.id !== p.entry);
      expect(q.documents.filter((d) => same.some((s) => s.id === d.id))).toEqual(same);
      if (domain === "vegetation") {
        const d = q.documents.find((d) => d.id === p.entry);
        if (d?.kind !== "vegetation") throw Error("Missing plant");
        expect(d.botanical?.growth.asymmetry).toBeCloseTo(0.495);
        expect(d.botanical?.canopy.density).toBeGreaterThan(0.85);
        expect(d.botanical?.canopy.density).toBeLessThanOrEqual(1);
      }
    }
  });
test("review rigs are identical across source revisions, preserve source, and refuse missing-stage fallback", () => {
  const p = transferTimber(),
    q = structuredClone(p);
  q.name = "Changed";
  const key = contentKey(p);
  const view = {
    id: "neutral",
    tick: 0,
    rig: "neutral" as const,
    camera: {
      position: [5, 3, 7] as [number, number, number],
      target: [0, 1, 0] as [number, number, number],
      fov: 42,
    },
  };
  const a = reviewStage(p, p.entry, view),
    b = reviewStage(q, q.entry, view);
  expect(a.lightingKey).toBe(b.lightingKey);
  expect(contentKey(p)).toBe(key);
  expect(a.project.documents.length).toBe(p.documents.length + 3);
  expect(() => reviewStage(p, p.entry, { ...view, rig: undefined, stage: "missing" })).toThrow("fallback");
});
test("bounded search retains failed alternatives, enforces CAS and promotes only explicitly accepted semantic recipes", async () => {
  const dir = await mkdtemp(join(tmpdir(), "wrela-study-"));
  try {
    const p = transferTimber(),
      store = new WorkFileStore(dir),
      bridge = new WorkspaceBridge(dir);
    await bridge.initialize();
    await bridge.save(p, null);
    const work = createWorkSession(p, { id: "wood", brief: "Weathered timber" });
    await store.put(work, null);
    const png = await store.saveArtifact(
      "fixture.png",
      new Blob([new Uint8Array([1])], { type: "image/png" }),
    );
    const review: StudyReviewer = async (b, cs, constraints, settings) => ({
      gallery: png,
      results: await Promise.all(
        cs.map(async (c, i) =>
          i === 1
            ? { proposal: c.proposal, error: "Injected hardware failure" }
            : {
                proposal: c.proposal,
                packet: {
                  version: 1 as const,
                  report: await reviewAuthoring(b, c.project, constraints),
                  contactSheet: png,
                  evidence: [],
                  captures: settings.views.map((v) => ({ view: v.id, baseline: png, candidate: png })),
                  timings: { reviewMs: 1, captureMs: 1, totalMs: 2, firstCandidateImageMs: 2 },
                },
              },
        ),
      ),
    });
    const input = {
      id: "search",
      expectedKey: contentKey(work),
      brief: { domain: "timber" as const, target: p.entry, conditions, quality: domainQuality("timber") },
      candidates: 3,
    };
    const result = await studyAuthoring(store, work.id, input, review);
    expect(result.candidates.map((c) => c.passed)).toEqual([true, false, true]);
    expect((await bridge.read())?.key).toBe(contentKey(p));
    await expect(studyAuthoring(store, work.id, input, review)).rejects.toThrow("Work changed");
    let current = await store.get(work.id);
    expect(current.proposals.length).toBe(3);
    expect(current.events.some((e) => e.kind === "failure")).toBe(true);
    const proposal = result.candidates[0].proposal;
    await expect(
      promoteStudyRecipe(current, proposal, "accepted-wood", (ref) => Bun.file(ref).json()),
    ).rejects.toThrow("artistic acceptance");
    current = decideWork(current, proposal, {
      reviewer: "test-fixture",
      verdict: "accepted",
      reason: "Execution fixture only, not an art judgement",
    });
    const recipe = await promoteStudyRecipe(current, proposal, "accepted-wood", (ref) =>
      Bun.file(ref).json(),
    );
    expect(recipe.semantic?.conditions).toEqual(conditions);
    const heldout = transferTimber(true),
      instantiated = instantiateEditRecipe(heldout, recipe, {}, { [p.entry]: heldout.entry });
    expect(instantiated.acceptance).toBe("unreviewed");
    const s = new AuthoringSession(heldout);
    s.apply({ expectedRevision: 0, operations: instantiated.operations });
    expect(
      (await reviewAuthoring(heldout, s.getSnapshot().project, instantiated.constraints)).results.every(
        (r) => r.status === "passed",
      ),
    ).toBe(true);
    expect(contentKey(materializeWorkProposal(current, proposal))).not.toBe(contentKey(p));
    const native = await invokeAuthoringAgent({
      workspace: dir,
      work: work.id,
      action: "context",
      input: { target: p.entry },
    });
    expect(native.result.study).toBeDefined();
    expect(authoringAgentTools.map((t) => t.name)).toEqual([
      "wrela_hero",
      "wrela_iterate",
      "wrela_toolkit",
      "wrela_job",
      "wrela_context",
      "wrela_study",
      "wrela_finish",
    ]);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
test("invalid conditions reject before proposal generation and geometry budgets measure both domains", async () => {
  const p = transferTimber();
  expect(() =>
    planDomainAuthoring(
      p,
      {
        domain: "timber",
        target: p.entry,
        conditions: { ...conditions, moisture: 2 },
        quality: domainQuality("timber"),
      },
      "invalid",
    ),
  ).toThrow();
  for (const p of [transferTimber(), transferTree(true)]) {
    const r = await reviewAuthoring(p, p, [
      {
        id: "budget",
        kind: "geometry",
        target: p.entry,
        minTriangles: 1,
        maxTriangles: 1,
        minimumExtent: [0, 0, 0],
        maximumExtent: [100, 100, 100],
      },
    ]);
    expect(r.results[0].status).toBe("failed");
    expect(Number(r.results[0].measured.triangles)).toBeGreaterThan(1);
  }
});

test("native MCP stdio advertises bounded tools and returns protocol errors without leaking diagnostics to stdout", async () => {
  const process = Bun.spawn([Bun.which("bun") ?? "bun", "tools/authoring-agent.ts", "--mcp"], {
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  process.stdin.write(
    [
      { jsonrpc: "2.0", id: 1, method: "initialize" },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/list" },
      { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "unknown", arguments: {} } },
    ]
      .map((value) => JSON.stringify(value))
      .join("\n") + "\n",
  );
  process.stdin.end();
  const lines = (await new Response(process.stdout).text())
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line));
  expect(await process.exited).toBe(0);
  expect(lines).toHaveLength(3);
  expect(lines[0].result.capabilities.tools).toEqual({});
  expect(lines[1].result.tools.map((t: { name: string }) => t.name)).toEqual(
    authoringAgentTools.map((t) => t.name),
  );
  expect(lines[2].result.isError).toBe(true);
});

test("exposure and maturity desaturate the underlying wood fibres rather than merely adding isolated pale patches", () => {
  const project = transferTimber();
  const colors = (exposure: number, maturity: number) => {
    const plan = planDomainAuthoring(
      project,
      {
        domain: "timber",
        target: project.entry,
        conditions: { ...conditions, exposure, maturity },
        quality: domainQuality("timber"),
      },
      "silvering",
    );
    return plan.operations.flatMap((op) =>
      op.kind === "document.create" && op.document.kind === "material" ? [op.document.color] : [],
    );
  };
  const fresh = colors(0, 0),
    exposed = colors(1, 1);
  expect(exposed.length).toBe(fresh.length);
  for (let i = 0; i < fresh.length; i++) {
    expect(Math.max(...exposed[i]) - Math.min(...exposed[i])).toBeLessThan(
      (Math.max(...fresh[i]) - Math.min(...fresh[i])) / 3,
    );
    expect(exposed[i][2]).toBeGreaterThan(fresh[i][2]);
  }
});

for (const domain of ["timber", "vegetation"] as const)
  test(`${domain} one-request job resolves its subject and target without discovery or automatic acceptance`, async () => {
    const dir = await mkdtemp(join(tmpdir(), "wrela-job-"));
    try {
      const project = domain === "timber" ? transferTimber() : transferTree();
      const store = new WorkFileStore(dir),
        bridge = new WorkspaceBridge(dir);
      await bridge.initialize();
      await bridge.save(project, null);
      const work = createWorkSession(project, {
        id: "job",
        brief: domain === "timber" ? "Weathered silver trail gate" : "Mature exposed trail pine",
      });
      await store.put(work, null);
      let reviews = 0;
      const review: StudyReviewer = async (b, cs, constraints, settings) => {
        reviews++;
        expect(settings.target).toBe(project.entry);
        expect(settings.budgetMs).toBe(15000);
        return {
          gallery: "fixture.png",
          results: await Promise.all(
            cs.map(async (c) => ({
              proposal: c.proposal,
              packet: {
                version: 1 as const,
                report: await reviewAuthoring(b, c.project, constraints),
                contactSheet: "fixture.png",
                evidence: [],
                captures: settings.views.map((v) => ({
                  view: v.id,
                  baseline: "fixture.png",
                  candidate: "fixture.png",
                })),
                timings: { reviewMs: 1, captureMs: 1, totalMs: 2, firstCandidateImageMs: 1 },
              },
            })),
          ),
        };
      };
      const result = await authoringJob(store, work.id, { budgetSeconds: 15 }, review);
      expect(reviews).toBe(1);
      expect(result.candidates).toHaveLength(3);
      expect(result.finish?.expectedKey).toBe(contentKey(await store.get(work.id)));
      expect(result.candidates.find((c) => c.proposal === result.recommendation)?.passed).toBe(true);
      expect(result.ranking.visualScore).toBeNull();
      expect(result.exemplar.approval).toBe("curated-unreviewed");
      expect((await bridge.read())?.key).toBe(contentKey(project));
      await expect(
        authoringJob(store, work.id, { expectedKey: contentKey(work), budgetSeconds: 15 }, review),
      ).rejects.toThrow("Work changed");
      await expect(
        authoringJob(
          store,
          work.id,
          { exemplar: domain === "timber" ? "dry-open-crown" : "silvered-trail-timber", budgetSeconds: 15 },
          review,
        ),
      ).rejects.toThrow("domain");
      expect(reviews).toBe(1);
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });
