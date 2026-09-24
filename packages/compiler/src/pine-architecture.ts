import { add, normalize, scale, sub, type Vec3, type VegetationDefinition } from "@wrela/model";

import { type BotanicalBranch, botanicalBranchPoint, botanicalRandom } from "./botanical-branch";
import type { BotanicalStructure } from "./botanical-structure";

export const MAX_PINE_ARCHITECTURE_BRANCHES = 8192;
const clamp = (n: number) => Math.max(0, Math.min(1, n));
const ease = (n: number) => {
  const t = clamp(n);
  return t * t * (3 - 2 * t);
};

/** A constrained developmental grammar: trunk → limbs → secondary branches →
 * short leafy twigs. Maturity advances a height front and branch extension schedule.
 * This is authored morphology, not a resource/competition or calendar-year model. */
export function pineArchitecture(doc: VegetationDefinition): BotanicalStructure {
  const source = doc.botanical,
    conifer = source?.conifer,
    shape = conifer?.architecture;
  if (!source || !conifer || !shape) throw Error("Pine architecture requires authored shape controls");
  const random = (id: string) => botanicalRandom(doc.seed, `architecture/${id}`);
  const stage = 0.08 + 0.92 * source.age;
  const height = doc.height * stage;
  const branches: BotanicalBranch[] = [];
  const removed = new Set(source.pruning.removedBranches);
  const edits = new Map(source.branchEdits.map((edit) => [edit.branch, edit]));
  let truncated = false;
  if (removed.has("trunk")) return { branches, height, truncated };
  const trunkEdit = edits.get("trunk");
  const lean: Vec3 = [
    (random("lean-x") - 0.5) * doc.height * 0.035 * doc.variation,
    0,
    (random("lean-z") - 0.5) * doc.height * 0.035 * doc.variation,
  ];
  const trunkAt = (t: number): Vec3 =>
    add(
      scale(
        [
          lean[0] * t + Math.sin(t * 7) * doc.height * 0.003 * t,
          doc.height * t,
          lean[2] * t + Math.sin(t * 9 + 1) * doc.height * 0.003 * t,
        ],
        trunkEdit?.lengthScale ?? 1,
      ),
      scale(trunkEdit?.bend ?? [0, 0, 0], Math.sin(t * Math.PI) * doc.height * 0.25),
    );
  const trunkPoints = Array.from({ length: 25 }, (_, i) => trunkAt((stage * i) / 24));
  const trunk: BotanicalBranch = {
    id: "trunk",
    parent: null,
    level: 0,
    start: trunkPoints[0],
    bend: trunkPoints[12],
    end: trunkPoints[24],
    radius: conifer.trunkRadius * (0.12 + source.age * 0.88),
    tipRadius: 0.002,
    bare: !!trunkEdit?.bare,
    broken: false,
    points: trunkPoints,
  };
  branches.push(trunk);
  const make = (
    id: string,
    parent: BotanicalBranch,
    level: number,
    points: Vec3[],
    radius: number,
    foliage = false,
  ): BotanicalBranch | undefined => {
    if (removed.has(id)) return;
    if (branches.length >= MAX_PINE_ARCHITECTURE_BRANCHES) {
      truncated = true;
      return;
    }
    const edit = edits.get(id),
      start = points[0];
    const broken = level === 1 && random(`${id}/damage`) < source.damage.brokenBranches;
    const length = Math.hypot(...sub(points[points.length - 1], start));
    const realized = points.map((point, i) => {
      const t = i / (points.length - 1);
      let p = add(start, scale(sub(point, start), (edit?.lengthScale ?? 1) * (broken ? 0.25 : 1)));
      p = add(p, scale(edit?.bend ?? [0, 0, 0], Math.sin(t * Math.PI) * length * 0.35));
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
      start: realized[0],
      bend: realized[Math.floor(realized.length / 2)],
      end: realized[realized.length - 1],
      points: realized,
      radius,
      tipRadius: Math.max(0.00065, radius * (foliage ? 0.4 : 0.09)),
      bare: parent.bare || !!edit?.bare || broken,
      broken,
      ...(foliage
        ? {
            cohort: {
              age: id.endsWith("/tip") || id === "leader" ? 0 : 1,
              retention: id.endsWith("/tip") || id === "leader" ? 1 : 0.78,
              count: conifer.needlesPerShoot,
              tint: 0.84 + random(`${parent.id}/cohort`) * 0.12 + random(`${id}/tint`) * 0.04,
            },
          }
        : {}),
    };
    branches.push(branch);
    return branch;
  };
  const tangentAt = (branch: BotanicalBranch, t: number) =>
    normalize(
      sub(
        botanicalBranchPoint(branch, Math.min(1, t + 0.025)),
        botanicalBranchPoint(branch, Math.max(0, t - 0.025)),
      ),
    );
  const leafyTwig = (parent: BotanicalBranch, id: string, t: number, direction: Vec3, length: number) => {
    const start = botanicalBranchPoint(parent, t);
    const points = Array.from({ length: 4 }, (_, i) => {
      const u = i / 3;
      return add(start, scale(direction, length * u));
    });
    make(id, parent, 3, points, Math.max(0.0009, Math.min(0.0022, length * 0.007)), true);
  };
  const whorls = Math.ceil(doc.branches / conifer.whorlSize);
  const crownPhase = random("crown-phase") * Math.PI * 2;
  for (let i = 0; i < doc.branches; i++) {
    const id = `b${i}`,
      whorl = Math.floor(i / conifer.whorlSize),
      rank = i % conifer.whorlSize;
    // Placement and identity use the mature schedule; older attachments do not
    // slide up the trunk as the height front advances.
    const tier = (whorl + 0.25 + (random(`${id}/height`) - 0.5) * conifer.whorlJitter * 1.4) / whorls;
    const t = conifer.crownStart + clamp(tier) ** conifer.crownBias * (0.96 - conifer.crownStart);
    if (t > stage - 0.006 || t / stage < source.pruning.clearTrunk) continue;
    const extension = ease((stage - t) / (0.12 + (1 - t) * 0.28));
    if (extension < 0.015) continue;
    const angle =
      (rank * Math.PI * 2) / conifer.whorlSize +
      whorl * 2.16 +
      (random(`${id}/angle`) - 0.5) * source.growth.asymmetry;
    const directional = 1 + (shape.crownAsymmetry ?? 0) * 0.42 * Math.cos(angle - crownPhase);
    const lobe =
      1 + doc.variation * (0.11 * Math.sin(angle * 2 + crownPhase) + (random(`${id}/reach`) - 0.5) * 0.18);
    const reach =
      directional *
      doc.radius *
      ((1 - t) / (1 - conifer.crownStart)) ** conifer.crownPower *
      lobe *
      extension;
    const start = botanicalBranchPoint(trunk, t / stage);
    const sag = conifer.limbSag * (0.7 + random(`${id}/sag`) * 0.5) * (1 - t * 0.5);
    const pitch =
      (random(`${id}/pitch`) - 0.35) * 0.72 +
      t * 0.23 +
      (shape.crownAsymmetry ?? 0) * Math.cos(angle - crownPhase) * 0.16;
    const points = Array.from({ length: 7 }, (_, j) => {
      const u = j / 6;
      const sweep = angle + Math.sin(u * Math.PI) * (random(`${id}/sweep`) - 0.5) * 0.3;
      return add(start, [
        Math.cos(sweep) * reach * u,
        reach * (-sag * Math.sin(u * Math.PI) + pitch * u),
        Math.sin(sweep) * reach * u,
      ]);
    });
    const limb = make(
      id,
      trunk,
      1,
      points,
      trunk.radius * shape.limbThickness * (1 - t) ** 0.65 * Math.sqrt(extension),
    );
    if (!limb || limb.broken || source.growth.levels < 2) continue;
    const pairs = Math.max(2, Math.round(conifer.shootsPerLimb * (0.6 + 0.4 * (1 - t))));
    for (let j = 0; j < pairs; j++) {
      for (let side = 0; side < 2; side++) {
        const secondaryId = `${id}/s${j}-${side}`;
        const u =
          0.2 + ((j + 0.35 + side * 0.22 + (random(`${secondaryId}/attachment`) - 0.5) * 0.32) / pairs) * 0.7;
        const origin = botanicalBranchPoint(limb, u),
          tangent = tangentAt(limb, u);
        const lateral: Vec3 = normalize([-tangent[2] * (side ? -1 : 1), 0, tangent[0] * (side ? -1 : 1)]);
        const direction = normalize(
          add(add(scale(tangent, 0.5), lateral), [0, 0.06 + random(`${secondaryId}/inclination`) * 0.65, 0]),
        );
        const length =
          reach * conifer.shootSpread * (1 - u) ** 0.55 * (0.8 + random(`${secondaryId}/length`) * 0.35);
        const secondary = make(
          secondaryId,
          limb,
          2,
          Array.from({ length: 4 }, (_, k) => {
            const v = k / 3;
            return add(origin, add(scale(direction, length * v), [0, length * 0.13 * v * v, 0]));
          }),
          Math.max(0.0013, limb.radius * 0.24 * (1 - u * 0.55)),
        );
        if (!secondary || source.growth.levels < 3) continue;
        // Correlated branch-level occupancy creates patches with open gaps, instead
        // of independently thinning every needle. Stable IDs preserve manual edits.
        const cluster = shape.clusterVariation ?? 0;
        const retention = 1 - cluster * 0.32 * (0.5 + 0.5 * Math.sin(angle * 3 + crownPhase + t * 11));
        for (let k = 0; k < shape.twigPairs; k++) {
          for (let face = 0; face < 2; face++) {
            const twigId = `${secondaryId}/t${k}-${face}`;
            if (random(`${secondaryId}/cluster/${k}`) > retention && !edits.has(twigId)) continue;
            const v =
              shape.foliageStart + ((k + 0.35 + face * 0.24) / shape.twigPairs) * (0.92 - shape.foliageStart);
            const along = tangentAt(secondary, v);
            const sideways: Vec3 = normalize([-along[2] * (face ? -1 : 1), 0, along[0] * (face ? -1 : 1)]);
            const twigDirection = normalize(
              add(add(scale(along, 0.75), scale(sideways, 0.65)), [
                0,
                0.2 + random(`${secondaryId}/lift`) * 0.55 + (random(`${twigId}/lift`) - 0.5) * 0.25,
                0,
              ]),
            );
            const twigLength =
              shape.twigLength *
              (0.8 +
                random(`${twigId}/length`) * 0.4 +
                cluster * (random(`${secondaryId}/mass`) - 0.5) * 0.6) *
              Math.min(1, Math.sqrt(length / 0.35)) *
              (0.85 + 0.15 * (1 - v));
            leafyTwig(secondary, twigId, v, twigDirection, twigLength);
          }
        }
        leafyTwig(
          secondary,
          `${secondaryId}/tip`,
          1,
          tangentAt(secondary, 1),
          shape.twigLength * Math.min(1, Math.sqrt(length / 0.35)),
        );
      }
    }
    leafyTwig(limb, `${id}/tip`, 0.95, tangentAt(limb, 1), shape.twigLength * Math.sqrt(extension));
  }
  // The current leader carries needles while the next whorl is still extending.
  leafyTwig(trunk, "leader", Math.max(0, 1 - 0.22 / height), tangentAt(trunk, 1), Math.min(0.22, height));
  return { branches, height: trunk.end[1], truncated };
}
