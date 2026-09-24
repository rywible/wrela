import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { assemblySchema } from "@wrela/model";
import { BrowserSceneHost } from "./scene-host";

test("persistent assembly placement edits move, remove, and restore per-part collision", async () => {
  const project = referenceProject();
  const stone = project.documents.find((document) => document.id === "river-stone");
  if (stone?.kind !== "object") throw new Error("Missing fixture object");
  stone.assembly = assemblySchema.parse({
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
        position: [0, 1, 0],
        rotation: [0, 0, 0],
        repeat: { count: 1, offset: [0, 0, 0] },
        sockets: [],
        wear: { amount: 0, scale: 1, seed: 1 },
      },
    ],
  });
  stone.collision = "box";
  const host = new BrowserSceneHost(project);
  try {
    await host.prepare("winter-valley");
    const world = host.world;
    const runtime = host.runtime;
    if (!world || !runtime) throw new Error("Missing world runtime");
    const id = "stone-instance/assembly/door";
    expect(host.resourceUsage.assemblyBytes).toBeGreaterThan(0);
    expect(runtime.physics.hasBody(id)).toBe(true);
    const original = runtime.physics.state(id).position;

    world.persistence.overrides.set("stone-instance", { position: [8, 0, 1], scale: 1.5 });
    host.advance(1 / 60);
    expect(runtime.physics.state(id).position[0] - original[0]).toBeGreaterThan(4);
    const surface = host
      .extract({ position: [8, 4, 10], target: [8, 1, 1], fov: 45 })
      .surfaces.find((item) => item.id === id);
    expect(surface).toBeDefined();
    expect(Math.hypot(...(surface?.matrix.slice(0, 3) ?? []))).toBeCloseTo(1.5, 4);

    world.persistence.overrides.set("stone-instance", { removed: true });
    host.advance(1 / 60);
    expect(runtime.physics.hasBody(id)).toBe(false);

    world.persistence.overrides.delete("stone-instance");
    host.advance(1 / 60);
    expect(runtime.physics.hasBody(id)).toBe(true);
    expect(runtime.physics.state(id).position[0]).toBeCloseTo(original[0], 4);
  } finally {
    host.dispose();
  }
});

test("persistent scale edits rebuild ordinary object collision to match the rendered instance", async () => {
  const host = new BrowserSceneHost(referenceProject());
  try {
    await host.prepare("winter-valley");
    const world = host.world;
    const runtime = host.runtime;
    if (!world || !runtime) throw new Error("Missing world runtime");
    const at = world.world.instances.find((instance) => instance.id === "stone-instance")?.position;
    if (!at) throw new Error("Missing stone instance");
    const ray = () => runtime.physics.raycast([at[0], at[1] + 8, at[2]], [0, -1, 0], 10);
    host.advance(1 / 60);
    const before = ray();
    expect(before?.id).toBe("stone-instance");

    world.persistence.overrides.set("stone-instance", { scale: 2 });
    host.advance(1 / 60);
    const after = ray();
    expect(after?.id).toBe("stone-instance");
    expect(after?.distance).toBeLessThan((before?.distance ?? 0) - 0.5);

    world.persistence.overrides.delete("stone-instance");
    host.advance(1 / 60);
    expect(ray()?.distance).toBeCloseTo(before?.distance ?? 0, 4);
  } finally {
    host.dispose();
  }
});
