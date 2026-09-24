import type { CharacterDefinition, CreatureDefinition, Joint, Motion, Vec3 } from "@wrela/model";
import { bakeLegPose, yawPoint } from "@wrela/model";

const TAU = Math.PI * 2;
const key = (
  joint: string,
  time: number,
  rotation: Vec3 = [0, 0, 0],
  translation: Vec3 = [0, 0, 0],
): Motion["keys"][number] => ({ joint, time, rotation, translation });
const mix = (a: number, b: number, t: number) => a + (b - a) * t;
const smooth = (x: number) => x * x * (3 - 2 * x);
const limbs = [
  { id: "hind-left", phase: 0 },
  { id: "fore-left", phase: 0.25 },
  { id: "hind-right", phase: 0.5 },
  { id: "fore-right", phase: 0.75 },
];
const DURATION = 2.4,
  DUTY = 0.72,
  STRIDE = 0.42,
  TRAVEL = STRIDE / DUTY;

/** Bake sagittal FABRIK into ordinary Euler keys; source stays editable and runtime-independent. */
function legRotations(chain: Joint[], target: Vec3): number[] {
  const points = chain.map((joint) => [joint.position[1], joint.position[2]]);
  const anchor = [...points[0]];
  const lengths = points
    .slice(1)
    .map((point, index) => Math.hypot(point[0] - points[index][0], point[1] - points[index][1]));
  const moveAtLength = (from: number[], to: number[], length: number) => {
    const dy = to[0] - from[0],
      dz = to[1] - from[1],
      distance = Math.max(1e-8, Math.hypot(dy, dz));
    return [from[0] + (dy * length) / distance, from[1] + (dz * length) / distance];
  };
  for (let iteration = 0; iteration < 32; iteration++) {
    points[points.length - 1] = [target[1], target[2]];
    for (let index = points.length - 2; index >= 0; index--)
      points[index] = moveAtLength(points[index + 1], points[index], lengths[index]);
    points[0] = [...anchor];
    for (let index = 1; index < points.length; index++)
      points[index] = moveAtLength(points[index - 1], points[index], lengths[index - 1]);
  }
  let parentRotation = 0;
  return points.slice(0, -1).map((point, index) => {
    const rest = chain[index + 1].position.map((value, axis) => value - chain[index].position[axis]);
    const restAngle = Math.atan2(rest[2], rest[1]);
    const solvedAngle = Math.atan2(points[index + 1][1] - point[1], points[index + 1][0] - point[0]);
    const worldRotation = Math.atan2(Math.sin(solvedAngle - restAngle), Math.cos(solvedAngle - restAngle));
    const localRotation = worldRotation - parentRotation;
    parentRotation = worldRotation;
    return localRotation;
  });
}
function chainFor(character: CharacterDefinition, limb: string): Joint[] {
  return ["upper", ...(limb.startsWith("hind") ? ["knee"] : []), "lower", "foot"].map((part) => {
    const joint = character.joints.find((entry) => entry.id === `${limb}-${part}`);
    if (!joint) throw new Error(`Lookdev requires Ash Warden joint ${limb}-${part}`);
    return joint;
  });
}
function footAt(rest: Vec3, phase: number): Vec3 {
  if (phase <= DUTY) return [rest[0], rest[1], rest[2] + STRIDE / 2 - TRAVEL * phase];
  const swing = (phase - DUTY) / (1 - DUTY);
  return [
    rest[0],
    rest[1] + 0.075 * Math.sin(Math.PI * swing),
    rest[2] + mix(-STRIDE / 2, STRIDE / 2, smooth(swing)),
  ];
}
function walk(character: CharacterDefinition): { motion: Motion; contacts: CreatureDefinition["contacts"] } {
  const motion: Motion = {
    id: "walk",
    name: "Warden — deliberate four-beat walk",
    duration: DURATION,
    loop: true,
    keys: [],
  };
  const contacts: CreatureDefinition["contacts"] = [];
  for (let sample = 0; sample <= 96; sample++) {
    const p = sample / 96,
      time = p * DURATION;
    const stridePhase = p * TAU;
    const bodyDrop = -0.035 + 0.012 * Math.cos(stridePhase * 2);
    motion.keys.push(key("root", time, [0.009 * Math.sin(stridePhase * 2), 0, 0], [0, bodyDrop, TRAVEL * p]));
    motion.keys.push(
      key("spine", time, [0.021 * Math.sin(stridePhase * 2 - 0.55), 0, 0.008 * Math.sin(stridePhase)]),
    );
    motion.keys.push(
      key("neck", time, [-0.018 * Math.sin(stridePhase * 2 - 0.7), 0.012 * Math.sin(stridePhase), 0]),
    );
    motion.keys.push(
      key("head", time, [0.015 - 0.01 * Math.sin(stridePhase * 2 - 0.95), -0.012 * Math.sin(stridePhase), 0]),
    );
    motion.keys.push(
      key("tail", time, [0.035 * Math.sin(stridePhase * 2 - 0.8), 0.07 * Math.sin(stridePhase - 0.5), 0]),
    );
    motion.keys.push(
      key("tail-tip", time, [
        0.055 * Math.sin(stridePhase * 2 - 1.25),
        0.11 * Math.sin(stridePhase - 1.1),
        0,
      ]),
    );
    motion.keys.push(key("ear-left", time, [0, 0.035 * Math.sin(stridePhase - 0.5), 0]));
    motion.keys.push(key("ear-right", time, [0, -0.035 * Math.sin(stridePhase + 0.2), 0]));
    for (const limb of limbs) {
      const chain = chainFor(character, limb.id),
        rest = chain[chain.length - 1].position;
      const phase = (p - limb.phase + 1) % 1;
      const target = footAt(rest, phase);
      target[1] -= bodyDrop;
      const rotations = legRotations(chain, target);
      for (let index = 0; index < rotations.length; index++)
        motion.keys.push(key(chain[index].id, time, [rotations[index], 0, 0]));
      motion.keys.push(
        key(chain[chain.length - 1].id, time, [-rotations.reduce((sum, value) => sum + value, 0), 0, 0]),
      );
    }
  }
  for (const limb of limbs) {
    const rest = chainFor(character, limb.id).at(-1)?.position;
    if (!rest) continue;
    const boundaries = [...new Set([0, limb.phase, (limb.phase + DUTY) % 1, 1])].sort((a, b) => a - b);
    for (let index = 0; index < boundaries.length - 1; index++) {
      const start = boundaries[index],
        end = boundaries[index + 1],
        middle = (start + end) / 2;
      const phase = (middle - limb.phase + 1) % 1;
      if (phase >= DUTY) continue;
      const target = footAt(rest, phase);
      target[2] += TRAVEL * middle;
      contacts.push({
        id: `${limb.id}-walk-${index}`,
        motion: "walk",
        joint: `${limb.id}-foot`,
        start: start * DURATION,
        end: end * DURATION,
        target,
        space: "character",
        weight: 1,
        tolerance: 0.012,
        blendIn: start === 0 ? 0 : 0.045,
        blendOut: end === 1 ? 0 : 0.045,
        ground: true,
        offset: rest[1],
      });
    }
  }
  return { motion, contacts };
}

/** Original lookdev performance for the Warden study; baseline fixtures remain unchanged. */
export function applyCharacterMotionLookdev(source: CharacterDefinition): CharacterDefinition {
  const character = structuredClone(source);
  if (!character.creature) throw new Error("Warden performance needs creature anatomy");
  const gait = walk(character);
  // Sampling 49 times retains smooth arcs while staying below the clip key budget.
  gait.motion.keys = gait.motion.keys.filter((entry) => Math.round((entry.time / DURATION) * 96) % 2 === 0);
  const idle: Motion = { id: "idle", name: "Warden — listening breath", duration: 4.8, loop: true, keys: [] };
  for (let sample = 0; sample <= 24; sample++) {
    const p = sample / 24,
      time = p * idle.duration;
    idle.keys.push(
      key("spine", time, [0.006 * Math.sin(p * TAU), 0, 0], [0, 0.012 * (1 - Math.cos(p * TAU)), 0]),
    );
    idle.keys.push(key("neck", time, [-0.009 * Math.sin(p * TAU), 0.025 * Math.sin(p * TAU), 0]));
    idle.keys.push(key("head", time, [0.01, -0.018 * Math.sin(p * TAU), 0.012 * Math.sin(p * TAU)]));
    idle.keys.push(key("ear-left", time, [0, 0.06 * Math.sin(p * TAU), -0.03]));
    idle.keys.push(key("ear-right", time, [0, -0.035 * Math.sin(p * TAU + 0.3), 0.02]));
    idle.keys.push(key("tail", time, [0.015 * Math.sin(p * TAU), 0.04 * Math.sin(p * TAU * 0.5), 0]));
    idle.keys.push(
      key("tail-tip", time, [0.025 * Math.sin(p * TAU - 0.8), 0.065 * Math.sin(p * TAU * 0.5 - 0.5), 0]),
    );
  }
  const turn: Motion = {
    id: "turn",
    name: "Warden — supported attention turn",
    duration: 3.2,
    loop: false,
    keys: [],
  };
  const turnContacts: CreatureDefinition["contacts"] = [];
  const turnSteps = ["fore-left", "hind-left", "fore-right", "hind-right"];
  for (let sample = 0; sample <= 64; sample++) {
    const time = (sample / 64) * turn.duration;
    const yaw = -0.52 * smooth(Math.max(0, Math.min(1, (time - 0.35) / 2.5)));
    turn.keys.push(key("root", time, [0, yaw, 0], [0, -0.025, 0]));
    turn.keys.push(key("head", time, [-0.01, -0.2 * Math.sin((Math.PI * time) / turn.duration), 0]));
    turn.keys.push(key("ear-left", time, [0, -0.2 * Math.sin((Math.PI * time) / turn.duration), -0.035]));
    turn.keys.push(
      key("tail", time, [
        0.02 * Math.sin((Math.PI * time) / turn.duration),
        -0.12 * Math.sin((Math.PI * time) / turn.duration),
        0,
      ]),
    );
    turn.keys.push(
      key("tail-tip", time, [
        0.04 * Math.sin((Math.PI * time) / turn.duration),
        -0.2 * Math.sin((Math.PI * time) / turn.duration),
        0,
      ]),
    );
    for (const [index, limb] of turnSteps.entries()) {
      const chain = chainFor(character, limb),
        rest = chain[chain.length - 1].position;
      const start = 0.45 + index * 0.53,
        end = start + 0.44;
      const step = Math.max(0, Math.min(1, (time - start) / (end - start)));
      const final = yawPoint(rest, -0.52);
      const world = rest.map((value, axis) => mix(value, final[axis], smooth(step))) as Vec3;
      world[1] += step > 0 && step < 1 ? 0.075 * Math.sin(Math.PI * step) : 0;
      const local = yawPoint(world, -yaw);
      local[1] += 0.025;
      bakeLegPose(chain, local).forEach((rotation, joint) => {
        turn.keys.push(key(chain[joint].id, time, rotation));
      });
    }
  }
  for (const [index, limb] of turnSteps.entries()) {
    const foot = chainFor(character, limb).at(-1);
    if (!foot) continue;
    const start = 0.45 + index * 0.53,
      end = start + 0.44;
    for (const [suffix, begin, finish, target] of [
      ["before", 0, start, foot.position],
      ["after", end, turn.duration, yawPoint(foot.position, -0.52)],
    ] as const)
      turnContacts.push({
        id: `${limb}-turn-${suffix}`,
        motion: "turn",
        joint: foot.id,
        start: begin,
        end: finish,
        target: [...target],
        space: "character",
        weight: 1,
        tolerance: 0.015,
        blendIn: begin === 0 ? 0 : 0.035,
        blendOut: finish === turn.duration ? 0 : 0.035,
        ground: true,
        offset: foot.position[1],
      });
  }
  const interaction: Motion = {
    id: "interact",
    name: "Warden — scent and acknowledge",
    duration: 4,
    loop: false,
    keys: [
      key("neck", 0),
      key("neck", 1.2, [0.14, 0, 0]),
      key("neck", 2.6, [0.14, 0, 0]),
      key("neck", 4),
      key("head", 0),
      key("head", 1.25, [0.16, 0.02, 0]),
      key("head", 1.7, [0.18, -0.025, 0]),
      key("head", 2.3, [0.17, 0.03, 0]),
      key("head", 2.7, [0.12, 0, 0]),
      key("head", 4),
      key("spine", 0),
      key("spine", 1.4, [0.035, 0, 0], [0, -0.025, 0.018]),
      key("spine", 2.6, [0.035, 0, 0], [0, -0.025, 0.018]),
      key("spine", 4),
    ],
  };
  character.motions = [
    idle,
    gait.motion,
    turn,
    interaction,
    ...character.motions.filter((motion) => !["idle", "walk", "turn", "interact"].includes(motion.id)),
  ];
  character.creature.contacts = character.creature.contacts.filter(
    (contact) => !["idle", "walk", "turn", "interact"].includes(contact.motion),
  );
  character.creature.contacts.push(...gait.contacts, ...turnContacts);
  for (const motion of [idle, interaction])
    for (const limb of limbs) {
      const foot = chainFor(character, limb.id).at(-1);
      if (!foot) continue;
      character.creature.contacts.push({
        id: `${limb.id}-${motion.id}-support`,
        motion: motion.id,
        joint: foot.id,
        start: 0,
        end: motion.duration,
        target: [...foot.position],
        space: "character",
        weight: 1,
        tolerance: 0.015,
        blendIn: 0,
        blendOut: 0,
        ground: true,
        offset: foot.position[1],
      });
    }
  character.performance = {
    clips: [
      {
        motion: "walk",
        facialKeys: [],
        contacts: [],
        alignments: [],
        events: limbs.map((limb) => ({
          id: `${limb.id}-plant`,
          time: limb.phase * DURATION,
          payload: { cue: "footstep", foot: limb.id },
        })),
      },
      {
        motion: "interact",
        facialKeys: [
          key("jaw", 0),
          key("jaw", 1.35, [0.025, 0, 0]),
          key("jaw", 2.1, [0.045, 0, 0]),
          key("jaw", 3.3),
        ],
        contacts: [],
        alignments: [],
        events: [
          { id: "scent-sample", time: 1.8, payload: { cue: "sniff" } },
          { id: "acknowledge", time: 3.2, payload: { cue: "interaction-complete" } },
        ],
      },
    ],
    transitions: [
      { from: "idle", to: "walk", duration: 0.35 },
      { from: "walk", to: "idle", duration: 0.45 },
      { from: "idle", to: "turn", duration: 0.2 },
      { from: "turn", to: "idle", duration: 0.35 },
      { from: "idle", to: "interact", duration: 0.3 },
    ],
  };
  for (const motion of [idle, gait.motion, turn, interaction]) {
    const existing = character.creature.reviewScenarios.find((review) => review.motion === motion.id);
    if (existing) {
      existing.duration = motion.duration;
      existing.name = motion.name;
    } else
      character.creature.reviewScenarios.push({
        id: `${motion.id}-review`,
        name: motion.name,
        motion: motion.id,
        duration: motion.duration,
        sampleRate: 30,
        cameras: [
          { id: "three-quarter", position: [4.8, 2.8, 5.8], target: [0, 1.4, 0] },
          { id: "side", position: [5.5, 1.7, 0.3], target: [0, 1.3, 0.3] },
        ],
        thresholds: { contactSlip: 0.025, penetration: 0.025, anchorError: 0.015, stretch: 0.2 },
      });
  }
  return character;
}
