import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { appendWorkEvent, createDoorwayAssembly, createWorkSession, proposeWork } from "@wrela/authoring";
import { referenceProject } from "@wrela/examples";
import { contentKey } from "@wrela/model";
import { reviewAuthoring } from "@wrela/review";
import { runAuthoringWork } from "./authoring-work";
import { WorkFileStore } from "./authoring-work-store";
import { WorkspaceBridge } from "./bridge";

test("independent work writers reject stale CAS and portable source survives process-equivalent reopen", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-work-test-"));
  try {
    const a = new WorkFileStore(directory),
      b = new WorkFileStore(directory);
    const work = createWorkSession(referenceProject(), { id: "test", brief: "Review material" });
    const key = await a.put(work, null);
    const next = proposeWork(work, "rough", [
      { kind: "document.set", target: "snow-fur", path: ["roughness"], value: 0.72 },
    ]);
    const results = await Promise.allSettled([a.put(next, key), b.put(next, key)]);
    expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
    expect(contentKey(await new WorkFileStore(directory).get("test"))).toBe(contentKey(next));
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
test("work evidence backup moves images and nested reports to a different workspace", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-portable-test-"));
  try {
    const a = new WorkFileStore(join(directory, "a")),
      b = new WorkFileStore(join(directory, "b"));
    const image = await a.saveArtifact(
      "image.png",
      new Blob([new Uint8Array([1, 2, 3])], { type: "image/png" }),
    );
    const report = await a.saveArtifact("report.json", { evidence: [image] });
    const work = appendWorkEvent(
      createWorkSession(referenceProject(), { id: "portable", brief: "Preserve evidence" }),
      {
        id: "capture",
        kind: "review",
        actor: "test",
        at: new Date().toISOString(),
        summary: "Captured",
        evidence: [report],
      },
    );
    await a.put(work, null);
    const restored = await b.restore(await a.backup(work.id));
    const ref = restored.events.at(-1)!.evidence[0];
    expect(ref).not.toBe(report);
    const data = await Bun.file(ref).json();
    expect(data.evidence[0]).not.toBe(image);
    expect(new Uint8Array(await Bun.file(data.evidence[0]).arrayBuffer())).toEqual(new Uint8Array([1, 2, 3]));
    const externalImage = join(directory, "external.png"),
      externalReport = join(directory, "external.json");
    await Bun.write(externalImage, new Uint8Array([4, 5, 6]));
    await Bun.write(externalReport, JSON.stringify({ evidence: [externalImage], image: externalImage }));
    const retained = await b.retainEvidence({ evidence: [externalReport, externalImage] }, directory);
    const nested = await Bun.file(retained.evidence[0]).json();
    expect(nested.evidence[0]).toBe(retained.evidence[1]);
    expect(nested.image).toBe(retained.evidence[1]);
    await expect(b.retainEvidence({ evidence: [externalReport] }, join(directory, "a"))).rejects.toThrow(
      "outside the request",
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
test("compiled result checks detect misplaced parts and blocked doorway volumes", async () => {
  const project = referenceProject(),
    object = project.documents.find((d) => d.kind === "object")!;
  object.assembly = createDoorwayAssembly(2, 2.6, 0.25);
  const candidate = structuredClone(project),
    changed = candidate.documents.find((d) => d.id === object.id);
  if (changed?.kind !== "object" || !changed.assembly) throw Error("Missing test object");
  changed.assembly.parts[1].position[0] = 0;
  const constraints = [
    {
      id: "clear",
      kind: "clearance" as const,
      target: object.id,
      minimum: [-0.99, 0.01, -0.12] as [number, number, number],
      maximum: [0.99, 2.59, 0.12] as [number, number, number],
    },
  ];
  expect((await reviewAuthoring(project, project, constraints)).results[0].status).toBe("passed");
  expect((await reviewAuthoring(project, candidate, constraints)).results[0].status).toBe("failed");
});
test("CLI review and adoption use actual constraints and publish through the workspace bridge", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-work-cli-"));
  try {
    const bridge = new WorkspaceBridge(directory);
    await bridge.initialize();
    await bridge.save(referenceProject(), null);
    const work = createWorkSession(referenceProject(), {
      id: "test",
      brief: "Matte fur",
      constraints: [{ id: "shape", kind: "source", target: "polar-bunny", path: ["field"] }],
    });
    const candidate = proposeWork(work, "rough", [
      { kind: "document.set", target: "snow-fur", path: ["roughness"], value: 0.72 },
    ]);
    const store = new WorkFileStore(directory);
    let key = await store.put(candidate, null);
    const request = join(directory, "request.json");
    await Bun.write(request, JSON.stringify({ expectedKey: key, proposal: "rough" }));
    await expect(runAuthoringWork(directory, ["adopt", "test", request])).rejects.toThrow("Every declared");
    await runAuthoringWork(directory, ["review", "test", request]);
    key = contentKey(await store.get("test"));
    await Bun.write(request, JSON.stringify({ expectedKey: key, proposal: "rough" }));
    await runAuthoringWork(directory, ["adopt", "test", request]);
    expect((await bridge.read())?.key).toBe(candidate.proposals[0].candidateKey);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
test("CPU review never grants silhouette approval without a hardware browser", async () => {
  const project = referenceProject();
  const result = await reviewAuthoring(project, project, [
    {
      id: "image",
      kind: "silhouette",
      target: "polar-bunny",
      camera: { position: [5, 3, 6], target: [0, 1, 0], fov: 45 },
      tick: 0,
      width: 128,
      height: 128,
      region: [0, 0, 1, 1],
      maxChangedFraction: 0,
    },
  ]);
  expect(result.results[0].status).toBe("unmeasured");
  expect(result.results[0].reason).toContain("Hardware browser");
});
