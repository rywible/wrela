import type { CharacterDefinition, CreatureDefinition, FieldNode, Vec3 } from "@wrela/model";
import { z } from "zod";

/** Recipes operate on semantic biped parts, leaving rig pivots and live fitting
 * frames stable. Dimensions describe volume around those pivots, not rig scale. */
export const bipedAnatomyRecipeSchema = z.strictObject({
  build: z.number().finite().min(0.65).max(1.35).default(0.88),
  shoulderBreadth: z.number().finite().min(0.8).max(1.2).default(1),
  pelvisBreadth: z.number().finite().min(0.8).max(1.2).default(1),
  headScale: z.number().finite().min(0.75).max(1.1).default(0.84),
  crownReach: z.number().finite().min(0.5).max(1.3).default(0.85),
});
export type BipedAnatomyRecipe = z.input<typeof bipedAnatomyRecipeSchema>;

export function applyBipedAnatomy(source: CharacterDefinition, input: BipedAnatomyRecipe = {}) {
  const recipe = bipedAnatomyRecipeSchema.parse(input);
  const character = structuredClone(source),
    creature = character.creature;
  if (!creature) throw Error("Biped construction requires anatomical source");
  const required = ["pelvis", "abdomen", "ribcage", "neck", "skull", "jaw-form", "anatomy"];
  for (const id of required)
    if (!character.field.nodes.some((node) => node.id === id)) throw Error(`Biped construction needs ${id}`);
  for (const id of ["spine", "head", "hand-left", "hand-right", "leg-left-foot", "leg-right-foot"])
    if (!character.joints.some((joint) => joint.id === id))
      throw Error(`Biped construction needs joint ${id}`);
  const anatomy = character.field.nodes.find((node) => node.id === "anatomy");
  if (!anatomy) throw Error("Missing body composition");
  const set = (id: string, size: Vec3, position?: Vec3, rotation?: Vec3) => {
    const node = character.field.nodes.find((entry) => entry.id === id);
    if (!node) throw Error(`Missing biped part ${id}`);
    node.size = size;
    if (position) node.position = position;
    if (rotation) node.rotation = rotation;
  };
  const add = (id: string, position: Vec3, size: Vec3, region: string, rotation: Vec3 = [0, 0, 0]) => {
    const node: FieldNode = {
      id,
      name: id.replaceAll("-", " "),
      kind: "ellipsoid",
      position,
      size,
      rotation,
      radius: 0.1,
      blend: 0.04,
      children: [],
    };
    const existing = character.field.nodes.findIndex((entry) => entry.id === id);
    if (existing >= 0) character.field.nodes[existing] = node;
    else character.field.nodes.push(node);
    if (!anatomy.children.includes(id)) anatomy.children.push(id);
    const owner = creature.regions.find((entry) => entry.id === region);
    if (owner && !owner.nodeIds.includes(id)) owner.nodeIds.push(id);
  };
  // Remove literal rib beads, flat cloth placeholders and branch-shaped ellipsoids.
  // Fitting source and garment surfaces remain separate from the underlying body.
  const remove = new Set(
    character.field.nodes.filter((node) => /^(rib-|finger-|crown-)/.test(node.id)).map((node) => node.id),
  );
  character.field.nodes = character.field.nodes.filter((node) => !remove.has(node.id));
  for (const node of character.field.nodes) node.children = node.children.filter((id) => !remove.has(id));
  for (const region of creature.regions) region.nodeIds = region.nodeIds.filter((id) => !remove.has(id));
  anatomy.blend = 0.055;
  const b = recipe.build,
    s = recipe.shoulderBreadth,
    p = recipe.pelvisBreadth,
    h = recipe.headScale;
  set("pelvis", [0.225 * p, 0.19, 0.155 * b], [0, 1.3, -0.005]);
  set("abdomen", [0.155 * b, 0.27, 0.125 * b], [0.014, 1.58, 0.008]);
  set("ribcage", [0.258 * s, 0.305, 0.167 * b], [0, 1.96, -0.005], [-0.07, 0, 0]);
  set("neck", [0.084 * b, 0.235, 0.096 * b], [0.06, 2.32, 0.005], [0.1, 0, -0.12]);
  set("skull", [0.18 * h, 0.245 * h, 0.17 * h], [0.105, 2.635, 0.045]);
  set("jaw-form", [0.116 * h, 0.108 * h, 0.12 * h], [0.105, 2.48, 0.096]);
  set("nose", [0.025, 0.067, 0.052], [0.105, 2.625, 0.207]);
  // Existing shoulder chart is a correspondence scaffold; showing it added a
  // second round torso over the field and hid all of the authored waist contour.
  for (const chart of creature.charts)
    if (["shoulder-surface", "vow-mount"].includes(chart.id)) chart.realization = "correspondence-only";
  const shoulder = creature.regions.find((region) => region.id === "shoulder");
  if (!shoulder) throw Error("Missing shoulder anatomy");
  for (const id of ["pelvis", "abdomen", "neck"])
    if (!shoulder.nodeIds.includes(id)) shoulder.nodeIds.push(id);
  shoulder.extent = [0.31 * s, 0.38, 0.19 * b];
  creature.appearance = creature.appearance.filter((layer) => layer.id !== "scar-response");
  const tissue = creature.appearance.find((layer) => layer.id === "shoulder-tissue");
  if (tissue)
    Object.assign(tissue, { color: [0.29, 0.255, 0.205], variation: 0.035, scale: 6, displacement: 0.0003 });
  // Build each articulated volume around fixed anatomical pivots. Deltoid and
  // trapezius overlap at the clavicle; taper occurs toward elbows and wrists.
  for (const [side, sign] of [
    ["left", -1],
    ["right", 1],
  ] as const) {
    const drop = side === "left" ? 0.1 : 0;
    const armRegion = `arm-${side}-anatomy`;
    if (!creature.regions.some((region) => region.id === armRegion)) {
      creature.regions.push({
        id: armRegion,
        name: `${side} arm anatomy`,
        parent: "shoulder",
        nodeIds: [],
        jointIds: [`arm-${side}-upper`, `arm-${side}-lower`, `hand-${side}`],
        frame: { position: [sign * 0.31, 2.19 - drop, 0], rotation: [0, 0, 0] },
        extent: [0.23, 0.7, 0.2],
      });
      creature.influenceRules.push({
        id: `${armRegion}-binding`,
        region: armRegion,
        allowedJoints: [`arm-${side}-upper`, `arm-${side}-lower`, `hand-${side}`],
        excludedJoints: [],
      });
    }
    const owner = creature.regions.find((region) => region.id === armRegion);
    if (!owner) throw Error("Missing constructed arm region");
    owner.nodeIds = [`arm-${side}-mass`, `arm-${side}-forearm`, `hand-${side}`];
    set(
      `arm-${side}-mass`,
      [0.085 * b, 0.32, 0.105 * b],
      [sign * 0.4, 1.88 - drop, 0.012],
      [0.04, 0, sign * 0.15],
    );
    set(
      `arm-${side}-forearm`,
      [0.066 * b, 0.292, 0.076 * b],
      [sign * 0.505, 1.33 - drop, 0.067],
      [-0.055, 0, sign * 0.025],
    );
    set(`hand-${side}`, [0.069, 0.135, 0.038], [sign * 0.52, 0.997 - drop, 0.1]);
    set(
      `leg-${side}-thigh`,
      [0.123 * b * p, 0.32, 0.145 * b],
      [sign * 0.165, 1.006, 0.005],
      [-0.04, 0, -sign * 0.035],
    );
    set(`leg-${side}-shin`, [0.061 * b, 0.29, 0.075 * b], [sign * 0.185, 0.43, 0.028]);
    set(`leg-${side}-foot-form`, [0.093 * p, 0.076, 0.185], [sign * 0.19, 0.093, 0.12]);
    add(
      `deltoid-${side}`,
      [sign * 0.292, 2.115 - drop * 0.55, -0.007],
      [0.119 * s, 0.154, 0.124 * b],
      armRegion,
      [0, 0, sign * 0.32],
    );
    add(
      `trapezius-${side}`,
      [sign * 0.154, 2.204 - drop * 0.2, -0.043],
      [0.153 * s, 0.075, 0.106 * b],
      "shoulder",
      [0, 0, -sign * 0.27],
    );
    add(
      `pectoral-${side}`,
      [sign * 0.13, 2.037 - drop * 0.15, 0.12 * b],
      [0.14 * s, 0.095, 0.066 * b],
      "shoulder",
      [0, 0, sign * 0.17],
    );
    add(
      `calf-${side}`,
      [sign * 0.18, 0.51, -0.012],
      [0.077 * b, 0.172, 0.091 * b],
      `leg-${side}`,
      [0.08, 0, 0],
    );
    add(`knee-${side}`, [sign * 0.18, 0.692, 0.06], [0.071 * b, 0.085, 0.075], `leg-${side}`);
    add(`elbow-${side}`, [sign * 0.49, 1.59 - drop, 0.012], [0.066 * b, 0.078, 0.069], armRegion);
    add(`cheek-${side}`, [0.105 + sign * 0.091, 2.57, 0.129], [0.071 * h, 0.059 * h, 0.055], "head-region", [
      0,
      0,
      sign * 0.2,
    ]);
    set(`eye-${side}`, [0.021, 0.013, 0.018], [0.105 + sign * 0.071, 2.674, 0.177]);
    add(`brow-${side}`, [0.105 + sign * 0.067, 2.697, 0.174], [0.059, 0.017, 0.027], "head-region", [
      0,
      0,
      -sign * 0.08,
    ]);
    add(`chin-${side}`, [0.105 + sign * 0.029, 2.457, 0.154], [0.051, 0.034, 0.026], "jaw-region");
    buildHand(creature, side, sign, drop, b);
  }
  buildCrown(creature, recipe.crownReach);
  // A single tall gameplay capsule intersected a fitted neck opening and forced
  // cloth away from its own pins. These bounded proxies follow the authored body
  // volumes, preserving collar clearance without disabling physical collision.
  character.physics.colliders = [
    { id: "pelvis-proxy", shape: "sphere", position: [0, 1.3, 0], rotation: [0, 0, 0], radius: 0.172 * b },
    {
      id: "abdomen-proxy",
      shape: "capsule",
      position: [0.01, 1.63, 0],
      rotation: [0, 0, 0],
      radius: 0.125 * b,
      halfHeight: 0.2,
    },
    {
      id: "ribcage-proxy",
      shape: "capsule",
      position: [0, 1.98, -0.005],
      rotation: [0, 0, 0],
      radius: 0.162 * b,
      halfHeight: 0.13,
    },
    {
      id: "neck-proxy",
      shape: "capsule",
      position: [0.06, 2.32, 0.005],
      rotation: [0, 0, -0.12],
      radius: 0.073 * b,
      halfHeight: 0.1,
    },
    {
      id: "head-proxy",
      shape: "sphere",
      position: [0.105, 2.635, 0.045],
      rotation: [0, 0, 0],
      radius: 0.17 * h,
    },
  ];
  character.field.resolution = 80;
  if (character.field.nodes.length > 128 || anatomy.children.length > 64)
    throw Error("Anatomy recipe exceeds field budget");
  return character;
}

function replaceChart(creature: CreatureDefinition, chart: CreatureDefinition["charts"][number]) {
  const index = creature.charts.findIndex((entry) => entry.id === chart.id);
  if (index < 0) creature.charts.push(chart);
  else creature.charts[index] = chart;
}
function buildHand(creature: CreatureDefinition, side: string, sign: number, drop: number, build: number) {
  const region = `hand-${side}-anatomy`;
  if (!creature.regions.some((entry) => entry.id === region)) {
    creature.regions.push({
      id: region,
      name: `${side} hand`,
      parent: `arm-${side}-anatomy`,
      nodeIds: [],
      jointIds: [`hand-${side}`],
      frame: { position: [sign * 0.52, 1.1 - drop, 0.1], rotation: [0, 0, 0] },
      extent: [0.14, 0.34, 0.08],
    });
    creature.influenceRules.push({
      id: `${region}-binding`,
      region,
      allowedJoints: [`hand-${side}`],
      excludedJoints: [],
      rigidJoint: `hand-${side}`,
    });
  }
  for (let digit = 0; digit < 4; digit++) {
    const x = sign * (-0.047 + digit * 0.031),
      length = [0.145, 0.174, 0.166, 0.126][digit];
    replaceChart(creature, {
      id: `${region}-digit-${digit}`,
      kind: "sweep",
      region,
      revision: 1,
      points: [
        [x, -0.18, 0],
        [x + sign * 0.002, -0.18 - length * 0.52, 0.012],
        [x, -0.18 - length, 0.031],
      ],
      radii: [0.018 * build, 0.014 * build, 0.007],
      caps: true,
    });
  }
  replaceChart(creature, {
    id: `${region}-thumb`,
    kind: "sweep",
    region,
    revision: 1,
    points: [
      [-sign * 0.045, -0.072, 0.012],
      [-sign * 0.089, -0.126, 0.042],
      [-sign * 0.087, -0.182, 0.052],
    ],
    radii: [0.025 * build, 0.018 * build, 0.008],
    caps: true,
  });
}
function buildCrown(creature: CreatureDefinition, reach: number) {
  const region = creature.regions.find((entry) => entry.id === "head-region");
  if (!region) throw Error("Missing crown fitting region");
  const local = (points: Vec3[]) =>
    points.map((point) => point.map((value, axis) => value - region.frame.position[axis]) as Vec3);
  for (const [id, points, radii] of [
    [
      "crown-left",
      [
        [-0.018, 2.775, -0.032],
        [-0.084, 2.96, -0.032],
        [-0.185, 3.105, -0.008],
        [-0.208, 3.29, 0.008],
      ],
      [0.052, 0.044, 0.025, 0.002],
    ],
    [
      "crown-fork",
      [
        [-0.095, 2.971, -0.03],
        [-0.245, 3.033, 0.008],
        [-0.32, 3.151, 0.035],
      ],
      [0.029, 0.021, 0.002],
    ],
    [
      "crown-right",
      [
        [0.213, 2.773, -0.026],
        [0.29, 2.92, -0.04],
        [0.33, 3.067, -0.005],
      ],
      [0.043, 0.029, 0.006],
    ],
  ] as [string, Vec3[], number[]][]) {
    const scaled = points.map((point) => [point[0], 2.775 + (point[1] - 2.775) * reach, point[2]] as Vec3);
    replaceChart(creature, {
      id: `${id}-growth`,
      kind: "sweep",
      region: region.id,
      revision: 1,
      points: local(scaled),
      radii,
      caps: true,
      material: "peat-antler",
    });
  }
}
