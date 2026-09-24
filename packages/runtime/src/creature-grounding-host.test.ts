import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { type CharacterDefinition, type CompiledCharacter, creatureSchema } from "@wrela/model";
import { BrowserSceneHost } from "./scene-host";

test("displayed creature inspection resolves selected mesh only and detaches its results", async () => {
  const project = referenceProject(),
    source = project.documents.find((d) => d.kind === "character") as CharacterDefinition;
  source.creature = creatureSchema.parse({ schemaVersion: 1 });
  const mesh = {
    positions: new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]),
    normals: new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1]),
    indices: new Uint32Array([0, 1, 2]),
    bounds: { min: [0, 0, 0] as [number, number, number], max: [1, 1, 0] as [number, number, number] },
  };
  const weights = new Float32Array([1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]),
    jointIndices = new Uint16Array(12);
  const artifact: CompiledCharacter = {
    kind: "character",
    id: source.id,
    key: "fixture",
    mesh,
    material: source.material,
    joints: source.joints,
    motions: [],
    jointIndices,
    weights,
    creature: source.creature,
    diagnostics: [],
    creatureRegions: [null, null, null],
    creatureCoordinates: [null, null, null],
    creatureDetails: [
      {
        label: "distant",
        mesh: { ...mesh, positions: new Float32Array(mesh.positions) },
        jointIndices,
        weights,
        maxProjectedDiameter: 999,
        maxError: null,
      },
    ],
  };
  const host = new BrowserSceneHost(project, {
    compile: async (d) => (d.id === source.id ? artifact : null),
  });
  try {
    await host.prepare(source.id);
    const camera = {
      position: [0, 1, 100] as [number, number, number],
      target: [0, 0, 0] as [number, number, number],
      fov: 60,
    };
    const scene = host.extract(camera),
      surface = scene.surfaces.find((s) => s.skin);
    if (!surface) throw new Error("fixture surface");
    const inspected = host.inspectDisplayedCreature(surface, (a) => ({
      key: a.key,
      positions: a.mesh.positions,
    }));
    expect(inspected?.key).toContain("detail:distant");
    expect(inspected?.positions).toEqual(surface.mesh.positions);
    if (!inspected) throw new Error("inspection");
    inspected.positions[0] = 99;
    expect(surface.mesh.positions[0]).toBe(0);
    expect(host.inspectDisplayedCreature({ ...surface }, (a) => a.key)).toBeUndefined();
    host.extract(camera);
    expect(host.inspectDisplayedCreature(surface, (a) => a.key)).toBeUndefined();
    const next = host.extract(camera).surfaces.find((s) => s.skin);
    if (!next) throw new Error("fixture surface");
    host.dispose();
    expect(host.inspectDisplayedCreature(next, (a) => a.key)).toBeUndefined();
  } finally {
    host.dispose();
  }
});
