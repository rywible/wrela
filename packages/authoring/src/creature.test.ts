import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { type CharacterDefinition, creatureSchema } from "@wrela/model";
import type { EditBatch } from "./commands";
import { AuthoringSession, RevisionConflict } from "./session";

function fixture() {
  const project = referenceProject(),
    character = project.documents.find((d) => d.id === "polar-bunny") as CharacterDefinition;
  const root = character.joints[0].id;
  character.creature = creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      {
        id: "shoulder",
        name: "Shoulder",
        nodeIds: ["body"],
        jointIds: [root],
        frame: { position: [0, 0.9, 0], rotation: [0, 0, 0] },
        extent: [0.52, 0.68, 0.43],
      },
      {
        id: "head-region",
        name: "Head",
        nodeIds: ["head"],
        jointIds: ["head-joint"],
        frame: { position: [0, 1.68, 0.12], rotation: [0, 0, 0] },
        extent: [0.4, 0.4, 0.37],
      },
    ],
    charts: [
      {
        id: "shoulder-sweep",
        region: "shoulder",
        kind: "sweep",
        revision: 0,
        points: [
          [0, -0.5, 0],
          [0, 0.5, 0],
        ],
        radii: [0.3, 0.3],
      },
    ],
    anchors: [
      {
        id: "scar",
        region: "shoulder",
        chart: "shoulder-sweep",
        chartRevision: 0,
        coordinates: [0.5, 0.1, 1],
        offset: 0.01,
        purpose: "scar",
        tolerance: 0.02,
      },
      {
        id: "armor",
        region: "shoulder",
        chart: "shoulder-sweep",
        chartRevision: 0,
        coordinates: [0.7, 0.4, 1],
        offset: 0.08,
        purpose: "attachment",
        tolerance: 0.02,
      },
    ],
    grooms: [
      {
        id: "mane",
        region: "shoulder",
        chart: "shoulder-sweep",
        chartRevision: 0,
        seed: 3,
        density: 20,
        length: 0.2,
        width: 0.03,
        direction: [0, 1, 0],
        taper: 0.8,
        clump: 0.3,
        curl: 0,
        rootColor: [0.2, 0.1, 0.1],
        tipColor: [0.4, 0.2, 0.1],
        maxCards: 100,
      },
    ],
    appearance: [
      {
        id: "scar-tissue",
        region: "shoulder",
        family: "skin",
        color: [0.3, 0.2, 0.2],
        roughness: 0.5,
        mask: { center: [0.1, 0, 0], radius: 0.1, falloff: 2 },
      },
    ],
  });
  return new AuthoringSession(project);
}
function grow(session: AuthoringSession, scale: [number, number, number] = [1.3, 1, 1]): EditBatch {
  return {
    expectedRevision: session.getSnapshot().revision,
    transactionId: "shoulder-growth",
    actor: "anatomy-agent",
    intent: "Widen shoulder and preserve head design",
    operations: [
      {
        kind: "creature.proportion",
        target: "polar-bunny",
        region: "shoulder",
        scale,
        preserve: ["head-region"],
      },
    ],
  };
}
test("a public anatomical edit coherently updates geometry, coat and scar correspondence and supports undo/retry", () => {
  const session = fixture(),
    before = session.inspectCreature("polar-bunny"),
    batch = grow(session);
  const head = structuredClone(before.regions[1]);
  const result = session.apply(batch),
    after = session.inspectCreature("polar-bunny");
  expect(after.regions[0].extent[0]).toBeCloseTo(0.52 * 1.3);
  expect(after.nodes.find((n) => n.id === "body")?.size[0]).toBeCloseTo(0.52 * 1.3);
  expect(after.regions[1]).toEqual(head);
  const chart = after.charts[0];
  expect(chart.kind).toBe("sweep");
  if (chart.kind === "sweep") expect(chart.crossSections?.[0]).toEqual([0.39, 0.3]);
  expect(after.anchors).toEqual(before.anchors);
  expect(after.grooms).toEqual(before.grooms);
  expect(after.appearance[0].mask?.center[0]).toBeCloseTo(0.13);
  expect(session.apply(batch)).toEqual(result);
  expect(session.getSnapshot().revision).toBe(1);
  session.undo();
  expect(session.inspectCreature("polar-bunny").regions).toEqual(before.regions);
  session.redo();
  expect(session.inspectCreature("polar-bunny").regions).toEqual(after.regions);
  expect(session.inspectTransaction(result.transactionId)?.actor).toBe("anatomy-agent");
});
test("candidate alternatives remain isolated, compare on a shared baseline, notify clients and reject stale adoption", () => {
  const session = fixture(),
    snapshot = session.getSnapshot();
  const narrow = { ...grow(session, [1.1, 1, 1]), transactionId: "narrow" };
  session.proposeCandidate({ id: "narrow", batch: narrow });
  expect(session.getSnapshot()).not.toBe(snapshot);
  expect(session.getSnapshot().revision).toBe(0);
  session.proposeCandidate({ id: "wide", batch: grow(session) });
  expect(session.compareCandidates("narrow", "wide").comparisons[0].equal).toBe(false);
  const result = session.adoptCandidate("narrow");
  expect(session.adoptCandidate("narrow")).toEqual(result);
  expect(() => session.adoptCandidate("wide")).toThrow(RevisionConflict);
  expect(session.inspectCandidate("wide")?.status).toBe("proposed");
  session.cancelCandidate("wide");
  expect(() => session.adoptCandidate("wide")).toThrow("cancelled");
  session.revert(result.transactionId);
  expect(session.inspectCreature("polar-bunny").regions[0].extent[0]).toBe(0.52);
});
test("candidate dependency preconditions permit unrelated edits and protect material dependencies", () => {
  const session = fixture(),
    before = session.getSnapshot();
  const character = session.inspect("polar-bunny") as CharacterDefinition;
  const batch: EditBatch = {
    ...grow(session),
    expectedRevision: undefined,
    preconditions: {
      writes: { "polar-bunny": 0 },
      reads: Object.fromEntries(
        [character.material, ...character.field.nodes.flatMap((n) => (n.material ? [n.material] : []))].map(
          (id) => [id, before.documentRevisions[id]],
        ),
      ),
    },
  };
  session.proposeCandidate({ id: "independent", batch });
  session.apply({
    expectedRevision: 0,
    operations: [{ kind: "document.rename", target: "winter-sky", name: "Unrelated sky" }],
  });
  expect(session.adoptCandidate("independent").revision).toBe(2);
});
test("hard preservation constraints, dependent deletion and unrepresentable shear fail atomically", () => {
  const session = fixture(),
    before = session.export();
  expect(() =>
    session.apply({
      ...grow(session),
      operations: [
        {
          kind: "creature.proportion",
          target: "polar-bunny",
          region: "shoulder",
          scale: [1.2, 1, 1],
          preserve: ["shoulder"],
        },
      ],
    }),
  ).toThrow("Hard preservation");
  expect(() =>
    session.apply({
      expectedRevision: 0,
      operations: [{ kind: "creature.remove", target: "polar-bunny", domain: "regions", id: "shoulder" }],
    }),
  ).toThrow("Missing");
  const chart = session.inspectCreature("polar-bunny").charts[0];
  if (chart.kind !== "sweep") throw Error("fixture");
  session.apply({
    expectedRevision: 0,
    operations: [
      {
        kind: "creature.chart",
        target: "polar-bunny",
        value: {
          ...chart,
          points: [
            [0, 0, 0],
            [0.2, 1, 0],
          ],
        },
        correspondence: "preserve",
      },
    ],
  });
  const curved = session.export();
  expect(() => session.apply(grow(session))).toThrow("shear");
  expect(session.export()).toBe(curved);
  session.undo();
  expect(session.export()).toBe(before);
});
test("chart replacement requires declared correspondence and repairs cannot silently move details", () => {
  const session = fixture(),
    chart = session.inspectCreature("polar-bunny").charts[0];
  if (chart.kind !== "sweep") throw Error("fixture");
  const reverse = { ...chart, points: [...chart.points].reverse() };
  expect(() =>
    session.apply({
      expectedRevision: 0,
      operations: [{ kind: "creature.chart", target: "polar-bunny", value: reverse }],
    }),
  ).toThrow("correspondence");
  expect(() =>
    session.apply({
      expectedRevision: 0,
      operations: [
        { kind: "creature.chart", target: "polar-bunny", value: reverse, correspondence: "preserve" },
      ],
    }),
  ).toThrow("order");
  expect(() =>
    session.apply({
      expectedRevision: 0,
      operations: [
        {
          kind: "creature.chart",
          target: "polar-bunny",
          value: { ...reverse, revision: 1 },
          correspondence: "repair",
        },
      ],
    }),
  ).toThrow("stale");
  expect(session.getSnapshot().revision).toBe(0);
});
test("source solver reaches a width target, reports residuals, retains hard constraints and enforces work limits", () => {
  const session = fixture();
  const result = session.solveCreature({
    id: "solved-shoulder",
    target: "polar-bunny",
    expectedRevision: 0,
    controls: [{ kind: "regionScale", region: "shoulder", axis: 0, minimum: 0.8, maximum: 1.6 }],
    objectives: [{ kind: "regionExtent", region: "shoulder", axis: 0, value: 0.7, tolerance: 0.00001 }],
    preserve: ["head-region"],
    budget: { evaluations: 20, iterations: 5 },
  });
  expect(result.status).toBe("converged");
  expect(result.evaluations).toBeLessThanOrEqual(20);
  expect(result.residuals[0].satisfied).toBe(true);
  expect(session.getSnapshot().revision).toBe(0);
  session.adoptCandidate("solved-shoulder");
  expect(session.inspectCreature("polar-bunny").regions[0].extent[0]).toBeCloseTo(0.7);
  const impossible = session.solveCreature({
    id: "limited",
    target: "polar-bunny",
    expectedRevision: 1,
    controls: [{ kind: "regionScale", region: "shoulder", axis: 0, minimum: 0.99, maximum: 1.01 }],
    objectives: [{ kind: "regionExtent", region: "shoulder", axis: 0, value: 10, tolerance: 0.001 }],
    budget: { evaluations: 4, iterations: 2 },
  });
  expect(impossible.status).not.toBe("converged");
  expect(impossible.evaluations).toBeLessThanOrEqual(4);
  expect(impossible.residuals[0].satisfied).toBe(false);
});
test("inspection grounds approximate selections and explains source rather than invented sensitivities", () => {
  const session = fixture();
  expect(session.selectCreature("polar-bunny", [0, 0.9, 0]).selections[0]).toMatchObject({
    region: "shoulder",
    exactSurface: false,
  });
  expect(session.explainCreature("polar-bunny", "shoulder")).toMatchObject({
    measuredSensitivity: false,
    geometry: ["body", "shoulder-sweep"],
    groom: [{ id: "mane", chart: "shoulder-sweep", maxCards: 100 }],
  });
  expect(() => session.selectCreature("polar-bunny", [NaN, 0, 0])).toThrow("finite");
  const cancelled = new AbortController();
  cancelled.abort();
  expect(() =>
    session.solveCreature(
      {
        id: "cancelled-solve",
        target: "polar-bunny",
        expectedRevision: 0,
        controls: [{ kind: "regionScale", region: "shoulder", axis: 0, minimum: 0.9, maximum: 1.2 }],
        objectives: [{ kind: "regionExtent", region: "shoulder", axis: 0, value: 0.6, tolerance: 0.001 }],
      },
      cancelled.signal,
    ),
  ).toThrow("cancelled");
  expect(session.listCandidates()).toHaveLength(0);
});

test("mounted components and anchored scars retain authored dimensions and offsets through a proportion edit", () => {
  const session = fixture();
  const before = session.inspectCreature("polar-bunny");
  session.apply({
    expectedRevision: 0,
    operations: [
      {
        kind: "creature.region",
        target: "polar-bunny",
        value: { ...before.regions[0], nodeIds: ["body", "nose"] },
      },
      {
        kind: "creature.attachment",
        target: "polar-bunny",
        value: {
          id: "armor-piece",
          anchor: "armor",
          nodeIds: ["nose"],
          offset: [0, 0.01, 0],
          minimumClearance: 0.02,
        },
      },
      {
        kind: "creature.appearance",
        target: "polar-bunny",
        value: { ...before.appearance[0], anchor: "scar" },
      },
    ],
  });
  const mounted = session.inspectCreature("polar-bunny");
  const nose = mounted.nodes.find((node) => node.id === "nose");
  session.apply(grow(session));
  const changed = session.inspectCreature("polar-bunny");
  expect(changed.nodes.find((node) => node.id === "nose")).toEqual(nose);
  expect(changed.attachments).toEqual(mounted.attachments);
  expect(changed.appearance[0].mask).toEqual(mounted.appearance[0].mask);
  expect(changed.anchors).toEqual(mounted.anchors);
  expect(changed.charts).not.toEqual(mounted.charts);
});
test("an identity edit has no source effect and candidate adoption cannot be cancelled by a reentrant observer", () => {
  const session = fixture(),
    before = session.export();
  session.apply(grow(session, [1, 1, 1]));
  expect(session.export()).toBe(before);
  expect(session.getSnapshot().revision).toBe(0);
  session.proposeCandidate({
    id: "observed",
    batch: { ...grow(session), transactionId: "observed-transaction" },
  });
  let blocked = false;
  const unsubscribe = session.subscribe(() => {
    if (session.getSnapshot().revision === 1 && session.inspectCandidate("observed")?.status === "proposed") {
      try {
        session.cancelCandidate("observed");
      } catch {
        blocked = true;
      }
    }
  });
  session.adoptCandidate("observed");
  unsubscribe();
  expect(blocked).toBe(true);
  expect(session.inspectCandidate("observed")?.status).toBe("adopted");
});

test("articulated physics source can be authored and cleared atomically with missing joint protection", () => {
  const session = fixture();
  const root = session.inspectCreature("polar-bunny").joints[0].id;
  const articulation = { bodies: [{ joint: root, mass: 12, radius: 0.2 }], joints: [] };
  session.apply({
    expectedRevision: 0,
    operations: [{ kind: "creature.articulation", target: "polar-bunny", value: articulation }],
  });
  expect(session.inspectCreature("polar-bunny").articulation).toEqual(articulation);
  expect(() =>
    session.apply({
      expectedRevision: 1,
      operations: [
        {
          kind: "creature.articulation",
          target: "polar-bunny",
          value: { bodies: [{ joint: "missing", mass: 12, radius: 0.2 }], joints: [] },
        },
      ],
    }),
  ).toThrow("Missing");
  expect(session.getSnapshot().revision).toBe(1);
  session.apply({
    expectedRevision: 1,
    operations: [{ kind: "creature.articulation", target: "polar-bunny", value: null }],
  });
  expect(session.inspectCreature("polar-bunny").articulation).toBeUndefined();
  session.undo();
  expect(session.inspectCreature("polar-bunny").articulation).toEqual(articulation);
});

test("bounded pelvis adaptation is editable and reversible without allowing missing joints", () => {
  const session = fixture(),
    joint = session.inspectCreature("polar-bunny").joints[0].id;
  const pelvis = { joint, maxOffset: [0.1, 0.2, 0.1] as [number, number, number], weight: 1, iterations: 3 };
  session.apply({
    expectedRevision: 0,
    operations: [{ kind: "creature.pelvis", target: "polar-bunny", value: pelvis }],
  });
  expect(session.inspectCreature("polar-bunny").pelvis).toEqual(pelvis);
  expect(() =>
    session.apply({
      expectedRevision: 1,
      operations: [
        { kind: "creature.pelvis", target: "polar-bunny", value: { ...pelvis, joint: "missing" } },
      ],
    }),
  ).toThrow("Missing");
  session.apply({
    expectedRevision: 1,
    operations: [{ kind: "creature.pelvis", target: "polar-bunny", value: null }],
  });
  expect(session.inspectCreature("polar-bunny").pelvis).toBeUndefined();
  session.undo();
  expect(session.inspectCreature("polar-bunny").pelvis).toEqual(pelvis);
});

test("hard preservation follows mounted components and cross-region cloth pins rather than comparing only source text", () => {
  const mounted = fixture(),
    head = mounted.inspectCreature("polar-bunny", "head-region").regions[0];
  mounted.apply({
    expectedRevision: 0,
    operations: [
      { kind: "creature.region", target: "polar-bunny", value: { ...head, nodeIds: ["head", "nose"] } },
      {
        kind: "creature.attachment",
        target: "polar-bunny",
        value: {
          id: "protected-mounted-part",
          anchor: "armor",
          nodeIds: ["nose"],
          offset: [0, 0, 0],
          minimumClearance: 0,
        },
      },
    ],
  });
  const before = mounted.export();
  expect(() => mounted.apply(grow(mounted))).toThrow("mounted component");
  expect(mounted.export()).toBe(before);
  const pinned = fixture();
  pinned.apply({
    expectedRevision: 0,
    operations: [
      {
        kind: "creature.chart",
        target: "polar-bunny",
        value: {
          id: "head-cloth",
          region: "head-region",
          kind: "patch",
          revision: 0,
          points: [
            [-0.2, 0, 0],
            [0.2, 0, 0],
            [-0.2, -0.4, 0],
            [0.2, -0.4, 0],
          ],
          thickness: 0.01,
        },
      },
      {
        kind: "creature.cloth",
        target: "polar-bunny",
        value: {
          id: "protected-cloth",
          region: "head-region",
          chart: "head-cloth",
          chartRevision: 0,
          pinEdges: [],
          pins: [{ coordinates: [0, 0], anchor: "armor" }],
          stiffness: 1,
          bendStiffness: 0.2,
          damping: 0.04,
          gravity: [0, -9.81, 0],
          wind: [0, 0, 0],
          iterations: 8,
          collisionRadius: 0.01,
          maxStretch: 1.1,
        },
      },
    ],
  });
  const unchanged = pinned.export();
  expect(() => pinned.apply(grow(pinned))).toThrow("pinned to edited anatomy");
  expect(pinned.export()).toBe(unchanged);
});

test("preservation allows unchanged axial ancestor transforms but rejects real ancestor movement and motion changes", () => {
  const session = fixture(),
    before = session.inspectCreature("polar-bunny"),
    root = before.joints.find((joint) => joint.id === "root");
  session.apply(grow(session));
  const after = session.inspectCreature("polar-bunny"),
    afterRoot = after.joints.find((joint) => joint.id === "root");
  expect(afterRoot?.position).toEqual(root?.position);
  expect(afterRoot?.rotation).toEqual(root?.rotation);
  const headJoint = before.joints.find((joint) => joint.id === "head-joint");
  if (!headJoint) throw Error("Missing head fixture");
  expect(session.inspectCreature("polar-bunny", "head-region").joints[0].position).toEqual(
    headJoint.position,
  );
  const unchanged = session.export();
  expect(() =>
    session.apply({
      expectedRevision: 1,
      operations: [
        {
          kind: "creature.move",
          target: "polar-bunny",
          region: "shoulder",
          translation: [0.1, 0, 0],
          preserve: ["head-region"],
        },
      ],
    }),
  ).toThrow("rig dependency");
  expect(session.export()).toBe(unchanged);
  const animated = fixture();
  const character = animated.inspect("polar-bunny") as CharacterDefinition;
  const motion = structuredClone(character.motions[0]);
  motion.keys[0] = { ...motion.keys[0], joint: "root", translation: [0.1, 0, 0] };
  animated.apply({
    expectedRevision: 0,
    operations: [{ kind: "document.set", target: "polar-bunny", path: ["motions", 0], value: motion }],
  });
  expect(() => animated.apply(grow(animated))).toThrow("rig dependency");
});

test("unchanged joint pivots do not bypass protected multi-joint binding envelope changes", () => {
  const session = fixture();
  session.apply({
    expectedRevision: 0,
    operations: [
      {
        kind: "creature.influence",
        target: "polar-bunny",
        value: {
          id: "head-blend",
          region: "head-region",
          allowedJoints: ["head-joint", "ear-l"],
          excludedJoints: [],
        },
      },
    ],
  });
  const before = session.export();
  expect(() => session.apply(grow(session))).toThrow("binding weights");
  expect(session.export()).toBe(before);
});
