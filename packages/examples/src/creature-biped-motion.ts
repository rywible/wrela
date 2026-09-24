import type { CharacterDefinition, Motion, Vec3 } from "@wrela/model";
import { bakeTwoBonePose } from "@wrela/model";
import { z } from "zod";

export const bipedWalkRecipeSchema = z.strictObject({
  duration: z.number().finite().min(1.2).max(4).default(2.4),
  stride: z.number().finite().min(0.15).max(0.65).default(0.42),
  supportFraction: z.number().finite().min(0.55).max(0.75).default(0.64),
  lift: z.number().finite().min(0.025).max(0.16).default(0.065),
  weightShift: z.number().finite().min(0).max(0.1).default(0.055),
});
export type BipedWalkRecipe = z.input<typeof bipedWalkRecipeSchema>;
const TAU = Math.PI * 2;
const smooth = (t: number) => t * t * (3 - 2 * t);
const key = (joint: string, time: number, rotation: Vec3 = [0, 0, 0], translation: Vec3 = [0, 0, 0]) => ({
  joint,
  time,
  rotation,
  translation,
});

/** Weight transfer, counter-rotation and foot arcs baked to the same editable
 * clip/contact/event source used by the runtime and Studio. */
export function applyBipedWalk(
  source: CharacterDefinition,
  input: BipedWalkRecipe = {},
): CharacterDefinition {
  const recipe = bipedWalkRecipeSchema.parse(input),
    character = structuredClone(source),
    creature = character.creature;
  if (!creature) throw Error("Biped gait needs creature anatomy");
  const chains = ["left", "right"].map((side) =>
    ["upper", "lower", "foot"].map((part) => {
      const joint = character.joints.find((entry) => entry.id === `leg-${side}-${part}`);
      if (!joint) throw Error(`Biped gait needs leg-${side}-${part}`);
      if (joint.rotation.some((angle) => angle !== 0))
        throw Error("Biped gait requires zero-rest-rotation leg joints");
      return joint;
    }),
  );
  const travel = recipe.stride / recipe.supportFraction;
  const footAt = (rest: Vec3, phase: number): Vec3 => {
    if (phase <= recipe.supportFraction)
      return [rest[0], rest[1], rest[2] + recipe.stride / 2 - travel * phase];
    const swing = (phase - recipe.supportFraction) / (1 - recipe.supportFraction);
    return [
      rest[0],
      rest[1] + recipe.lift * Math.sin(Math.PI * swing),
      rest[2] + recipe.stride * (smooth(swing) - 0.5),
    ];
  };
  const motion: Motion = {
    id: "walk",
    name: "Pilgrim — supported weight transfer",
    duration: recipe.duration,
    loop: true,
    keys: [],
  };
  for (let sample = 0; sample <= 64; sample++) {
    const p = sample / 64,
      phase = TAU * p,
      time = p * recipe.duration;
    // Left supports the first half, right the second. Lower hips at double
    // support, rising as the stance leg passes under the body.
    const x = -recipe.weightShift * Math.sin(phase),
      y = -0.058 - 0.012 * Math.cos(phase * 2);
    motion.keys.push(key("root", time, [0, 0, 0], [x, y, travel * p]));
    motion.keys.push(key("spine", time, [0.025, 0.022 * Math.sin(phase), -0.018 * Math.sin(phase)]));
    motion.keys.push(key("neck", time, [-0.014, -0.012 * Math.sin(phase), 0.009 * Math.sin(phase)]));
    motion.keys.push(key("head", time, [-0.012, -0.01 * Math.sin(phase), 0.006 * Math.sin(phase)]));
    for (const [index, chain] of chains.entries()) {
      const side = index === 0 ? "left" : "right",
        offset = index * 0.5;
      const localPhase = (p - offset + 1) % 1,
        rest = chain[2].position;
      const target = footAt(rest, localPhase);
      target[0] -= x;
      target[1] -= y;
      const pole: Vec3 = [chain[0].position[0], chain[0].position[1] - 0.5, chain[0].position[2] + 0.8];
      bakeTwoBonePose(chain, target, pole).forEach((rotation, joint) => {
        motion.keys.push(key(chain[joint].id, time, rotation));
      });
      motion.keys.push(
        key(`arm-${side}-upper`, time, [
          0.12 * Math.sin(phase - offset * TAU),
          0,
          index === 0 ? 0.04 : -0.04,
        ]),
      );
      motion.keys.push(
        key(`arm-${side}-lower`, time, [-0.06 - 0.035 * Math.sin(phase - offset * TAU), 0, 0]),
      );
    }
  }
  character.motions = character.motions.map((entry) => (entry.id === "walk" ? motion : entry));
  if (!character.motions.some((entry) => entry.id === "walk")) character.motions.push(motion);
  creature.contacts = creature.contacts.filter((contact) => contact.motion !== "walk");
  for (const [index, chain] of chains.entries()) {
    const offset = index * 0.5,
      rest = chain[2].position;
    const boundaries = [...new Set([0, offset, (offset + recipe.supportFraction) % 1, 1])].sort(
      (a, b) => a - b,
    );
    for (let i = 1; i < boundaries.length; i++) {
      const start = boundaries[i - 1],
        end = boundaries[i],
        mid = (start + end) / 2,
        phase = (mid - offset + 1) % 1;
      if (phase >= recipe.supportFraction) continue;
      const target = footAt(rest, phase);
      target[2] += travel * mid;
      creature.contacts.push({
        id: `pilgrim-${index}-walk-${i}`,
        motion: "walk",
        joint: chain[2].id,
        start: start * recipe.duration,
        end: end * recipe.duration,
        target,
        space: "character",
        weight: 1,
        tolerance: 0.008,
        blendIn: start === 0 ? 0 : 0.035,
        blendOut: end === 1 ? 0 : 0.035,
        ground: true,
        offset: rest[1],
      });
    }
  }
  const performance = character.performance ?? { clips: [], transitions: [] };
  performance.clips = performance.clips.filter((clip) => clip.motion !== "walk");
  performance.clips.push({
    motion: "walk",
    facialKeys: [],
    contacts: [],
    alignments: [],
    events: [
      { id: "left-heel-plant", time: 0, payload: { cue: "footstep", foot: "left" } },
      { id: "right-heel-plant", time: recipe.duration / 2, payload: { cue: "footstep", foot: "right" } },
    ],
  });
  performance.transitions = performance.transitions.filter(
    (entry) => entry.from !== "walk" && entry.to !== "walk",
  );
  performance.transitions.push(
    { from: "idle", to: "walk", duration: 0.3 },
    { from: "walk", to: "idle", duration: 0.35 },
  );
  character.performance = performance;
  const review = creature.reviewScenarios.find((entry) => entry.motion === "walk");
  if (review) {
    review.duration = recipe.duration;
    review.name = motion.name;
  }
  return character;
}
