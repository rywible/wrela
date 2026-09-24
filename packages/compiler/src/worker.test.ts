import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import type { SurfaceArtifact } from "@wrela/model";

test("compile worker returns owned typed buffers and an explicit invalid-input failure", async () => {
  const worker = new Worker(new URL("./worker.ts", import.meta.url).href);
  try {
    const document = referenceProject().documents.find((d) => d.kind === "object");
    const send = (data: unknown) =>
      new Promise<{ id: string; artifact: SurfaceArtifact | null; error: string | null }>(
        (resolve, reject) => {
          worker.onmessage = (event) => resolve(event.data);
          worker.onerror = (event) => reject(new Error(event.message));
          worker.postMessage(data);
        },
      );
    const result = await send({ id: "compile-one", document, quality: "interactive" });
    expect(result.id).toBe("compile-one");
    expect(result.error).toBeNull();
    expect(result.artifact?.kind).toBe("surface");
    if (result.artifact?.kind !== "surface") throw new Error("Expected a surface artifact");
    expect(result.artifact.mesh.positions).toBeInstanceOf(Float32Array);
    expect(result.artifact.mesh.positions.byteLength).toBeGreaterThan(0);
    expect(result.artifact.mesh.indices).toBeInstanceOf(Uint32Array);
    const reused = await send({ id: "reuse-after-transfer", document, quality: "interactive" });
    expect(reused.error).toBeNull();
    if (reused.artifact?.kind !== "surface") throw new Error("Expected reused surface artifact");
    expect(reused.artifact.mesh.positions).toEqual(result.artifact.mesh.positions);
    expect(reused.artifact.mesh.indices).toEqual(result.artifact.mesh.indices);
    const failure = await send({ id: "invalid", document: { kind: "object" }, quality: "interactive" });
    expect(failure.id).toBe("invalid");
    expect(failure.artifact).toBeNull();
    expect(failure.error).toBeTypeOf("string");
  } finally {
    worker.terminate();
  }
}, 10000);

test("terrain generation worker transfers a bounded patch and rejects oversized requests", async () => {
  const worker = new Worker(new URL("./worker.ts", import.meta.url).href);
  try {
    const terrain = referenceProject().documents.find((d) => d.kind === "terrain");
    const send = (data: unknown) =>
      new Promise<{
        id: string;
        mesh: { positions: Float32Array; indices: Uint32Array } | null;
        error: string | null;
      }>((resolve, reject) => {
        worker.onmessage = (event) => resolve(event.data);
        worker.onerror = (event) => reject(new Error(event.message));
        worker.postMessage(data);
      });
    const result = await send({
      id: "terrain-one",
      terrain,
      patch: { x: -64, z: 32, size: 32, resolution: 16, stitch: { north: true } },
    });
    expect(result.id).toBe("terrain-one");
    expect(result.error).toBeNull();
    expect(result.mesh?.positions).toBeInstanceOf(Float32Array);
    expect(result.mesh?.positions.length).toBe(17 * 17 * 3);
    const failure = await send({
      id: "terrain-invalid",
      terrain,
      patch: { x: 0, z: 0, size: 32, resolution: 65536 },
    });
    expect(failure.mesh).toBeNull();
    expect(failure.error).toContain("256");
  } finally {
    worker.terminate();
  }
}, 10000);

test("growth worker continues authoritative checkpoints and transfers the matching realization", async () => {
  const { alpinePineLookdevDefinition } = await import("@wrela/examples");
  const { botanicalDevelopmentSchema } = await import("@wrela/model");
  const { prepareVegetationGrowth } = await import("./botanical-growth-request");
  const source = alpinePineLookdevDefinition();
  if (!source.botanical) throw Error("Expected botanical source");
  source.botanical.development = botanicalDevelopmentSchema.parse({ species: "lodgepole-pine", steps: 6 });
  const worker = new Worker(new URL("./worker.ts", import.meta.url).href);
  const send = (data: unknown) =>
    new Promise<{ result: import("./botanical-growth-request").GrowthCompileResult; error: string | null }>(
      (resolve, reject) => {
        worker.onmessage = (event) => resolve(event.data);
        worker.onerror = (event) => reject(Error(event.message));
        worker.postMessage(data);
      },
    );
  try {
    const first = await send({
      id: "grow",
      growth: { document: source, instanceId: "forest/tree/1", steps: 8, events: [] },
      quality: "interactive",
    });
    expect(first.error).toBeNull();
    expect(first.result.growth).toEqual(prepareVegetationGrowth(source, 8));
    const next = await send({
      id: "continue",
      growth: {
        document: source,
        instanceId: "forest/tree/1",
        steps: 10,
        previous: first.result.growth,
        events: [],
      },
      quality: "interactive",
    });
    expect(next.error).toBeNull();
    expect(next.result.growth).toEqual(prepareVegetationGrowth(source, 10, first.result.growth));
    expect(next.result.artifact.id).toBe(first.result.artifact.id);
    expect(next.result.artifact.surfaces[0].mesh.materialCoordinates).toBeInstanceOf(Float32Array);
    expect(next.result.artifact.surfaces[1].mesh.positions.byteLength).toBeGreaterThan(0);
  } finally {
    worker.terminate();
  }
}, 10000);
