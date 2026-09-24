import type { CharacterDefinition, FieldNode, Vec3 } from "@wrela/model";
import { type CreatureDefinition, creatureSculptSchema } from "@wrela/model";

/** Ordinary source revision developed from the clay atlas, with no render-only anatomy. */
export function refineWardenClay(character: CharacterDefinition) {
  if (character.id !== "ash-warden" || !character.creature) return;
  const c: CreatureDefinition = character.creature;
  const update = (id: string, changes: Partial<FieldNode>) => {
    const node = character.field.nodes.find((n) => n.id === id);
    if (!node) throw Error(`Missing clay study node ${id}`);
    Object.assign(node, changes);
  };
  update("skull", { position: [0, 2.06, 1.19], size: [0.285, 0.27, 0.4] });
  update("muzzle", { position: [0, 1.995, 1.565], size: [0.173, 0.113, 0.355] });
  update("jaw-form", { position: [0, 1.826, 1.505], size: [0.139, 0.064, 0.32] });
  update("nose", { position: [0, 2.003, 1.877], size: [0.111, 0.059, 0.065] });
  update("belly", { position: [0, 1.37, -0.29], size: [0.29, 0.255, 0.6] });
  update("throat-countershade", { size: [0.17, 0.22, 0.1] });
  // Jaw stays an independently bound anatomical surface, joined at its hinge.
  const anatomy = character.field.nodes.find((n) => n.id === "anatomy"),
    root = character.field.nodes.find((n) => n.id === character.field.root);
  if (anatomy && root) {
    anatomy.children = anatomy.children.filter((id) => id !== "jaw-form" && !id.startsWith("tail-"));
    root.children.push("jaw-form");
  }
  character.field.nodes = character.field.nodes.filter((n) => !["tail-base", "tail-tip"].includes(n.id));
  c.regions.push({
    id: "tail-region",
    name: "Tapered tail",
    nodeIds: [],
    jointIds: ["tail", "tail-tip"],
    frame: { position: [0, 1.25, -1.25], rotation: [0, 0, 0] },
    extent: [0.24, 0.7, 0.95],
    material: character.material,
  });
  c.charts.push({
    id: "tail-surface",
    kind: "sweep",
    region: "tail-region",
    revision: 1,
    points: [
      [0, 0, 0],
      [0, -0.12, -0.27],
      [0.035, -0.35, -0.53],
      [0.08, -0.52, -0.7],
      [0.12, -0.55, -0.84],
    ],
    radii: [0.17, 0.155, 0.12, 0.07, 0.009],
    caps: true,
    material: character.material,
  });
  c.influenceRules.push({
    id: "tail-binding",
    region: "tail-region",
    allowedJoints: ["tail", "tail-tip"],
    excludedJoints: [],
  });
  const head = c.regions.find((r) => r.id === "head-region");
  if (!head) throw Error("Missing head region");
  const local = (p: Vec3) => p.map((v, i) => v - head.frame.position[i]) as Vec3;
  const sculpt = (id: string, center: Vec3, displacement: Vec3, radii: Vec3, extra: object = {}) =>
    c.sculpts.push(
      creatureSculptSchema.parse({
        id,
        region: head.id,
        center: local(center),
        radius: Math.max(...radii),
        displacement,
        strength: 1,
        falloff: 2,
        mirror: true,
        support: { radii, rotation: [0, 0, 0] },
        detail: { maxEdgeLength: 0.025, passes: 4 },
        ...extra,
      }),
    );
  sculpt("cheek-plane", [0.235, 1.99, 1.31], [0.055, 0, 0], [0.16, 0.13, 0.21], {
    mode: "flatten",
    strength: 0.8,
    nodeIds: ["skull", "cheek-left", "cheek-right"],
  });
  sculpt("nasal-bridge", [0, 2.07, 1.55], [0, 0.019, 0], [0.052, 0.1, 0.08], {
    mirror: false,
    path: [
      [0, 2.105, 1.42],
      [0, 2.065, 1.65],
      [0, 2.04, 1.82],
    ].map((p) => local(p as Vec3)),
  });
  sculpt("nostril-recess", [0.067, 2.015, 1.927], [0, -0.006, -0.026], [0.03, 0.026, 0.044], {
    nodeIds: ["nose"],
    detail: { maxEdgeLength: 0.009, passes: 5 },
  });
  sculpt("orbital-recess", [0.266, 2.105, 1.42], [-0.039, 0, 0], [0.09, 0.049, 0.094], {
    nodeIds: ["skull", "cheek-left", "cheek-right", "brow-left", "brow-right"],
    detail: { maxEdgeLength: 0.015, passes: 4 },
  });
  sculpt("lip-crease", [0.15, 1.93, 1.6], [-0.025, -0.009, 0], [0.075, 0.028, 0.055], {
    path: [
      [0.18, 1.925, 1.38],
      [0.161, 1.928, 1.59],
      [0.12, 1.945, 1.79],
    ].map((p) => local(p as Vec3)),
  });
  for (const [side, sign] of [
    ["left", -1],
    ["right", 1],
  ] as const) {
    update(`cheek-${side}`, { position: [sign * 0.217, 1.99, 1.25], size: [0.105, 0.118, 0.18] });
    update(`brow-${side}`, { position: [sign * 0.235, 2.136, 1.387], size: [0.096, 0.023, 0.139] });
    update(`eye-${side}`, { position: [sign * 0.253, 2.103, 1.415], size: [0.034, 0.022, 0.044] });
    update(`fang-${side}`, { position: [sign * 0.118, 1.84, 1.685], size: [0.023, 0.056, 0.031] });
    for (const [edge, dy] of [
      ["upper", 0.018],
      ["lower", -0.02],
    ] as const) {
      c.charts.push({
        id: `eyelid-${side}-${edge}`,
        region: head.id,
        kind: "sweep",
        revision: 1,
        points: [
          [sign * 0.209, 2.106 + dy * 0.25, 1.454],
          [sign * 0.254, 2.105 + dy, 1.444],
          [sign * 0.285, 2.1 + dy * 0.1, 1.399],
        ].map((p) => local(p as Vec3)),
        radii: [0.003, edge === "upper" ? 0.009 : 0.006, 0.003],
        caps: true,
        material: "ashen-face",
      });
    }
  }
}
