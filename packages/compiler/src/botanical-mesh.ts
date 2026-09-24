import {
  add,
  botanicalSpeciesTraits,
  cross,
  normalize,
  type Quality,
  scale,
  sub,
  type Vec3,
  type VegetationDefinition,
} from "@wrela/model";

import { bindBotanicalMotion } from "./botanical-motion";
import { builder, finish, MAX_BOTANICAL_VERTICES, stem } from "./botanical-primitives";
import { botanicalBranchPoint, botanicalRandom, botanicalStructure } from "./botanical-structure";
import { botanicalWood } from "./botanical-surfaces";
import { coniferMeshes } from "./conifer-mesh";
import { leafCoverageTile, leafRibbon } from "./leaf-ribbons";
export const MAX_BOTANICAL_LEAVES = 16000;
export function botanicalMeshes(doc: VegetationDefinition, quality: Quality, distant = false) {
  const source = doc.botanical;
  if (!source) throw new Error("Botanical mesh requires botanical authoring");
  if (source.species === "pine" && (source.conifer || source.development))
    return coniferMeshes(doc, quality, distant);
  const structure = botanicalStructure(doc);
  const wood = builder(),
    leaves = builder();
  const coverageUV: number[] = [];
  const traits = source.development ? botanicalSpeciesTraits[source.development.species] : undefined;
  const coverage = leafCoverageTile(source.species);
  const sides = distant ? 3 : quality === "interactive" ? 4 : quality === "review" ? 6 : 8;
  const parentIds = new Set(structure.branches.map((branch) => branch.parent));
  let leafCount = 0,
    truncated = structure.truncated;
  const herb = source.species === "grass" || source.species === "fern";
  const random = (key: string) => botanicalRandom(doc.seed, key);
  for (const branch of structure.branches) {
    const id = `${doc.id}/${branch.id}`;
    const mid = botanicalBranchPoint(branch, 0.5);
    if (source.species !== "grass") {
      if (source.development) {
        const woodSides = branch.habit === "short" ? 3 : sides;
        const woodSegments = branch.habit === "short" || distant ? 1 : 3;
        if (wood.positions.length / 3 + (woodSides + 1) * (woodSegments + 1) + 1 > MAX_BOTANICAL_VERTICES)
          truncated = true;
        else botanicalWood(wood, branch, woodSides, woodSegments, id, "birch", source.age);
      } else {
        stem(wood, branch.start, mid, branch.radius, branch.radius * 0.72, sides, id);
        stem(wood, mid, branch.end, branch.radius * 0.72, branch.radius * 0.25, sides, id);
      }
    }
    const leader = !source.development && branch.level === 0 && source.species !== "shrub";
    if (branch.bare || (!source.development && !leader && parentIds.has(branch.id))) continue;
    for (let cluster = 0; cluster < (traits ? 1 : source.canopy.clusters); cluster++) {
      const key = `${branch.id}/c${cluster}`;
      if (random(`${key}/density`) > source.canopy.density) continue;
      const point = botanicalBranchPoint(
        branch,
        (leader ? 0.9 : 0.3) + ((cluster + 0.5) / source.canopy.clusters) * (leader ? 0.1 : 0.7),
      );
      for (let leaf = 0; leaf < (branch.cohort?.count ?? source.canopy.leavesPerCluster); leaf++) {
        const keyLeaf = `${key}/l${leaf}`;
        if (random(`${keyLeaf}/loss`) < 1 - (1 - source.damage.leafLoss) * (branch.cohort?.retention ?? 1))
          continue;
        if (leafCount >= MAX_BOTANICAL_LEAVES || leaves.positions.length / 3 + 24 > MAX_BOTANICAL_VERTICES) {
          truncated = true;
          break;
        }
        const angle = leaf * 2.39996 + random(`${keyLeaf}/angle`) * 0.5;
        const along = normalize(sub(branch.end, branch.start));
        let vector: Vec3 = herb
          ? add(scale(along, 0.6), [Math.cos(angle) * 0.4, 0.2, Math.sin(angle) * 0.4])
          : normalize([Math.cos(angle), (random(`${keyLeaf}/lift`) - 0.25) * 1.3, Math.sin(angle)]);
        let length =
          (traits?.leafLength ?? source.canopy.leafLength) *
          (0.65 + source.age * 0.35) *
          (0.75 + random(`${keyLeaf}/size`) * 0.5);
        let base = traits
          ? botanicalBranchPoint(branch, 0.1 + ((leaf + 0.5) / Math.max(1, branch.cohort?.count ?? 1)) * 0.85)
          : point;
        if (source.species === "grass") {
          base = branch.start;
          length *= Math.hypot(...sub(branch.end, branch.start)) / 0.65;
        } else if (source.species === "fern") {
          const t =
            0.15 +
            ((cluster + (leaf + 0.5) / source.canopy.leavesPerCluster) / source.canopy.clusters) * 0.85;
          base = botanicalBranchPoint(branch, t);
          const side = normalize(cross(along, [0, 1, 0]));
          vector = normalize(add(scale(along, 0.45), scale(side, leaf % 2 ? -1 : 1)));
          length *= Math.sin(Math.PI * t) ** 0.65;
        }
        const width = traits?.leafWidth ?? source.canopy.leafWidth;
        const tint = branch.cohort
          ? branch.cohort.tint * (0.82 + random(`${keyLeaf}/tint`) * 0.18)
          : 0.72 + random(`${keyLeaf}/tint`) * 0.28;
        if (traits) {
          const attachment = base;
          base = add(base, scale(vector, 0.018 * (0.8 + random(`${keyLeaf}/petiole`) * 0.4)));
          if (wood.positions.length / 3 + 8 > MAX_BOTANICAL_VERTICES) truncated = true;
          else
            botanicalWood(
              wood,
              {
                id: `${keyLeaf}/petiole`,
                parent: branch.id,
                level: branch.level + 1,
                start: attachment,
                bend: scale(add(attachment, base), 0.5),
                end: base,
                radius: 0.00055,
                tipRadius: 0.00035,
                bare: true,
                broken: false,
              },
              3,
              1,
              `${doc.id}/${keyLeaf}/petiole`,
              "birch",
              source.age,
            );
        }
        leafRibbon(
          leaves,
          coverageUV,
          base,
          vector,
          length,
          width,
          angle,
          source.canopy.droop,
          [tint, tint, tint],
          `${doc.id}/${keyLeaf}`,
          distant ? 1 : 2,
        );
        leafCount++;
      }
    }
  }
  return {
    trunk: bindBotanicalMotion(finish(wood), doc, structure.branches, false),
    foliage: bindBotanicalMotion(
      { ...finish(leaves), thinCoverage: { ...coverage, uv: new Float32Array(coverageUV) } },
      doc,
      structure.branches,
      true,
    ),
    structure,
    leafCount,
    truncated,
  };
}
