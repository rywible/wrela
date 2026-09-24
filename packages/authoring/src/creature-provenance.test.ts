import { describe, expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { type CharacterDefinition, type CompiledCharacter, contentKey, creatureSchema } from "@wrela/model";
import {
  creatureGroundingStamp,
  groundCreatureHit,
  inspectCreatureFields,
  traceCreatureDependencies,
} from "./creature-provenance";

function fixture() {
  const character = referenceProject().documents.find((d) => d.kind === "character") as CharacterDefinition;
  const node = character.field.nodes[0].id,
    joint = character.joints[0].id;
  character.creature = creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      {
        id: "shoulder",
        name: "Shoulder",
        nodeIds: [node],
        jointIds: [joint],
        frame: { position: [0, 0, 0], rotation: [0, 0, 0] },
        extent: [1, 1, 1],
      },
    ],
    charts: [
      {
        id: "shoulder-chart",
        region: "shoulder",
        revision: 1,
        kind: "sweep",
        points: [
          [0, 0, 0],
          [0, 1, 0],
        ],
        radii: [0.2, 0.2],
      },
    ],
    anchors: [
      {
        id: "scar-anchor",
        region: "shoulder",
        chart: "shoulder-chart",
        chartRevision: 1,
        coordinates: [0.5, 0, 1],
        offset: 0.01,
        purpose: "scar",
        tolerance: 0.01,
      },
    ],
    sculpts: [
      {
        id: "bulge",
        region: "shoulder",
        center: [0, 0.5, 0],
        radius: 0.2,
        displacement: [0.01, 0, 0],
        strength: 1,
        falloff: 2,
      },
    ],
    appearance: [
      {
        id: "scar",
        region: "shoulder",
        anchor: "scar-anchor",
        family: "skin",
        color: [0.3, 0.2, 0.1],
        roughness: 0.5,
        growthSuppression: 1,
        mask: { center: [0, 0, 0], radius: 0.2, falloff: 1 },
      },
    ],
    grooms: [
      {
        id: "mane",
        region: "shoulder",
        chart: "shoulder-chart",
        chartRevision: 1,
        seed: 1,
        density: 100,
        length: 0.2,
        width: 0.01,
        direction: [0, 1, 0],
        taper: 1,
        clump: 0,
        curl: 0,
        rootColor: [0, 0, 0],
        tipColor: [1, 1, 1],
        maxCards: 20,
      },
    ],
    attachments: [{ id: "plate", anchor: "scar-anchor", nodeIds: [node] }],
  });
  const artifact: CompiledCharacter = {
    kind: "character",
    id: character.id,
    key: "compiled-creature",
    material: character.material,
    mesh: {
      positions: new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]),
      normals: new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1]),
      indices: new Uint32Array([0, 1, 2]),
      sourceIds: [node, "shoulder-chart", "shoulder-chart"],
      materialGroups: [{ material: "scar-material", start: 0, count: 3 }],
      bounds: { min: [0, 0, 0], max: [1, 1, 0] },
    },
    joints: character.joints,
    motions: character.motions,
    jointIndices: new Uint16Array([0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]),
    weights: new Float32Array([1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]),
    diagnostics: [],
    creature: character.creature,
    creatureSourceKey: contentKey(character),
    creatureRegions: ["shoulder", "shoulder", "shoulder"],
    creatureCoordinates: [
      { region: "shoulder", chart: "shoulder-chart", chartRevision: 1, coordinates: [0.4, 0.95, 1] },
      { region: "shoulder", chart: "shoulder-chart", chartRevision: 1, coordinates: [0.5, 0.05, 1] },
      { region: "shoulder", chart: "shoulder-chart", chartRevision: 1, coordinates: [0.6, 0.05, 1] },
    ],
  };
  return {
    character,
    artifact,
    request: {
      ...creatureGroundingStamp(character, artifact, 4),
      triangle: 0,
      barycentric: [0.5, 0.25, 0.25] as [number, number, number],
    },
  };
}

describe("creature field introspection and grounded provenance", () => {
  test("source introspection gives physical units, editable paths and actual local support", () => {
    const { character } = fixture(),
      index = inspectCreatureFields(character);
    const sculpt = index.controls.find((c) => c.id === "bulge");
    expect(sculpt?.operation).toBe("creature.sculpt");
    expect(sculpt?.support).toEqual({
      kind: "sculpt-field",
      region: "shoulder",
      center: [0, 0.5, 0],
      radii: [0.2, 0.2, 0.2],
      rotation: [0, 0, 0],
      mirror: false,
      path: undefined,
      nodeIds: undefined,
    });
    expect(sculpt?.parameters.find((p) => p.path.at(-1) === "displacement")).toMatchObject({
      unit: "metres",
      space: "region-local",
      value: [0.01, 0, 0],
    });
    const anchor = index.controls.find((c) => c.id === "scar-anchor");
    expect(anchor?.parameters.find((p) => p.path.at(-1) === "coordinates")?.unit).toBe("unitless");
    expect(index.sourceKey).toBe(contentKey(character));
  });
  test("chart dependency explanations follow scar, coat suppression and mounted attachment", () => {
    const { character } = fixture(),
      trace = traceCreatureDependencies(character, { domain: "charts", id: "shoulder-chart" });
    expect(trace.controls.map((c) => c.id)).toEqual(
      expect.arrayContaining(["shoulder-chart", "scar-anchor", "scar", "mane", "plate"]),
    );
    expect(
      trace.edges.some(
        (e) => e.from.id === "mane" && e.to.id === "scar" && /growth-suppression/.test(e.reason),
      ),
    ).toBe(true);
    expect(
      traceCreatureDependencies(
        character,
        { domain: "attachments", id: "plate" },
        "dependencies",
      ).controls.some((c) => c.domain === "field.nodes"),
    ).toBe(true);
  });
  test("triangle grounding resolves surface provenance, material and weighted skin influences", () => {
    const { character, artifact, request } = fixture(),
      hit = groundCreatureHit(character, artifact, request);
    expect(hit.restPoint).toEqual([0.25, 0.25, 0]);
    expect(hit.material).toBe("scar-material");
    expect(hit.correspondence.status).toBe("resolved");
    expect(hit.skinInfluences[0]).toEqual({ id: character.joints[0].id, weight: 0.75 });
    expect(hit.skinInfluences[1]).toEqual({ id: character.joints[1].id, weight: 0.25 });
    expect(hit.controls.some((c) => c.id === "bulge")).toBe(true);
    expect(hit.sourceRevision).toBe(4);
  });
  test("wrapped sweep coordinates interpolate across the seam without moving to the opposite side", () => {
    const { character, artifact, request } = fixture(),
      hit = groundCreatureHit(character, artifact, request);
    expect(hit.correspondence.coordinate?.coordinates[1]).toBeCloseTo(0, 8);
  });
  test("mixed chart triangle is explicitly ambiguous and never manufactures a stable coordinate", () => {
    const { character, artifact, request } = fixture();
    const c = artifact.creatureCoordinates?.[1];
    if (!c) throw new Error("fixture");
    c.chart = "other-chart";
    const hit = groundCreatureHit(character, artifact, request);
    expect(hit.correspondence.status).toBe("ambiguous");
    expect(hit.correspondence.coordinate).toBeUndefined();
  });
  test("old source, wrong detail realization and malformed hits are rejected", () => {
    const { character, artifact, request } = fixture();
    expect(() => groundCreatureHit(character, artifact, { ...request, barycentric: [1, 1, 1] })).toThrow(
      /sum to one/,
    );
    expect(() => groundCreatureHit(character, artifact, { ...request, triangle: 2 })).toThrow(/outside/);
    artifact.mesh.positions[0] = 0.1;
    expect(() => groundCreatureHit(character, artifact, request)).toThrow(/realized mesh/);
    artifact.mesh.positions[0] = 0;
    character.field.nodes[0].radius += 0.1;
    expect(() =>
      groundCreatureHit(character, artifact, {
        ...request,
        ...creatureGroundingStamp(character, artifact, 5),
      }),
    ).toThrow(/Stale/);
  });
  test("returned parameter values cannot mutate source, and unknown controls fail explicitly", () => {
    const { character } = fixture(),
      before = JSON.stringify(character),
      index = inspectCreatureFields(character);
    const param = index.controls
      .find((c) => c.id === "bulge")
      ?.parameters.find((p) => p.path.at(-1) === "center");
    if (!param || !Array.isArray(param.value)) throw new Error("fixture");
    param.value[0] = 99;
    expect(JSON.stringify(character)).toBe(before);
    expect(() => traceCreatureDependencies(character, { domain: "charts", id: "absent" })).toThrow(/Unknown/);
  });
});
