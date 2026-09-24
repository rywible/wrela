import { applyCharacterMotionLookdev } from "@wrela/examples/character-motion-lookdev";
import { createCreatureFixture } from "@wrela/examples/creature-fixtures";
import type { CharacterDefinition, Document, FieldNode, MaterialDefinition, Vec3 } from "@wrela/model";
import { z } from "zod";

/** A separate, editable art-directed revision of the original Warden study.
 * Rendering experiments never mutate the reference fixture used by other work. */
export const characterLookdevRecipeSchema = z.strictObject({
  bodyMass: z.number().finite().min(0.8).max(1.2).default(0.92),
  chestDepth: z.number().finite().min(0.85).max(1.15).default(1),
  legStrength: z.number().finite().min(0.85).max(1.2).default(1.08),
});
export type CharacterLookdevRecipe = z.input<typeof characterLookdevRecipeSchema>;
export function createCharacterLookdev(input: CharacterLookdevRecipe = {}): {
  documents: Document[];
  character: string;
} {
  const recipe = characterLookdevRecipeSchema.parse(input);
  const fixture = createCreatureFixture("ash-warden");
  const character = fixture.project.documents.find(
    (document) => document.id === fixture.characterId,
  ) as CharacterDefinition;
  const source = character.creature;
  if (!source) throw Error("Missing Warden anatomy");
  character.name = "Ash Warden — Alpine sentinel";
  character.field.resolution = 72;
  const shape = (id: string, changes: Partial<FieldNode>) => {
    const node = character.field.nodes.find((item) => item.id === id);
    if (!node) throw Error(`Missing Warden form ${id}`);
    Object.assign(node, changes);
  };
  // Shape around the fixture's stable limb and head pivots: a sloped topline,
  // separate tucked abdomen, forward-set head, and slimmer shoulder-to-paw arcs.
  shape("ribcage", {
    position: [0, 1.5, 0.16],
    size: [0.34 * recipe.bodyMass, 0.37 * recipe.chestDepth, 0.74],
  });
  shape("belly", { position: [0, 1.46, -0.4], size: [0.235 * recipe.bodyMass, 0.21, 0.46] });
  shape("haunch", { position: [0, 1.39, -0.82], size: [0.3 * recipe.bodyMass, 0.325, 0.36] });
  shape("neck", { position: [0, 1.73, 0.83], size: [0.265, 0.32, 0.38], rotation: [0.26, 0, 0] });
  shape("cervical-ridge", { position: [0, 1.88, 0.52], size: [0.22, 0.13, 0.38], rotation: [0.12, 0, 0] });
  shape("skull", { position: [0, 2.025, 1.2], size: [0.255, 0.235, 0.32] });
  shape("muzzle", { position: [0, 1.96, 1.54], size: [0.14, 0.105, 0.27] });
  shape("jaw-form", { position: [0, 1.887, 1.51], size: [0.112, 0.046, 0.23], material: "charcoal-coat" });
  shape("nose", { position: [0, 1.973, 1.807], size: [0.075, 0.044, 0.047] });
  shape("throat-countershade", {
    position: [0, 1.68, 0.99],
    size: [0.14, 0.15, 0.085],
    material: "charcoal-coat",
  });
  const removed = new Set(["scar", "collar-front", "broken-pauldron", "fang-left", "fang-right"]);
  character.field.nodes = character.field.nodes.filter((node) => !removed.has(node.id));
  for (const node of character.field.nodes) node.children = node.children.filter((id) => !removed.has(id));
  for (const region of source.regions) region.nodeIds = region.nodeIds.filter((id) => !removed.has(id));
  source.attachments = [];
  source.appearance = source.appearance.filter((layer) => layer.id !== "scar-response");
  for (const [side, sign] of [
    ["left", -1],
    ["right", 1],
  ] as const) {
    const anatomy = character.field.nodes.find((node) => node.id === "anatomy");
    if (!anatomy) throw Error("Missing Warden body composition");
    for (const [end, position, size, rotation] of [
      ["fore", [sign * 0.285, 1.5, 0.51], [0.125, 0.265, 0.235], [0.21, 0, sign * 0.09]],
      ["hind", [sign * 0.27, 1.335, -0.82], [0.145, 0.235, 0.22], [-0.3, 0, 0]],
    ] as [string, Vec3, Vec3, Vec3][]) {
      const id = `${end}-${side}-girdle`;
      const template = character.field.nodes.find((node) => node.id === `${end}-${side}-mass`);
      const region = source.regions.find((entry) => entry.id === `${end}-${side}`);
      if (!template || !region) throw Error("Missing Warden limb attachment region");
      character.field.nodes.push({
        ...structuredClone(template),
        id,
        name: `${end} ${side} girdle`,
        position,
        size: [size[0] * recipe.bodyMass, size[1], size[2] * recipe.legStrength],
        rotation,
      });
      anatomy.children.push(id);
      region.nodeIds.push(id);
    }
    shape(`cheek-${side}`, {
      position: [sign * 0.2, 1.965, 1.25],
      size: [0.082, 0.082, 0.15],
      material: "charcoal-coat",
    });
    shape(`brow-${side}`, { position: [sign * 0.205, 2.132, 1.39], size: [0.061, 0.02, 0.092] });
    shape(`eye-${side}`, { position: [sign * 0.2, 2.1, 1.402], size: [0.022, 0.016, 0.026] });
    const headRegion = source.regions.find((region) => region.id === "head-region");
    if (!headRegion) throw Error("Missing head region");
    for (const [edge, rise] of [
      ["upper", 0.01],
      ["lower", -0.012],
    ] as const) {
      const eyelid = source.charts.find((chart) => chart.id === `eyelid-${side}-${edge}`);
      if (eyelid?.kind !== "sweep") continue;
      eyelid.points = [
        [sign * 0.164, 2.096, 1.421],
        [sign * 0.202, 2.101 + rise, 1.422],
        [sign * 0.23, 2.096, 1.386],
      ].map((point) => point.map((value, axis) => value - headRegion.frame.position[axis]) as Vec3);
      eyelid.radii = [0.002, edge === "upper" ? 0.005 : 0.003, 0.002];
      eyelid.material = "charcoal-coat";
    }
    const ear = source.regions.find((region) => region.id === `ear-${side}-region`);
    if (ear) {
      ear.frame.position = [sign * 0.225, 2.205, 1.11];
      ear.frame.rotation = [0.12, 0, -sign * 0.24];
      ear.extent = [0.105, 0.24, 0.06];
    }
    const pinna = source.charts.find((chart) => chart.id === `ear-${side}-pinna`);
    if (pinna?.kind === "sweep") {
      pinna.points = [
        [0, -0.035, 0],
        [0, 0.09, -0.01],
        [-sign * 0.015, 0.24, -0.03],
      ];
      pinna.radii = [0.095, 0.07, 0.007];
      pinna.crossSections = [
        [0.095, 0.045],
        [0.07, 0.031],
        [0.007, 0.006],
      ];
    }
    const membrane = source.charts.find((chart) => chart.id === `ear-${side}-membrane`);
    if (membrane?.kind === "patch")
      membrane.points = [
        [-0.065, 0.015, 0.043],
        [0.065, 0.015, 0.043],
        [-sign * 0.015 - 0.006, 0.205, 0.003],
        [-sign * 0.015 + 0.006, 0.205, 0.003],
      ];
    for (const end of ["fore", "hind"]) {
      shape(`${end}-${side}-mass`, {
        size: [
          (end === "fore" ? 0.12 : 0.145) * recipe.legStrength,
          end === "fore" ? 0.35 : 0.26,
          (end === "fore" ? 0.14 : 0.175) * recipe.legStrength,
        ],
      });
      shape(`${end}-${side}-shin`, {
        size: [
          (end === "fore" ? 0.078 : 0.076) * recipe.legStrength,
          end === "fore" ? 0.365 : 0.3,
          0.084 * recipe.legStrength,
        ],
      });
      shape(`${end}-${side}-paw`, { size: [0.13, 0.085, 0.16] });
      if (end === "hind") {
        shape(`${end}-${side}-metatarsal`, { size: [0.063, 0.225, 0.07] });
        shape(`${end}-${side}-hock`, { size: [0.07, 0.078, 0.078] });
      }
      for (let digit = 0; digit < 3; digit++)
        shape(`${end}-${side}-claw-${digit}`, { material: "wet-obsidian", size: [0.014, 0.016, 0.033] });
    }
  }
  const tail = source.charts.find((chart) => chart.id === "tail-surface");
  if (tail?.kind === "sweep") {
    tail.points[0] = [0, 0.1, 0.2];
    tail.points[1] = [0, -0.04, -0.2];
    tail.radii[0] = 0.16;
  }
  const shoulder = source.charts.find((chart) => chart.id === "shoulder-surface");
  if (shoulder?.kind === "sweep") {
    // Anatomical fields own the continuous torso. This chart is retained for
    // anchors and groom correspondence without adding a second barrel-shaped skin.
    shoulder.realization = "correspondence-only";
    shoulder.points = [
      [0, 0, -0.64],
      [0, 0, 0.08],
      [0, 0.08, 0.66],
    ];
    shoulder.radii = [0.26, 0.35, 0.24];
    shoulder.crossSections = [
      [0.26, 0.29],
      [0.35, 0.47],
      [0.24, 0.34],
    ];
  }
  // Closed, tapered tufts retain actual depth at this pixel density. The former
  // near-hairline ribbons aliased into black specks at ordinary game distance.
  for (const groom of source.grooms) {
    const mane = groom.id === "silver-mane";
    Object.assign(groom, {
      density: mane ? 225 : 60,
      maxCards: mane ? 130 : 180,
      length: mane ? 0.06 : 0.027,
      width: mane ? 0.009 : 0.006,
      representation: "tufts",
      ribbonThickness: 0.08,
      direction: [0, -0.1, -1],
      guides: [],
      lift: mane ? 0.34 : 0.45,
      clump: 0.12,
      curl: 0.025,
      frizz: 0.035,
      rootColor: mane ? [0.2, 0.19, 0.17] : [0.16, 0.15, 0.13],
      tipColor: mane ? [0.37, 0.36, 0.33] : [0.24, 0.23, 0.2],
      lodFractions: [1, 0.7, 0.35],
      rootProjection: { maxDistance: 0.5, direction: "both" },
    });
  }
  const coat = source.grooms.find((groom) => groom.id === "guard-coat");
  if (!coat) throw Error("Missing guard coat source");
  source.charts.push(
    {
      id: "head-coat-growth",
      kind: "sweep",
      region: "head-region",
      realization: "correspondence-only",
      revision: 1,
      points: [
        [0, 0, -0.14],
        [0, 0.025, 0.04],
        [0, -0.01, 0.2],
      ],
      radii: [0.18, 0.255, 0.17],
      crossSections: [
        [0.18, 0.18],
        [0.255, 0.255],
        [0.17, 0.17],
      ],
      caps: true,
    },
    {
      id: "neck-coat-growth",
      kind: "sweep",
      region: "shoulder",
      realization: "correspondence-only",
      revision: 1,
      points: [
        [0, 0.12, 0.5],
        [0, 0.28, 0.7],
        [0, 0.45, 0.89],
      ],
      radii: [0.28, 0.3, 0.2],
      caps: true,
    },
  );
  source.grooms.push(
    {
      ...structuredClone(coat),
      id: "head-guard-coat",
      region: "head-region",
      chart: "head-coat-growth",
      chartRevision: 1,
      seed: 8821,
      density: 130,
      maxCards: 65,
      length: 0.017,
      width: 0.004,
      direction: [0, -0.15, -1],
      lift: 0.45,
      rootProjection: {
        maxDistance: 0.2,
        direction: "both",
        nodeIds: ["skull", "cheek-left", "cheek-right"],
      },
    },
    {
      ...structuredClone(coat),
      id: "neck-guard-coat",
      region: "shoulder",
      chart: "neck-coat-growth",
      chartRevision: 1,
      seed: 8847,
      density: 110,
      maxCards: 100,
      length: 0.026,
      width: 0.006,
      direction: [0, -0.3, -1],
      lift: 0.5,
      rootProjection: {
        maxDistance: 0.3,
        direction: "both",
        nodeIds: ["neck", "cervical-ridge", "throat-countershade"],
      },
    },
  );
  const tissue = source.appearance.find((layer) => layer.id === "shoulder-tissue");
  if (tissue)
    Object.assign(tissue, {
      color: [0.22, 0.2, 0.16],
      variation: 0.2,
      roughness: 0.91,
      anisotropy: 0.35,
      scale: 34,
      displacement: 0.0004,
    });
  const colors: Record<string, { color: Vec3; secondary: Vec3; roughness: number }> = {
    "charcoal-coat": { color: [0.21, 0.19, 0.155], secondary: [0.25, 0.23, 0.195], roughness: 0.9 },
    "silver-guard-hair": { color: [0.4, 0.38, 0.325], secondary: [0.56, 0.535, 0.47], roughness: 0.84 },
    "ashen-face": { color: [0.38, 0.355, 0.3], secondary: [0.52, 0.49, 0.41], roughness: 0.91 },
    "ember-iris": { color: [0.4, 0.19, 0.045], secondary: [0.09, 0.045, 0.012], roughness: 0.19 },
  };
  for (const material of fixture.project.documents.filter(
    (document): document is MaterialDefinition => document.kind === "material",
  )) {
    if (colors[material.id]) Object.assign(material, colors[material.id]);
    if (material.id.includes("coat") || material.id.includes("hair") || material.id === "ashen-face") {
      material.pattern = "noise";
      material.scale = 55;
      material.normalStrength = 0.025;
      material.creature = { family: "fiber", anisotropy: 0.35, sheen: 0.16, fiberDirection: [0, 0, -1] };
    }
  }
  const animated = applyCharacterMotionLookdev(character);
  return {
    documents: fixture.project.documents.map((document) =>
      document.id === character.id ? animated : document,
    ),
    character: character.id,
  };
}
