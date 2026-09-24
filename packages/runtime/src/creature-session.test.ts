import { expect, test } from "bun:test";
import { compileCharacter } from "@wrela/compiler";
import { createCreatureFixture, referenceProject } from "@wrela/examples";
import type { CharacterDefinition, TerrainDefinition, WorldDefinition } from "@wrela/model";
import { WorldSession } from "@wrela/world";
import { RuntimeSession } from "./session";

function character(id: "ash-warden" | "reed-penitent") {
  const fixture = createCreatureFixture(id);
  const source = fixture.project.documents.find(
    (document) => document.id === fixture.characterId,
  ) as CharacterDefinition;
  return { source, artifact: compileCharacter(source, "interactive") };
}

test("compiled creature groom deforms across selected LODs and replays the same canonical guides", async () => {
  const { source, artifact } = character("ash-warden");
  const runtime = await RuntimeSession.create();
  try {
    runtime.physics.addGround();
    runtime.addCharacter("warden", artifact, source);
    for (let i = 0; i < 12; i++) runtime.advance(1 / 60);
    const saved = runtime.snapshotEntities();
    expect(JSON.parse(saved[0].state.groomState as string).guides.length).toBeGreaterThan(0);
    const hero = runtime.evaluatedCharacters()[0];
    expect(hero.deformation?.positionDeltas.length).toBe(hero.artifact.mesh.positions.length);
    expect(hero.deformation?.maxDisplacement).toBeGreaterThan(0);
    const farDetail = artifact.creatureDetails?.at(-1);
    expect(farDetail).toBeDefined();
    const far = runtime.evaluatedCharacters(
      undefined,
      undefined,
      (_artifact, _position, _scale, id, matrices) => {
        expect(id).toBe("warden");
        expect(matrices.length).toBe(artifact.joints.length * 16);
        return farDetail;
      },
    )[0];
    expect(far.artifact.mesh.positions.length).toBeLessThan(hero.artifact.mesh.positions.length);
    expect(far.deformation?.positionDeltas.length).toBe(far.artifact.mesh.positions.length);
    expect(far.deformation?.normalDeltas?.length).toBe(far.artifact.mesh.normals.length);
    expect(far.deformation?.positionDeltas.every(Number.isFinite)).toBe(true);
    const farAgain = runtime.evaluatedCharacters(undefined, undefined, () => farDetail)[0];
    expect(farAgain.artifact).toBe(far.artifact);
    expect(farAgain.deformation).toBe(far.deformation);
    expect(runtime.snapshotEntities()).toEqual(saved);
    runtime.seek(12);
    expect(runtime.snapshotEntities()).toEqual(saved);
    expect(runtime.evaluatedCharacters()[0].deformation?.positionDeltas).toEqual(
      hero.deformation?.positionDeltas,
    );
    await runtime.teleport("warden", [4, 1, 0]);
    expect(JSON.parse(runtime.snapshotEntities()[0].state.groomState as string).guides).toEqual([]);
  } finally {
    runtime.dispose();
  }
});

test("compiled cloth panels affect rendered vertices and persist through save and replay", async () => {
  const { source, artifact } = character("reed-penitent");
  const runtime = await RuntimeSession.create();
  try {
    runtime.physics.addGround();
    runtime.addCharacter("penitent", artifact, source);
    for (let i = 0; i < 12; i++) runtime.advance(1 / 60);
    const saved = runtime.snapshotEntities(),
      state = JSON.parse(saved[0].state.clothState as string);
    expect(state.panels.length).toBe(2);
    expect(state.revision).toBeGreaterThan(0);
    const evaluated = runtime.evaluatedCharacters()[0];
    expect(evaluated.deformation?.maxDisplacement).toBeGreaterThan(0);
    expect(evaluated.deformation?.positionDeltas.every(Number.isFinite)).toBe(true);
    for (let i = 0; i < 5; i++) runtime.evaluatedCharacters();
    expect(runtime.snapshotEntities()).toEqual(saved);
    runtime.seek(12);
    expect(runtime.snapshotEntities()).toEqual(saved);
    for (let i = 0; i < 4; i++) runtime.advance(1 / 60);
    runtime.restoreEntityStates(saved);
    expect(runtime.snapshotEntities()).toEqual(saved);
    const malformed = structuredClone(saved);
    malformed[0].state.clothState = '{"revision":1,"accumulator":0,"panels":[{}]}';
    expect(() => runtime.restoreEntityStates(malformed)).toThrow();
    expect(runtime.snapshotEntities()).toEqual(saved);
    await runtime.teleport("penitent", [2, 2, 0]);
    expect(JSON.parse(runtime.snapshotEntities()[0].state.clothState as string).panels).toEqual([]);
  } finally {
    runtime.dispose();
  }
});

test("known stage plane specializes secondary ground only, with bounds fallback and replay boundary", async () => {
  const { source, artifact } = character("ash-warden");
  const runtime = await RuntimeSession.create();
  try {
    runtime.physics.addGround();
    runtime.addCharacter("warden", artifact, source);
    const originalRaycast = runtime.physics.raycast.bind(runtime.physics);
    let calls = 0;
    runtime.physics.raycast = (...args) => {
      calls++;
      return originalRaycast(...args);
    };
    runtime.advance(1 / 60);
    const generalCalls = calls;
    expect(generalCalls).toBeGreaterThan(128);
    const plane = { height: 0, minX: -5, maxX: 5, minZ: -5, maxZ: 5 };
    runtime.setCreatureSecondaryGroundPlane(plane);
    expect(runtime.earliestSeekTick).toBe(runtime.clock.tick);
    expect(JSON.parse(runtime.snapshotEntities()[0].state.groomState as string).guides).toEqual([]);
    calls = 0;
    runtime.advance(1 / 60);
    expect(calls).toBeGreaterThan(0); // feet keep full support queries
    expect(calls).toBeLessThan(generalCalls / 4);
    expect(runtime.creatureGroundPolicy.contacts).toBe("physics-ray");
    expect(runtime.creatureGroundPolicy.secondary.mode).toBe("authored-plane");
    await runtime.teleport("warden", [20, 1, 0]);
    calls = 0;
    runtime.advance(1 / 60);
    expect(calls).toBeGreaterThan(128); // outside the declared domain uses the actual world
    expect(() => runtime.setCreatureSecondaryGroundPlane({ ...plane, height: Number.NaN })).toThrow();
    expect(runtime.creatureGroundPolicy.secondary.mode).toBe("authored-plane");
    runtime.setCreatureSecondaryGroundPlane();
    expect(runtime.creatureGroundPolicy.secondary.mode).toBe("general");
  } finally {
    runtime.dispose();
  }
});

test("world attachment invalidates standalone plane assumptions", async () => {
  const project = referenceProject();
  const definition = project.documents.find((document) => document.kind === "world") as WorldDefinition;
  const terrain = project.documents.find(
    (document) => document.id === definition.terrain,
  ) as TerrainDefinition;
  const world = new WorldSession(definition, terrain),
    runtime = await RuntimeSession.create();
  try {
    const plane = { height: 0, minX: -5, maxX: 5, minZ: -5, maxZ: 5 };
    runtime.setCreatureSecondaryGroundPlane(plane);
    runtime.attachWorld(world);
    expect(runtime.creatureGroundPolicy).toEqual({
      contacts: "resident-heightfield",
      secondary: { mode: "general" },
    });
    expect(() => runtime.setCreatureSecondaryGroundPlane(plane)).toThrow();
  } finally {
    runtime.dispose();
    world.dispose();
  }
});

test("ribbon groom realizes the same bounded dynamics and selected detail offsets", async () => {
  const source = character("ash-warden").source;
  for (const groom of source.creature?.grooms ?? []) groom.representation = "ribbons";
  const artifact = compileCharacter(source, "interactive"),
    runtime = await RuntimeSession.create();
  try {
    runtime.physics.addGround();
    runtime.setCreatureSecondaryGroundPlane({ height: 0, minX: -5, maxX: 5, minZ: -5, maxZ: 5 });
    runtime.addCharacter("ribbon-warden", artifact, source);
    for (let i = 0; i < 3; i++) runtime.advance(1 / 60);
    const near = runtime.evaluatedCharacters()[0];
    const far = runtime.evaluatedCharacters(undefined, undefined, () => artifact.creatureDetails?.at(-1))[0];
    expect(artifact.creatureGroom?.representation).toBe("opaque-ribbons");
    expect(near.deformation?.positionDeltas.length).toBe(near.artifact.mesh.positions.length);
    expect(far.deformation?.positionDeltas.length).toBe(far.artifact.mesh.positions.length);
    expect(far.deformation?.positionDeltas.every(Number.isFinite)).toBe(true);
    expect(far.deformation?.normalDeltas?.every(Number.isFinite)).toBe(true);
  } finally {
    runtime.dispose();
  }
});
