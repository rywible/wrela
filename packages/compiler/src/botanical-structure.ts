import { add, cross, normalize, scale, sub, type Vec3, type VegetationDefinition } from "@wrela/model";

import {
  type BotanicalBranch,
  botanicalBranchPoint,
  botanicalRandom,
  MAX_BOTANICAL_BRANCHES,
} from "./botanical-branch";
import { developmentalStructure } from "./botanical-growth-structure";
import { coniferStructure } from "./conifer-growth";

export {
  type BotanicalBranch,
  botanicalBranchPoint,
  botanicalRandom,
  MAX_BOTANICAL_BRANCHES,
} from "./botanical-branch";
export type BotanicalStructure = { branches: BotanicalBranch[]; height: number; truncated: boolean };

export function botanicalStructure(doc: VegetationDefinition): BotanicalStructure {
  const source = doc.botanical;
  if (!source) return { branches: [], height: doc.height, truncated: false };
  if (source.development) return developmentalStructure(doc);
  if (source.species === "pine" && source.conifer) return coniferStructure(doc);
  const { growth, pruning, damage } = source;
  const random = (key: string) => botanicalRandom(doc.seed, key);
  const herb = source.species === "grass" || source.species === "fern";
  const height = doc.height * (0.2 + source.age * 0.8);
  const radius = doc.radius * (0.3 + source.age * 0.7);
  const branches: BotanicalBranch[] = [];
  const edits = new Map(source.branchEdits.map((edit) => [edit.branch, edit]));
  const removed = new Set(pruning.removedBranches);
  const axis: Vec3 = [
    height * growth.asymmetry * (random("lean-x") - 0.5) * 0.15,
    height * (source.species === "oak" || source.species === "shrub" ? 0.75 : 1),
    height * growth.asymmetry * (random("lean-z") - 0.5) * 0.15,
  ];
  const trunkRadius = Math.max(0.005, height * (herb ? 0.004 : 0.026) * (0.65 + source.age * 0.35));
  let truncated = false;
  const addBranch = (
    id: string,
    parent: string | null,
    level: number,
    start: Vec3,
    proposed: Vec3,
    thickness: number,
  ): BotanicalBranch | undefined => {
    if (removed.has(id)) return undefined;
    if (branches.length >= MAX_BOTANICAL_BRANCHES) {
      truncated = true;
      return undefined;
    }
    const edit = edits.get(id);
    const broken = level > 0 && random(`${id}/damage`) < damage.brokenBranches;
    let end = add(start, scale(sub(proposed, start), (edit?.lengthScale ?? 1) * (broken ? 0.32 : 1)));
    const radial = Math.hypot(end[0], end[2]);
    if (radial > pruning.radiusLimit)
      end = [(end[0] * pruning.radiusLimit) / radial, end[1], (end[2] * pruning.radiusLimit) / radial];
    const length = Math.hypot(...sub(end, start));
    const bend = add(
      scale(add(start, end), 0.5),
      scale(edit?.bend ?? [0, growth.tropism * 0.18, 0], length * 0.4),
    );
    const branch: BotanicalBranch = {
      id,
      parent,
      level,
      start,
      bend,
      end,
      radius: thickness,
      bare: broken || !!edit?.bare,
      broken,
    };
    branches.push(branch);
    return branch;
  };
  const trunk = herb
    ? undefined
    : addBranch(
        "trunk",
        null,
        0,
        [0, 0, 0],
        source.species === "shrub" ? scale(axis, 0.3) : axis,
        trunkRadius,
      );
  if (!herb && !trunk) return { branches, height, truncated };
  const recurse = (branch: BotanicalBranch) => {
    if (branch.broken || branch.level >= growth.levels || source.species === "grass") return;
    const direction = normalize(sub(branch.end, branch.start));
    const side = normalize(cross(direction, Math.abs(direction[1]) > 0.95 ? [1, 0, 0] : [0, 1, 0]));
    const up = normalize(cross(side, direction));
    const length = Math.hypot(...sub(branch.end, branch.start)) * growth.lengthRatio;
    for (let i = 0; i < growth.children; i++) {
      const id = `${branch.id}/${i}`;
      const t = 0.35 + ((i + 0.5) / growth.children) * 0.58;
      const start = botanicalBranchPoint(branch, t);
      const angle = i * 2.39996 + random(`${id}/angle`) * growth.asymmetry;
      const lateral =
        source.species === "fern"
          ? scale(side, i % 2 ? -1 : 1)
          : add(scale(side, Math.cos(angle)), scale(up, Math.sin(angle)));
      const vector = normalize(
        add(
          add(scale(direction, Math.cos(growth.branchAngle)), scale(lateral, Math.sin(growth.branchAngle))),
          [0, growth.tropism * 0.5, 0],
        ),
      );
      const child = addBranch(
        id,
        branch.id,
        branch.level + 1,
        start,
        add(start, scale(vector, length * (0.85 + random(`${id}/length`) * 0.3))),
        branch.radius * growth.taper,
      );
      if (child) {
        child.bare ||= branch.bare;
        recurse(child);
      }
    }
  };
  for (let i = 0; i < doc.branches; i++) {
    const id = `b${i}`;
    const t = herb
      ? 0
      : source.species === "shrub"
        ? 0.06 + (i / doc.branches) * 0.22
        : 0.18 + ((i + 0.5) / doc.branches) * 0.72;
    if (t < pruning.clearTrunk) continue;
    const angle = i * 2.399963 + (random(`${id}/angle`) - 0.5) * growth.asymmetry;
    const crown =
      source.species === "pine"
        ? (1 - t) ** 0.8
        : herb
          ? 0.5 + random(`${id}/spread`) * 0.5
          : Math.sin(Math.PI * Math.min(0.95, t + 0.15)) ** 0.6;
    const length = radius * crown * (0.8 + random(`${id}/length`) * doc.variation * 0.4);
    const attachment = source.species === "shrub" ? t / 0.3 : t;
    const start = trunk ? botanicalBranchPoint(trunk, attachment) : ([0, 0, 0] as Vec3);
    const end: Vec3 = [
      start[0] + Math.cos(angle) * length * Math.sin(growth.branchAngle),
      start[1] +
        (herb ? height * (0.5 + random(`${id}/height`) * 0.5) : length * Math.cos(growth.branchAngle)),
      start[2] + Math.sin(angle) * length * Math.sin(growth.branchAngle),
    ];
    const branch = addBranch(
      id,
      herb ? null : "trunk",
      1,
      start,
      end,
      trunkRadius * (herb ? 0.8 : 0.55 * (1 - t)),
    );
    if (branch) {
      branch.bare ||= !!trunk?.bare;
      recurse(branch);
    }
  }
  return { branches, height, truncated };
}
