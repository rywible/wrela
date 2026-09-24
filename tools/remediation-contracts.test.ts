import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  AuthoringSession,
  attachWorkReview,
  authoringError,
  createWorkSession,
  materializeWorkProposal,
  proposeWork,
  publishWork,
  sessionWorkPublisher,
} from "@wrela/authoring";
import {
  artifactTransfers,
  compileDocument,
  cookProject,
  deserializeArtifact,
  serializeArtifact,
} from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import {
  contentKey,
  loadWorkingSet,
  ownedBuffers,
  ownedBufferViews,
  parseProject,
  searchDocuments,
} from "@wrela/model";
import { reviewAuthoring } from "@wrela/review";
import { GAME_STEP, GameDriver } from "@wrela/runtime";
import { switchGateGame, switchGateProject } from "../games/switch-gate/src/module";
import { runAuthoring } from "./author";
import { WorkFileStore } from "./authoring-work-store";
import { WorkspaceBridge } from "./bridge";

test("unknown request and imported source properties are actionable failures, never accepted no-ops", () => {
  const source = referenceProject(),
    session = new AuthoringSession(source),
    before = contentKey(source);
  expect(
    session.discover({ target: "snow-fur", limit: 32 }).operations.some((o) => o.kind === "material.assign"),
  ).toBe(false);
  for (const call of [
    () =>
      session.apply({
        expectedRevision: 0,
        operations: [{ kind: "field.update", target: "polar-bunny", node: "body", changes: { raduis: 2 } }],
      } as never),
    () =>
      session.proposeCandidate({
        id: "typo",
        batch: {
          expectedRevision: 0,
          operations: [{ kind: "field.update", target: "polar-bunny", node: "body", changes: { raduis: 2 } }],
        },
      } as never),
  ]) {
    try {
      call();
      throw Error("Unexpected success");
    } catch (error) {
      const envelope = authoringError(error);
      expect(envelope.error.code).toBe("authoring.invalid-request");
      expect(envelope.error.path).toBeDefined();
      expect(envelope.error.nextAction).toBeDefined();
    }
  }
  const malformed = structuredClone(source) as unknown as { documents: Record<string, unknown>[] };
  malformed.documents[0].roughnes = 0.7;
  expect(() => parseProject(malformed)).toThrow("roughnes");
  expect(contentKey(session.getSnapshot().project)).toBe(before);
});
test("a thousand-definition catalog supports bounded reads, independent edits, and unchanged release payloads", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-catalog-"));
  try {
    const source = referenceProject(),
      material = source.documents.find((d) => d.kind === "material")!;
    for (let i = source.documents.length; i < 1000; i++)
      source.documents.push({ ...material, id: `library-${i}`, name: `Library material ${i}` });
    // The world itself exceeds a task budget; an isolated library edit must still work.
    source.documents.find((d) => d.id === source.entry)!.dependencies.push("library-300");
    for (let i = 300; i < 600; i++)
      source.documents.find((d) => d.id === `library-${i}`)!.dependencies = [`library-${i + 1}`];
    const bridge = new WorkspaceBridge(directory);
    await bridge.initialize();
    const initial = await bridge.save(source, null);
    const catalog = await bridge.catalog(),
      page = searchDocuments(catalog, { search: "library material", limit: 12 });
    expect(catalog.index).toHaveLength(1000);
    expect(await runAuthoring([directory, "discover"])).toMatchObject({ workspaceKey: initial.key });
    await expect(loadWorkingSet(catalog, [source.entry])).rejects.toThrow("working set budget");
    expect(page.entries).toHaveLength(12);
    let reads = 0;
    const task = await loadWorkingSet(
      {
        ...catalog,
        async read(id) {
          reads++;
          return catalog.read(id);
        },
      },
      ["library-999"],
    );
    expect(reads).toBe(1);
    expect(task.documents).toHaveLength(1);
    const release = cookProject(source, "interactive");
    const request = join(directory, "edit.json");
    await Bun.write(
      request,
      JSON.stringify({
        workspaceKey: initial.key,
        batch: {
          expectedRevision: 0,
          transactionId: "library-edit",
          operations: [{ kind: "document.rename", target: "library-999", name: "Unused library edit" }],
        },
      }),
    );
    const receipt = await runAuthoring([directory, "apply", request]);
    expect(receipt).toMatchObject({ published: true, transactionId: "library-edit" });
    expect(cookProject((await bridge.read())!.project, "interactive")).toEqual(release);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
test("owned buffers survive JSON cooking and share the transfer/accounting inventory", () => {
  const source = referenceProject().documents.find((d) => d.kind === "object")!,
    artifact = compileDocument(source, "interactive")!;
  if (artifact.kind !== "surface") throw Error("Expected surface");
  artifact.mesh.indirectProofs = new Float32Array((artifact.mesh.positions.length / 3) * 4);
  const restored = deserializeArtifact(JSON.parse(JSON.stringify(serializeArtifact(artifact))));
  const layout = (value: unknown) =>
    ownedBufferViews(value)
      .map(({ path, view }) => ({
        path: path.join("."),
        type: view.constructor.name,
        bytes: view.byteLength,
      }))
      .sort((a, b) => a.path.localeCompare(b.path));
  expect(layout(restored)).toEqual(layout(artifact));
  expect(artifactTransfers(artifact)).toEqual(ownedBuffers(artifact));
});
test("discover → inspect → propose → review → adopt → reopen → play → cook uses public services", async () => {
  const directory = await mkdtemp(join(tmpdir(), "wrela-agent-game-"));
  try {
    const project = switchGateProject(),
      session = new AuthoringSession(project),
      store = new WorkFileStore(directory);
    expect(session.discover({ target: "switch" }).operations.length).toBeGreaterThan(0);
    expect(session.inspectFields({ target: "switch", path: ["name"] }).value).toBe("Brass switch");
    let work = proposeWork(
      createWorkSession(project, {
        id: "gate-work",
        brief: "Give the switch a clearer name",
        constraints: [{ id: "shape", kind: "source", target: "switch", path: ["field"] }],
      }),
      "clear-switch",
      [{ kind: "document.rename", target: "switch", name: "Open the gate" }],
    );
    work = attachWorkReview(
      work,
      "clear-switch",
      await reviewAuthoring(work.baseline, materializeWorkProposal(work, "clear-switch"), work.constraints),
    );
    const key = await store.put(work, null);
    await publishWork(
      store,
      sessionWorkPublisher(() => session),
      { id: work.id, expectedKey: key, proposal: "clear-switch" },
    );
    const bridge = new WorkspaceBridge(directory);
    await bridge.initialize();
    await bridge.save(session.getSnapshot().project, null);
    const reopened = (await bridge.read())!.project,
      driver = await GameDriver.create(switchGateGame, { project: reopened });
    try {
      driver.input({ move: [1, 0], interact: true });
      for (let i = 0; i < 44; i++) await driver.advance(GAME_STEP);
      const save = await driver.save(),
        fresh = await GameDriver.create(switchGateGame, { project: reopened });
      try {
        await fresh.load(save);
        expect(fresh.inspect()).toEqual(driver.inspect());
        fresh.input({ move: [1, 0] });
        for (let i = 0; i < 150; i++) await fresh.advance(GAME_STEP);
        expect(fresh.inspect().state).toMatchObject({ won: true, activated: true, gate: 1 });
      } finally {
        fresh.dispose();
      }
      const cooked = cookProject(reopened, "interactive");
      expect(cooked.entries).toHaveLength(3);
      for (const doc of reopened.documents.filter((d) => d.kind === "object")) {
        const artifact = compileDocument(doc, "interactive");
        expect(artifact && artifact.kind === "surface" && artifact.mesh.indices.length > 0).toBe(true);
      }
    } finally {
      driver.dispose();
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
