import {
  add,
  type BotanicalGrowthState,
  botanicalSpeciesTraits,
  scale,
  sub,
  type VegetationDefinition,
} from "@wrela/model";

import { botanicalBranchPoint } from "./botanical-branch";
import { botanicalGrowthKey, growBotanical } from "./botanical-growth";
import type { BotanicalStructure } from "./botanical-structure";

const snapshots = new Map<string, BotanicalGrowthState>();
/** Small immutable source cache; changing optics or review does not replay developmental history. */
export function compiledBotanicalGrowth(doc: VegetationDefinition): BotanicalGrowthState {
  const source = doc.botanical?.development;
  if (!source) throw Error("Developmental source required");
  const key = `${botanicalGrowthKey(doc.seed, source)}@${source.steps}`;
  let state = snapshots.get(key);
  if (!state) {
    state = growBotanical(doc.seed, source);
    if (snapshots.size >= 8) {
      const oldest = snapshots.keys().next().value;
      if (oldest !== undefined) snapshots.delete(oldest);
    }
    snapshots.set(key, state);
  }
  return structuredClone(state);
}
/** A validated worker checkpoint avoids replaying the same history while meshing. */
export function rememberBotanicalGrowth(doc: VegetationDefinition, state: BotanicalGrowthState) {
  const source = doc.botanical?.development;
  if (!source || state.step !== source.steps || state.sourceKey !== botanicalGrowthKey(doc.seed, source))
    throw Error("Growth snapshot identity mismatch");
  if (snapshots.size >= 8) {
    const oldest = snapshots.keys().next().value;
    if (oldest !== undefined) snapshots.delete(oldest);
  }
  snapshots.set(`${state.sourceKey}@${state.step}`, structuredClone(state));
}
export function developmentalStructure(doc: VegetationDefinition): BotanicalStructure {
  const source = doc.botanical;
  if (!source) throw Error("Botanical source required");
  const state = compiledBotanicalGrowth(doc);
  const size = doc.height / (16 * botanicalSpeciesTraits[state.species].extension);
  const removed = new Set(source.pruning.removedBranches);
  const edits = new Map(source.branchEdits.map((edit) => [edit.branch, edit]));
  const realized = new Map<string, BotanicalStructure["branches"][number]>();
  const bareOverrides = new Set<string>();
  const branches: BotanicalStructure["branches"] = [];
  let height = 0;
  for (const shoot of state.shoots) {
    if (shoot.state === "pruned" || (shoot.parent && removed.has(shoot.parent))) {
      removed.add(shoot.id);
      continue;
    }
    if (removed.has(shoot.id)) continue;
    if (shoot.order > 0 && shoot.start[1] * size < doc.height * source.pruning.clearTrunk) {
      removed.add(shoot.id);
      continue;
    }
    const parent = shoot.parent ? realized.get(shoot.parent) : undefined;
    const start = parent ? botanicalBranchPoint(parent, shoot.attachment ?? 1) : scale(shoot.start, size);
    const edit = edits.get(shoot.id);
    if (edit?.bare || (shoot.parent && bareOverrides.has(shoot.parent))) bareOverrides.add(shoot.id);
    let end = add(start, scale(sub(shoot.end, shoot.start), size * (edit?.lengthScale ?? 1)));
    const radial = Math.hypot(end[0], end[2]);
    if (radial > source.pruning.radiusLimit)
      end = [
        (end[0] * source.pruning.radiusLimit) / radial,
        end[1],
        (end[2] * source.pruning.radiusLimit) / radial,
      ];
    const bend = add(
      scale(add(start, end), 0.5),
      scale(edit?.bend ?? [0, 0, 0], Math.hypot(...sub(end, start)) * 0.35),
    );
    height = Math.max(height, end[1]);
    branches.push({
      id: shoot.id,
      parent: shoot.parent,
      level: shoot.order,
      habit: shoot.habit,
      start,
      bend,
      end,
      points: [start, bend, end],
      radius: shoot.radius * size,
      tipRadius: shoot.tipRadius * size,
      bare: shoot.state !== "living" || shoot.foliage.retained <= 0 || bareOverrides.has(shoot.id),
      broken: shoot.state === "dead",
      cohort: {
        age: state.step - shoot.foliage.birth,
        retention: shoot.foliage.retained,
        count: shoot.foliage.count,
        tint: shoot.foliage.tint,
      },
    });
    realized.set(shoot.id, branches[branches.length - 1]);
  }
  return { branches, height, truncated: state.limited };
}
