import type { GrowthSpecies } from "./botanical-growth";

/** Dimensions are meters. Development/resource coefficients are hypotheses, not calibrated years.
 * Sources anchor anatomy; distributions require the held-out study in the growth plan. */
export const botanicalSpeciesTraits: Record<
  GrowthSpecies,
  {
    commonName: string;
    scientificName: string;
    source: string;
    calibrated: false;
    extension: number;
    lateralLength: number;
    branchAngle: number;
    whorl: number;
    apicalControl: number;
    maxOrder: number;
    retention: number;
    shadeTolerance: number;
    gravity: number;
    phototropism: number;
    needleBundle: number;
    leafLength: number;
    leafWidth: number;
  }
> = {
  "lodgepole-pine": {
    commonName: "Rocky Mountain lodgepole pine",
    scientificName: "Pinus contorta var. latifolia",
    source: "https://research.fs.usda.gov/feis/species-reviews/pinconl",
    calibrated: false,
    extension: 0.48,
    lateralLength: 0.52,
    branchAngle: 1.4,
    whorl: 3,
    apicalControl: 0.88,
    maxOrder: 3,
    retention: 5,
    shadeTolerance: 0.15,
    gravity: 0.13,
    phototropism: 0.32,
    needleBundle: 2,
    leafLength: 0.055,
    leafWidth: 0.0018,
  },
  "paper-birch": {
    commonName: "Paper birch",
    scientificName: "Betula papyrifera",
    source: "https://research.fs.usda.gov/silvics/paper-birch",
    calibrated: false,
    extension: 0.43,
    lateralLength: 0.72,
    branchAngle: 0.8,
    whorl: 1,
    apicalControl: 0.52,
    maxOrder: 4,
    retention: 1,
    shadeTolerance: 0.11,
    gravity: 0.24,
    phototropism: 0.5,
    needleBundle: 1,
    leafLength: 0.065,
    leafWidth: 0.045,
  },
};
