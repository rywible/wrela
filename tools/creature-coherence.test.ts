import { expect, test } from "bun:test";
import { AuthoringSession, authorCreatureGarment, authorCreatureGarmentLandmarks } from "@wrela/authoring";
import { compileDocument, evaluateCreatureChart } from "@wrela/compiler";
import { referenceProject } from "@wrela/examples";
import { type CharacterDefinition, creatureSchema } from "@wrela/model";
import { createCreatureClothState, stepCreatureCloth } from "@wrela/runtime";
import { createCreatureFittingLookdev } from "./fixtures/creature-fitting";

test("Studio-authored garment compiles to bound geometry and its pins follow animated skin transforms", () => {
  const project = referenceProject();
  const character = project.documents.find((item) => item.kind === "character") as CharacterDefinition;
  character.creature = creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      {
        id: "torso",
        name: "Torso",
        jointIds: character.joints.map((joint) => joint.id),
        frame: { position: [0, 1, 0], rotation: [0, 0, 0] },
        extent: [0.5, 0.7, 0.4],
      },
    ],
  });
  const session = new AuthoringSession(project);
  const read = () => session.inspect(character.id) as CharacterDefinition;
  session.apply({
    expectedRevision: 0,
    operations: authorCreatureGarmentLandmarks(character, "torso", "cloak"),
  });
  session.apply({
    expectedRevision: 1,
    operations: authorCreatureGarment(read(), {
      id: "cloak",
      region: "torso",
      landmarks: ["cloak-top-left", "cloak-top-right", "cloak-bottom-left", "cloak-bottom-right"],
    }),
  });
  session.apply({
    expectedRevision: 2,
    operations: [
      {
        kind: "creature.landmark",
        target: character.id,
        value: { id: "cloak-top-left", region: "torso", position: [-0.6, 0.55, -0.48] },
      },
    ],
  });
  const compiled = compileDocument(read(), "interactive");
  if (!compiled || compiled.kind !== "character") throw Error("Missing compiled character");
  expect(compiled.diagnostics.filter((item) => item.severity === "error")).toEqual([]);
  expect(compiled.creatureCoordinates?.some((coordinate) => coordinate?.chart === "cloak-surface")).toBe(
    true,
  );
  const matrices = new Float32Array(compiled.joints.length * 16);
  for (let index = 0; index < compiled.joints.length; index++) {
    matrices.set([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1], index * 16);
  }
  const state = createCreatureClothState();
  const context = {
    position: [0, 0, 0] as [number, number, number],
    rotation: [0, 0, 0, 1] as [number, number, number, number],
    scale: 1,
    time: 0,
    motionTime: 0,
  };
  stepCreatureCloth(compiled, matrices, state, context, 1 / 60);
  expect(state.panels).toHaveLength(1);
  const rest = structuredClone(state.panels[0].positions);
  for (let index = 0; index < compiled.joints.length; index++) matrices[index * 16 + 12] = 0.4;
  for (let frame = 0; frame < 30; frame++) stepCreatureCloth(compiled, matrices, state, context, 1 / 120);
  // A six-segment lattice has seven particles along its top edge; its first and
  // seventh particles are the two explicitly authored pins.
  for (const index of [0, 6]) {
    expect(state.panels[0].positions[index][0] - rest[index][0]).toBeCloseTo(0.4, 5);
    expect(state.panels[0].positions[index][1]).toBeCloseTo(rest[index][1], 5);
  }
  expect(state.panels[0].positions.flat().every(Number.isFinite)).toBe(true);
});

test("fitting lookdev is deterministic valid editable source with live refitted cloth", () => {
  const fixture = createCreatureFittingLookdev();
  const again = createCreatureFittingLookdev();
  expect(fixture).toEqual(again);
  expect(fixture.fitting.beforeFit).not.toBe(fixture.fitting.afterFit);
  const character = fixture.project.documents.find((document) => document.id === fixture.characterId);
  if (!character || character.kind !== "character" || !character.creature)
    throw Error("Missing fitting character");
  const cloak = character.creature.cloth.find((cloth) => cloth.id === fixture.fitting.garment);
  expect(cloak?.fittingLandmarks).toEqual(fixture.fitting.landmarks);
  expect(character.creature.charts.find((chart) => chart.id === cloak?.chart)?.points[2]).toEqual(
    fixture.fitting.refitCorner,
  );
  const compiled = compileDocument(character, "interactive");
  if (!compiled || compiled.kind !== "character") throw Error("Missing fitting artifact");
  expect(compiled.diagnostics.filter((diagnostic) => diagnostic.severity === "error")).toEqual([]);
  expect(compiled.creatureCoordinates?.some((coordinate) => coordinate?.chart === cloak?.chart)).toBe(true);
  expect(compiled.mesh.materialGroups?.some((group) => group.material === "pilgrim-ochre-wool")).toBe(true);
  expect(character.field.nodes.some((node) => node.id.startsWith("cloth-panel-"))).toBe(false);
  if (!cloak) throw Error("Missing fitted cloak");
  const creature = character.creature;
  const samples = [0.1, 0.25, 0.5, 0.75, 0.9].map(
    (u) => evaluateCreatureChart(creature, cloak.chart, [u, 0.5, 0]).position[2],
  );
  // Patterned gores retain physical gathers without the old oversized 58 mm flutes.
  expect(Math.max(...samples) - Math.min(...samples)).toBeGreaterThan(0.006);
  expect([...compiled.mesh.positions].every(Number.isFinite)).toBe(true);
});
