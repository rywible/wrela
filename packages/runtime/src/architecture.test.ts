import { expect, test } from "bun:test";
import { compileCharacter } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import type { CharacterDefinition, Motion, Vec3 } from "@wrela/model";
import { compileMotionTracks, sampleMotion } from "./animation";
import { planEntityResidency } from "./lifecycle";
import { PhysicsAdapter } from "./physics";
import { artifactBytes, BrowserSceneHost, gridMesh } from "./scene-host";
import { RuntimeSession } from "./session";

function required<T>(value: T | undefined): T {
  if (value === undefined) throw new Error("Missing test fixture");
  return value;
}

const character = () =>
  referenceProject().documents.find((document) => document.kind === "character") as CharacterDefinition;
test("continuous gameplay input rolls bounded replay baselines and preserves replay/branch behavior", async () => {
  const definition = character(),
    runtime = await RuntimeSession.create();
  runtime.addCharacter("actor", compileCharacter(definition, "interactive"), definition);
  try {
    for (let tick = 0; tick < 5000; tick++) {
      runtime.setTarget("actor", [tick / 1000, 1, 0]);
      runtime.advance(1 / 60);
    }
    expect(runtime.clock.tick).toBe(5000);
    expect(runtime.replayUsage.commands).toBeLessThanOrEqual(4096);
    expect(runtime.earliestSeekTick).toBeGreaterThan(0);
    const end = runtime.bodyState("actor");
    runtime.seek(4900);
    runtime.seek(5000);
    expect(runtime.bodyState("actor")).toEqual(end);
    runtime.seek(4900);
    runtime.setTarget("actor", [12, 1, 0]);
    runtime.advance(1 / 60);
    expect(runtime.bodyState("actor").position[0]).toBeCloseTo(12, 5);
    runtime.seek(5000);
    expect(runtime.bodyState("actor").position[0]).toBeCloseTo(12, 5);
  } finally {
    runtime.dispose();
  }
});

test("visual residency does not pause physical time and snapshot extraction is simulation read-only", async () => {
  const host = new BrowserSceneHost(referenceProject());
  await host.prepare("winter-valley");
  try {
    const camera = { position: [80, 20, 7] as Vec3, target: [0, 1, 0] as Vec3, fov: 42 };
    host.updateView(camera);
    expect(host.world?.metrics.pending).toBe(true);
    const before = required(host.runtime).clock.tick;
    expect(host.advance(1 / 60)).toBe(1);
    expect(required(host.runtime).clock.tick).toBe(before + 1);
    const interests = required(host.world).interestSources();
    const scene = host.extract(camera);
    expect(scene.time).toBe(required(host.runtime).clock.time);
    expect(scene.origin).toEqual([0, 0, 0]);
    host.extract({ ...camera, position: [400, 20, 7] });
    expect(required(host.runtime).clock.tick).toBe(before + 1);
    expect(required(host.world).interestSources()).toEqual(interests);
    await required(host.world).prepare();
  } finally {
    host.dispose();
  }
});

test("distant entities stay dormant without blocking play, survive saves, and activate on approach", async () => {
  const project = referenceProject(),
    world = project.documents.find((document) => document.kind === "world");
  if (!world || world.kind !== "world") throw new Error("Missing fixture world");
  const source = required(world.instances.find((instance) => instance.definition === "polar-bunny"));
  world.instances.push({ ...source, id: "distant-bunny", position: [200, 0, 0] });
  const host = new BrowserSceneHost(project),
    restored = new BrowserSceneHost(project);
  await host.prepare(world.id);
  try {
    expect(host.advance(1 / 60)).toBe(1);
    expect(required(host.runtime).entityLifecycle).toContainEqual({ id: "distant-bunny", state: "dormant" });
    const saved = host.saveRuntime();
    await restored.prepare(world.id);
    await restored.loadRuntime(saved);
    expect(required(restored.runtime).entityLifecycle).toContainEqual({
      id: "distant-bunny",
      state: "dormant",
    });
    expect(required(restored.runtime).bodyState("distant-bunny").position[0]).toBe(200);
    await required(host.runtime).teleport(source.id, [200, 2, 0]);
    host.advance(1 / 60);
    expect(required(host.runtime).entityLifecycle).toContainEqual({ id: "distant-bunny", state: "active" });
    await required(host.world).prepare();
  } finally {
    host.dispose();
    restored.dispose();
  }
});

test("actor leases share regions and never exceed physical planning capacity", () => {
  const entities = Array.from({ length: 100 }, (_, index) => ({
    id: String(index),
    position: [index % 10, 0, Math.floor(index / 10)] as Vec3,
    dormant: false,
    removed: false,
  }));
  const plan = planEntityResidency(entities, "0", 6);
  expect(plan.wanted.size).toBe(100);
  expect(plan.regions.size).toBe(1);
  const separated = entities.map((entity, index) => ({ ...entity, position: [index * 30, 0, 0] as Vec3 }));
  expect(planEntityResidency(separated, "0", 2).regions.size).toBeLessThanOrEqual(2);
});

test("compiled tracks reuse indexing and root displacement accumulates across loops", () => {
  const definition = character(),
    root = required(definition.joints.find((joint) => !joint.parent));
  const motion: Motion = {
    id: "walk",
    name: "Walk",
    duration: 2,
    loop: true,
    keys: [
      { joint: root.id, time: 2, translation: [2, 0, 0], rotation: [0, 0, 0] },
      { joint: root.id, time: 0, translation: [0, 0, 0], rotation: [0, 0, 0] },
    ],
  };
  expect(compileMotionTracks(motion)).toBe(compileMotionTracks(motion));
  expect(sampleMotion([root], motion, 4.5, true).get(root.id)?.translation[0]).toBeCloseTo(4.5);
  expect(sampleMotion([root], motion, 4.5).get(root.id)?.translation[0]).toBeCloseTo(0.5);
});

test("collision realizations preserve a doorway and expose queries/contact events through snapshots", async () => {
  const physics = await PhysicsAdapter.create();
  try {
    physics.addStaticCompound(
      "door",
      [-1.5, 1.5].map((x, index) => ({
        id: `pillar-${index}`,
        shape: "box" as const,
        position: [x, 1, 0] as Vec3,
        rotation: [0, 0, 0] as Vec3,
        size: [0.3, 1, 0.3] as Vec3,
      })),
      [0, 0, 0],
    );
    physics.addStaticMesh("floor", gridMesh(20), [0, 0, 0]);
    physics.add({
      id: "ball",
      mode: "dynamic",
      position: [4, 2, 0],
      radius: 0.2,
      halfHeight: 0,
      mass: 1,
      restitution: 0,
      friction: 0.5,
    });
    physics.step(1 / 60);
    expect(physics.raycast([0, 1, 3], [0, 0, -1], 6)).toBeNull();
    expect(physics.raycast([1.5, 1, 3], [0, 0, -1], 6)?.id).toBe("door");
    expect(physics.raycast([0, 2, 0], [0, -1, 0], 4)?.id).toBe("floor");
    expect(physics.overlapSphere([1.5, 1, 0], 0.5)).toEqual(["door"]);
    for (let tick = 0; tick < 120; tick++) physics.step(1 / 60);
    expect(
      physics
        .drainContactEvents()
        .some(
          (event) =>
            event.started && [event.a, event.b].includes("ball") && [event.a, event.b].includes("floor"),
        ),
    ).toBe(true);
    const checkpoint = physics.checkpoint();
    physics.updateStaticObject("door", true, [10, 0, 0], [0, 0, 0, 1]);
    physics.restore(checkpoint);
    physics.step(1 / 60);
    expect(physics.raycast([1.5, 1, 3], [0, 0, -1], 6)?.id).toBe("door");
    physics.setMode("ball", "kinematic");
    physics.teleport("ball", [1.5, 1, 3]);
    physics.step(1 / 60);
    physics.target("ball", [1.5, 1, -3], [0, 0, 0, 1], 1 / 60);
    physics.step(1 / 60);
    expect(physics.state("ball").position[2]).toBeGreaterThan(0.49);
  } finally {
    physics.dispose();
  }
});

test("material groups retain one geometry resource and select their own triangle ranges", async () => {
  const host = new BrowserSceneHost(referenceProject());
  await host.prepare("polar-bunny");
  try {
    const surfaces = host
      .extract({ position: [5, 3, 7], target: [0, 1, 0], fov: 42 })
      .surfaces.filter((surface) => surface.source === "polar-bunny");
    expect(surfaces.length).toBeGreaterThan(1);
    expect(new Set(surfaces.map((surface) => surface.mesh)).size).toBe(1);
    expect(surfaces.every((surface) => !!surface.drawRange)).toBe(true);
  } finally {
    host.dispose();
  }
});

test("authored save migration remaps only exact known artifact/motion versions and preserves originals", async () => {
  const project = referenceProject(),
    first = new BrowserSceneHost(project);
  const changed = structuredClone(project),
    definition = changed.documents.find((document) => document.kind === "character");
  if (!definition || definition.kind !== "character") throw new Error("Missing fixture character");
  const oldMotion = definition.motions[0].id;
  definition.motions[0].id = "renamed-idle";
  const second = new BrowserSceneHost(changed);
  await first.prepare("winter-valley");
  await second.prepare("winter-valley");
  try {
    first.advance(1 / 60);
    const save = first.saveRuntime(),
      original = structuredClone(save);
    const source = required(save.dormant.find((entity) => entity.id === "bunny-instance"));
    const target = required(second.saveRuntime().dormant.find((entity) => entity.id === source.id));
    const before = required(second.runtime).bodyState(source.id);
    await expect(second.loadRuntime(save)).rejects.toThrow("incompatible");
    const migration = {
      id: "winter-release-2",
      characters: [
        {
          definition: definition.id,
          fromArtifactKey: String(source.state.artifactKey),
          toArtifactKey: String(target.state.artifactKey),
          motionIds: { [oldMotion]: "renamed-idle" },
        },
      ],
    };
    const prepared = second.prepareRuntimeSave(save, migration);
    expect(prepared.migration?.changedEntities).toEqual([source.id]);
    expect(required(second.runtime).bodyState(source.id)).toEqual(before);
    expect(() =>
      second.prepareRuntimeSave(save, {
        ...migration,
        characters: [{ ...migration.characters[0], toArtifactKey: "unknown" }],
      }),
    ).toThrow("incompatible");
    const restored = await second.loadRuntime(save, { migration });
    expect(restored.time).toBe(1 / 60);
    expect(required(second.runtime).bodyState(source.id)).toEqual(
      required(first.runtime).bodyState(source.id),
    );
    expect(second.saveRuntime().dormant.find((entity) => entity.id === source.id)?.state.motion).toBe(
      "renamed-idle",
    );
    expect(save).toEqual(original);
  } finally {
    first.dispose();
    second.dispose();
  }
});

test("motion markers cross tick and loop boundaries once and replay suppresses transient effects", async () => {
  const definition = character(),
    root = required(definition.joints.find((joint) => !joint.parent));
  definition.motions = [
    {
      id: "stride",
      name: "Stride",
      duration: 0.2,
      loop: true,
      keys: [{ joint: root.id, time: 0, translation: [0, 0, 0], rotation: [0, 0, 0] }],
    },
  ];
  const runtime = await RuntimeSession.create();
  runtime.addCharacter("actor", compileCharacter(definition, "interactive"), definition);
  try {
    const markers = [
      { id: "foot", time: 0.05, payload: { surface: "snow" } },
      { id: "loop", time: 0 },
    ];
    runtime.registerMotionEvents(definition.id, "stride", markers);
    markers[0].time = 0.1;
    for (let tick = 0; tick < 30; tick++) runtime.advance(1 / 60);
    const delivered = runtime.drainAnimationEvents();
    expect(delivered.dropped).toBe(0);
    expect(delivered.events.filter((event) => event.eventId === "foot").map((event) => event.tick)).toEqual([
      3, 15, 27,
    ]);
    expect(delivered.events.filter((event) => event.eventId === "loop").map((event) => event.cycle)).toEqual([
      0, 1, 2,
    ]);
    runtime.seek(0);
    await runtime.seekAsync(30);
    expect(runtime.drainAnimationEvents()).toEqual({ events: [], dropped: 0 });
    runtime.seek(12);
    for (let tick = 0; tick < 3; tick++) runtime.advance(1 / 60);
    expect(runtime.drainAnimationEvents().events.map((event) => event.eventId)).toEqual(["foot"]);
    runtime.registerMotionEvents(
      definition.id,
      "stride",
      Array.from({ length: 64 }, (_, index) => ({ id: `marker-${index}`, time: 0 })),
    );
    for (let tick = 0; tick < 240; tick++) runtime.advance(1 / 60);
    const overflow = runtime.drainAnimationEvents();
    expect(overflow.events.length).toBe(1024);
    expect(overflow.dropped).toBeGreaterThan(0);
    expect(runtime.drainAnimationEvents()).toEqual({ events: [], dropped: 0 });
  } finally {
    runtime.dispose();
  }
});

test("artifact live accounting deduplicates installed/cache ownership and shared metadata", async () => {
  const host = new BrowserSceneHost(referenceProject());
  await host.prepare("polar-bunny");
  try {
    expect(host.resourceUsage.liveBytes).toBe(host.resourceUsage.installedBytes);
    expect(host.resourceUsage.liveBytes).toBe(host.resourceUsage.cacheBytes);
    const mesh = gridMesh(2),
      distant = gridMesh(2, 2);
    const artifact = {
      kind: "surface" as const,
      id: "surface",
      key: "shared",
      mesh,
      material: "snow",
      diagnostics: [],
      details: [{ label: "distant", mesh: distant, maxProjectedDiameter: 64, maxError: null }],
    };
    const bytes = (geometry: typeof mesh) =>
      geometry.positions.byteLength + geometry.normals.byteLength + geometry.indices.byteLength;
    expect(artifactBytes([artifact, artifact, { ...artifact }])).toBe(bytes(mesh) + bytes(distant));
  } finally {
    host.dispose();
  }
});

test("controller speed is independent of visual root animation and its policy survives replay/save", async () => {
  const definition = character(),
    artifact = compileCharacter(definition, "interactive");
  const runtime = await RuntimeSession.create(),
    restored = await RuntimeSession.create();
  runtime.addCharacter("actor", artifact, definition);
  restored.addCharacter("actor", artifact, definition);
  try {
    runtime.setRootMotionPolicy("actor", "visual");
    runtime.playMotion("actor", "hop", 0.18);
    runtime.setFacing("actor", Math.PI);
    for (let tick = 0; tick < 60; tick++) {
      const position = runtime.bodyState("actor").position;
      runtime.setTarget("actor", [position[0], position[1], position[2] - 4.3 / 60]);
      runtime.advance(1 / 60);
    }
    const body = runtime.bodyState("actor");
    expect(body.position[2]).toBeCloseTo(-4.3, 4);
    const rootIndex = artifact.joints.findIndex((joint) => !joint.parent);
    const rendered = required(runtime.evaluatedCharacters()[0]);
    expect(Math.abs(rendered.skinMatrices[rootIndex * 16 + 14])).toBeGreaterThan(0.05);
    const save = runtime.snapshotEntities();
    expect(save[0].state.rootMotionPolicy).toBe("visual");
    runtime.seek(0);
    runtime.seek(60);
    expect(runtime.bodyState("actor")).toEqual(body);
    restored.restoreEntityStates(save);
    const position = restored.bodyState("actor").position;
    restored.setTarget("actor", [position[0], position[1], position[2] - 4.3 / 60]);
    restored.advance(1 / 60);
    expect(restored.bodyState("actor").position[2]).toBeCloseTo(-4.3 - 4.3 / 60, 4);
  } finally {
    runtime.dispose();
    restored.dispose();
  }
});
