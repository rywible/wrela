import { expect, test } from "bun:test";
import { type CharacterDefinition, creatureSchema, type FieldNode } from "@wrela/model";

import { prepareCreatureAttachments } from "./creature-attachments";
import { compileDocument } from "./index";

const primitive = (id: string, position: [number, number, number], radius = 0.1): FieldNode => ({
  id,
  name: id,
  kind: "sphere",
  position,
  rotation: [0, 0, 0],
  size: [radius, radius, radius],
  radius,
  blend: 0,
  children: [],
});
function source(): CharacterDefinition {
  const root: FieldNode = { ...primitive("root", [0, 0, 0]), kind: "union", children: ["body", "plate"] };
  return {
    id: "mounted",
    name: "Mounted",
    schemaVersion: 1,
    dependencies: [],
    kind: "character",
    material: "skin",
    physics: { mode: "kinematic", mass: 1, restitution: 0, friction: 0.5 },
    field: {
      root: "root",
      nodes: [root, primitive("body", [0, 0, 0], 0.15), primitive("plate", [4, 0, 0], 0.05)],
      bounds: { min: [-0.8, -0.8, -0.8], max: [0.8, 0.8, 0.8] },
      resolution: 24,
    },
    joints: [
      {
        id: "shoulder",
        name: "Shoulder",
        parent: null,
        position: [0, 0, 0],
        rotation: [0, 0, 0],
        radius: 2,
        minimum: -2,
        maximum: 2,
      },
    ],
    motions: [],
    creature: creatureSchema.parse({
      schemaVersion: 1,
      regions: [
        {
          id: "shoulder",
          name: "Shoulder",
          nodeIds: ["body", "plate"],
          jointIds: ["shoulder"],
          frame: { position: [0, 0, 0], rotation: [0, 0, 0] },
          extent: [1, 1, 1],
        },
      ],
      charts: [
        {
          id: "shoulder-chart",
          region: "shoulder",
          kind: "sweep",
          revision: 1,
          points: [
            [0, -0.4, 0],
            [0, 0.4, 0],
          ],
          radii: [0.2, 0.2],
        },
      ],
      anchors: [
        {
          id: "mount",
          region: "shoulder",
          chart: "shoulder-chart",
          chartRevision: 1,
          coordinates: [0.5, 0, 1],
          offset: 0,
          purpose: "attachment",
          tolerance: 0.01,
        },
      ],
      attachments: [
        {
          id: "plate-mount",
          anchor: "mount",
          nodeIds: ["plate"],
          offset: [-0.2, 0, 0],
          rigidJoint: "shoulder",
          minimumClearance: 0.1,
        },
      ],
    }),
  };
}

test("anchored primitive moves with proportions without mutating authored source", () => {
  const doc = source(),
    before = prepareCreatureAttachments(doc);
  expect(before.document.field.nodes[2].position[0]).toBeCloseTo(-0.4, 5);
  expect(doc.field.nodes[2].position).toEqual([4, 0, 0]);
  const chart = doc.creature?.charts[0];
  if (!chart || chart.kind !== "sweep") throw new Error("fixture");
  chart.radii = [0.4, 0.4];
  const after = prepareCreatureAttachments(doc);
  expect(after.document.field.nodes[2].position[0]).toBeCloseTo(-0.6, 5);
  expect(after.document.field.nodes[2].radius).toBe(0.05);
  expect(after.diagnostics[0].severity).toBe("info");
});

test("mounting honors transformed parent coordinates and diagnoses clearance failures", () => {
  const doc = source();
  doc.field.nodes[0].position = [1, 2, 3];
  doc.field.nodes[0].rotation = [0, 0, Math.PI / 2];
  const prepared = prepareCreatureAttachments(doc),
    p = prepared.document.field.nodes[2].position;
  expect(-p[1] + 1).toBeCloseTo(-0.4, 5);
  expect(p[0] + 2).toBeCloseTo(0, 5);
  expect(p[2] + 3).toBeCloseTo(0, 5);
  if (!doc.creature) throw new Error("fixture");
  doc.creature.attachments[0].offset = [0, 0, 0];
  expect(prepareCreatureAttachments(doc).diagnostics[0].code).toBe("creature.attachment.clearance");
});

test("topology invalidation blocks mounted source instead of leaving stale geometry", () => {
  const doc = source();
  if (!doc.creature) throw new Error("fixture");
  doc.creature.charts[0].revision++;
  expect(() => prepareCreatureAttachments(doc)).toThrow("topology transfer was not established");
});

test("compiled mounting uses prepared field geometry and rigid attachment ownership", () => {
  const doc = source(),
    artifact = compileDocument(doc, "interactive");
  if (!artifact || artifact.kind !== "character") throw new Error("fixture");
  expect(artifact.mesh.bounds.max[0]).toBeLessThan(2);
  const plate = artifact.mesh.sourceIds?.indexOf("plate") ?? -1;
  expect(plate).toBeGreaterThanOrEqual(0);
  expect(artifact.weights[plate * 4]).toBe(1);
  expect(artifact.diagnostics.some((d) => d.code === "creature.attachment.mounted")).toBe(true);
});

test("surface placement seats rotated primitive support at the requested clearance", () => {
  const doc = source();
  if (!doc.creature) throw Error("Missing creature");
  const plate = doc.field.nodes[2];
  plate.kind = "box";
  plate.size = [0.1, 0.2, 0.05];
  plate.rotation = [0, 0, Math.PI / 4];
  doc.creature.attachments[0].offset = [0, 0, 0];
  doc.creature.attachments[0].placement = "surface";
  const prepared = prepareCreatureAttachments(doc);
  expect(prepared.diagnostics[0].severity).toBe("info");
  expect(prepared.document.field.nodes[2].position[0]).toBeCloseTo(-0.2 - 0.3 / Math.sqrt(2) - 0.1, 5);
  expect(prepared.document.field.nodes[2].size).toEqual(plate.size);
  expect(prepared.document.field.nodes[2].rotation).toEqual(plate.rotation);
  expect(plate.position).toEqual([4, 0, 0]);
  // Parent transforms are included in support rather than using unrotated size.
  doc.field.nodes[0].rotation = [0, 0, Math.PI / 4];
  const rotated = prepareCreatureAttachments(doc);
  expect(rotated.diagnostics[0].severity).toBe("info");
  expect(rotated.diagnostics[0].message).toContain("0.10000 m");
});
