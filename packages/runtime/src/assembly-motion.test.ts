import { expect, test } from "bun:test";
import { compileAssemblyMesh } from "@wrela/compiler";
import { assemblySchema, identityMatrix } from "@wrela/model";

import { AssemblyMotion } from "./assembly-motion";
import { PhysicsAdapter } from "./physics";
import { RuntimeSession } from "./session";

const source = () =>
  assemblySchema.parse({
    grid: 0.1,
    clearances: [],
    parts: [
      {
        id: "door",
        name: "Door",
        profile: { kind: "rectangle", width: 1, height: 2 },
        path: [
          [0, 0, 0],
          [0, 0, 0.1],
        ],
        bevel: 0,
        position: [1, 1, 0],
        rotation: [0, 0, 0],
        repeat: { count: 1, offset: [0, 0, 0] },
        sockets: [],
        wear: { amount: 0, scale: 1, seed: 1 },
        joint: {
          kind: "slider",
          axis: [1, 0, 0],
          pivot: [0, 0, 0],
          minimum: 0,
          maximum: 2,
          value: 0,
          drive: { period: 4, phase: 0 },
        },
      },
    ],
  });
test("machinery moves immutable part geometry with absolute-time transforms and commands", () => {
  const assembly = source(),
    mesh = compileAssemblyMesh(assembly, "metal").mesh,
    motion = new AssemblyMotion(assembly, mesh);
  const initial = motion.evaluate(0, identityMatrix())[0],
    open = motion.evaluate(2, identityMatrix())[0];
  expect(open.mesh).toBe(initial.mesh);
  expect(initial.dynamic).toBe(true);
  expect(initial.matrix[12]).toBe(1);
  expect(open.matrix[12]).toBe(3);
  motion.setJoint("door", 10);
  expect(motion.evaluate(0, identityMatrix())[0].matrix[12]).toBe(3);
  motion.setJoint("door", null);
  expect(motion.evaluate(0, identityMatrix())[0].matrix[12]).toBe(1);
  expect(new Set(open.mesh.sourceIds)).toEqual(new Set(["door"]));
});
test("kinematic collision follows driven assembly transforms", async () => {
  const assembly = source(),
    motion = new AssemblyMotion(assembly, compileAssemblyMesh(assembly, "metal").mesh),
    physics = await PhysicsAdapter.create([0, 0, 0]);
  try {
    motion.installCollision(physics, "machine", identityMatrix());
    expect(physics.state("machine/assembly/door").position[0]).toBe(1);
    motion.syncCollision(physics, "machine", identityMatrix(), 2);
    physics.step(1 / 60);
    expect(physics.state("machine/assembly/door").position[0]).toBeCloseTo(3);
    motion.removeCollision(physics, "machine");
  } finally {
    physics.dispose();
  }
});
test("decorative assembly hardware is visible without allocating physics bodies", async () => {
  const assembly = source();
  assembly.parts.push({
    ...structuredClone(assembly.parts[0]),
    id: "rivet",
    name: "Decorative rivet",
    joint: undefined,
    collision: false,
  });
  const motion = new AssemblyMotion(assembly, compileAssemblyMesh(assembly, "metal").mesh),
    physics = await PhysicsAdapter.create([0, 0, 0]);
  try {
    expect(motion.evaluate(0, identityMatrix()).map((part) => part.id)).toEqual(["door", "rivet"]);
    motion.installCollision(physics, "machine", identityMatrix());
    expect(physics.state("machine/assembly/door").position[0]).toBe(1);
    expect(() => physics.state("machine/assembly/rivet")).toThrow("Unknown physics body");
    motion.syncCollision(physics, "machine", identityMatrix(), 2);
    physics.step(1 / 60);
    expect(physics.state("machine/assembly/door").position[0]).toBeCloseTo(3);
    motion.removeCollision(physics, "machine");
  } finally {
    physics.dispose();
  }
});

test("scene-host playback, collision and replay share the same machinery clock", async () => {
  const { referenceProject } = await import("@wrela/examples");
  const { BrowserSceneHost } = await import("./scene-host");
  const project = referenceProject(),
    object = project.documents.find((d) => d.id === "river-stone");
  if (object?.kind !== "object") throw new Error("Missing fixture object");
  object.assembly = source();
  object.collision = "box";
  const host = new BrowserSceneHost(project),
    camera = {
      position: [5, 3, 7] as [number, number, number],
      target: [0, 1, 0] as [number, number, number],
      fov: 42,
    };
  try {
    await host.prepare(object.id);
    const before = host.extract(camera).surfaces.find((s) => s.id === "river-stone/assembly/door");
    expect(before?.matrix[12]).toBe(1);
    for (let tick = 0; tick < 15; tick++) host.advance(1 / 60);
    const after = host.extract(camera).surfaces.find((s) => s.id === "river-stone/assembly/door");
    const body = host.runtime?.physics.state("river-stone/assembly/door");
    expect(after?.matrix[12]).toBeCloseTo(1 + 1 - Math.cos(Math.PI / 8));
    expect(after?.matrix[12]).toBeCloseTo(body?.position[0] ?? -1);
    expect(after?.mesh).toBe(before?.mesh);
    host.runtime?.seek(0);
    expect(host.extract(camera).surfaces.find((s) => s.id === "river-stone/assembly/door")?.matrix[12]).toBe(
      1,
    );
    host.runtime?.seek(15);
    expect(host.runtime?.physics.state("river-stone/assembly/door")).toEqual(body);
  } finally {
    host.dispose();
  }
});

test("fixed socket mates follow driven targets with immutable local meshes", () => {
  const assembly = source(),
    target = assembly.parts[0];
  target.sockets = [
    {
      id: "tip",
      position: [0, 0, 0],
      rotation: [0, 0, 0.4],
      anchor: { point: "end", profileOffset: [1, 0] },
    },
  ];
  assembly.parts.push({
    ...structuredClone(target),
    id: "handle",
    name: "Handle",
    joint: undefined,
    sockets: [{ id: "root", position: [0, 0, 0], rotation: [0.2, 0, 0] }],
    mate: { part: target.id, socket: "tip", ownSocket: "root" },
  });
  const motion = new AssemblyMotion(assembly, compileAssemblyMesh(assembly, "metal").mesh);
  const first = motion.evaluate(0, identityMatrix()),
    second = motion.evaluate(2, identityMatrix());
  expect(second[1].matrix[12] - first[1].matrix[12]).toBeCloseTo(2);
  expect(second[1].mesh).toBe(first[1].mesh);
  expect(motion.byteLength).toBeGreaterThan(
    second.reduce(
      (n, p) => n + p.mesh.positions.byteLength + p.mesh.normals.byteLength + p.mesh.indices.byteLength,
      0,
    ),
  );
});

test("linked machinery moves render and collision parts together through source overrides", async () => {
  const assembly = source();
  const driven = structuredClone(assembly.parts[0]);
  driven.id = "counterweight";
  driven.position = [0, 4, 0];
  if (!driven.joint) throw new Error("Missing joint fixture");
  driven.joint = {
    ...driven.joint,
    axis: [0, 1, 0],
    minimum: -2,
    maximum: 0,
    drive: undefined,
    link: { part: "door", ratio: -1, offset: 0 },
  };
  assembly.parts.unshift(driven);
  const motion = new AssemblyMotion(assembly, compileAssemblyMesh(assembly, "metal").mesh);
  const physics = await PhysicsAdapter.create([0, 0, 0]);
  try {
    motion.installCollision(physics, "machine", identityMatrix());
    motion.setJoint("door", 1.5);
    motion.syncCollision(physics, "machine", identityMatrix(), 0);
    physics.step(1 / 60);
    const rendered = motion.evaluate(0, identityMatrix()).find((part) => part.id === "counterweight");
    expect(rendered?.matrix[13]).toBeCloseTo(2.5);
    expect(physics.state("machine/assembly/counterweight").position[1]).toBeCloseTo(2.5);
    motion.setJoint("door", null);
    expect(motion.evaluate(2, identityMatrix())[0].matrix[13]).toBeCloseTo(2);
  } finally {
    physics.dispose();
  }
});

test("runtime machinery commands coalesce, seek, branch and restore collision at the same tick", async () => {
  const assembly = source(),
    runtime = await RuntimeSession.create();
  runtime.addAssembly(
    "machine",
    assembly,
    compileAssemblyMesh(assembly, "metal").mesh,
    identityMatrix(),
    true,
  );
  const rendered = () => runtime.assemblyParts("machine", identityMatrix())?.[0].matrix[12];
  try {
    runtime.setAssemblyJoint("machine", "door", 0.5);
    runtime.setAssemblyJoint("machine", "door", 1.5);
    expect(runtime.replayUsage.commands).toBe(1);
    expect(rendered()).toBe(1);
    runtime.advance(1 / 60);
    expect(rendered()).toBe(2.5);
    expect(runtime.physics.state("machine/assembly/door").position[0]).toBe(2.5);
    runtime.seek(0);
    expect(rendered()).toBe(1);
    runtime.seek(1);
    expect(rendered()).toBe(2.5);
    runtime.seek(0);
    runtime.setAssemblyJoint("machine", "door", 0.25);
    runtime.advance(1 / 60);
    expect(rendered()).toBe(1.25);
    runtime.seek(0);
    runtime.seek(1);
    expect(rendered()).toBe(1.25);
    runtime.setAssemblyJoint("machine", "door", null);
    runtime.advance(1 / 60);
    expect(rendered()).toBeCloseTo(2 - Math.cos(Math.PI / 60));
    expect(() => runtime.setAssemblyJoint("machine", "missing", 0)).toThrow("Unknown assembly joint");
    expect(() => runtime.setAssemblyJoint("machine", "door", Number.NaN)).toThrow("finite");
  } finally {
    runtime.dispose();
  }
});

test("assembly save states restore fixed-tick overrides and reject incompatible state atomically", async () => {
  const assembly = source(),
    runtime = await RuntimeSession.create(),
    restored = await RuntimeSession.create();
  const mesh = compileAssemblyMesh(assembly, "metal").mesh;
  for (const session of [runtime, restored])
    session.addAssembly("machine", assembly, mesh, identityMatrix(), true);
  try {
    runtime.setAssemblyJoint("machine", "door", 1.25);
    runtime.advance(1 / 60);
    const saved = runtime.snapshotEntities();
    expect(saved).toHaveLength(1);
    expect(saved[0].state.runtime).toBe("wrela-assembly-1");
    restored.restoreEntityStates(saved);
    expect(restored.clock.tick).toBe(1);
    expect(restored.assemblyParts("machine", identityMatrix())?.[0].matrix[12]).toBe(2.25);
    expect(restored.physics.state("machine/assembly/door").position[0]).toBe(2.25);
    restored.seek(1);
    expect(restored.assemblyParts("machine", identityMatrix())?.[0].matrix[12]).toBe(2.25);
    const invalid = structuredClone(saved);
    invalid[0].state.joints = "[100]";
    expect(() => restored.restoreEntityStates(invalid)).toThrow("Invalid saved assembly joint");
    expect(restored.snapshotEntities()).toEqual(saved);
    invalid[0].state.sourceKey = "changed-source";
    expect(() => restored.restoreEntityStates(invalid)).toThrow("incompatible");
    expect(restored.snapshotEntities()).toEqual(saved);
  } finally {
    runtime.dispose();
    restored.dispose();
  }
});

test("world host save/load retains operated assembly joints", async () => {
  const { referenceProject } = await import("@wrela/examples"),
    { BrowserSceneHost } = await import("./scene-host");
  const project = referenceProject(),
    object = project.documents.find((document) => document.id === "river-stone");
  if (object?.kind !== "object") throw new Error("Missing fixture object");
  object.assembly = source();
  const original = new BrowserSceneHost(project),
    restored = new BrowserSceneHost(project);
  try {
    await original.prepare("winter-valley");
    await restored.prepare("winter-valley");
    original.runtime?.setAssemblyJoint("stone-instance", "door", 1.7);
    original.advance(1 / 60);
    const saved = original.saveRuntime();
    await restored.loadRuntime(saved);
    const matrix = identityMatrix();
    expect(restored.runtime?.assemblyParts("stone-instance", matrix)?.[0].matrix[12]).toBeCloseTo(2.7);
    const state = restored.saveRuntime().dormant.find((entity) => entity.id === "stone-instance");
    expect(state?.state.joints).toBe("[1.7]");
  } finally {
    original.dispose();
    restored.dispose();
  }
});
