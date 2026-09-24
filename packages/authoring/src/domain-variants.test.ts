import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import {
  botanicalPreset,
  type CharacterDefinition,
  createSurfaceAppearance,
  creatureSchema,
  defaultTerrainGeology,
  environmentSequenceSchema,
} from "@wrela/model";
import { createDoorwayAssembly } from "./assembly";
import { sourceValueChanges } from "./domain-constraints";
import { availableDomainRecipes, type DomainRecipe } from "./domain-recipes";
import { AuthoringSession, RevisionConflict } from "./session";

function fixture() {
  const project = referenceProject();
  const character = project.documents.find(
    (document): document is CharacterDefinition => document.kind === "character",
  );
  const vegetation = project.documents.find((document) => document.kind === "vegetation");
  const assembly = project.documents.find((document) => document.kind === "object");
  const terrain = project.documents.find((document) => document.kind === "terrain");
  const material = project.documents.find((document) => document.kind === "material");
  const environment = project.documents.find((document) => document.kind === "environment");
  const world = project.documents.find((document) => document.kind === "world");
  if (!character || !vegetation || !assembly || !terrain || !material || !environment || !world)
    throw Error("Missing fixture domain");
  vegetation.botanical = botanicalPreset("pine");
  assembly.assembly = createDoorwayAssembly();
  terrain.geology = defaultTerrainGeology();
  terrain.geology.corridors = [
    {
      id: "route",
      points: [
        [-5, 1, 0],
        [5, 1, 0],
      ],
      halfWidth: 1,
      shoulder: 2,
    },
  ];
  material.appearance = createSurfaceAppearance();
  environment.sequence = environmentSequenceSchema.parse({
    duration: 10,
    loop: false,
    keyframes: [0, 10].map((time) => ({
      time,
      state: {
        sunElevation: 0.5 + time / 100,
        sunAzimuth: 0,
        turbidity: 2,
        fogDensity: 0.01,
        wind: [1, 0, 0],
        ambient: 0.5,
        sunIntensity: 5,
        wetness: time / 10,
      },
    })),
  });
  character.creature = creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      {
        id: "body-region",
        name: "Body",
        nodeIds: character.field.nodes.filter((node) => !node.children.length).map((node) => node.id),
        jointIds: character.joints.map((joint) => joint.id),
        frame: { position: [0, 1, 0], rotation: [0, 0, 0] },
        extent: [0.5, 1, 0.3],
      },
    ],
    landmarks: [
      { id: "left", region: "body-region", position: [-0.5, 0, 0] },
      { id: "right", region: "body-region", position: [0.5, 0, 0] },
    ],
  });
  return { project, character, vegetation, assembly, terrain, material, environment, world };
}

test("all eight domains propose three ordinary alternatives, adopt, persist, undo and reject stale siblings", () => {
  const f = fixture();
  const groups: DomainRecipe[][] = [
    [0.8, 1.2, 1.4].map((distance) => ({
      kind: "creature.landmarkSpan",
      target: f.character.id,
      first: "left",
      second: "right",
      distance,
    })),
    [0.2, 0.5, 0.8].map((amount) => ({ kind: "assembly.weather", target: f.assembly.id, amount })),
    [0.2, 0.5, 0.8].map((density) => ({ kind: "vegetation.canopy", target: f.vegetation.id, density })),
    [0.2, 0.5, 0.8].map((strength) => ({ kind: "geology.erosion", target: f.terrain.id, strength })),
    [0.2, 0.5, 0.8].map((weathering) => ({ kind: "material.history", target: f.material.id, weathering })),
    [0.21, 0.51, 0.81].map((density) => ({
      kind: "world.population",
      target: f.world.id,
      rule: f.world.populations[0].id,
      density,
    })),
    [0.75, 1.25, 1.5].map((factor) => ({
      kind: "performance.retime",
      target: f.character.id,
      motion: f.character.motions[0].id,
      duration: f.character.motions[0].duration * factor,
    })),
    [-1, 0.5, 1.5].map((exposureCompensation) => ({
      kind: "environment.grade",
      target: f.environment.id,
      grade: { exposureCompensation, tint: [1, 1, 1] },
    })),
  ];
  for (const recipes of groups) {
    const session = new AuthoringSession(f.project),
      before = session.export();
    const result = session.proposeDomainVariants({
      id: "trial",
      expectedRevision: 0,
      variants: recipes.map((recipe, index) => ({ id: `v${index}`, recipe })),
    });
    expect(session.export()).toBe(before);
    expect(session.getSnapshot().revision).toBe(0);
    expect(result.variants).toHaveLength(3);
    expect(new Set(result.variants.map((variant) => variant.review.candidateKey)).size).toBe(3);
    for (const variant of result.variants) {
      expect(
        variant.constraints.every(
          (constraint) => constraint.passed && constraint.scope === "authored-source",
        ),
      ).toBe(true);
      expect(variant.sourceDiff.changes.length).toBeGreaterThan(0);
      expect(variant.sourceCost.beforeBytes).toBeGreaterThan(0);
      expect(variant.review.status).toBe("unreviewed");
      expect(variant.review.pendingMeasurements).toContain("editToCompleteFrameMs");
      expect(variant.candidate.batch.expectedRevision).toBe(0);
      expect(session.compareCandidates(variant.candidate.id).baseline).toBe(result.sourceKey);
    }
    session.adoptCandidate(result.variants[1].candidate.id);
    const saved = session.export();
    expect(saved).not.toBe(before);
    expect(new AuthoringSession(JSON.parse(saved)).export()).toBe(saved);
    expect(() => session.adoptCandidate(result.variants[0].candidate.id)).toThrow(RevisionConflict);
    session.undo();
    expect(session.export()).toBe(before);
    session.redo();
    expect(session.export()).toBe(saved);
  }
});

test("whole-variant preparation is atomic when a late preservation guard or recipe fails", () => {
  const { project, vegetation, assembly } = fixture();
  const session = new AuthoringSession(project),
    before = session.export();
  expect(() =>
    session.proposeDomainVariants({
      id: "protected",
      expectedRevision: 0,
      variants: [
        { id: "valid", recipe: { kind: "assembly.weather", target: assembly.id, amount: 0.5 } },
        {
          id: "blocked",
          recipe: { kind: "vegetation.canopy", target: vegetation.id, density: 0.3 },
          preserve: [{ target: vegetation.id, path: ["botanical", "canopy", "density"] }],
        },
      ],
    }),
  ).toThrow("protected source");
  expect(session.listCandidates()).toHaveLength(0);
  expect(session.export()).toBe(before);
  expect(() =>
    session.proposeDomainVariants({
      id: "invalid",
      expectedRevision: 0,
      variants: [
        { id: "valid", recipe: { kind: "assembly.weather", target: assembly.id, amount: 0.5 } },
        {
          id: "missing",
          recipe: { kind: "assembly.weather", target: assembly.id, amount: 0.5, parts: ["missing"] },
        },
      ],
    }),
  ).toThrow("Unknown assembly part");
  expect(session.listCandidates()).toHaveLength(0);
  expect(session.getSnapshot().canUndo).toBe(false);
});

test("generic candidate batches preflight late conflicts and do not leak early proposals", () => {
  const session = new AuthoringSession(referenceProject());
  const batch = {
    expectedRevision: 0,
    operations: [{ kind: "document.rename" as const, target: "winter-sky", name: "New sky" }],
  };
  session.proposeCandidate({ id: "reserved", batch });
  expect(() =>
    session.proposeCandidates([
      { id: "fresh", batch },
      { id: "reserved", batch: { ...batch, operations: [{ ...batch.operations[0], name: "Other" }] } },
    ]),
  ).toThrow("cannot be reused");
  expect(session.listCandidates().map((candidate) => candidate.id)).toEqual(["reserved"]);
});

test("unknown and unsafe protected paths reject; no-op recipes cannot inflate accepted-variant counts", () => {
  const { project, environment } = fixture();
  const session = new AuthoringSession(project);
  for (const path of [["absent"], ["__proto__"]])
    expect(() =>
      session.proposeDomainVariants({
        id: "guard",
        expectedRevision: 0,
        variants: [
          {
            id: "a",
            recipe: {
              kind: "environment.grade",
              target: environment.id,
              grade: { exposureCompensation: 1, tint: [1, 1, 1] },
            },
            preserve: [{ target: environment.id, path }],
          },
        ],
      }),
    ).toThrow();
  expect(() =>
    session.proposeDomainVariants({
      id: "noop",
      expectedRevision: 0,
      variants: [
        {
          id: "a",
          recipe: {
            kind: "environment.grade",
            target: environment.id,
            grade: { exposureCompensation: 0, tint: [1, 1, 1] },
          },
        },
      ],
    }),
  ).not.toThrow();
  session.adoptCandidate(session.listCandidates()[0].id);
  expect(() =>
    session.proposeDomainVariants({
      id: "noop-again",
      expectedRevision: 1,
      variants: [
        {
          id: "a",
          recipe: {
            kind: "environment.grade",
            target: environment.id,
            grade: { exposureCompensation: 0, tint: [1, 1, 1] },
          },
        },
      ],
    }),
  ).toThrow("does not change");
});

test("discovery exposes only usable recipe targets and detailed source diffs retain a bounded truncation flag", () => {
  const f = fixture(),
    session = new AuthoringSession(f.project);
  expect(session.discover().domainVariants.recipes).toHaveLength(8);
  expect(
    session.discover().domainVariants.targets.find((target) => target.id === f.terrain.id)?.recipes,
  ).toEqual(["geology.erosion"]);
  const after = {
    ...f.environment,
    sunElevation: 0.7,
    turbidity: 9,
    wind: [2, 1, 0] as [number, number, number],
  };
  const diff = sourceValueChanges(f.environment, after, 1);
  expect(diff.changes).toHaveLength(1);
  expect(diff.truncated).toBe(true);
});

test("valid long variant IDs remain bounded, retryable and distinct across ambiguous pairs", () => {
  const { project, vegetation } = fixture();
  const session = new AuthoringSession(project);
  const pairs = [
    ["a-b", "c"],
    ["a", "b-c"],
    ["r".repeat(100), "v".repeat(100)],
    [`${"r".repeat(99)}s`, "v".repeat(100)],
  ];
  const ids = pairs.map(([id, variant]) => {
    const request = {
      id,
      expectedRevision: 0,
      variants: [
        { id: variant, recipe: { kind: "vegetation.canopy" as const, target: vegetation.id, density: 0.3 } },
      ],
    };
    const proposed = session.proposeDomainVariants(request).variants[0].candidate;
    expect(proposed.id.length).toBeLessThanOrEqual(100);
    expect(proposed.batch.label?.length).toBeLessThanOrEqual(120);
    expect(session.proposeDomainVariants(request).variants[0].candidate.id).toBe(proposed.id);
    return proposed.id;
  });
  expect(new Set(ids).size).toBe(pairs.length);
  expect(session.listCandidates()).toHaveLength(pairs.length);
  session.adoptCandidate(ids[2]);
  expect(session.getSnapshot().revision).toBe(1);
});

test("landmark fit discovery requires a distinct, noncoincident pair in one anatomical region", () => {
  const { character } = fixture();
  if (!character.creature) throw Error("Missing creature source");
  const [left, right] = character.creature.landmarks;
  for (const landmarks of [
    [],
    [left],
    [left, { ...right, id: left.id }],
    [left, { ...right, region: "another-region" }],
    [left, { ...right, position: [...left.position] as [number, number, number] }],
    [left, { ...right, position: [left.position[0] + 1e-10, 0, 0] as [number, number, number] }],
  ]) {
    character.creature.landmarks = landmarks;
    expect(availableDomainRecipes(character)).not.toContain("creature.landmarkSpan");
    expect(availableDomainRecipes(character)).toContain("performance.retime");
  }
  character.creature.landmarks = [left, right];
  expect(availableDomainRecipes(character)).toContain("creature.landmarkSpan");
});
