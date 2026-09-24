import { createLookdevMaterials } from "@wrela/examples/material-lookdev";
import { createWaysideArchitecture } from "@wrela/examples/wayside-lookdev";
import type {
  AssemblyDefinition,
  AssemblyPart,
  AssemblyProfile,
  Document,
  ObjectDefinition,
  Vec3,
} from "@wrela/model";
import {
  alpineSurfaceHistory,
  applyConstructionHistory,
  assemblySchema,
  type ConstructionHistoryRecord,
  type ConstructionRecipe,
  constructionRecipeSchema,
  createConstructionBuilder,
  createSurfaceHistoryPalette,
  cutStoneProfile,
  type SurfaceHistory,
  sampleSurfaceHistory,
} from "@wrela/model";

const STONE = "alpine-lookdev-masonry",
  WOOD = "alpine-lookdev-wood",
  IRON = "alpine-lookdev-iron",
  BRONZE = "alpine-lookdev-bronze",
  DAMP_STONE = "alpine-lookdev-rock-dark";
/** A human-scale, partly collapsed alpine gateway; materials come from createLookdevMaterials. */
export type ArchitectureLookdevOptions = {
  variant?: "gateway" | "wayside";
  seed?: number;
  history?: SurfaceHistory;
};
export type ArchitectureLookdevBundle = {
  documents: Document[];
  object: string;
  recipe: ConstructionRecipe;
  history: ConstructionHistoryRecord[];
};
export function createArchitectureLookdev(
  options: ArchitectureLookdevOptions = {},
): ArchitectureLookdevBundle {
  const history = options.history ?? alpineSurfaceHistory();
  const recipe = constructionRecipeSchema.parse({ seed: options.seed ?? 73, history, stoneMaterial: STONE });
  if (options.variant === "wayside") return createWaysideArchitecture(recipe);
  const builder = createConstructionBuilder(recipe);
  const parts = builder.parts;
  const part = (
    id: string,
    profile: AssemblyProfile,
    position: Vec3,
    depth: number,
    material = STONE,
    bevel = 0.02,
  ): AssemblyPart => builder.part(id, profile, position, depth, material, bevel);
  const block = (
    id: string,
    width: number,
    height: number,
    position: Vec3,
    depth: number,
    material = STONE,
    bevel = 0.02,
  ) => {
    if (material !== STONE && material !== DAMP_STONE)
      return part(id, { kind: "rectangle", width, height }, position, depth, material, bevel);
    const signals = sampleSurfaceHistory(history, { position, normal: [0, 0, -1] });
    const profile = cutStoneProfile(width, height, recipe.seed, id, signals.damage);
    return part(id, profile, position, depth, material, bevel * 0.35);
  };
  // Alternating jamb headers and stretchers show actual masonry courses and mortar gaps.
  for (const side of [-1, 1])
    for (let course = 0; course < 7; course++) {
      const width = course % 2 === 0 ? 0.6 : 0.55;
      const stone = block(
        `${side < 0 ? "west" : "east"}-jamb-${course + 1}`,
        width,
        0.294,
        [side * (1.045 + width / 2), 0.153 + course * 0.306, -0.33 + ((course % 3) - 1) * 0.008],
        0.65 + (course % 2) * 0.04,
        course === 0 ? DAMP_STONE : STONE,
      );
      stone.rotation[2] = Math.sin(course * 5 + side) * 0.007;
    }
  block("west-footing", 0.86, 0.16, [-1.34, 0.08, -0.44], 0.86, DAMP_STONE);
  block("east-footing", 0.82, 0.16, [1.32, 0.08, -0.44], 0.84, DAMP_STONE);
  block("west-impost", 0.73, 0.13, [-1.35, 2.19, -0.37], 0.74);
  block("east-impost", 0.73, 0.13, [1.35, 2.19, -0.37], 0.74);
  // True voussoir wedges follow the arch thrust line instead of stacked boxes.
  const count = 11,
    inner = 1.045,
    outer = 1.57,
    spring = 2.22;
  const jointAngle = (index: number) =>
    (index * Math.PI) / count + (index === 0 || index === count ? 0 : Math.sin(index * 4.47) * 0.012);
  for (let i = 0; i < count; i++) {
    const a = jointAngle(i) + 0.006,
      b = jointAngle(i + 1) - 0.006,
      outerA = outer + Math.sin(i * 2.7) * 0.035,
      outerB = outer + Math.sin(i * 3.9 + 1) * 0.03;
    const points: [number, number][] = [
      [Math.cos(a) * inner, Math.sin(a) * inner],
      [Math.cos(a) * outerA, Math.sin(a) * outerA],
      [Math.cos(b) * outerB, Math.sin(b) * outerB],
      [Math.cos(b) * inner, Math.sin(b) * inner],
    ];
    part(
      `arch-voussoir-${i + 1}`,
      { kind: "polygon", points },
      [0, spring, -0.34 + Math.sin(i * 5.1) * 0.013],
      0.66 + (i % 3) * 0.023,
      STONE,
      0.018,
    );
  }
  // A proud keystone and thin drip courses give the portal a readable silhouette.
  part(
    "proud-keystone",
    {
      kind: "polygon",
      points: [
        [-0.1, 0],
        [0.1, 0],
        [0.155, 0.46],
        [-0.155, 0.46],
      ],
    },
    [0, 3.265, -0.39],
    0.8,
    STONE,
    0.013,
  );
  for (const side of [-1, 1]) {
    block(`${side < 0 ? "west" : "east"}-base-course`, 1.55, 0.17, [side * 2.4, 0.095, -0.32], 0.73);
    for (let row = 0; row < (side < 0 ? 6 : 3); row++) {
      const end = side < 0 ? 3 - (row > 3 ? 0.3 * (row - 3) : 0) : 3.25 - row * 0.25;
      const width = (end - 1.65) / 2 - 0.012;
      for (let column = 0; column < 2; column++) {
        const x = side * (1.65 + width / 2 + column * (width + 0.024));
        const stone = block(
          `${side < 0 ? "west" : "east"}-wall-${row}-${column}`,
          width,
          0.285,
          [x, 0.33 + row * 0.304, -0.255 + Math.sin(row * 7 + column) * 0.022],
          0.6 + (row % 2) * 0.025,
        );
        stone.rotation[2] = column === 1 && row > 2 ? side * 0.014 : 0;
      }
    }
  }
  // A battered buttress carries the springing course into each wall remnant.
  // The offset courses break the straight extrusion silhouette at oblique angles.
  for (const side of [-1, 1]) {
    for (let course = 0; course < 4; course++) {
      const width = 0.54 - course * 0.045;
      const buttress = block(
        `${side < 0 ? "west" : "east"}-buttress-${course}`,
        width,
        0.275,
        [side * (1.72 + width / 2 - course * 0.015), 0.22 + course * 0.28, -0.55],
        0.89 - course * 0.045,
        course === 0 ? DAMP_STONE : STONE,
      );
      buttress.rotation[2] = side * course * 0.006;
    }
  }
  // Uneven threshold slabs tie the gateway to the walking surface. Their tops
  // stay below the reserved standing clearance, so traversal is still open.
  for (let i = 0; i < 4; i++) {
    const slab = block(
      `threshold-slab-${i}`,
      0.49 + (i % 2) * 0.06,
      0.085,
      [-0.77 + i * 0.51, 0.012 + (i % 2) * 0.006, -0.94 - (i % 2) * 0.055],
      1.49 + (i % 3) * 0.06,
      DAMP_STONE,
      0.022,
    );
    slab.rotation[1] = (i - 1.5) * 0.008;
    slab.wear.amount = 0.4;
    slab.wear.scale = 8;
  }
  // Broken, rotated fallen blocks continue the wall rhythm into the ground.
  for (let i = 0; i < 7; i++) {
    const width = 0.28 + (i % 3) * 0.095,
      height = 0.16 + (i % 2) * 0.09;
    const stone = part(
      `fallen-block-${i}`,
      {
        kind: "polygon",
        points: [
          [-width * 0.48, -height * 0.5],
          [width * 0.34, -height * 0.5],
          [width * 0.5, -height * 0.18],
          [width * 0.23, height * 0.47],
          [-width * 0.3, height * 0.5],
          [-width * 0.5, height * 0.04],
        ],
      },
      [1.75 + i * 0.23, 0.1 + (i % 2) * 0.03, -0.67 - (i % 3) * 0.27],
      0.27 + (i % 3) * 0.07,
      i % 3 === 0 ? DAMP_STONE : STONE,
      0.008,
    );
    stone.rotation = [0.04 * i, 0.31 * i, 0.045 * (i - 2)];
    stone.wear.amount = 0.24;
  }
  // One hinged timber leaf: asymmetry leaves the arched opening visible.
  const hinge = block("gate-hinge-stile", 0.105, 2.07, [-0.925, 1.08, -0.43], 0.12, WOOD, 0.008);
  hinge.joint = {
    kind: "hinge",
    axis: [0, 1, 0],
    pivot: [0, 0, 0],
    minimum: 0.18,
    maximum: 1.05,
    value: 0.42,
    drive: { period: 5, phase: 0.12 },
  };
  hinge.sockets = [
    { id: "hinge-midpoint", position: [0, 0, 0] },
    { id: "latch", position: [1.79, 0, -0.02] },
  ];
  for (const y of [-0.74, 0.74]) {
    const plate = block(
      `hinge-wear-plate-${y < 0 ? "lower" : "upper"}`,
      0.19,
      0.25,
      [0, y, -0.078],
      0.018,
      IRON,
      0.006,
    );
    plate.parent = hinge.id;
    plate.collision = false;
    plate.wear.amount = 0.46;
    plate.wear.scale = 9;
  }
  for (let i = 0; i < 9; i++) {
    const width = 0.186,
      top = 0.98 + Math.sin(i * 1.7) * 0.027;
    const plank = part(
      `gate-oak-plank-${i}`,
      {
        kind: "polygon",
        points: [
          [-width / 2, -1.0],
          [width / 2, -1.0],
          [width / 2, top - 0.008],
          [-width / 2, top],
        ],
      },
      [0.15 + i * 0.19, 0, 0.016],
      0.085,
      WOOD,
      0.004,
    );
    plank.parent = hinge.id;
    plank.wear.amount = 0.13;
    plank.wear.scale = 6;
  }
  const freeStile = block("gate-latch-stile", 0.105, 2.07, [1.79, 0, 0], 0.12, WOOD, 0.008);
  freeStile.parent = hinge.id;
  for (const y of [-0.74, 0.64]) {
    const strap = block(
      `iron-strap-${y < 0 ? "lower" : "upper"}`,
      1.94,
      0.073,
      [0.87, y, -0.032],
      0.025,
      IRON,
      0.009,
    );
    strap.parent = hinge.id;
    strap.collision = false;
    for (const x of [0, 0.48, 0.96, 1.45, 1.79]) {
      const rivet = part(
        `rivet-${y}-${x}`.replaceAll(".", "d").replaceAll("-0", "n0"),
        { kind: "circle", radius: 0.019, segments: 10 },
        [x, y, -0.045],
        0.016,
        IRON,
        0,
      );
      rivet.parent = hinge.id;
      rivet.collision = false;
    }
  }
  const brace = block("diagonal-oak-brace", 0.095, 0.095, [0, 0, 0], 0.1, WOOD, 0.008);
  brace.path = [
    [0.03, -0.79, 0.12],
    [1.75, 0.7, 0.12],
  ];
  brace.parent = hinge.id;
  brace.collision = false;
  const latch = block("bronze-latch-plate", 0.09, 0.2, [1.7, 0.03, -0.055], 0.024, BRONZE, 0.01);
  latch.parent = hinge.id;
  latch.collision = false;
  const handle = part(
    "bronze-latch-handle",
    { kind: "circle", radius: 0.018, segments: 10 },
    [1.7, 0.03, -0.075],
    0.1,
    BRONZE,
    0,
  );
  handle.path = [
    [0, 0, 0],
    [0.13, 0, 0],
  ];
  handle.parent = hinge.id;
  handle.collision = false;
  for (const y of [0.34, 1.82]) {
    const pintle = part(
      `hinge-pintle-${y}`.replaceAll(".", "d"),
      { kind: "circle", radius: 0.034, segments: 12 },
      [-0.94, y, -0.45],
      0.13,
      IRON,
      0,
    );
    pintle.path = [
      [0, -0.065, 0],
      [0, 0.065, 0],
    ];
    pintle.collision = false;
  }
  builder.rubble({
    id: "west-footing-fragment",
    center: [-2.4, -0.015, -0.75],
    radius: [0.65, 0.8],
    count: 9,
    size: 0.2,
  });
  builder.rubble({
    id: "east-collapse-fragment",
    center: [2.7, -0.01, -0.5],
    radius: [0.7, 0.8],
    count: 9,
    size: 0.16,
  });
  const palette = createSurfaceHistoryPalette(
    createLookdevMaterials().filter((material) =>
      [STONE, WOOD, IRON, BRONZE, DAMP_STONE].includes(material.id),
    ),
    history,
    "gateway-history",
  );
  const assembly: AssemblyDefinition = assemblySchema.parse({
    parts,
    grid: 0.025,
    clearances: [
      {
        id: "portal-clearance",
        name: "Portal standing clearance",
        min: [-0.94, 0.18, -0.2],
        max: [0.94, 2.05, 0.2],
      },
    ],
  });
  const historyRecords = applyConstructionHistory(assembly, recipe, palette.material);
  const object: ObjectDefinition = {
    id: "alpine-lookdev-gateway",
    name: "Weathered alpine toll gate",
    schemaVersion: 1,
    dependencies: [...new Set(assembly.parts.map((part) => part.material ?? STONE))],
    kind: "object",
    material: STONE,
    collision: "mesh",
    assembly,
    field: {
      root: "legacy-placeholder",
      nodes: [
        {
          id: "legacy-placeholder",
          name: "Assembly source retained",
          kind: "box",
          position: [0, 1.5, 0],
          rotation: [0, 0, 0],
          size: [1, 1.5, 0.3],
          radius: 0.1,
          blend: 0,
          children: [],
        },
      ],
      bounds: { min: [-3.5, -0.1, -1.8], max: [3.5, 3.95, 1] },
      resolution: 24,
    },
  };
  return { documents: [...palette.documents, object], object: object.id, recipe, history: historyRecords };
}
