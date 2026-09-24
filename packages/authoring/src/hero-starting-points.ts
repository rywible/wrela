import { type HeroDesignInput, heroDesignSchema } from "./hero-design";

/** Explicit editable construction starting points for Studio. Fresh blank-asset benchmarks do not use them. */
export function expeditionLantern(variant = 0): HeroDesignInput {
  const parts: HeroDesignInput["components"] = [];
  const shell = (
    id: string,
    name: string,
    profile: [number, number][],
    material: "iron" | "brass" = "iron",
  ) => parts.push({ id, name, kind: "shell", material, shell: { profile, segments: 64 } });
  shell(
    "reservoir",
    "Spun fuel reservoir",
    [
      [0.035, 0.004],
      [0.088, 0.004],
      [0.108, 0.016],
      [0.115, 0.035],
      [0.109, 0.057],
      [0.095, 0.066],
      [0.035, 0.066],
      [0.035, 0.06],
      [0.091, 0.06],
      [0.102, 0.053],
      [0.107, 0.035],
      [0.101, 0.022],
      [0.085, 0.012],
      [0.035, 0.012],
    ],
    "brass",
  );
  shell("hood", "Folded storm hood", [
    [0.133, 0.365],
    [0.139, 0.371],
    [0.137, 0.382],
    [0.096, 0.405],
    [0.054, 0.429],
    [0.047, 0.443],
    [0.04, 0.443],
    [0.047, 0.424],
    [0.091, 0.398],
    [0.128, 0.375],
    [0.126, 0.37],
  ]);
  shell(
    "chimney",
    "Chimney crown",
    [
      [0.044, 0.46],
      [0.048, 0.468],
      [0.039, 0.489],
      [0.021, 0.496],
      [0.02, 0.49],
      [0.034, 0.482],
      [0.04, 0.467],
      [0.039, 0.462],
    ],
    "brass",
  );
  for (const [id, y, r] of [
    ["lower-rim", 0.074, 0.108],
    ["upper-rim", 0.352, 0.111],
    ["hood-roll", 0.373, 0.135],
  ] as const)
    parts.push({
      id,
      name: id,
      kind: "ring",
      material: "brass",
      radius: r,
      tube: 0.005,
      position: [0, y, 0],
    });
  for (const x of [-1, 1])
    for (const z of [-1, 1])
      parts.push({
        id: `guard-${x < 0 ? "l" : "r"}-${z < 0 ? "b" : "f"}`,
        name: "Bowed cage stay",
        kind: "rod",
        material: "iron",
        radius: 0.005,
        path: [
          [x * 0.079, 0.066, z * 0.079],
          [x * 0.099, 0.14, z * 0.099],
          [x * 0.097, 0.29, z * 0.097],
          [x * 0.078, 0.36, z * 0.078],
        ],
      });
  parts.push({
    id: "door-hinge",
    name: "Opening front door",
    kind: "rod",
    material: "brass",
    radius: 0.0045,
    position: [-0.076, 0.077, 0.091],
    path: [
      [0, 0, 0],
      [0, 0.266, 0],
    ],
    joint: {
      kind: "hinge",
      axis: [0, 1, 0],
      pivot: [0, 0, 0],
      minimum: 0,
      maximum: 1.3,
      value: 0,
      drive: { period: 4, phase: 0 },
    },
  });
  for (const [id, path] of [
    [
      "door-top",
      [
        [0, 0.266, 0],
        [0.152, 0.266, 0],
      ],
    ],
    [
      "door-bottom",
      [
        [0, 0, 0],
        [0.152, 0, 0],
      ],
    ],
    [
      "door-right",
      [
        [0.152, 0, 0],
        [0.152, 0.266, 0],
      ],
    ],
  ] as const)
    parts.push({
      id,
      name: id,
      kind: "rod",
      material: "brass",
      radius: 0.0035,
      parent: "door-hinge",
      path: path.map((p) => [...p]),
    });
  parts.push({
    id: "front-glass",
    name: "Door glass",
    kind: "plate",
    material: "glass",
    width: 0.143,
    height: 0.251,
    thickness: 0.0025,
    position: [0.076, 0.133, 0],
    parent: "door-hinge",
    bevel: 0.001,
  });
  for (const [id, position, rotation] of [
    ["back-glass", [0, 0.21, -0.089], [0, 0, 0]],
    ["left-glass", [-0.089, 0.21, 0], [0, Math.PI / 2, 0]],
    ["right-glass", [0.089, 0.21, 0], [0, Math.PI / 2, 0]],
  ] as const)
    parts.push({
      id,
      name: id,
      kind: "plate",
      material: "glass",
      width: 0.145,
      height: 0.255,
      thickness: 0.0025,
      position: [...position],
      rotation: [...rotation],
      bevel: 0.001,
    });
  parts.push({
    id: "burner",
    name: "Wick brass cup",
    kind: "rod",
    material: "brass",
    radius: 0.023,
    path: [
      [0, 0.06, 0],
      [0, 0.095, 0],
    ],
  });
  parts.push({
    id: "wick",
    name: "Warm luminous wick",
    kind: "rod",
    material: "light",
    radius: 0.009,
    path: [
      [0, 0.092, 0],
      [0, 0.126, 0],
    ],
  });
  parts.push({
    id: "handle",
    name: "Forged carrying bail",
    kind: "rod",
    material: "iron",
    radius: 0.0055,
    path: Array.from({ length: 25 }, (_, i) => {
      const a = (i * Math.PI) / 24;
      return [Math.cos(a) * 0.103, 0.422 + Math.sin(a) * (0.12 + variant * 0.015), 0];
    }),
  });
  for (const x of [-1, 1])
    parts.push({
      id: `handle-pin-${x < 0 ? "l" : "r"}`,
      name: "Handle pivot pin",
      kind: "rod",
      material: "brass",
      radius: 0.013,
      path: [
        [x * 0.089, 0.42, 0],
        [x * 0.111, 0.42, 0],
      ],
    });
  for (let i = 0; i < 8; i++) {
    const a = (i * Math.PI) / 4;
    parts.push({
      id: `vent-${i}`,
      name: "Open chimney vent rib",
      kind: "rod",
      material: "iron",
      radius: 0.002,
      path: [
        [Math.cos(a) * 0.044, 0.439, Math.sin(a) * 0.044],
        [Math.cos(a) * 0.044, 0.469, Math.sin(a) * 0.044],
      ],
    });
  }
  return heroDesignSchema.parse({
    id: "hero-lantern",
    name:
      ["Wayfarer storm lantern", "Tall expedition lantern", "Broad storm lantern"][variant] ??
      "Storm lantern",
    family: "lantern",
    intent:
      "A hand-built expedition lantern with a spun brass reservoir, iron storm hood, slender cage, glass door and carrying bail. Distinct construction and restrained wear.",
    seed: 73 + variant,
    stage: "production",
    age: 0.48 + variant * 0.08,
    components: parts,
  });
}
export function hangingBell(): HeroDesignInput {
  return heroDesignSchema.parse({
    id: "hero-bell",
    name: "Wayside warning bell",
    family: "bell",
    intent:
      "A cast bronze bell suspended from a compact forged bracket, with a hollow flared mouth and articulated clapper.",
    age: 0.6,
    stage: "production",
    components: [
      {
        id: "body",
        name: "Hollow cast bell",
        kind: "shell",
        material: "brass",
        position: [0, 0.07, 0],
        shell: {
          segments: 64,
          profile: [
            [0.15, 0.01],
            [0.148, 0.028],
            [0.125, 0.052],
            [0.091, 0.104],
            [0.068, 0.17],
            [0.062, 0.222],
            [0.04, 0.239],
            [0.015, 0.24],
            [0.015, 0.23],
            [0.034, 0.23],
            [0.051, 0.215],
            [0.057, 0.17],
            [0.081, 0.105],
            [0.116, 0.049],
            [0.135, 0.025],
            [0.137, 0.01],
          ],
        },
      },
      {
        id: "mouth-bead",
        name: "Rolled mouth rim",
        kind: "ring",
        material: "brass",
        radius: 0.144,
        tube: 0.007,
        position: [0, 0.085, 0],
      },
      {
        id: "waist-band",
        name: "Cast waist band",
        kind: "ring",
        material: "brass",
        radius: 0.066,
        tube: 0.003,
        position: [0, 0.24, 0],
      },
      {
        id: "crown-loop",
        name: "Crown suspension eye",
        kind: "ring",
        material: "brass",
        radius: 0.026,
        tube: 0.007,
        position: [0, 0.334, 0],
        rotation: [Math.PI / 2, 0, 0],
      },
      {
        id: "hanger",
        name: "Forged hanger",
        kind: "rod",
        material: "iron",
        radius: 0.006,
        path: [
          [0, 0.335, 0],
          [0, 0.4, 0],
          [0.16, 0.4, 0],
          [0.16, 0.1, 0],
        ],
      },
      {
        id: "clapper",
        name: "Swinging clapper stem",
        kind: "rod",
        material: "iron",
        radius: 0.005,
        position: [0, 0.29, 0],
        path: [
          [0, 0, 0],
          [0, -0.17, 0],
        ],
        joint: {
          kind: "hinge",
          axis: [0, 0, 1],
          pivot: [0, 0, 0],
          minimum: -0.35,
          maximum: 0.35,
          value: 0,
          drive: { period: 3, phase: 0 },
        },
      },
      {
        id: "clapper-ball",
        name: "Clapper strike head",
        kind: "rod",
        material: "iron",
        radius: 0.022,
        path: [
          [0, -0.165, 0],
          [0, -0.19, 0],
        ],
        parent: "clapper",
      },
      {
        id: "mount",
        name: "Timber mounting block",
        kind: "beam",
        material: "wood",
        position: [0.17, 0, 0],
        path: [
          [0, 0.06, 0],
          [0, 0.43, 0],
        ],
        width: 0.058,
        depth: 0.08,
        bevel: 0.004,
      },
    ],
  });
}
