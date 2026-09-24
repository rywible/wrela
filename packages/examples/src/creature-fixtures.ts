import type {
  CharacterDefinition,
  Document,
  FieldNode,
  Joint,
  MaterialDefinition,
  Motion,
  Project,
  RecipeDefinition,
  Vec3,
} from "@wrela/model";
import { type CreatureDefinition, creatureSchema } from "@wrela/model";
import { refineWardenClay } from "./creature-clay-study";

export type CreatureFixtureId = "ash-warden" | "reed-penitent";
export type CreatureFixtureReviewScenario = {
  id: string;
  purpose: string;
  motion: string;
  samples: number[];
  camera: { position: Vec3; target: Vec3; fov: number };
  light: "neutral" | "raking" | "backlight";
  maximumContactError: number;
  maximumAnchorResidual: number;
  requiresVisualReview: true;
};
export type CreatureFixture = {
  project: Project;
  characterId: CreatureFixtureId;
  stageId: string;
  brief: string;
  protectedFeatures: string[];
  reviewScenarios: CreatureFixtureReviewScenario[];
  encounter: {
    sequence: { motion: string; repeat: number; intent: string }[];
    arenaRadius: number;
    playerStart: Vec3;
    attackWindows: { motion: string; start: number; end: number; reach: number }[];
  };
};
export const creatureFixtureCatalog: readonly { id: CreatureFixtureId; name: string; description: string }[] =
  [
    {
      id: "ash-warden",
      name: "Ash Warden",
      description:
        "A charcoal guardian with a broken ceremonial collar, amber eyes, exposed scar tissue, and a windswept silver mane.",
    },
    {
      id: "reed-penitent",
      name: "Reed Penitent",
      description:
        "A gaunt marsh pilgrim with an off-center antler crown, long grasping arms, and split weathered cloth panels.",
    },
  ];
const base = (id: string, name: string) => ({
  id,
  name,
  schemaVersion: 1 as const,
  dependencies: [] as string[],
});
const node = (
  id: string,
  position: Vec3,
  size: Vec3,
  material?: string,
  rotation: Vec3 = [0, 0, 0],
): FieldNode => ({
  id,
  name: id.replaceAll("-", " "),
  kind: "ellipsoid",
  position,
  size,
  rotation,
  radius: 0.1,
  blend: 0.06,
  children: [],
  ...(material ? { material } : {}),
});
const joint = (
  id: string,
  parent: string | null,
  position: Vec3,
  radius: number,
  minimum = -1.4,
  maximum = 1.4,
): Joint => ({
  id,
  name: id.replaceAll("-", " "),
  parent,
  position,
  radius,
  rotation: [0, 0, 0],
  minimum,
  maximum,
});
const key = (jointId: string, time: number, rotation: Vec3 = [0, 0, 0], translation: Vec3 = [0, 0, 0]) => ({
  joint: jointId,
  time,
  rotation,
  translation,
});
function material(
  id: string,
  color: Vec3,
  secondary: Vec3,
  roughness: number,
  metallic = 0,
  pattern: MaterialDefinition["pattern"] = "noise",
  scale = 24,
): MaterialDefinition {
  return {
    ...base(id, id.replaceAll("-", " ")),
    kind: "material",
    color,
    secondary,
    roughness,
    metallic,
    pattern,
    scale,
    normalStrength: pattern === "solid" ? 0 : 0.18,
    domain: "local",
  };
}
function compose(
  body: FieldNode[],
  details: FieldNode[],
  bounds: CharacterDefinition["field"]["bounds"],
): CharacterDefinition["field"] {
  return {
    root: "creature",
    bounds,
    resolution: 48,
    nodes: [
      {
        ...node("creature", [0, 0, 0], [1, 1, 1]),
        kind: "union",
        children: ["anatomy", ...details.map((n) => n.id)],
      },
      {
        ...node("anatomy", [0, 0, 0], [1, 1, 1]),
        kind: "smoothUnion",
        blend: 0.07,
        children: body.map((n) => n.id),
      },
      ...body,
      ...details,
    ],
  };
}
function performance(quadruped: boolean): Motion[] {
  const duration = quadruped ? 1.8 : 2.4;
  const limbs = quadruped
    ? ["fore-left", "fore-right", "hind-left", "hind-right"]
    : ["leg-left", "leg-right"];
  const gait = (id: string, seconds: number, amplitude: number): Motion => ({
    id,
    name: id === "walk" ? "Measured stalking steps" : "Committed pursuit",
    duration: seconds,
    loop: true,
    keys: [
      ...limbs.flatMap((limb, i) =>
        [0, 0.25, 0.5, 0.75, 1].flatMap((phase) => {
          const swing = Math.sin(phase * Math.PI * 2 + (i % 2 ? Math.PI : 0));
          return [
            key(`${limb}-upper`, phase * seconds, [swing * amplitude, 0, 0]),
            key(`${limb}-lower`, phase * seconds, [Math.max(0, -swing) * amplitude * 1.1, 0, 0]),
          ];
        }),
      ),
      ...[0, 0.25, 0.5, 0.75, 1].map((p) =>
        key(
          "spine",
          p * seconds,
          [0, Math.sin(p * Math.PI * 2) * 0.035, 0],
          [0, Math.sin(p * Math.PI * 4) * amplitude * 0.05, 0],
        ),
      ),
    ],
  });
  return [
    {
      id: "idle",
      name: "Listening breath",
      duration: 4,
      loop: true,
      keys: [
        key("spine", 0),
        key("spine", 2, [0.018, 0, 0], [0, 0.022, 0]),
        key("spine", 4),
        key("head", 0, [0, -0.08, 0.035]),
        key("head", 1.7, [-0.025, 0.045, -0.02]),
        key("head", 4, [0, -0.08, 0.035]),
        key("jaw", 0),
        key("jaw", 2.2, [0.07, 0, 0]),
        key("jaw", 4),
      ],
    },
    gait("walk", duration, 0.27),
    gait("run", duration * 0.5, 0.58),
    {
      id: "lunge",
      name: "Coil, commit, strike, recover",
      duration: 2.4,
      loop: false,
      keys: [
        key("root", 0),
        key("root", 0.65, [0.035, 0, 0], [0, -0.08, -0.18]),
        key("root", 1.05, [0.11, 0, 0], [0, 0.3, 1.25]),
        key("root", 1.3, [0, 0, 0], [0, 0, 1.5]),
        key("root", 2.4, [0, 0, 0], [0, 0, 1.5]),
        key("head", 0),
        key("head", 0.65, [-0.22, 0, 0]),
        key("head", 1.05, [0.22, 0, 0]),
        key("head", 2.4),
        key("jaw", 0),
        key("jaw", 0.7, [0.58, 0, 0]),
        key("jaw", 1.15, [0.09, 0, 0]),
        key("jaw", 2.4),
      ],
    },
    {
      id: "hit",
      name: "Yield and gather",
      duration: 0.8,
      loop: false,
      keys: [
        key("spine", 0),
        key("spine", 0.12, [0.1, -0.13, 0.16]),
        key("spine", 0.4, [-0.025, 0.04, -0.04]),
        key("spine", 0.8),
        key("head", 0),
        key("head", 0.18, [-0.15, 0.2, -0.12]),
        key("head", 0.8),
      ],
    },
    {
      id: "recovery",
      name: "Replant and watch",
      duration: 1.6,
      loop: false,
      keys: [
        key("spine", 0, [0.08, 0, 0], [0, -0.08, 0]),
        key("spine", 1.6),
        key("head", 0, [0.12, -0.18, 0]),
        key("head", 0.7, [0, 0.16, 0]),
        key("head", 1.6),
        key("jaw", 0, [0.18, 0, 0]),
        key("jaw", 1.6),
      ],
    },
  ];
}
function warden(): { character: CharacterDefinition; materials: MaterialDefinition[] } {
  const body = [
    node("ribcage", [0, 1.5, 0.1], [0.43, 0.59, 0.91]),
    node("belly", [0, 1.28, -0.22], [0.31, 0.32, 0.62]),
    node("haunch", [0, 1.33, -0.85], [0.35, 0.4, 0.46]),
    node("neck", [0, 1.74, 0.81], [0.37, 0.48, 0.48], undefined, [0.38, 0, 0]),
    node("skull", [0, 2.05, 1.19], [0.32, 0.33, 0.43]),
    node("muzzle", [0, 1.985, 1.54], [0.185, 0.135, 0.355]),
    node("jaw-form", [0, 1.805, 1.49], [0.145, 0.055, 0.315], "ashen-face"),
    node("cervical-ridge", [0, 1.91, 0.55], [0.29, 0.28, 0.57], undefined, [0.28, 0, 0]),
    node("tail-base", [0, 1.25, -1.28], [0.19, 0.2, 0.44], undefined, [-0.45, 0, 0]),
    node("tail-tip", [0.04, 0.87, -1.71], [0.16, 0.32, 0.3], undefined, [-0.35, 0, 0]),
  ];
  const details: FieldNode[] = [
    node("nose", [0, 1.985, 1.87], [0.12, 0.067, 0.057], "wet-obsidian"),
    node("scar", [-0.44, 1.68, 0.52], [0.006, 0.25, 0.12], "scar-tissue", [0, 0.199, -0.0994]),
    node("collar-front", [0, 1.64, 1.07], [0.4, 0.12, 0.1], "tarnished-bronze"),
  ];
  const joints = [
    joint("root", null, [0, 1.1, -0.5], 0.4),
    joint("spine", "root", [0, 1.45, 0.1], 0.62),
    joint("neck", "spine", [0, 1.76, 0.85], 0.33),
    joint("head", "neck", [0, 2, 1.2], 0.35),
    joint("jaw", "head", [0, 1.83, 1.25], 0.2, 0, 0.7),
    joint("tail", "root", [0, 1.25, -1.25], 0.42),
  ];
  for (const [side, x] of [
    ["left", -1],
    ["right", 1],
  ] as const) {
    details.push(
      node(`eye-${side}`, [x * 0.275, 2.1, 1.41], [0.039, 0.029, 0.054], "ember-iris", [0, x * 0.3, 0]),
      node(`fang-${side}`, [x * 0.125, 1.825, 1.66], [0.026, 0.07, 0.035], "old-ivory"),
    );
    body.push(
      node(`cheek-${side}`, [x * 0.225, 1.98, 1.24], [0.13, 0.145, 0.22], "ashen-face", [0.12, 0, -x * 0.18]),
      node(`brow-${side}`, [x * 0.245, 2.155, 1.385], [0.115, 0.035, 0.15], undefined, [0, 0, -x * 0.13]),
    );
    joints.push(joint(`ear-${side}`, "head", [x * 0.25, 2.25, 1.09], 0.18));
    for (const [end, z] of [
      ["fore", 0.65],
      ["hind", -0.92],
    ] as const) {
      const limb = `${end}-${side}`,
        px = x * 0.37;
      body.push(
        node(
          `${limb}-mass`,
          [px, end === "hind" ? 1.14 : 1.16, z + (end === "hind" ? 0.09 : 0)],
          [0.17, end === "hind" ? 0.3 : 0.43, 0.22],
          undefined,
          [end === "hind" ? -0.48 : 0.08, 0, 0],
        ),
        node(
          `${limb}-shin`,
          [px, end === "hind" ? 0.7 : 0.55, z + (end === "hind" ? 0.035 : 0.08)],
          [0.085, end === "hind" ? 0.33 : 0.4, 0.105],
          undefined,
          [end === "hind" ? 0.637 : 0, 0, 0],
        ),
        node(`${limb}-paw`, [px, 0.13, z + 0.16], [0.15, 0.11, 0.19], "charcoal-coat"),
      );
      if (end === "hind")
        body.push(
          node(`${limb}-metatarsal`, [px, 0.3, z], [0.072, 0.24, 0.083], undefined, [-0.82, 0, 0]),
          node(`${limb}-hock`, [px, 0.46, z - 0.14], [0.085, 0.095, 0.095]),
        );
      joints.push(
        joint(`${limb}-upper`, end === "fore" ? "spine" : "root", [px, 1.35, z], 0.24),
        ...(end === "hind" ? [joint(`${limb}-knee`, `${limb}-upper`, [px, 0.95, z + 0.22], 0.16)] : []),
        joint(
          `${limb}-lower`,
          end === "hind" ? `${limb}-knee` : `${limb}-upper`,
          [px, end === "hind" ? 0.45 : 0.7, z + (end === "hind" ? -0.15 : 0.04)],
          0.15,
        ),
        joint(`${limb}-foot`, `${limb}-lower`, [px, 0.16, z + 0.16], 0.18),
      );
      details.push(node(`${limb}-pad`, [px, 0.043, z + 0.15], [0.105, 0.02, 0.125], "wet-obsidian"));
      for (const digit of [-1, 0, 1]) {
        body.push(
          node(`${limb}-toe-${digit + 1}`, [px + digit * 0.074, 0.1, z + 0.285], [0.051, 0.067, 0.088]),
        );
        details.push(
          node(
            `${limb}-claw-${digit + 1}`,
            [px + digit * 0.074, 0.095, z + 0.365],
            [0.022, 0.025, 0.045],
            "old-ivory",
            [0.15, 0, 0],
          ),
        );
      }
    }
  }
  // Form stays coherent without a coat: the mane is a groom, not a row of solid blobs.
  body.push(node("throat-countershade", [0, 1.67, 1.045], [0.2, 0.245, 0.14], "ashen-face", [-0.15, 0, 0]));
  details.push(
    node("broken-pauldron", [0.48, 1.72, 0.54], [0.095, 0.31, 0.37], "tarnished-bronze", [0.05, 0, -0.24]),
  );
  return {
    character: {
      ...base("ash-warden", "Ash Warden"),
      kind: "character",
      material: "charcoal-coat",
      field: compose(body, details, { min: [-0.95, -0.05, -2.25], max: [0.95, 2.95, 2.2] }),
      joints,
      motions: performance(true),
      physics: {
        mode: "kinematic",
        mass: 210,
        friction: 0.9,
        restitution: 0.02,
        colliders: [
          {
            id: "torso-proxy",
            shape: "capsule",
            position: [0, 1.3, 0],
            rotation: [Math.PI / 2, 0, 0],
            radius: 0.38,
            halfHeight: 0.7,
          },
        ],
      },
    },
    materials: [
      material("charcoal-coat", [0.12, 0.15, 0.19], [0.25, 0.28, 0.31], 0.76),
      material("silver-guard-hair", [0.53, 0.58, 0.6], [0.8, 0.81, 0.75], 0.58, 0, "stripes", 16),
      material("ashen-face", [0.38, 0.41, 0.41], [0.64, 0.64, 0.56], 0.7),
      material("scar-tissue", [0.25, 0.115, 0.085], [0.38, 0.23, 0.18], 0.63),
      material("wet-obsidian", [0.013, 0.018, 0.02], [0.013, 0.018, 0.02], 0.12, 0, "solid"),
      material("ember-iris", [0.7, 0.29, 0.035], [0.05, 0.02, 0.008], 0.16, 0, "marble", 6),
      material("old-ivory", [0.64, 0.56, 0.37], [0.27, 0.22, 0.13], 0.4, 0, "stripes", 28),
      material("tarnished-bronze", [0.22, 0.17, 0.07], [0.045, 0.1, 0.075], 0.58, 0.8),
    ],
  };
}
function penitent(): { character: CharacterDefinition; materials: MaterialDefinition[] } {
  const body = [
    node("pelvis", [0, 1.3, 0], [0.24, 0.23, 0.18]),
    node("abdomen", [0.02, 1.65, 0], [0.18, 0.36, 0.14]),
    node("ribcage", [0, 1.99, -0.035], [0.32, 0.36, 0.21]),
    node("neck", [0.07, 2.35, 0.025], [0.1, 0.23, 0.12], undefined, [0.2, 0, -0.18]),
    node("skull", [0.12, 2.63, 0.07], [0.18, 0.28, 0.19]),
    node("jaw-form", [0.12, 2.45, 0.16], [0.115, 0.11, 0.14]),
  ];
  const details = [
    node("nose", [0.12, 2.62, 0.26], [0.035, 0.095, 0.065], "sallow-skin"),
    node("crown-trunk", [-0.05, 2.96, -0.02], [0.06, 0.32, 0.065], "peat-antler", [0, 0, 0.3]),
    node("crown-tine", [-0.25, 3.1, -0.01], [0.04, 0.25, 0.045], "peat-antler", [0, 0, 0.7]),
    node("crown-broken", [0.29, 2.89, 0.01], [0.055, 0.14, 0.065], "peat-antler", [0, 0, -0.35]),
  ];
  const joints = [
    joint("root", null, [0, 1.3, 0], 0.32),
    joint("spine", "root", [0, 1.98, 0], 0.39),
    joint("neck", "spine", [0.07, 2.35, 0.025], 0.15),
    joint("head", "neck", [0.12, 2.62, 0.07], 0.27),
    joint("jaw", "head", [0.12, 2.49, 0.055], 0.12, 0, 0.65),
  ];
  for (const [side, x] of [
    ["left", -1],
    ["right", 1],
  ] as const) {
    const drop = side === "left" ? 0.1 : 0;
    body.push(
      node(`arm-${side}-mass`, [x * 0.42, 1.89 - drop, 0], [0.095, 0.38, 0.1], undefined, [0, 0, x * 0.15]),
      node(`arm-${side}-forearm`, [x * 0.51, 1.35 - drop, 0.07], [0.07, 0.32, 0.08]),
      node(`hand-${side}`, [x * 0.52, 1 - drop, 0.12], [0.085, 0.16, 0.055]),
      node(`leg-${side}-thigh`, [x * 0.17, 0.99, 0], [0.115, 0.36, 0.13]),
      node(`leg-${side}-shin`, [x * 0.19, 0.43, 0.025], [0.065, 0.29, 0.08]),
      node(`leg-${side}-foot-form`, [x * 0.19, 0.105, 0.14], [0.105, 0.09, 0.22]),
    );
    details.push(
      node(`eye-${side}`, [0.12 + x * 0.095, 2.69, 0.238], [0.047, 0.026, 0.027], "clouded-eye"),
      node(
        `cloth-panel-${side}`,
        [x * 0.175, 0.99 + drop, 0.145],
        [0.15, 0.59 - drop, 0.055],
        "marsh-linen",
        [0.08, x * 0.15, x * 0.1],
      ),
    );
    joints.push(
      joint(`arm-${side}-upper`, "spine", [x * 0.31, 2.19 - drop, 0], 0.18),
      joint(`arm-${side}-lower`, `arm-${side}-upper`, [x * 0.49, 1.59 - drop, 0.035], 0.15),
      joint(`hand-${side}`, `arm-${side}-lower`, [x * 0.52, 1.1 - drop, 0.1], 0.17),
      joint(`leg-${side}-upper`, "root", [x * 0.17, 1.27, 0], 0.19),
      joint(`leg-${side}-lower`, `leg-${side}-upper`, [x * 0.18, 0.69, 0.015], 0.14),
      joint(`leg-${side}-foot`, `leg-${side}-lower`, [x * 0.19, 0.105, 0.14], 0.17),
    );
    for (let i = 0; i < 3; i++)
      details.push(
        node(
          `finger-${side}-${i}`,
          [x * (0.46 + i * 0.048), 0.82 - drop, 0.12],
          [0.021, 0.16 - i * 0.014, 0.024],
          "sallow-skin",
          [0.06 * i, 0, x * (i - 1) * 0.12],
        ),
      );
    for (let i = 0; i < 4; i++)
      details.push(
        node(
          `rib-${side}-${i}`,
          [x * (0.15 + i * 0.018), 1.83 + i * 0.1, 0.145],
          [0.09, 0.024, 0.045],
          "sallow-skin",
          [0, 0, x * 0.28],
        ),
      );
  }
  details.push(
    node("shoulder-wrap", [-0.29, 2.12, 0.045], [0.2, 0.2, 0.27], "marsh-linen", [0, 0, -0.3]),
    node("chest-vow", [0.07, 1.99, 0.207], [0.035, 0.11, 0.025], "vow-copper"),
  );
  return {
    character: {
      ...base("reed-penitent", "Reed Penitent"),
      kind: "character",
      material: "sallow-skin",
      field: compose(body, details, { min: [-0.85, -0.05, -0.5], max: [0.85, 3.5, 0.65] }),
      joints,
      motions: performance(false),
      physics: {
        mode: "kinematic",
        mass: 48,
        friction: 0.8,
        restitution: 0,
        colliders: [
          {
            id: "body-proxy",
            shape: "capsule",
            position: [0, 1.5, 0],
            rotation: [0, 0, 0],
            radius: 0.25,
            halfHeight: 1.15,
          },
        ],
      },
    },
    materials: [
      material("sallow-skin", [0.34, 0.31, 0.23], [0.16, 0.18, 0.135], 0.77),
      material("marsh-linen", [0.11, 0.145, 0.115], [0.24, 0.25, 0.17], 0.96, 0, "stripes", 45),
      material("peat-antler", [0.18, 0.15, 0.085], [0.4, 0.35, 0.2], 0.65, 0, "stripes", 21),
      material("clouded-eye", [0.48, 0.55, 0.48], [0.19, 0.23, 0.19], 0.19, 0, "solid"),
      material("vow-copper", [0.37, 0.19, 0.08], [0.08, 0.18, 0.13], 0.5, 0.72),
    ],
  };
}
function anatomy(character: CharacterDefinition): CreatureDefinition {
  const wolf = character.id === "ash-warden";
  const source = creatureSchema.parse({
    schemaVersion: 1,
    pelvis: { joint: "root", maxOffset: [0.2, 0.3, 0.2], weight: 1, iterations: 3 },
  });
  const chest = character.field.nodes.find((n) => n.id === "ribcage")!;
  const head = character.field.nodes.find((n) => n.id === "skull")!;
  const jaw = character.field.nodes.find((n) => n.id === "jaw-form")!;
  for (const [id, shape, joints] of [
    ["shoulder", chest, ["spine", "neck"]],
    ["head-region", head, ["head"]],
    ["jaw-region", jaw, ["jaw"]],
  ] as const) {
    source.regions.push({
      id,
      name: id,
      nodeIds: [shape.id],
      jointIds: [...joints],
      frame: { position: [...shape.position], rotation: [0, 0, 0] },
      extent: [...shape.size],
      material: character.material,
    });
  }
  if (wolf)
    source.regions[0].nodeIds.push("belly", "haunch", "neck", "cervical-ridge", "throat-countershade");
  const headRegion = source.regions.find((region) => region.id === "head-region")!;
  headRegion.nodeIds.push(
    ...character.field.nodes
      .filter(
        (entry) =>
          entry.id === "muzzle" || entry.id === "nose" || /^(eye|brow|cheek|fang|inner-ear)-/.test(entry.id),
      )
      .map((entry) => entry.id),
  );
  source.charts.push({
    id: "shoulder-surface",
    kind: "sweep",
    region: "shoulder",
    revision: 1,
    points: [
      [0, 0, -chest.size[2] * 0.7],
      [0, 0, 0],
      [0, 0, chest.size[2] * 0.7],
    ],
    radii: [chest.size[0] * 0.55, chest.size[0], chest.size[0] * 0.72],
    crossSections: [
      [chest.size[0] * 0.55, chest.size[1] * 0.65],
      [chest.size[0], chest.size[1]],
      [chest.size[0] * 0.72, chest.size[1] * 0.8],
    ],
    caps: true,
    material: character.material,
  });
  source.anchors.push(
    {
      id: "shoulder-scar",
      region: "shoulder",
      chart: "shoulder-surface",
      chartRevision: 1,
      coordinates: [0.72, 0.48, 1],
      offset: 0.004,
      purpose: "scar",
      tolerance: 0.012,
    },
    {
      id: "shoulder-attachment",
      region: "shoulder",
      chart: "shoulder-surface",
      chartRevision: 1,
      coordinates: [0.65, 0.03, 1],
      offset: 0.045,
      purpose: "attachment",
      tolerance: 0.012,
    },
    {
      id: "mane-root",
      region: "shoulder",
      chart: "shoulder-surface",
      chartRevision: 1,
      coordinates: [0.65, 0.25, 1],
      offset: 0.005,
      purpose: "groom",
      tolerance: 0.012,
    },
  );
  source.landmarks.push(
    { id: "jaw-hinge", region: "jaw-region", position: [0, 0.05, -0.17] },
    { id: "sternum", region: "shoulder", position: [0, -chest.size[1], 0.1] },
  );
  source.influenceRules.push(
    {
      id: "shoulder-influences",
      region: "shoulder",
      allowedJoints: ["root", "spine", "neck"],
      excludedJoints: ["jaw"],
    },
    {
      id: "head-rigid",
      region: "head-region",
      allowedJoints: ["head"],
      excludedJoints: [],
      rigidJoint: "head",
    },
    { id: "jaw-rigid", region: "jaw-region", allowedJoints: ["jaw"], excludedJoints: [], rigidJoint: "jaw" },
  );
  source.correctives.push(
    {
      id: "shoulder-compression",
      region: "shoulder",
      joint: "spine",
      axis: "x",
      angle: 0.4,
      radius: 0.5,
      center: [0, 0.15, 0.25],
      displacement: [0, 0.08, -0.025],
    },
    {
      id: "jaw-open-throat",
      region: "jaw-region",
      joint: "jaw",
      axis: "x",
      angle: 0.6,
      radius: 0.3,
      center: [0, 0, -0.08],
      displacement: [0, -0.025, 0.015],
    },
  );
  source.appearance.push(
    {
      id: "shoulder-tissue",
      region: "shoulder",
      family: wolf ? "hair" : "skin",
      growthSuppression: 0,
      color: wolf ? [0.18, 0.22, 0.26] : [0.34, 0.31, 0.23],
      roughness: 0.78,
      metallic: 0,
      subsurface: wolf ? 0 : 0.18,
      transmission: 0.05,
      anisotropy: wolf ? 0.65 : 0,
      direction: [0, 0, -1],
      scale: 24,
      variation: 0.15,
      displacement: 0.001,
    },
    {
      id: "scar-response",
      region: "shoulder",
      anchor: "shoulder-scar",
      family: "skin",
      growthSuppression: 1,
      color: [0.28, 0.16, 0.12],
      roughness: 0.58,
      metallic: 0,
      subsurface: 0.15,
      transmission: 0.08,
      anisotropy: 0,
      direction: [0, 1, 0],
      scale: 12,
      variation: 0.07,
      displacement: 0.002,
      mask: { center: [0, 0, 0], radius: 0.24, falloff: 2 },
    },
  );
  const limbNames = wolf ? ["fore-left", "fore-right", "hind-left", "hind-right"] : ["leg-left", "leg-right"];
  for (const [index, limb] of limbNames.entries()) {
    const joints = (
      limb.startsWith("hind") ? ["upper", "knee", "lower", "foot"] : ["upper", "lower", "foot"]
    ).map((part) => `${limb}-${part}`);
    const foot = character.joints.find((j) => j.id === joints.at(-1))!;
    const root = character.joints.find((j) => j.id === joints[0])!;
    const ids = character.field.nodes.filter((n) => n.id.startsWith(limb)).map((n) => n.id);
    source.regions.push({
      id: limb,
      name: limb,
      parent: "shoulder",
      nodeIds: ids,
      jointIds: joints,
      frame: { position: [...root.position], rotation: [0, 0, 0] },
      extent: [0.3, 0.8, 0.4],
    });
    source.influenceRules.push({
      id: `${limb}-binding`,
      region: limb,
      allowedJoints: joints,
      excludedJoints: [],
    });
    source.ikChains.push({
      id: `${limb}-ik`,
      joints,
      target: [...foot.position],
      pole: [
        root.position[0],
        root.position[1] - 0.35,
        root.position[2] + (limb.startsWith("hind") ? -0.8 : 0.8),
      ],
      weight: 0,
      iterations: 20,
      tolerance: 0.008,
    });
    for (const motion of character.motions.filter((m) =>
      ["idle", "walk", "run", "recovery"].includes(m.id),
    )) {
      const start =
        motion.id === "idle" || motion.id === "recovery" ? 0 : index % 2 ? motion.duration * 0.5 : 0;
      const end =
        motion.id === "idle" || motion.id === "recovery" ? motion.duration : start + motion.duration * 0.42;
      source.contacts.push({
        id: `${limb}-${motion.id}-plant`,
        motion: motion.id,
        joint: foot.id,
        start,
        end,
        target: [...foot.position],
        space: "character",
        weight: 1,
        tolerance: 0.025,
        blendIn: motion.id === "recovery" ? 0 : 0.07,
        blendOut: motion.id === "recovery" ? 0 : 0.07,
        ground: true,
        offset: foot.position[1],
      });
    }
    const leap = character.motions.find((motion) => motion.id === "lunge")!;
    leap.keys.push(
      key(`${limb}-upper`, 0),
      key(`${limb}-upper`, 0.65),
      key(`${limb}-upper`, 0.78, [0.42, 0, 0]),
      key(`${limb}-upper`, 1.08, [0.3, 0, 0]),
      key(`${limb}-upper`, 1.3),
      key(`${limb}-lower`, 0),
      key(`${limb}-lower`, 0.65),
      key(`${limb}-lower`, 0.78, [0.58, 0, 0]),
      key(`${limb}-lower`, 1.08, [0.42, 0, 0]),
      key(`${limb}-lower`, 1.3),
    );
    source.contacts.push(
      {
        id: `${limb}-hit-support`,
        motion: "hit",
        joint: foot.id,
        start: 0,
        end: 0.8,
        target: [...foot.position],
        space: "character",
        weight: 1,
        tolerance: 0.025,
        blendIn: 0,
        blendOut: 0,
        ground: true,
        offset: foot.position[1],
      },
      {
        id: `${limb}-lunge-coil`,
        motion: "lunge",
        joint: foot.id,
        start: 0,
        end: 0.72,
        target: [...foot.position],
        space: "character",
        weight: 1,
        tolerance: 0.025,
        blendIn: 0,
        blendOut: 0.01,
        ground: true,
        offset: foot.position[1],
      },
      {
        id: `${limb}-lunge-land`,
        motion: "lunge",
        joint: foot.id,
        start: 1.3,
        end: 2.4,
        target: [foot.position[0], foot.position[1], foot.position[2] + 1.5],
        space: "character",
        weight: 1,
        tolerance: 0.025,
        blendIn: 0,
        blendOut: 0,
        ground: true,
        offset: foot.position[1],
      },
    );
  }
  source.expressions.push(
    {
      id: "watchful",
      weight: 0,
      weights: [{ joint: "head", rotation: [0, 0.12, -0.025], translation: [0, 0, 0] }],
    },
    {
      id: "snarl",
      weight: 0,
      weights: [
        { joint: "jaw", rotation: [0.38, 0, 0], translation: [0, 0, 0] },
        { joint: "head", rotation: [-0.08, 0, 0], translation: [0, 0, 0] },
      ],
    },
  );
  if (wolf) {
    // Ears are tapered authored surfaces with an inset membrane, driven by their own joints.
    for (const [side, sign] of [
      ["left", -1],
      ["right", 1],
    ] as const) {
      const region = `ear-${side}-region`;
      source.regions.push({
        id: region,
        name: `${side} pinna`,
        parent: "head-region",
        nodeIds: [],
        jointIds: [`ear-${side}`],
        frame: { position: [sign * 0.25, 2.25, 1.09], rotation: [0.1, 0, -sign * 0.18] },
        extent: [0.12, 0.43, 0.07],
        material: "charcoal-coat",
      });
      source.charts.push(
        {
          id: `ear-${side}-pinna`,
          kind: "sweep",
          region,
          revision: 1,
          points: [
            [0, 0, 0],
            [0, 0.22, -0.01],
            [0, 0.43, -0.04],
          ],
          radii: [0.105, 0.07, 0.006],
          crossSections: [
            [0.105, 0.055],
            [0.07, 0.035],
            [0.006, 0.006],
          ],
          caps: true,
          material: "charcoal-coat",
        },
        {
          id: `ear-${side}-membrane`,
          kind: "patch",
          region,
          revision: 1,
          points: [
            [-0.075, 0.06, 0.055],
            [0.075, 0.06, 0.055],
            [-0.008, 0.36, 0.004],
            [0.008, 0.36, 0.004],
          ],
          thickness: 0.008,
          material: "ashen-face",
        },
      );
      source.influenceRules.push({
        id: `ear-${side}-rigid`,
        region,
        allowedJoints: [`ear-${side}`],
        excludedJoints: [],
        rigidJoint: `ear-${side}`,
      });
    }
    source.attachments.push(
      {
        id: "mounted-pauldron",
        anchor: "shoulder-attachment",
        nodeIds: ["broken-pauldron"],
        offset: [0.155, 0.024, 0.032],
        rigidJoint: "spine",
        minimumClearance: 0.03,
      },
      {
        id: "scar-relief",
        anchor: "shoulder-scar",
        nodeIds: ["scar"],
        offset: [-0.004, 0.0004, 0.0008],
        rigidJoint: "spine",
        minimumClearance: 0,
      },
    );
    source.charts.push({
      id: "dorsal-mane-growth",
      kind: "patch",
      region: "shoulder",
      revision: 1,
      realization: "correspondence-only",
      points: [
        [-0.23, 0.2, 0.6],
        [0.23, 0.2, 0.6],
        [-0.23, 0.2, -0.6],
        [0.23, 0.2, -0.6],
      ],
      thickness: 0.004,
    });
    source.charts[0].realization = "correspondence-only";
    source.grooms.push(
      {
        id: "guard-coat",
        rootProjection: { maxDistance: 0.5, direction: "outward" },
        region: "shoulder",
        chart: "shoulder-surface",
        chartRevision: 1,
        material: "charcoal-coat",
        seed: 712,
        density: 260,
        length: 0.09,
        width: 0.026,
        direction: [0, -0.2, -1],
        guides: [],
        taper: 0.85,
        clump: 0.3,
        curl: 0.1,
        rootColor: [0.11, 0.14, 0.18],
        tipColor: [0.35, 0.39, 0.42],
        masks: [],
        maxCards: 360,
        representation: "tufts",
        ribbonThickness: 0.08,
        lodFractions: [1, 0.5, 0.2],
        lift: 0.45,
        frizz: 0.08,
        stiffness: 55,
        damping: 8,
      },
      {
        id: "silver-mane",
        rootProjection: { maxDistance: 0.5, direction: "outward" },
        region: "shoulder",
        chart: "dorsal-mane-growth",
        chartRevision: 1,
        material: "silver-guard-hair",
        seed: 1701,
        density: 190,
        length: 0.3,
        width: 0.06,
        direction: [0, 0.35, -1],
        guides: [
          {
            id: "crest-flow",
            points: [
              [0, 0.53, 0.35],
              [0, 0.69, 0.2],
              [0.015, 0.7, -0.08],
            ],
          },
        ],
        taper: 0.95,
        clump: 0.6,
        curl: 0.25,
        rootColor: [0.28, 0.33, 0.35],
        tipColor: [0.82, 0.84, 0.79],
        masks: [],
        maxCards: 130,
        representation: "tufts",
        ribbonThickness: 0.08,
        lodFractions: [1, 0.6, 0.3],
        lift: 0.7,
        frizz: 0.12,
        stiffness: 24,
        damping: 7,
      },
    );
    character.joints.push(joint("tail-tip", "tail", [0.04, 0.8, -1.8], 0.2));
    source.secondaryChains.push({
      id: "tail-sway",
      joints: ["tail", "tail-tip"],
      stiffness: 30,
      damping: 7,
      gravity: [0, -2, 0],
      wind: [0.3, 0, 0.1],
      maxAngle: 0.65,
      collisionRadius: 0.08,
      weight: 0.7,
    });
  } else {
    source.charts.push({
      id: "vow-mount",
      kind: "patch",
      region: "shoulder",
      revision: 1,
      material: "sallow-skin",
      thickness: 0.005,
      points: [
        [-0.12, -0.12, 0.24],
        [0.12, -0.12, 0.24],
        [-0.12, 0.12, 0.24],
        [0.12, 0.12, 0.24],
      ],
    });
    source.anchors.push({
      id: "vow-anchor",
      region: "shoulder",
      chart: "vow-mount",
      chartRevision: 1,
      coordinates: [0.79, 0.5, 0],
      offset: 0.028,
      purpose: "attachment",
      tolerance: 0.01,
    });
    source.attachments.push({
      id: "vow-charm",
      anchor: "vow-anchor",
      nodeIds: ["chest-vow"],
      offset: [0, 0, 0],
      rigidJoint: "spine",
      minimumClearance: 0,
    });
    for (const [side, sign] of [
      ["left", -1],
      ["right", 1],
    ] as const) {
      const region = `cloth-${side}`;
      source.regions.push({
        id: region,
        name: `Split linen ${side}`,
        nodeIds: [`cloth-panel-${side}`],
        jointIds: ["root"],
        frame: { position: [sign * 0.17, 1.4, 0.18], rotation: [0, 0, sign * 0.1] },
        extent: [0.16, 0.7, 0.03],
        material: "marsh-linen",
      });
      source.charts.push({
        id: `${region}-surface`,
        kind: "patch",
        region,
        revision: 1,
        material: "marsh-linen",
        thickness: 0.012,
        points: [
          [-0.13, 0, 0],
          [0.13, 0, 0],
          [-0.15, -0.83, 0.1],
          [0.16, -0.77, 0.05],
        ],
      });
      source.appearance.push({
        id: `${region}-weave`,
        region,
        family: "cloth",
        growthSuppression: 0,
        material: "marsh-linen",
        color: [0.12, 0.15, 0.11],
        roughness: 0.96,
        metallic: 0,
        subsurface: 0,
        transmission: 0.08,
        anisotropy: 0.3,
        direction: [0, -1, 0],
        scale: 45,
        variation: 0.12,
        displacement: 0.001,
      });
      source.influenceRules.push({
        id: `${region}-binding`,
        region,
        allowedJoints: ["root"],
        rigidJoint: "root",
        excludedJoints: [],
      });
      source.cloth.push({
        id: `${region}-drape`,
        region,
        chart: `${region}-surface`,
        chartRevision: 1,
        material: "marsh-linen",
        pinEdges: ["v0"],
        pins: [],
        stiffness: 0.96,
        bendStiffness: 0.16,
        damping: 0.055,
        gravity: [0, -9.81, 0],
        wind: [0.5, 0, 0.2],
        iterations: 10,
        collisionRadius: 0.012,
        maxStretch: 1.08,
      });
    }
  }
  source.reviewScenarios = character.motions.map((motion) => ({
    id: `${motion.id}-review`,
    name: `${motion.name} review`,
    motion: motion.id,
    duration: motion.duration,
    sampleRate: 30,
    cameras: [
      { id: "three-quarter", position: [4, 2.7, 5], target: [0, 1.5, 0] },
      { id: "side", position: [5, 1.7, 0], target: [0, 1.5, 0] },
    ],
    thresholds: { contactSlip: 0.025, penetration: 0.03, anchorError: 0.015, stretch: 0.2 },
  }));
  return creatureSchema.parse(source);
}
/** Returns fresh, ordinary source data; never mutates the reference project or a prior fixture. */
export function createCreatureFixture(id: CreatureFixtureId): CreatureFixture {
  const { character, materials } = id === "ash-warden" ? warden() : penitent();
  character.creature = anatomy(character);
  refineWardenClay(character);
  for (const material of materials) {
    if (material.id.includes("eye") || material.id.includes("iris"))
      material.creature = { family: "eye", clearcoat: 1, clearcoatRoughness: 0.05, thickness: 0.006 };
    else if (material.id.includes("skin") || material.id.includes("tissue"))
      material.creature = {
        family: "skin",
        subsurface: 0.24,
        transmission: 0.12,
        thickness: 0.004,
        scatterColor: [0.55, 0.23, 0.14],
      };
    else if (material.id.includes("linen"))
      material.creature = { family: "cloth", sheen: 0.45, anisotropy: 0.28, fiberDirection: [0, 1, 0] };
    else if (material.id.includes("coat") || material.id.includes("hair"))
      material.creature = { family: "fiber", anisotropy: 0.65, sheen: 0.35, fiberDirection: [0, 0, -1] };
    else if (material.id.includes("wet"))
      material.creature = { family: "wet", clearcoat: 0.8, clearcoatRoughness: 0.08 };
  }
  const camera = {
    position: (id === "ash-warden" ? [4.8, 2.8, 5.8] : [3.4, 2.3, 5.5]) as Vec3,
    target: [0, 1.5, 0] as Vec3,
    fov: 40,
  };
  const stageId = `${id}-stage`;
  const recipe: RecipeDefinition = {
    id: `${id}-family`,
    version: "1",
    template: structuredClone(character),
    parameters: {
      chestWidth: {
        default: character.field.nodes.find((n) => n.id === "ribcage")!.size[0],
        min: 0.2,
        max: 0.65,
      },
      jawDepth: {
        default: character.field.nodes.find((n) => n.id === "jaw-form")!.size[2],
        min: 0.1,
        max: 0.5,
      },
    },
    bindings: [
      { parameter: "chestWidth", node: "ribcage", path: ["size", 0] },
      { parameter: "jawDepth", node: "jaw-form", path: ["size", 2] },
    ],
  };
  recipe.bindings.push({ parameter: "chestWidth", path: ["creature", "regions", 0, "extent", 0] });
  for (const [index, multiplier] of [0.55, 1, 0.72].entries()) {
    recipe.bindings.push(
      { parameter: "chestWidth", path: ["creature", "charts", 0, "radii", index], scale: multiplier },
      {
        parameter: "chestWidth",
        path: ["creature", "charts", 0, "crossSections", index, 0],
        scale: multiplier,
      },
    );
  }
  recipe.bindings.push({ parameter: "jawDepth", path: ["creature", "regions", 2, "extent", 2] });
  const documents: Document[] = [
    ...materials,
    character,
    {
      ...base("creature-review-sky", "Neutral creature review environment"),
      kind: "environment",
      model: "analytic-sky",
      sunElevation: 0.8,
      sunAzimuth: -0.8,
      turbidity: 2,
      fogDensity: 0,
      skyColor: [0.25, 0.3, 0.36],
      horizonColor: [0.4, 0.42, 0.44],
      groundColor: [0.15, 0.14, 0.13],
      wind: [0.3, 0, 0.1],
    },
    {
      ...base("creature-review-light", "Neutral broad key and cool fill"),
      kind: "lighting",
      ambient: 0.35,
      lights: [
        { id: "key", type: "directional", position: [3, 5, 4], color: [1, 0.94, 0.85], intensity: 3 },
        { id: "fill", type: "point", position: [3, 3, 5], color: [0.82, 0.89, 1], intensity: 26 },
      ],
    },
    {
      ...base(stageId, `${character.name} review stage`),
      kind: "stage",
      environment: "creature-review-sky",
      lighting: "creature-review-light",
      ground: true,
      exposure: 1,
      camera,
      subjects: [id],
    },
  ];
  return {
    project: {
      schemaVersion: 1,
      id: `${id}-project`,
      name: `${character.name} authoring study`,
      documents,
      entry: stageId,
      recipes: [recipe],
    },
    characterId: id,
    stageId,
    brief: creatureFixtureCatalog.find((entry) => entry.id === id)!.description,
    protectedFeatures:
      id === "ash-warden"
        ? [
            "forward-heavy shoulder silhouette",
            "broken asymmetric collar",
            "scar crossing the left shoulder",
            "silver mane flowing away from the jaw",
            "amber watchful eyes",
          ]
        : [
            "off-center crown with broken right tine",
            "hollow chest and long hands",
            "lower left shoulder",
            "split cloth silhouette",
            "clouded asymmetric gaze",
          ],
    reviewScenarios: [
      {
        id: "neutral-turntable",
        purpose: "Judge proportions and material separation",
        motion: "idle",
        samples: [0, 1, 2, 3],
        camera,
        light: "neutral",
        maximumContactError: 0.02,
        maximumAnchorResidual: 0.01,
        requiresVisualReview: true,
      },
      {
        id: "jaw-and-shoulder",
        purpose: "Expose jaw clearance and shoulder compression",
        motion: "lunge",
        samples: [0, 0.65, 1.05, 1.3, 2.4],
        camera: { position: [2.7, 2.5, 3.4], target: [0, 1.95, id === "ash-warden" ? 1 : 0], fov: 30 },
        light: "raking",
        maximumContactError: 0.04,
        maximumAnchorResidual: 0.015,
        requiresVisualReview: true,
      },
      {
        id: "walk-contacts",
        purpose: "Detect planted foot sliding over a complete gait",
        motion: "walk",
        samples: [0, 0.25, 0.5, 0.75, 1].map(
          (phase) => phase * character.motions.find((m) => m.id === "walk")!.duration,
        ),
        camera,
        light: "neutral",
        maximumContactError: 0.025,
        maximumAnchorResidual: 0.01,
        requiresVisualReview: true,
      },
      {
        id: "secondary-recovery",
        purpose: "Inspect silhouette and secondary response after impact",
        motion: "hit",
        samples: [0, 0.12, 0.4, 0.8],
        camera: { ...camera, position: [-4, 2.6, -3.8] },
        light: "backlight",
        maximumContactError: 0.03,
        maximumAnchorResidual: 0.01,
        requiresVisualReview: true,
      },
    ],
    encounter: {
      arenaRadius: 8,
      playerStart: [0, 0, 5],
      sequence: [
        { motion: "idle", repeat: 1, intent: "Establish awareness" },
        { motion: "walk", repeat: 2, intent: "Approach while watching the player" },
        { motion: "lunge", repeat: 1, intent: "Readable anticipation before a committed attack" },
        { motion: "recovery", repeat: 1, intent: "Expose a punishable recovery window" },
        { motion: "hit", repeat: 1, intent: "Yield to a player strike" },
      ],
      attackWindows: [{ motion: "lunge", start: 0.98, end: 1.22, reach: id === "ash-warden" ? 1.4 : 1.1 }],
    },
  };
}
