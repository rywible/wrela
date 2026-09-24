import { expect, test } from "bun:test";
import { AuthoringSession } from "@wrela/authoring";
import { authorCreatureMantle } from "@wrela/authoring/creature-mantle";
import { compileCharacter, evaluateCreatureChart } from "@wrela/compiler";
import { createCreatureFixture } from "@wrela/examples";
import { applyBipedAnatomy } from "@wrela/examples/creature-biped-construction";
import { applyBipedWalk } from "@wrela/examples/creature-biped-motion";
import { type CharacterDefinition, canonical, documentSchema, validateProject } from "@wrela/model";
import { RuntimeSession } from "@wrela/runtime";
import { auditCreatureMotion } from "@wrela/runtime/creature-motion-audit";
import { CREATURE_FITTING_VARIANTS, createCreatureFittingLookdev } from "./fixtures/creature-fitting";

function required<T>(value: T | undefined): T {
  if (value === undefined) throw Error("Missing constructed source");
  return value;
}

function biped() {
  const fixture = createCreatureFixture("reed-penitent");
  const character = fixture.project.documents.find(
    (document): document is CharacterDefinition => document.kind === "character",
  );
  if (!character) throw Error("Missing biped");
  return { fixture, character };
}

test("semantic anatomy preserves pivots and original source while changing coherent body volumes", () => {
  const { character } = biped(),
    before = canonical(character);
  const lean = applyBipedAnatomy(character, { build: 0.75 }),
    broad = applyBipedAnatomy(character, { build: 1.25, shoulderBreadth: 1.16 });
  expect(canonical(character)).toBe(before);
  expect(lean.joints).toEqual(character.joints);
  expect(documentSchema.safeParse(lean).success).toBe(true);
  const extent = (source: CharacterDefinition, id: string) =>
    required(source.field.nodes.find((node) => node.id === id)).size[0];
  expect(extent(broad, "deltoid-left")).toBeGreaterThan(extent(lean, "deltoid-left"));
  expect(extent(broad, "calf-left")).toBeGreaterThan(extent(lean, "calf-left"));
  expect(lean.creature?.charts.find((chart) => chart.id === "shoulder-surface")?.realization).toBe(
    "correspondence-only",
  );
  expect(lean.creature?.charts.filter((chart) => /hand-.*-(digit|thumb)/.test(chart.id))).toHaveLength(10);
  expect(() => applyBipedAnatomy(character, { build: NaN })).toThrow();
});

test("continuous mantle survives landmark edits, undo and export", () => {
  const { fixture, character } = biped();
  const session = new AuthoringSession(fixture.project);
  const operations = authorCreatureMantle(character, {
    id: "review-wrap",
    region: "shoulder",
    material: "marsh-linen",
    shoulderWidth: 0.68,
    backDepth: 0.24,
    length: 1.1,
    flare: 0.06,
  });
  session.apply({ expectedRevision: 0, operations });
  const result = session.inspect(character.id) as CharacterDefinition;
  expect(result.creature?.cloth.filter((cloth) => cloth.id.startsWith("review-wrap"))).toHaveLength(1);
  const patch = required(
    required(result.creature).charts.find((chart) => chart.id === "review-wrap-surface"),
  );
  if (patch.kind !== "patch") throw Error("Missing continuous mantle patch");
  expect(patch.controlOffsets?.length).toBe(9);
  const center = evaluateCreatureChart(required(result.creature), patch.id, [0.5, 0.5, 0]).position;
  const opening = evaluateCreatureChart(required(result.creature), patch.id, [0, 0.5, 0]).position;
  expect(center[2]).toBeLessThan(opening[2] - 0.2);
  for (let column = 1; column < 32; column++) {
    const left = evaluateCreatureChart(required(result.creature), patch.id, [column / 32 - 1e-6, 0.5, 0]);
    const right = evaluateCreatureChart(required(result.creature), patch.id, [column / 32 + 1e-6, 0.5, 0]);
    expect(Math.hypot(...left.position.map((value, axis) => value - right.position[axis]))).toBeLessThan(
      1e-4,
    );
    expect(Math.hypot(...left.normal.map((value, axis) => value - right.normal[axis]))).toBeLessThan(0.003);
  }
  const landmark = required(
    required(result.creature).landmarks.find((entry) => entry.id === "review-wrap-corner-0"),
  );
  session.apply({
    expectedRevision: 1,
    operations: [
      {
        kind: "creature.landmark",
        target: character.id,
        value: {
          ...landmark,
          position: [landmark.position[0], landmark.position[1] + 0.03, landmark.position[2]],
        },
      },
    ],
  });
  const moved = session.inspect(character.id) as CharacterDefinition;
  const cloth = required(required(moved.creature).cloth.find((entry) => entry.id === "review-wrap"));
  const anchor = required(
    required(moved.creature).anchors.find((entry) => entry.id === cloth.pins[0].anchor),
  );
  expect(anchor.coordinates[1]).toBeCloseTo(landmark.position[1] + 0.03, 9);
  expect(
    evaluateCreatureChart(required(moved.creature), cloth.chart, [0, 0, 0]).position.every(Number.isFinite),
  ).toBe(true);
  expect(
    validateProject(JSON.parse(session.export())).diagnostics.filter((entry) => entry.severity === "error"),
  ).toEqual([]);
  expect(() =>
    authorCreatureMantle(result, {
      id: "review-wrap",
      region: "shoulder",
      material: "marsh-linen",
      shoulderWidth: 0.68,
      backDepth: 0.24,
      length: 1.1,
      flare: 0.06,
    }),
  ).toThrow();
  session.undo();
  expect(
    (session.inspect(character.id) as CharacterDefinition).creature?.landmarks.find(
      (entry) => entry.id === landmark.id,
    )?.position,
  ).toEqual(landmark.position);
});

for (const variant of CREATURE_FITTING_VARIANTS)
  test(`${variant} held-out construction compiles complete source with bounded physical panel geometry`, () => {
    const fixture = createCreatureFittingLookdev(variant);
    const character = required(
      fixture.project.documents.find(
        (document): document is CharacterDefinition => document.kind === "character",
      ),
    );
    expect(
      validateProject(fixture.project).diagnostics.filter((entry) => entry.severity === "error"),
    ).toEqual([]);
    const artifact = compileCharacter(character, "interactive");
    expect(artifact.diagnostics.filter((entry) => entry.severity === "error")).toEqual([]);
    expect(artifact.mesh.positions.every(Number.isFinite)).toBe(true);
    expect(artifact.mesh.indices.length / 3).toBeLessThan(160000);
    const panels = required(character.creature).cloth.filter((cloth) =>
      cloth.id.startsWith("pilgrim-mantle"),
    );
    expect(panels).toHaveLength(1);
    expect(panels.every((panel) => panel.fittingLandmarks?.length === 4)).toBe(true);
  });

test("biped weight transfer retains planted feet while hips alternate over support", async () => {
  const character = applyBipedWalk(applyBipedAnatomy(biped().character));
  const walk = required(character.motions.find((clip) => clip.id === "walk"));
  expect(walk.keys.filter((key) => key.joint === "root").some((key) => key.translation[0] < -0.05)).toBe(
    true,
  );
  expect(walk.keys.filter((key) => key.joint === "root").some((key) => key.translation[0] > 0.05)).toBe(true);
  expect(walk.keys.length).toBeLessThan(2048);
  const report = await auditCreatureMotion(character, compileCharacter(character, "interactive"), "walk");
  expect(report.plantedContacts.length).toBeGreaterThanOrEqual(3);
  expect(report.maximumPlantedSlip).toBeLessThan(0.01);
  expect(report.maximumContactResidual).toBeLessThan(0.015);
}, 30000);

test("continuous fitted mantle retains pins and bounded strain during the actual supported walk", async () => {
  const character = required(
    createCreatureFittingLookdev().project.documents.find(
      (document): document is CharacterDefinition => document.kind === "character",
    ),
  );
  const runtime = await RuntimeSession.create();
  try {
    runtime.physics.addGround();
    runtime.addCharacter(character.id, compileCharacter(character, "interactive"), character);
    runtime.playMotion(character.id, "walk", 0);
    for (let tick = 0; tick < 150; tick++) {
      runtime.advance(1 / 60);
      const cloth = required(
        runtime.creatureDiagnostics(character.id).find((item) => item.id === "pilgrim-mantle"),
      );
      expect(cloth.residual).toBeLessThan(0.002);
      expect(cloth.status).toBe("satisfied");
    }
    const state = runtime.checkpoint().instances[0].clothState;
    const mantle = required(state?.panels.find((panel) => panel.id === "pilgrim-mantle"));
    expect(mantle.positions.flat().every(Number.isFinite)).toBe(true);
    expect(mantle.positions.length).toBe(289);
  } finally {
    runtime.dispose();
  }
}, 30000);
