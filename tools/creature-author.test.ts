import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { referenceProject } from "@wrela/examples";
import { type CharacterDefinition, creatureSchema } from "@wrela/model";
import { runAuthoring } from "./author";
import { WorkspaceBridge } from "./bridge";

async function workspace(
  run: (
    directory: string,
    bridge: WorkspaceBridge,
    key: string,
    character: CharacterDefinition,
  ) => Promise<void>,
) {
  const directory = await mkdtemp(join(tmpdir(), "wrela-creature-author-"));
  try {
    const project = referenceProject(),
      character = project.documents.find((d) => d.kind === "character") as CharacterDefinition;
    character.creature = creatureSchema.parse({
      schemaVersion: 1,
      regions: [
        {
          id: "body-region",
          name: "Body",
          nodeIds: ["body"],
          frame: { position: [0, 0.9, 0], rotation: [0, 0, 0] },
          extent: [0.52, 0.68, 0.43],
        },
      ],
    });
    const bridge = new WorkspaceBridge(directory);
    await bridge.initialize();
    const saved = await bridge.save(project, null);
    await run(directory, bridge, saved.key, character);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}
test("creature CLI returns transferable bounded solve proposals and publishes only on reviewed apply", async () => {
  await workspace(async (directory, bridge, key) => {
    const inspect = await runAuthoring([directory, "creature-inspect", "polar-bunny", "body-region"]);
    expect(inspect).toMatchObject({
      workspaceKey: key,
      result: { target: "polar-bunny", regions: [{ id: "body-region" }] },
    });
    const request = join(directory, "solve.json");
    await Bun.write(
      request,
      JSON.stringify({
        workspaceKey: key,
        request: {
          id: "wider",
          target: "polar-bunny",
          expectedRevision: 0,
          controls: [{ kind: "regionScale", region: "body-region", axis: 0, minimum: 0.8, maximum: 1.5 }],
          objectives: [
            { kind: "regionExtent", region: "body-region", axis: 0, value: 0.65, tolerance: 0.00001 },
          ],
          budget: { evaluations: 20, iterations: 5 },
        },
      }),
    );
    const result = await runAuthoring([directory, "creature-solve", request]);
    expect(result).toMatchObject({
      published: false,
      workspaceKey: key,
      report: { status: "converged", ephemeral: true },
    });
    expect((await bridge.read())?.key).toBe(key);
    const proposal = join(directory, "proposal.json");
    await Bun.write(proposal, JSON.stringify(result));
    await runAuthoring([directory, "preview", proposal]);
    expect((await bridge.read())?.key).toBe(key);
    await runAuthoring([directory, "apply", proposal]);
    const changed = (await bridge.read())?.project.documents.find(
      (d) => d.id === "polar-bunny",
    ) as CharacterDefinition;
    expect(changed.creature?.regions[0].extent[0]).toBeCloseTo(0.65);
    await expect(runAuthoring([directory, "creature-solve", request])).rejects.toThrow("Workspace changed");
  });
});
test("creature CLI selection and dependency explanation ground source without editing it", async () => {
  await workspace(async (directory, bridge, key) => {
    const request = join(directory, "select.json");
    await Bun.write(
      request,
      JSON.stringify({ workspaceKey: key, target: "polar-bunny", point: [0, 0.9, 0] }),
    );
    expect(await runAuthoring([directory, "creature-select", request])).toMatchObject({
      result: { selections: [{ region: "body-region", exactSurface: false }] },
    });
    expect(await runAuthoring([directory, "creature-explain", "polar-bunny", "body-region"])).toMatchObject({
      result: { basis: "declared-source-dependencies", measuredSensitivity: false },
    });
    expect((await bridge.read())?.key).toBe(key);
  });
});
test("named pose motion authoring and retargeting produce ordinary editable proposals", async () => {
  await workspace(async (directory, bridge, key, character) => {
    const request = join(directory, "motion.json");
    await Bun.write(
      request,
      JSON.stringify({
        workspaceKey: key,
        target: character.id,
        clip: {
          id: "attention",
          name: "Attention",
          duration: 1,
          loop: false,
          poses: [
            {
              id: "ready",
              name: "Ready",
              joints: [{ joint: character.joints[0].id, rotation: [0, 0.1, 0], translation: [0, 0, 0] }],
            },
          ],
          keys: [
            { time: 0, pose: "ready" },
            { time: 1, pose: "ready" },
          ],
        },
      }),
    );
    const authored = await runAuthoring([directory, "creature-motion", request]);
    expect(authored).toMatchObject({
      published: false,
      batch: { operations: [{ kind: "document.set", path: ["motions"] }] },
      report: { motion: "attention", keys: 2 },
    });
    expect((await bridge.read())?.key).toBe(key);
    const motion = character.motions[0];
    await Bun.write(
      request,
      JSON.stringify({
        workspaceKey: key,
        source: character.id,
        target: character.id,
        retarget: {
          motion: motion.id,
          targetMotion: { id: "adapted", name: "Adapted" },
          translationScale: [1, 1, 1],
          mapping: character.joints.map((joint) => ({ source: joint.id, target: joint.id })),
        },
      }),
    );
    expect(await runAuthoring([directory, "creature-retarget", request])).toMatchObject({
      published: false,
      report: { sourceCharacter: character.id, sourceMotion: motion.id, targetCharacter: character.id },
    });
    expect((await bridge.read())?.key).toBe(key);
  });
});

test("async creature CLI waits for a real bounded job and returns a reviewable proposal without publication", async () => {
  await workspace(async (directory, bridge, key) => {
    const file = join(directory, "async-solve.json");
    await Bun.write(
      file,
      JSON.stringify({
        workspaceKey: key,
        job: {
          request: {
            id: "async-cli",
            target: "polar-bunny",
            expectedRevision: 0,
            controls: [{ kind: "regionScale", region: "body-region", axis: 0, minimum: 0.8, maximum: 1.5 }],
            objectives: [
              { kind: "regionExtent", region: "body-region", axis: 0, value: 0.65, tolerance: 0.00001 },
            ],
          },
          deadlineMs: 2000,
          evaluationsPerSlice: 1,
        },
      }),
    );
    const result = await runAuthoring([directory, "creature-solve-async", file]);
    expect(result).toMatchObject({
      published: false,
      workspaceKey: key,
      report: { job: { status: "completed", candidateId: "async-cli" } },
    });
    expect((await bridge.read())?.key).toBe(key);
  });
});
