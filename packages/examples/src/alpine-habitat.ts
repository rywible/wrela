import type { Document, VegetationDefinition, WorldDefinition, WorldPathGeometry } from "@wrela/model";
import {
  alpineSurfaceHistory,
  botanicalPreset,
  type SurfaceHistory,
  samplePlantCommunity,
} from "@wrela/model";

type Instance = WorldDefinition["instances"][number];
type HabitatPatch = {
  id: string;
  center: [number, number];
  radius: [number, number];
  grass: number;
  fern: number;
  shrub: number;
  seed: number;
};

/** Local habitat patches are authored around the dry approach and creek banks.
 * Stable patch IDs keep individual placement exceptions intact during edits. */
const patches: readonly HabitatPatch[] = [
  { id: "south-west-sedge", center: [-10.5, -18], radius: [3, 4], grass: 12, fern: 2, shrub: 1, seed: 41 },
  { id: "approach-verge", center: [-10.5, -7], radius: [2.5, 5], grass: 12, fern: 3, shrub: 1, seed: 57 },
  { id: "clearing-edge", center: [-0.5, 2], radius: [2, 3.5], grass: 8, fern: 3, shrub: 1, seed: 73 },
  { id: "gate-verge", center: [-15, 6], radius: [2.5, 3], grass: 9, fern: 2, shrub: 1, seed: 91 },
  { id: "east-bank", center: [13, -11], radius: [3.5, 5], grass: 12, fern: 4, shrub: 1, seed: 113 },
  { id: "creek-bend", center: [11, 3], radius: [2, 4], grass: 9, fern: 4, shrub: 1, seed: 137 },
  { id: "far-bank", center: [15, 12], radius: [3, 4], grass: 10, fern: 3, shrub: 1, seed: 151 },
];

function herb(id: string, species: "grass" | "fern" | "shrub", material: string): VegetationDefinition {
  const botanical = botanicalPreset(species);
  botanical.age = 1;
  botanical.growth.asymmetry = 0.78;
  botanical.damage.leafLoss = 0.035;
  botanical.motion.stiffness = species === "shrub" ? 0.55 : 0.2;
  if (species === "grass") {
    botanical.canopy.clusters = 1;
    botanical.canopy.leavesPerCluster = 3;
    botanical.canopy.leafLength = 0.6;
    botanical.canopy.leafWidth = 0.028;
  }
  return {
    id,
    name:
      species === "grass"
        ? "Alpine sedge tuft"
        : species === "fern"
          ? "Creekside fern"
          : "Dwarf alpine shrub",
    kind: "vegetation",
    schemaVersion: 1,
    dependencies: [material],
    seed: species === "grass" ? 612 : species === "fern" ? 703 : 818,
    height: species === "grass" ? 0.6 : species === "fern" ? 0.65 : 1.3,
    radius: species === "grass" ? 0.38 : species === "fern" ? 0.55 : 0.75,
    branches: species === "grass" ? 18 : species === "fern" ? 9 : 13,
    material,
    trunkMaterial: material,
    windResponse: species === "shrub" ? 0.14 : 0.35,
    variation: 0.7,
    botanical,
  };
}

export function createAlpineHabitat(
  options: { history?: SurfaceHistory; paths?: readonly (WorldPathGeometry & { width: number })[] } = {},
): {
  documents: Document[];
  placements: Instance[];
  evidence: { id: string; wetness: number; shelter: number; plants: number }[];
} {
  const history = options.history ?? alpineSurfaceHistory();
  const species = {
    grass: herb("alpine-lookdev-sedge", "grass", "alpine-lookdev-understory-dry"),
    fern: herb("alpine-lookdev-fern", "fern", "alpine-lookdev-understory"),
    shrub: herb("alpine-lookdev-dwarf-shrub", "shrub", "alpine-lookdev-understory"),
  };
  const placements: Instance[] = [],
    evidence = [];
  for (const patch of patches) {
    const shelter = /gate|verge/.test(patch.id) ? 0.58 : /bank|creek/.test(patch.id) ? 0.2 : 0.38;
    const members = samplePlantCommunity({
      ...patch,
      counts: { grass: patch.grass, fern: patch.fern, shrub: patch.shrub },
      shelter,
      history,
      paths: options.paths,
    });
    for (const member of members)
      placements.push({
        id: member.id,
        definition: species[member.species].id,
        position: member.position,
        rotation: [0, member.yaw, 0],
        scale: member.scale,
        grounding: { offset: member.species === "grass" ? -0.035 : -0.05 },
      });
    evidence.push({
      id: patch.id,
      wetness: members.reduce((sum, member) => sum + member.history.wetness, 0) / Math.max(1, members.length),
      shelter,
      plants: members.length,
    });
  }
  return { documents: Object.values(species), placements, evidence };
}
