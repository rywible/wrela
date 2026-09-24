import { describe, expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples/fixtures";
import { creatureSchema } from "./creature";
import type { CharacterDefinition } from "./documents";
import { parseProject, validateProject } from "./validation";

function fixture() {
  const project = referenceProject();
  const character = project.documents.find((d) => d.kind === "character") as CharacterDefinition;
  const joint = character.joints[0].id;
  character.creature = creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      {
        id: "shoulder",
        name: "Shoulder",
        nodeIds: [character.field.nodes[0].id],
        jointIds: [joint],
        frame: { position: [0, 0, 0], rotation: [0, 0, 0] },
        extent: [1, 1, 1],
      },
    ],
    charts: [
      {
        id: "shoulder-chart",
        kind: "sweep",
        region: "shoulder",
        revision: 1,
        points: [
          [0, 0, 0],
          [0, 1, 0],
        ],
        radii: [0.2, 0.1],
      },
    ],
    anchors: [
      {
        id: "scar",
        region: "shoulder",
        chart: "shoulder-chart",
        chartRevision: 1,
        coordinates: [0.5, 0.25, 1],
        offset: 0.01,
        purpose: "scar",
        tolerance: 0.02,
      },
    ],
    influenceRules: [{ id: "bind", region: "shoulder", allowedJoints: [joint], excludedJoints: [] }],
    contacts: [
      {
        id: "plant",
        joint,
        motion: character.motions[0].id,
        start: 0,
        end: 0.1,
        target: [0, 0, 0],
        space: "character",
        weight: 1,
        tolerance: 0.02,
      },
    ],
  });
  return { project, character, source: character.creature };
}
function errors(project: unknown) {
  return validateProject(project).diagnostics.filter((d) => d.severity === "error");
}

describe("creature source contracts", () => {
  test("legacy source round trips without inserting creature fields", () => {
    const project = referenceProject();
    expect(parseProject(JSON.parse(JSON.stringify(project)))).toEqual(project);
  });
  test("anatomy and chart sources round trip with all default values explicit", () => {
    const { project } = fixture();
    expect(errors(project)).toEqual([]);
    expect(parseProject(JSON.parse(JSON.stringify(project)))).toEqual(project);
  });
  test("unsupported nested source edits fail instead of silently disappearing", () => {
    const { source } = fixture();
    expect(
      creatureSchema.safeParse({ ...source, regions: [{ ...source.regions[0], extentTypo: [1, 1, 1] }] })
        .success,
    ).toBe(false);
    expect(creatureSchema.safeParse({ ...source, inventedProperty: true }).success).toBe(false);
  });
  test("missing anatomy identities localize diagnostics to source", () => {
    const { project, character, source } = fixture();
    source.regions[0].nodeIds.push("missing-shape");
    source.anchors[0].region = "missing-region";
    const found = errors(project);
    expect(
      found.some((d) => d.document === character.id && d.node === "shoulder" && /field node/.test(d.message)),
    ).toBe(true);
    expect(found.some((d) => d.node === "scar" && /region/.test(d.message))).toBe(true);
  });
  test("rejects region cycles and nonreciprocal mirrors", () => {
    const { project, source } = fixture();
    source.regions.push({ ...structuredClone(source.regions[0]), id: "other", parent: "shoulder" });
    source.regions[0].parent = "other";
    source.regions[0].mirror = "other";
    expect(errors(project).some((d) => /cycle/.test(d.message))).toBe(true);
    expect(errors(project).some((d) => /reciprocal/.test(d.message))).toBe(true);
  });
  test("topology edits reject stale anchors but proportion edits retain correspondence", () => {
    const { project, source } = fixture();
    const chart = source.charts[0];
    if (chart.kind !== "sweep") throw new Error("fixture");
    chart.radii = [0.4, 0.3];
    expect(errors(project)).toEqual([]);
    chart.revision++;
    expect(errors(project).some((d) => d.node === "scar" && /stale/.test(d.message))).toBe(true);
  });
  test("rejects incomplete and degenerate chart geometry", () => {
    const { project, source } = fixture();
    const chart = source.charts[0];
    if (chart.kind !== "sweep") throw new Error("fixture");
    chart.points.push([0, 1, 0]);
    expect(errors(project).some((d) => /arrays must match/.test(d.message))).toBe(true);
    expect(errors(project).some((d) => /distinct/.test(d.message))).toBe(true);
  });
  test("anchors cannot silently reassign across regions or leave chart domain", () => {
    const { project, source } = fixture();
    source.regions.push({ ...structuredClone(source.regions[0]), id: "other" });
    source.anchors[0].region = "other";
    source.anchors[0].coordinates[0] = 2;
    expect(errors(project).some((d) => /different region/.test(d.message))).toBe(true);
    expect(errors(project).some((d) => /coordinates/.test(d.message))).toBe(true);
  });
  test("rejects contradictory bindings and missing corrective joint", () => {
    const { project, source } = fixture();
    source.influenceRules[0].excludedJoints = [...source.influenceRules[0].allowedJoints];
    source.correctives.push({
      id: "fix",
      region: "shoulder",
      joint: "missing",
      axis: "x",
      angle: 1,
      radius: 1,
      center: [0, 0, 0],
      displacement: [0.1, 0, 0],
    });
    expect(errors(project).some((d) => /overlap/.test(d.message))).toBe(true);
    expect(errors(project).some((d) => d.node === "fix" && /joint/.test(d.message))).toBe(true);
  });
  test("contact intervals stay inside existing motions", () => {
    const { project, source } = fixture();
    source.contacts[0].end = 100;
    expect(errors(project).some((d) => /interval/.test(d.message))).toBe(true);
    source.contacts[0].motion = "missing-motion";
    expect(errors(project).some((d) => /contact motion/.test(d.message))).toBe(true);
  });
  test("chain ordering cannot jump across unrelated joints", () => {
    const { project, character, source } = fixture();
    source.ikChains.push({
      id: "ik",
      joints: [character.joints[1].id, character.joints[0].id],
      target: [0, 0, 0],
      pole: [0, 0, 1],
      weight: 1,
      iterations: 12,
      tolerance: 0.01,
    });
    expect(errors(project).some((d) => /parent-child/.test(d.message))).toBe(true);
  });
  test("creature material dependencies participate in project type validation", () => {
    const { project, source } = fixture();
    source.regions[0].material = "daylight";
    expect(errors(project).some((d) => /must be material/.test(d.message))).toBe(true);
    source.regions[0].material = "unknown-material";
    expect(errors(project).some((d) => /Missing definition/.test(d.message))).toBe(true);
  });
  test("chart grooms require topology revision and monotonically cheaper detail", () => {
    const { project, source } = fixture();
    source.grooms = creatureSchema.parse({
      schemaVersion: 1,
      grooms: [
        {
          id: "coat",
          region: "shoulder",
          chart: "shoulder-chart",
          seed: 1,
          density: 20,
          length: 0.1,
          width: 0.02,
          direction: [0, 1, 0],
          taper: 1,
          clump: 0,
          curl: 0,
          rootColor: [0, 0, 0],
          tipColor: [1, 1, 1],
          maxCards: 100,
        },
      ],
    }).grooms;
    expect(errors(project).some((d) => /groom must declare chartRevision/.test(d.message))).toBe(true);
    source.grooms[0].chartRevision = 1;
    expect(errors(project)).toEqual([]);
    source.grooms[0].lodFractions = [1, 0.2, 0.8];
    expect(errors(project).some((d) => /nonincreasing/.test(d.message))).toBe(true);
    source.grooms[0].lodFractions = [1, 0.5, 0.2];
    source.grooms[0].chartRevision = 0;
    expect(errors(project).some((d) => /Groom chart revision is stale/.test(d.message))).toBe(true);
  });
  test("reserved identities and non-finite authored values fail at the public boundary", () => {
    const { source } = fixture();
    for (const id of ["__proto__", "constructor", "prototype"])
      expect(creatureSchema.safeParse({ ...source, regions: [{ ...source.regions[0], id }] }).success).toBe(
        false,
      );
    expect(
      creatureSchema.safeParse({ ...source, anchors: [{ ...source.anchors[0], offset: NaN }] }).success,
    ).toBe(false);
  });
  test("attachment, anchored appearance, cloth, and articulation relationships reject invalid coupling", () => {
    const { project, character, source } = fixture();
    source.attachments = [
      {
        id: "plate",
        anchor: "absent",
        nodeIds: [character.field.root],
        offset: [0, 0, 0],
        minimumClearance: 0,
      },
    ];
    expect(errors(project).some((d) => /Missing attachment anchor/.test(d.message))).toBe(true);
    source.attachments = [];
    source.cloth = creatureSchema.parse({
      schemaVersion: 1,
      cloth: [
        { id: "drape", region: "shoulder", chart: "shoulder-chart", chartRevision: 1, pinEdges: ["v0"] },
      ],
    }).cloth;
    expect(errors(project).some((d) => /existing patch chart/.test(d.message))).toBe(true);
    source.cloth = [];
    source.articulation = {
      bodies: character.joints.slice(0, 2).map((j) => ({ joint: j.id, mass: 1, radius: 0.1 })),
      joints: [
        {
          id: "hinge",
          parent: character.joints[0].id,
          child: character.joints[1].id,
          kind: "revolute",
          axis: [0, 0, 0],
          minimum: 1,
          maximum: -1,
        },
      ],
    };
    expect(errors(project).some((d) => /nonzero hinge axis/.test(d.message))).toBe(true);
    expect(errors(project).some((d) => /minimum exceeds/.test(d.message))).toBe(true);
    source.articulation.joints[0].axis = [1, 0, 0];
    source.articulation.joints[0].minimum = -1;
    source.articulation.joints[0].maximum = 1;
    source.articulation.joints.push({
      id: "cycle",
      parent: character.joints[1].id,
      child: character.joints[0].id,
      kind: "fixed",
    });
    expect(errors(project).some((d) => /Articulation contains a cycle/.test(d.message))).toBe(true);
  });
  test("correspondence-only charts retain coordinates and projection cannot reassign unrelated anatomy", () => {
    const { project, character, source } = fixture();
    source.charts[0].realization = "correspondence-only";
    expect(errors(project)).toEqual([]);
    const node = character.field.nodes[0].id;
    source.grooms = creatureSchema.parse({
      schemaVersion: 1,
      grooms: [
        {
          id: "projected-coat",
          region: "shoulder",
          chart: "shoulder-chart",
          chartRevision: 1,
          seed: 1,
          density: 20,
          length: 0.1,
          width: 0.01,
          direction: [0, 1, 0],
          taper: 1,
          clump: 0,
          curl: 0,
          rootColor: [0, 0, 0],
          tipColor: [1, 1, 1],
          maxCards: 20,
          rootProjection: { maxDistance: 0.2, direction: "outward", nodeIds: [node] },
        },
      ],
    }).grooms;
    expect(errors(project)).toEqual([]);
    character.field.nodes.push({
      ...structuredClone(character.field.nodes[0]),
      id: "unrelated",
      children: [],
    });
    const projection = source.grooms[0].rootProjection;
    if (!projection) throw new Error("fixture");
    projection.nodeIds = ["unrelated"];
    expect(errors(project).some((d) => /outside the declared anatomy/.test(d.message))).toBe(true);
    projection.nodeIds = ["missing"];
    expect(errors(project).some((d) => /Missing groom projection/.test(d.message))).toBe(true);
  });
});
