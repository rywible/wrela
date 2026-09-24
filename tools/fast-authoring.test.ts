import { expect, test } from "bun:test";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  AuthoringSession,
  authoringTaskContext,
  createWorkSession,
  evaluateWork,
  materializeWorkProposal,
  planAuthoringIntent,
} from "@wrela/authoring";
import { compileAssemblyMesh } from "@wrela/compiler";
import { type AssemblyDefinition, contentKey } from "@wrela/model";
import { reviewAuthoring } from "@wrela/review";
import { AssemblyMotion } from "@wrela/runtime";
import { runAuthoring } from "./author";
import { finishAuthoringWork } from "./authoring-finish";
import { readAuthoringTrace, summarizeAuthoringTrace } from "./authoring-trace";
import { runAuthoringWork } from "./authoring-work";
import { WorkFileStore } from "./authoring-work-store";
import { WorkspaceBridge } from "./bridge";
import { fastAuthoringProject } from "./fixtures/fast-authoring";

function assembly(project = fastAuthoringProject()) {
  const d = project.documents.find((d) => d.id === "timber-frame");
  if (d?.kind !== "object" || !d.assembly) throw Error("Missing frame");
  return d.assembly;
}

test("task context supplies bounded semantic members, real dependencies and usable review cameras", () => {
  const p = fastAuthoringProject(),
    c = authoringTaskContext(p, "timber-frame");
  expect(c.openingMembers).toEqual({ left: "left-post", right: "right-post", span: "lintel" });
  expect(c.views).toHaveLength(4);
  expect(JSON.stringify(c).length).toBeLessThan(14000);
  expect(c.dependencies.some((d) => d.id === "bark")).toBe(true);
  expect(c.workflow.evaluate).toContain("work evaluate");
});
test("opening plan preserves beam profiles and socket anchors, closes the widened span and proves clear volume", async () => {
  const p = fastAuthoringProject(),
    plan = planAuthoringIntent(p, { kind: "assembly.opening", target: "timber-frame", width: 3.2 });
  const s = new AuthoringSession(p);
  s.apply({ expectedRevision: 0, operations: plan.operations });
  const after = s.getSnapshot().project;
  for (const [i, part] of assembly(after).parts.entries()) {
    expect(part.profile).toEqual(assembly(p).parts[i].profile);
    expect(part.sockets).toEqual(assembly(p).parts[i].sockets);
  }
  const result = await reviewAuthoring(p, after, plan.constraints);
  expect(result.results.every((r) => r.status === "passed")).toBe(true);
  const mesh = compileAssemblyMesh(assembly(after), "bark").mesh;
  expect(mesh.bounds.min[0]).toBeCloseTo(-1.85);
  expect(mesh.bounds.max[0]).toBeCloseTo(1.85);
  const ambiguous = structuredClone(p);
  assembly(ambiguous).parts[0].rotation[1] = 0.3;
  expect(() =>
    planAuthoringIntent(ambiguous, { kind: "assembly.opening", target: "timber-frame", width: 3 }),
  ).toThrow("ambiguous");
});
test("timber editing isolates materials and does not paint a horizontal lintel as a vertical post", () => {
  const p = fastAuthoringProject(),
    plan = planAuthoringIntent(p, {
      kind: "assembly.timber",
      target: "timber-frame",
      material: "timber",
      age: 0.7,
    });
  const s = new AuthoringSession(p);
  s.apply({ expectedRevision: 0, operations: plan.operations });
  const q = s.getSnapshot().project;
  expect(q.documents.find((d) => d.id === "bark")).toEqual(p.documents.find((d) => d.id === "bark"));
  const beam = q.documents.find((d) => d.id === "timber-2"),
    post = q.documents.find((d) => d.id === "timber-0");
  expect(beam?.kind === "material" && beam.appearance?.layers[0].coverage).toBeLessThan(0.0001);
  expect(post?.kind === "material" && post.appearance?.layers[0].mask.kind).toBe("height");
  expect(() =>
    planAuthoringIntent(p, { kind: "assembly.timber", target: "timber-frame", material: "bark" }),
  ).toThrow("shared");
});
test("grain stays in the member frame through joint rotation, translation and repeats", () => {
  const source = assembly();
  source.parts[0].repeat = { count: 2, offset: [4, 0, 0] };
  source.parts[1].joint = {
    kind: "hinge",
    axis: [0, 0, 1],
    pivot: [0, 0, 0],
    minimum: -2,
    maximum: 2,
    value: 0,
  };
  const a = compileAssemblyMesh(source, "bark").mesh;
  expect(a.materialCoordinates?.length).toBe(a.positions.length);
  const turned: AssemblyDefinition = structuredClone(source);
  turned.parts.forEach((p) => {
    p.rotation = [0.2, 0.8, 0.4];
    p.position = p.position.map((v) => v + 10) as [number, number, number];
  });
  const b = compileAssemblyMesh(turned, "bark", { "right-post": 0.6 }).mesh;
  expect(
    Math.max(...Array.from(a.materialCoordinates!, (v, i) => Math.abs(v - b.materialCoordinates![i]))),
  ).toBeLessThan(1e-6);
  const lintelIndices = a.sourceIds!.flatMap((id, i) => (id === "lintel" ? [i] : []));
  const longitudinal = lintelIndices.map((i) => a.materialCoordinates![i * 3 + 1]);
  expect(Math.min(...longitudinal)).toBeCloseTo(0);
  expect(Math.max(...longitudinal)).toBeCloseTo(2.5);
  a.skyVisibility = new Float32Array((a.positions.length / 3) * 4);
  for (let i = 0; i < a.positions.length / 3; i++) a.skyVisibility.set([0, 0.75, 0, 0.5], i * 4);
  const runtime = new AssemblyMotion(source, a);
  for (const part of runtime.meshes) {
    const sourceId = part.sourceIds?.[0];
    const original = Array.from(a.indices).filter((i) => a.sourceIds?.[i] === sourceId);
    expect(Array.from(part.materialCoordinates ?? [])).toEqual(
      original.flatMap((i) => Array.from(a.materialCoordinates!.subarray(i * 3, i * 3 + 3))),
    );
    expect(part.skyVisibility?.length).toBe((part.positions.length / 3) * 4);
    expect(part.skyVisibility?.[3]).toBe(0.5);
  }
});
test("evaluation retains failed candidates and finish exports reviewed source without hand-built manifests", async () => {
  const dir = await mkdtemp(join(tmpdir(), "wrela-fast-"));
  try {
    const p = fastAuthoringProject(),
      bridge = new WorkspaceBridge(dir),
      store = new WorkFileStore(dir);
    await bridge.initialize();
    await bridge.save(p, null);
    let w = createWorkSession(p, { id: "gate", brief: "Widen the opening" });
    await store.put(w, null);
    await expect(
      evaluateWork(
        store,
        w.id,
        {
          expectedKey: contentKey(w),
          proposal: "failed",
          target: "timber-frame",
          operations: [{ kind: "document.rename", target: "timber-frame", name: "Renamed" }],
        },
        async () => {
          throw Error("GPU unavailable");
        },
      ),
    ).rejects.toThrow("GPU unavailable");
    w = await store.get(w.id);
    expect(w.proposals).toHaveLength(1);
    expect(w.events.at(-1)?.kind).toBe("failure");
    expect((await bridge.read())?.key).toBe(contentKey(p));
    // The source rename protects geometry; CPU review is real. Tiny image is only a storage fixture.
    const evaluated = await evaluateWork(
      store,
      w.id,
      { expectedKey: contentKey(w), proposal: "failed", target: "timber-frame" },
      async (b, c, work, settings) => {
        const png = await store.saveArtifact(
          "fixture.png",
          new Blob(
            [
              Uint8Array.from(
                atob(
                  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/l9sAAAAASUVORK5CYII=",
                ),
                (c) => c.charCodeAt(0),
              ),
            ],
            { type: "image/png" },
          ),
        );
        return {
          version: 1,
          report: await reviewAuthoring(b, c, work.constraints),
          contactSheet: png,
          evidence: [],
          captures: settings.views.map((v) => ({ view: v.id, baseline: png, candidate: png })),
          timings: { reviewMs: 1, captureMs: 1, totalMs: 2, firstCandidateImageMs: 2 },
        };
      },
    );
    const pending = await finishAuthoringWork(
      dir,
      w.id,
      { expectedKey: evaluated.key, proposal: "failed" },
      async () => ({ outcome: "committed", receiptPending: true }),
    );
    expect(pending.published).toBe(true);
    expect(pending.packagingPending).toBe(true);
    expect(await Bun.file(join(pending.output!, "final.json")).exists()).toBe(false);
    const result = await finishAuthoringWork(
      dir,
      w.id,
      { expectedKey: evaluated.key, proposal: "failed" },
      async (input) => {
        const path = join(dir, "adopt.json");
        await Bun.write(path, JSON.stringify(input));
        return runAuthoringWork(dir, ["adopt", w.id, path]);
      },
    );
    expect(result.published).toBe(true);
    expect(result.packagingPending).toBe(false);
    expect(await Bun.file(join(result.output!, "handoff-bundle.json")).exists()).toBe(true);
    expect((await bridge.read())?.key).toBe(
      contentKey(materializeWorkProposal(await store.get(w.id), "failed")),
    );
    const output = join(dir, "benchmark");
    await mkdir(output);
    const request = join(output, "request.json");
    await Bun.write(
      request,
      JSON.stringify({
        workspace: dir,
        task: { workId: w.id },
        output,
        submission: join(output, "submission.json"),
      }),
    );
    const finishInput = {
      expectedKey: contentKey(await store.get(w.id)),
      proposal: "failed",
      benchmarkRequest: request,
    };
    const submitted = await finishAuthoringWork(dir, w.id, finishInput, async () => {
      throw Error("Already adopted source must not be published again");
    });
    expect(submitted.packagingPending).toBe(false);
    const submission = await Bun.file(join(output, "submission.json")).text();
    expect(
      JSON.parse(submission).artifacts.some((a: { path: string }) => a.path === "contact-sheet.png"),
    ).toBe(true);
    await expect(finishAuthoringWork(dir, w.id, finishInput, async () => undefined)).rejects.toThrow(
      "cannot be replaced",
    );
    expect(await Bun.file(join(output, "submission.json")).text()).toBe(submission);
    await expect(runAuthoring([dir, "work", "context", "missing"])).rejects.toThrow();
    const trace = await readAuthoringTrace(dir);
    expect(trace.at(-1)?.ok).toBe(false);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
test("timing counts actual failures and does not double-count overlapping calls as model time", () => {
  const rows = [
    {
      version: 1 as const,
      id: "a",
      command: "work.context",
      startedAt: 100,
      endedAt: 300,
      durationMs: 200,
      ok: true,
    },
    {
      version: 1 as const,
      id: "b",
      command: "work.evaluate",
      firstCandidateImageAt: 380,
      startedAt: 200,
      endedAt: 400,
      durationMs: 200,
      ok: false,
    },
  ];
  const m = summarizeAuthoringTrace(rows, 0, 1000);
  expect(m.observedToolWallMs).toBe(300);
  expect(m.unattributedMs).toBe(700);
  expect(m.failedCalls).toBe(1);
  expect(m.discoveryCalls).toBe(1);
  expect(m.discoveryMs).toBe(200);
  expect(m.firstCandidateImageMs).toBe(380);
  expect(m.modelMs).toBeNull();
});
