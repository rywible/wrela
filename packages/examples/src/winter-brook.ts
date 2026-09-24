import type { Project } from "@wrela/model";

/** Authored scene changes, never capture-time removal of expected geometry. */
export function applyWinterBrookLayout(project: Project): Project {
  project.name = "Winter brook trail";
  const world = project.documents.find((document) => document.kind === "world");
  const terrain = project.documents.find((document) => document.kind === "terrain");
  const water = project.documents.find((document) => document.kind === "water");
  const sky = project.documents.find((document) => document.id === "winter-sky");
  const stone = project.documents.find((document) => document.id === "stone");
  if (!world || !terrain || !water || sky?.kind !== "environment" || stone?.kind !== "material")
    throw Error("Winter brook sources missing");
  terrain.interventions = [
    { id: "home-clearing", kind: "flatten", center: [0, 0], radius: 18, strength: 1, targetHeight: 0 },
    { id: "hero-sightline", kind: "clearing", center: [0, 2], radius: 20, strength: 1, targetHeight: 0 },
    ...Array.from({ length: 8 }, (_, index) => ({
      id: `brook-${index}`,
      kind: "river" as const,
      center: [5 + Math.sin(index * 0.6) * 0.6, -11 + index * 3] as [number, number],
      radius: 4.2,
      strength: 1.1,
      targetHeight: -0.3,
    })),
  ];
  water.level = -0.3;
  water.color = [0.06, 0.2, 0.23];
  water.roughness = 0.08;
  water.waves = water.waves.map((wave) => ({ ...wave, amplitude: wave.amplitude * 0.5 }));
  sky.sunElevation = 0.2;
  sky.sunAzimuth = -0.65;
  sky.fogDensity = 0.00045;
  sky.turbidity = 2;
  sky.skyColor = [0.2, 0.36, 0.52];
  sky.horizonColor = [0.82, 0.65, 0.44];
  sky.groundColor = [0.58, 0.67, 0.7];
  sky.wind = [0.5, 0, 0.2];
  const daylight = project.documents.find((document) => document.id === "daylight");
  if (daylight?.kind === "lighting")
    for (const light of daylight.lights) if (light.type === "directional") light.color = [1, 1, 1];
  stone.color = [0.1, 0.17, 0.19];
  stone.secondary = [0.28, 0.34, 0.34];
  stone.roughness = 0.17;
  stone.normalStrength = 0.07;
  for (const rule of world.populations) {
    rule.density = 0.85;
    rule.spacing = 7;
  }
  const existingStone = world.instances.find((instance) => instance.id === "stone-instance");
  if (existingStone) {
    existingStone.position = [2.7, -0.34, -0.5];
    existingStone.scale = 0.65;
  }
  for (let index = 0; index < 14; index++) {
    const x = (index % 2 ? 7 : 2.5) + Math.sin(index) * 0.2,
      z = -8 + Math.floor(index / 2) * 2.4;
    // A broad authored seat keeps the stone grounded across resident terrain tessellation.
    terrain.interventions.push({
      id: `stone-seat-${index}`,
      kind: "flatten",
      center: [x, z],
      radius: 1.6,
      strength: 1,
      targetHeight: -0.45,
    });
    world.instances.push({
      id: `brook-stone-${index}`,
      definition: "river-stone",
      position: [x, -0.61, z],
      rotation: [0, index * 0.7, 0.08 * Math.sin(index)],
      scale: 0.22 + (index % 3) * 0.12,
    });
  }
  const base = (id: string, name: string) => ({ id, name, schemaVersion: 1 as const, dependencies: [] });
  project.documents.push({
    ...base("packed-snow", "Packed trail snow"),
    kind: "material",
    color: [0.42, 0.51, 0.54],
    secondary: [0.55, 0.63, 0.64],
    roughness: 0.95,
    metallic: 0,
    pattern: "solid",
    scale: 1,
    normalStrength: 0,
  });
  project.documents.push({
    ...base("trail-print", "Bunny trail footprint"),
    kind: "object",
    material: "packed-snow",
    collision: "none",
    field: {
      root: "print",
      nodes: [
        {
          id: "print",
          name: "Compressed snow",
          kind: "ellipsoid",
          position: [0, 0, 0],
          rotation: [0, 0, 0],
          size: [0.105, 0.012, 0.16],
          radius: 1,
          blend: 0,
          children: [],
        },
      ],
      bounds: { min: [-0.12, -0.02, -0.18], max: [0.12, 0.02, 0.18] },
      resolution: 16,
    },
  });
  for (let step = 0; step < 12; step++) {
    terrain.interventions.push({
      id: `trail-bed-${step}`,
      kind: "flatten",
      center: [0, 0.65 + step * 0.6],
      radius: 1.2,
      strength: 1,
      targetHeight: 0,
    });
    for (const side of [-1, 1]) {
      const z = 0.65 + step * 0.6,
        x = side * 0.18 + Math.sin(step * 0.25) * 0.25;
      world.instances.push({
        id: `trail-print-${step}-${side}`,
        definition: "trail-print",
        position: [x, 0.018, z],
        rotation: [0, side * 0.12, 0],
        scale: 1,
      });
    }
  }
  return project;
}
