import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { assemblyPartSchema, identityMatrix } from "@wrela/model";
import { BrowserSceneHost } from "./scene-host";

test("reprepare retains compatible live machinery and saves fresh identity after authored shape changes", async () => {
  const project = referenceProject(),
    object = project.documents.find((document) => document.id === "river-stone");
  if (object?.kind !== "object") throw new Error("Missing object fixture");
  const part = assemblyPartSchema.parse({
    id: "gate",
    name: "Gate",
    profile: { kind: "rectangle", width: 0.2, height: 0.2 },
    path: [
      [0, 0, 0],
      [0, 0, 2],
    ],
    position: [0, 0, 0],
    rotation: [0, 0, 0],
    bevel: 0,
    repeat: { count: 1, offset: [1, 0, 0] },
    sockets: [],
    wear: { amount: 0, scale: 1, seed: 1 },
  });
  part.joint = { kind: "slider", axis: [1, 0, 0], pivot: [0, 0, 0], minimum: 0, maximum: 2, value: 0 };
  object.assembly = { parts: [part], clearances: [], grid: 0.1 };
  const host = new BrowserSceneHost(project);
  let restored: BrowserSceneHost | undefined;
  try {
    await host.prepare("winter-valley");
    host.runtime?.setAssemblyJoint("stone-instance", "gate", 1.5);
    host.advance(1 / 60);
    const runtime = host.runtime;
    const recolored = structuredClone(project),
      material = recolored.documents.find((document) => document.id === "stone");
    if (material?.kind !== "material") throw new Error("Missing material fixture");
    material.roughness = 0.25;
    await host.setProject(recolored);
    expect(host.runtime).toBe(runtime);
    expect(host.runtime?.assemblyParts("stone-instance", identityMatrix())?.[0].matrix[12]).toBe(1.5);
    const unrelatedEdit = structuredClone(recolored);
    const vegetation = unrelatedEdit.documents.find((document) => document.kind === "vegetation");
    if (!vegetation || vegetation.kind !== "vegetation") throw new Error("Missing vegetation fixture");
    vegetation.height += 0.25;
    await host.setProject(unrelatedEdit);
    expect(host.runtime).not.toBe(runtime);
    expect(host.runtime?.assemblyParts("stone-instance", identityMatrix())?.[0].matrix[12]).toBe(1.5);
    const oldSave = host.saveRuntime();
    await host.loadRuntime(oldSave);
    const reshaped = structuredClone(recolored),
      changed = reshaped.documents.find((document) => document.id === "river-stone");
    if (changed?.kind !== "object" || !changed.assembly) throw new Error("Missing assembly fixture");
    changed.assembly.parts[0].path[1][2] += 0.5;
    await host.setProject(reshaped);
    const newSave = host.saveRuntime(),
      saved = newSave.dormant.find((entity) => entity.id === "stone-instance");
    // Geometry changes intentionally establish the current host's fresh runtime baseline.
    expect(host.runtime).not.toBe(runtime);
    expect(saved?.state.joints).toBe("[null]");
    expect(saved?.state.sourceKey).not.toBe(
      oldSave.dormant.find((entity) => entity.id === "stone-instance")?.state.sourceKey,
    );
    restored = new BrowserSceneHost(reshaped);
    await restored.prepare("winter-valley");
    await expect(restored.loadRuntime(newSave)).resolves.toMatchObject({ time: 0 });
    expect(() => restored?.prepareRuntimeSave(oldSave)).toThrow("incompatible");
  } finally {
    host.dispose();
    restored?.dispose();
  }
});
