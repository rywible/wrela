import { expect, test } from "bun:test";
import { assemblyPartPoint, assemblySocketFrame } from "@wrela/model";

import { createAssemblyPart, createDoorwayAssembly, snapAssemblySockets } from "./assembly";

test("doorway reserves exact authored width and height", () => {
  const doorway = createDoorwayAssembly(1.2, 2.2, 0.15);
  expect(doorway.clearances[0].max[0] - doorway.clearances[0].min[0]).toBe(1.2);
  expect(doorway.clearances[0].max[1]).toBe(2.2);
});
test("sockets align through a rotated articulated parent", () => {
  const parent = createAssemblyPart("parent");
  parent.rotation = [0, 0, Math.PI / 3];
  parent.joint = { kind: "hinge", axis: [1, 0, 0], pivot: [1, 2, 0], minimum: 0, maximum: 2, value: 1 };
  const moving = createAssemblyPart("moving");
  moving.parent = parent.id;
  const target = createAssemblyPart("target");
  target.position = [5, 3, 2];
  const assembly = snapAssemblySockets(
    { grid: 0.1, clearances: [], parts: [parent, moving, target] },
    "moving",
    "end",
    "target",
    "start",
  );
  const a = assemblyPartPoint(
      assembly,
      assembly.parts[1],
      assemblySocketFrame(moving, moving.sockets[1]).origin,
    ),
    b = assemblyPartPoint(assembly, assembly.parts[2], assemblySocketFrame(target, target.sockets[0]).origin);
  a.forEach((v, i) => {
    expect(v).toBeCloseTo(b[i]);
  });
});
test("new part and doorway sockets remain attached when dimensions and sweep direction change", () => {
  const part = createAssemblyPart();
  part.path[1] = [1, 3, -2];
  expect(assemblySocketFrame(part, part.sockets[1]).origin).toEqual([1, 3, -2]);
  const doorway = createDoorwayAssembly(1.2, 2.2, 0.15);
  doorway.parts[0].path[1] = [0, 3.4, 0];
  expect(assemblySocketFrame(doorway.parts[0], doorway.parts[0].sockets[1]).origin).toEqual([0, 3.4, 0]);
});

test("persistent socket mates preserve complete frames through geometry and joint edits", async () => {
  const { assemblySocketFrame, assemblySchema } = await import("@wrela/model");
  const { mateAssemblySockets, detachAssemblyMate } = await import("./assembly");
  const target = createAssemblyPart("target"),
    child = createAssemblyPart("child");
  target.sockets[1] = {
    id: "end",
    position: [0, 0, 0],
    rotation: [0, 0, 0.3],
    anchor: { point: "end", profileOffset: [1, 0] },
  };
  child.sockets[0].rotation = [0.2, 0.1, 0];
  target.joint = { kind: "hinge", axis: [0, 1, 0], pivot: [0, 0, 0], minimum: 0, maximum: 2, value: 0 };
  const assembly = mateAssemblySockets(
    { parts: [child, target], grid: 0.1, clearances: [] },
    "child",
    "start",
    "target",
    "end",
  );
  assembly.parts[1].path[1] = [0, 0, 5];
  assembly.parts[1].profile = { kind: "rectangle", width: 2, height: 1 };
  if (assembly.parts[1].joint) assembly.parts[1].joint.value = 0.8;
  const check = (source: typeof assembly) => {
    const frames = [source.parts[0], source.parts[1]].map((part, i) => {
      const socket = assemblySocketFrame(part, part.sockets[i === 0 ? 0 : 1]);
      return [
        socket.origin,
        ...socket.axes.map((axis) => socket.origin.map((v, n) => v + axis[n]) as [number, number, number]),
      ].map((point) => assemblyPartPoint(source, part, point));
    });
    frames[0].forEach((point, i) => {
      point.forEach((value, n) => {
        expect(value).toBeCloseTo(frames[1][i][n]);
      });
    });
  };
  check(assembly);
  const detached = detachAssemblyMate(assembly, "child");
  check(detached);
  const cyclic = structuredClone(assembly);
  cyclic.parts[1].joint = undefined;
  cyclic.parts[1].mate = { part: "child", socket: "start", ownSocket: "end" };
  expect(assemblySchema.safeParse(cyclic).success).toBe(false);
});

test("persistent mates participate in source undo and redo", async () => {
  const { referenceProject } = await import("@wrela/examples");
  const { AuthoringSession } = await import("./session");
  const { mateAssemblySockets } = await import("./assembly");
  const session = new AuthoringSession(referenceProject());
  const assembly = mateAssemblySockets(
    { parts: [createAssemblyPart("a"), createAssemblyPart("b")], grid: 0.1, clearances: [] },
    "b",
    "start",
    "a",
    "end",
  );
  session.apply({
    expectedRevision: 0,
    operations: [{ kind: "document.set", target: "river-stone", path: ["assembly"], value: assembly }],
  });
  session.undo();
  const undone = session.getSnapshot().project.documents.find((d) => d.id === "river-stone");
  expect(undone?.kind === "object" && undone.assembly).toBeUndefined();
  session.redo();
  const restored = session.getSnapshot().project.documents.find((d) => d.id === "river-stone");
  expect(restored?.kind === "object" && restored.assembly?.parts[1].mate).toEqual(assembly.parts[1].mate);
});
