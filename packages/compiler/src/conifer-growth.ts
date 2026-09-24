import { add, normalize, scale, sub, type Vec3, type VegetationDefinition } from "@wrela/model";

import { type BotanicalBranch, botanicalBranchPoint, botanicalRandom } from "./botanical-branch";
import type { BotanicalStructure } from "./botanical-structure";
import { pineArchitecture } from "./pine-architecture";

/** 96 authored limbs with 40 lateral shoots each fit without clipping the crown. */
export const MAX_CONIFER_BRANCHES = 4096;

/** Whorled, irregular woody structure plus alternating short lateral shoots. */
export function coniferStructure(doc: VegetationDefinition): BotanicalStructure {
  const source = doc.botanical;
  const conifer = source?.conifer;
  if (!source || !conifer) throw new Error("Conifer growth requires authored conifer source");
  if (conifer.architecture) return pineArchitecture(doc);
  const height = doc.height * (0.25 + source.age * 0.75);
  const radius = doc.radius * (0.35 + source.age * 0.65);
  const random = (id: string) => botanicalRandom(doc.seed, id);
  const branches: BotanicalBranch[] = [];
  const edits = new Map(source.branchEdits.map((edit) => [edit.branch, edit]));
  const removed = new Set(source.pruning.removedBranches);
  if (removed.has("trunk")) return { branches, height, truncated: false };
  let truncated = false;
  const trunkEdit = edits.get("trunk");
  const trunkPoints: Vec3[] = [];
  const lean: Vec3 = [
    (random("lean-x") - 0.5) * height * 0.04 * doc.variation,
    0,
    (random("lean-z") - 0.5) * height * 0.04 * doc.variation,
  ];
  for (let i = 0; i <= 16; i++) {
    const t = i / 16;
    const point: Vec3 = [
      lean[0] * t + Math.sin(t * 9) * height * 0.003 * t,
      height * t,
      lean[2] * t + Math.sin(t * 11 + 1.7) * height * 0.003 * t,
    ];
    trunkPoints.push(
      add(
        scale(point, trunkEdit?.lengthScale ?? 1),
        scale(trunkEdit?.bend ?? [0, 0, 0], Math.sin(t * Math.PI) * height * 0.35),
      ),
    );
  }
  const trunk: BotanicalBranch = {
    id: "trunk",
    parent: null,
    level: 0,
    start: [0, 0, 0],
    bend: trunkPoints[8],
    end: trunkPoints[16],
    radius: conifer.trunkRadius * (0.3 + source.age * 0.7),
    bare: !!trunkEdit?.bare,
    broken: false,
    points: trunkPoints,
  };
  branches.push(trunk);
  const make = (id: string, parent: BotanicalBranch, level: number, points: Vec3[], thickness: number) => {
    if (removed.has(id)) return undefined;
    if (branches.length >= MAX_CONIFER_BRANCHES) {
      truncated = true;
      return undefined;
    }
    const edit = edits.get(id);
    const broken = level === 1 && random(`${id}/damage`) < source.damage.brokenBranches;
    const start = points[0];
    points = points.map((point, i) => {
      const t = i / (points.length - 1);
      let p = add(start, scale(sub(point, start), (edit?.lengthScale ?? 1) * (broken ? 0.28 : 1)));
      if (edit)
        p = add(
          p,
          scale(
            edit.bend,
            Math.sin(t * Math.PI) * Math.hypot(...sub(points[points.length - 1], start)) * 0.35,
          ),
        );
      const radial = Math.hypot(p[0], p[2]);
      if (radial > source.pruning.radiusLimit)
        p = [
          (p[0] * source.pruning.radiusLimit) / radial,
          p[1],
          (p[2] * source.pruning.radiusLimit) / radial,
        ];
      return p;
    });
    const branch: BotanicalBranch = {
      id,
      parent: parent.id,
      level,
      start: points[0],
      bend: points[Math.floor(points.length / 2)],
      end: points[points.length - 1],
      radius: thickness,
      bare: parent.bare || !!edit?.bare || broken,
      broken,
      points,
    };
    branches.push(branch);
    return branch;
  };
  const whorls = Math.ceil(doc.branches / conifer.whorlSize);
  for (let i = 0; i < doc.branches; i++) {
    const id = `b${i}`;
    const whorl = Math.floor(i / conifer.whorlSize);
    const rank = i % conifer.whorlSize;
    const whorlPosition = Math.max(
      0,
      Math.min(1, (whorl + 0.18 + (random(`${id}/height`) - 0.5) * conifer.whorlJitter) / whorls),
    );
    const t = conifer.crownStart + whorlPosition ** conifer.crownBias * (0.96 - conifer.crownStart);
    if (t < source.pruning.clearTrunk) continue;
    const angle =
      (rank / conifer.whorlSize) * Math.PI * 2 +
      whorl * 1.93 +
      (random(`${id}/angle`) - 0.5) * source.growth.asymmetry;
    const lowerThinning = Math.min(1, 0.68 + t * 2);
    const reach =
      radius *
      ((1 - t) / (1 - conifer.crownStart)) ** conifer.crownPower *
      lowerThinning *
      (0.97 + (random(`${id}/reach`) - 0.5) * 0.38 * doc.variation);
    const start = botanicalBranchPoint(trunk, t);
    const droop = conifer.limbSag * reach * (0.7 + random(`${id}/droop`) * 0.6) * (1 - t * 0.65);
    const points: Vec3[] = [];
    for (let j = 0; j <= 6; j++) {
      const u = j / 6;
      const sweep = angle + Math.sin(u * Math.PI) * (random(`${id}/sweep`) - 0.5) * 0.22;
      points.push([
        start[0] + Math.cos(sweep) * reach * u,
        start[1] - Math.sin(u * Math.PI * 0.85) * droop + reach * (0.08 + t * 0.24) * u * u,
        start[2] + Math.sin(sweep) * reach * u,
      ]);
    }
    const limb = make(id, trunk, 1, points, trunk.radius * 0.24 * (1 - t) ** 0.7);
    if (!limb || limb.broken || source.growth.levels < 2) continue;
    for (let j = 0; j < conifer.shootsPerLimb; j++) {
      const u = 0.18 + ((j + 0.45) / conifer.shootsPerLimb) * 0.77;
      const origin = botanicalBranchPoint(limb, u);
      const tangent = normalize(
        sub(
          botanicalBranchPoint(limb, Math.min(1, u + 0.035)),
          botanicalBranchPoint(limb, Math.max(0, u - 0.035)),
        ),
      );
      for (let side = 0; side < 2; side++) {
        const shootId = `${id}/${j * 2 + side}`;
        const spread =
          reach * conifer.shootSpread * (1 - u) ** 0.7 * (0.72 + random(`${shootId}/reach`) * 0.5);
        // The local tangent follows authored limb bends, keeping edited shoots attached and aligned.
        const lateral: Vec3 = normalize([-tangent[2] * (side ? -1 : 1), 0, tangent[0] * (side ? -1 : 1)]);
        const direction = normalize(add(scale(tangent, 0.5), lateral));
        const shootLift = 0.28 + random(`${shootId}/lift`) * 0.32;
        const shootPoints: Vec3[] = [];
        for (let k = 0; k <= 3; k++) {
          const v = k / 3;
          shootPoints.push(
            add(origin, [
              direction[0] * spread * v,
              direction[1] * spread * v - spread * 0.12 * Math.sin(v * Math.PI) + spread * shootLift * v * v,
              direction[2] * spread * v,
            ]),
          );
        }
        make(shootId, limb, 2, shootPoints, limb.radius * 0.3 * (1 - u * 0.5));
      }
    }
  }
  return { branches, height, truncated };
}
