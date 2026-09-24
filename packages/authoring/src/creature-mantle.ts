import { type CharacterDefinition, creatureClothSchema, type Vec3 } from "@wrela/model";

import { authorCreatureGarment } from "./creature-coherence";
import type { CreatureOperation } from "./creature-commands";

export type CreatureMantleRecipe = {
  id: string;
  region: string;
  material: string;
  shoulderWidth: number;
  backDepth: number;
  length: number;
  flare: number;
  /** Region-local collar offset accommodates asymmetric neck placement. */
  collarOffset?: Vec3;
  /** Pattern gores are sewn into one continuous surface and physical lattice. */
  panels?: number;
  thickness?: number;
};
const mix = (a: Vec3, b: Vec3, t: number): Vec3 =>
  a.map((value, axis) => value + (b[axis] - value) * t) as Vec3;
const smooth = (value: number) => value * value * (3 - 2 * value);

/** A continuous shoulder-to-hem loft avoids open seams between independent
 * simulations. Fitted corners, shoulder stays and drape are editable source. */
export function authorCreatureMantle(
  character: CharacterDefinition,
  recipe: CreatureMantleRecipe,
): CreatureOperation[] {
  const source = character.creature;
  if (!source) throw Error("A mantle requires creature anatomy");
  const parent = source.regions.find((entry) => entry.id === recipe.region);
  if (!parent) throw Error("Unknown mantle fitting region");
  const joint = parent.jointIds.includes("spine") ? "spine" : parent.jointIds[0];
  if (!joint || !character.joints.some((entry) => entry.id === joint))
    throw Error("Mantle fitting needs a supporting joint");
  const region = {
    ...structuredClone(parent),
    id: `${recipe.id}-fit`,
    name: "Mantle fitting",
    parent: parent.id,
    nodeIds: [],
    jointIds: [joint],
    material: recipe.material,
  };
  const panels = recipe.panels ?? 6;
  if (!Number.isInteger(panels) || panels < 4 || panels > 8)
    throw Error("A mantle needs four to eight pattern gores");
  for (const [name, value, min, max] of [
    ["shoulder width", recipe.shoulderWidth, 0.2, 2],
    ["back depth", recipe.backDepth, 0.08, 1],
    ["length", recipe.length, 0.25, 2.5],
    ["flare", recipe.flare, 0, 0.5],
  ] as const)
    if (!Number.isFinite(value) || value < min || value > max) throw Error(`Invalid mantle ${name}`);
  if (!/^[a-zA-Z0-9_-]{1,50}$/.test(recipe.id)) throw Error("Mantle needs a short source identity");
  const collarOffset = recipe.collarOffset ?? [0, 0, 0];
  if (collarOffset.some((value) => !Number.isFinite(value) || Math.abs(value) > 1))
    throw Error("Invalid mantle collar offset");
  if (source.cloth.length >= 32) throw Error("Mantle exceeds the cloth panel budget");
  if (
    source.charts.some((entry) => entry.id.startsWith(`${recipe.id}-`)) ||
    source.landmarks.some((entry) => entry.id.startsWith(`${recipe.id}-`))
  )
    throw Error("Mantle identity already exists");

  const arc = 2.16,
    rows = 9,
    columns = 33;
  const sample = (u: number, v: number): Vec3 => {
    const angle = (u * 2 - 1) * arc,
      yoke = smooth(Math.min(1, v / 0.19));
    const skirt = Math.max(0, (v - 0.19) / 0.81);
    const halfWidth = 0.115 + (recipe.shoulderWidth * 0.5 - 0.115) * yoke + recipe.flare * skirt * skirt;
    const depth = 0.125 + (recipe.backDepth - 0.125) * yoke + (recipe.flare * 0.7 + 0.025) * skirt;
    const fold = Math.sin(u * Math.PI * panels + v * 0.7) * 0.014 * smooth(Math.min(1, skirt * 3));
    // A hanging catenary-like arc across the back gives the hem a deliberate
    // center drop and short front opening, rather than a rectangular sheet edge.
    const front = Math.max(0, -Math.cos(angle));
    const centerDrop = 0.055 * Math.max(0, Math.cos(angle));
    const scallop = 0.014 * Math.cos(u * Math.PI * panels);
    const y =
      0.315 -
      0.195 * yoke -
      recipe.length * skirt * (1 - front * 0.24) -
      centerDrop * skirt +
      scallop * skirt ** 4;
    return [
      (halfWidth + fold) * Math.sin(angle) + collarOffset[0] * (1 - yoke),
      y + collarOffset[1] * (1 - yoke),
      -(depth + fold) * Math.cos(angle) + collarOffset[2] * (1 - yoke),
    ];
  };
  const points: [Vec3, Vec3, Vec3, Vec3] = [sample(0, 0), sample(1, 0), sample(0, 1), sample(1, 1)];
  const controlOffsets = Array.from({ length: rows }, (_, row) =>
    Array.from({ length: columns }, (_, column): Vec3 => {
      const u = column / (columns - 1),
        v = row / (rows - 1),
        p = sample(u, v);
      const base = mix(mix(points[0], points[1], u), mix(points[2], points[3], u), v);
      return p.map((value, axis) => value - base[axis]) as Vec3;
    }),
  );
  const working = structuredClone(character),
    operations: CreatureOperation[] = [
      { kind: "creature.region", target: character.id, value: region },
      {
        kind: "creature.influence",
        target: character.id,
        value: {
          id: `${recipe.id}-binding`,
          region: region.id,
          allowedJoints: [joint],
          excludedJoints: [],
          rigidJoint: joint,
        },
      },
    ];
  working.creature?.regions.push(region);
  const landmarks = points.map((position, index) => ({
    id: `${recipe.id}-corner-${index}`,
    region: region.id,
    position,
  }));
  for (const value of landmarks) {
    operations.push({ kind: "creature.landmark", target: character.id, value });
    working.creature?.landmarks.push(value);
  }
  const fitted = authorCreatureGarment(working, {
    id: recipe.id,
    region: region.id,
    landmarks: landmarks.map((entry) => entry.id) as [string, string, string, string],
    material: recipe.material,
    thickness: recipe.thickness ?? 0.004,
  });
  const stays: { anchor: string; coordinates: [number, number]; weight: number }[] = [];
  // Shoulder stays carry the yoke along the actual skin while the continuous
  // cloth below remains free. Sharing a lattice removes dynamic seam separation.
  for (let index = 0; index <= 8; index++) {
    const coordinates: [number, number] = [index / 8, 0.125];
    const anchor = `${recipe.id}-shoulder-${index}-stay`;
    operations.push({
      kind: "creature.anchor",
      target: character.id,
      value: {
        id: anchor,
        region: region.id,
        chart: `${recipe.id}-surface`,
        chartRevision: 0,
        coordinates: [coordinates[0], coordinates[1], 0],
        offset: 0,
        purpose: "attachment",
        tolerance: 0.01,
      },
    });
    stays.push({ anchor, coordinates, weight: 1 });
  }
  for (const operation of fitted) {
    if (operation.kind === "creature.chart" && operation.value.kind === "patch")
      operation.value = { ...operation.value, controlOffsets };
    if (operation.kind === "creature.cloth")
      operation.value = creatureClothSchema.parse({
        ...operation.value,
        simulationResolution: 16,
        pinEdges: ["v0"],
        pins: [...operation.value.pins, ...stays],
        stiffness: 1,
        bendStiffness: 0.12,
        damping: 0.09,
        wind: [0.08, 0, -0.16],
        iterations: 32,
        maxStretch: 1.035,
      });
    operations.push(operation);
  }
  return operations;
}
