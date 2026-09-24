import type { CreatureDefinition, Joint, Motion, Quat, Vec3 } from "@wrela/model";

import { type Pose, quatFromEuler, quatIdentity, quatMultiply, quatSlerp, rotateVector } from "./animation";

const add = (a: Vec3, b: Vec3): Vec3 => a.map((v, i) => v + b[i]) as Vec3;
const sub = (a: Vec3, b: Vec3): Vec3 => a.map((v, i) => v - b[i]) as Vec3;
const mul = (a: Vec3, s: number): Vec3 => a.map((v) => v * s) as Vec3;
const dot = (a: Vec3, b: Vec3) => a.reduce((s, v, i) => s + v * b[i], 0);
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
const distance = (a: Vec3, b: Vec3) => Math.hypot(...sub(a, b));
const unit = (a: Vec3, fallback: Vec3 = [1, 0, 0]): Vec3 =>
  Math.hypot(...a) > 1e-9 ? mul(a, 1 / Math.hypot(...a)) : [...fallback];
const clamp = (v: number, low = 0, high = 1) => Math.max(low, Math.min(high, v));
const inverse = (q: Quat): Quat => [-q[0], -q[1], -q[2], q[3]];
const own = <T>(record: Record<string, T>, id: string): T | undefined =>
  Object.hasOwn(record, id) ? record[id] : undefined;
const setOwn = <T>(record: Record<string, T>, id: string, value: T) =>
  Object.defineProperty(record, id, { value, writable: true, enumerable: true, configurable: true });
const identityPose = () => ({ translation: [0, 0, 0] as Vec3, rotation: quatIdentity() });
const clonePose = (pose: Pose): Pose =>
  new Map([...pose].map(([id, p]) => [id, { translation: [...p.translation], rotation: [...p.rotation] }]));

/** Stable even for opposite directions; the fallback axis is deterministic. */
export function rotationBetween(from: Vec3, to: Vec3): Quat {
  const a = unit(from),
    b = unit(to),
    cosine = clamp(dot(a, b), -1, 1);
  if (cosine > 1 - 1e-10) return quatIdentity();
  if (cosine < -1 + 1e-10) {
    const axis = unit(cross(a, Math.abs(a[0]) < 0.8 ? [1, 0, 0] : [0, 1, 0]));
    return [...axis, 0];
  }
  const axis = cross(a, b),
    q: Quat = [...axis, 1 + cosine],
    length = Math.hypot(...q);
  return q.map((v) => v / length) as Quat;
}

export type CreatureJointFrame = { position: Vec3; rotation: Quat; restRotation: Quat };
type SkeletonFrames = { ordered: Joint[]; byId: Map<string, Joint>; rest: Map<string, Quat> };
const skeletonFrames = new WeakMap<Joint[], SkeletonFrames>();
/** Skeleton artifacts are immutable. Compile hierarchy order and rest rotations
 * once, instead of traversing/allocating ancestry during every solver iteration. */
function compileSkeletonFrames(joints: Joint[]): SkeletonFrames {
  const cached = skeletonFrames.get(joints);
  if (cached) return cached;
  const byId = new Map(joints.map((joint) => [joint.id, joint])),
    ordered: Joint[] = [],
    rest = new Map<string, Quat>(),
    visiting = new Set<string>();
  const visit = (joint: Joint) => {
    if (rest.has(joint.id)) return;
    if (visiting.has(joint.id)) throw new Error("Cyclic creature skeleton");
    visiting.add(joint.id);
    if (joint.parent) {
      const parent = byId.get(joint.parent);
      if (!parent) throw new Error(`Missing parent ${joint.parent}`);
      visit(parent);
    }
    rest.set(
      joint.id,
      quatMultiply(joint.parent ? rest.get(joint.parent)! : quatIdentity(), quatFromEuler(joint.rotation)),
    );
    ordered.push(joint);
    visiting.delete(joint.id);
  };
  for (const joint of joints) visit(joint);
  const result = { ordered, byId, rest };
  skeletonFrames.set(joints, result);
  return result;
}
/** Matches poseMatrices' rest-space joint convention, including unsorted skeletons. */
export function creatureJointFrames(joints: Joint[], pose: Pose): Map<string, CreatureJointFrame> {
  const compiled = compileSkeletonFrames(joints),
    result = new Map<string, CreatureJointFrame>();
  for (const joint of compiled.ordered) {
    const parentJoint = joint.parent ? compiled.byId.get(joint.parent) : undefined;
    const parent = joint.parent ? result.get(joint.parent) : undefined;
    const local = pose.get(joint.id) ?? identityPose();
    const restRotation = compiled.rest.get(joint.id)!;
    const parentDeform = parent
      ? quatMultiply(parent.rotation, inverse(parent.restRotation))
      : quatIdentity();
    const offset: Vec3 = [
      joint.position[0] - (parentJoint?.position[0] ?? 0) + local.translation[0],
      joint.position[1] - (parentJoint?.position[1] ?? 0) + local.translation[1],
      joint.position[2] - (parentJoint?.position[2] ?? 0) + local.translation[2],
    ];
    const moved = rotateVector(parentDeform, offset);
    result.set(joint.id, {
      position: [
        moved[0] + (parent?.position[0] ?? 0),
        moved[1] + (parent?.position[1] ?? 0),
        moved[2] + (parent?.position[2] ?? 0),
      ],
      rotation: quatMultiply(parentDeform, quatMultiply(restRotation, local.rotation)),
      restRotation,
    });
  }
  return result;
}

export function creatureEulerFromQuaternion(q: Quat): Vec3 {
  const [x, y, z, w] = q;
  const m13 = 2 * (x * z + y * w);
  const euler: Vec3 =
    Math.abs(m13) < 0.9999999
      ? [
          Math.atan2(2 * (x * w - y * z), 1 - 2 * (x * x + y * y)),
          Math.asin(clamp(m13, -1, 1)),
          Math.atan2(2 * (z * w - x * y), 1 - 2 * (y * y + z * z)),
        ]
      : [Math.atan2(2 * (y * z + x * w), 1 - 2 * (x * x + z * z)), Math.asin(clamp(m13, -1, 1)), 0];
  return euler;
}

function limitedRotation(q: Quat, joint: Joint): Quat {
  if (joint.minimum <= -Math.PI && joint.maximum >= Math.PI) return q;
  return quatFromEuler(
    creatureEulerFromQuaternion(q).map((v) => clamp(v, joint.minimum, joint.maximum)) as Vec3,
  );
}

function rotateJointWorld(joints: Joint[], pose: Pose, id: string, delta: Quat, weight = 1) {
  const joint = joints.find((j) => j.id === id);
  if (!joint) throw new Error(`Unknown creature joint ${id}`);
  const frames = creatureJointFrames(joints, pose),
    frame = frames.get(id)!;
  const parent = joint.parent ? frames.get(joint.parent) : undefined;
  const parentDeform = parent ? quatMultiply(parent.rotation, inverse(parent.restRotation)) : quatIdentity();
  const beforeLocal = quatMultiply(parentDeform, frame.restRotation);
  const wanted = quatMultiply(inverse(beforeLocal), quatMultiply(delta, frame.rotation));
  const local = pose.get(id) ?? identityPose();
  pose.set(id, {
    translation: [...local.translation],
    rotation: quatSlerp(local.rotation, limitedRotation(wanted, joint), clamp(weight)),
  });
}

export type CreatureSolveDiagnostic = {
  id: string;
  kind: "ik" | "contact" | "secondary" | "ownership" | "pelvis" | "cloth" | "articulation";
  residual: number;
  status: "satisfied" | "unreachable" | "limited" | "unavailable" | "conflict";
  message?: string;
};

/** Analytic two-bone solution with pole-plane control and explicit reach clamping. */
export function solveTwoBonePositions(
  root: Vec3,
  middle: Vec3,
  end: Vec3,
  target: Vec3,
  pole: Vec3,
): { points: Vec3[]; residual: number } {
  const upper = distance(root, middle),
    lower = distance(middle, end);
  if (upper < 1e-8 || lower < 1e-8) return { points: [root, middle, end], residual: distance(end, target) };
  const axis = unit(sub(target, root), unit(sub(end, root))),
    requested = distance(root, target);
  const reach = clamp(requested, Math.abs(upper - lower) + 1e-8, upper + lower - 1e-8);
  const side = sub(sub(pole, root), mul(axis, dot(sub(pole, root), axis)));
  const perpendicular = unit(side, unit(cross(axis, Math.abs(axis[1]) < 0.8 ? [0, 1, 0] : [1, 0, 0])));
  const along = (upper * upper - lower * lower + reach * reach) / (2 * reach);
  const bend = Math.sqrt(Math.max(0, upper * upper - along * along));
  const points: Vec3[] = [
    [...root],
    add(root, add(mul(axis, along), mul(perpendicular, bend))),
    add(root, mul(axis, reach)),
  ];
  return { points, residual: distance(points[2], target) };
}

/** FABRIK preserves authored segment lengths. No hidden root translation. */
export function solveChainPositions(
  input: Vec3[],
  target: Vec3,
  iterations = 16,
  tolerance = 0.001,
): { points: Vec3[]; residual: number } {
  if (input.length < 2 || input.length > 64) throw new RangeError("A creature chain needs 2–64 points");
  if (
    ![...input.flat(), ...target, iterations, tolerance].every(Number.isFinite) ||
    iterations < 1 ||
    iterations > 128 ||
    tolerance <= 0
  )
    throw new RangeError("Invalid creature chain solve parameters");
  const points = input.map((p) => [...p] as Vec3),
    lengths = input.slice(1).map((p, i) => distance(p, input[i])),
    root = [...input[0]] as Vec3;
  const total = lengths.reduce((a, b) => a + b, 0);
  if (distance(root, target) >= total) {
    const direction = unit(sub(target, root));
    for (let i = 1; i < points.length; i++) points[i] = add(points[i - 1], mul(direction, lengths[i - 1]));
  } else {
    for (let iteration = 0; iteration < Math.ceil(iterations); iteration++) {
      points[points.length - 1] = [...target];
      for (let i = points.length - 2; i >= 0; i--)
        points[i] = add(points[i + 1], mul(unit(sub(points[i], points[i + 1])), lengths[i]));
      points[0] = [...root];
      for (let i = 1; i < points.length; i++)
        points[i] = add(points[i - 1], mul(unit(sub(points[i], points[i - 1])), lengths[i - 1]));
      if (distance(points[points.length - 1], target) <= tolerance) break;
    }
  }
  return { points, residual: distance(points[points.length - 1], target) };
}

function chainPoints(joints: Joint[], pose: Pose, ids: string[]): Vec3[] {
  const frames = creatureJointFrames(joints, pose),
    byId = new Map(joints.map((j) => [j.id, j]));
  if (ids.length < 2 || new Set(ids).size !== ids.length)
    throw new Error("Creature chains require unique connected joints");
  return ids.map((id, i) => {
    const frame = frames.get(id);
    if (!frame || (i > 0 && byId.get(id)?.parent !== ids[i - 1]))
      throw new Error(`Disconnected creature chain at ${id}`);
    return [...frame.position];
  });
}

function applyChainPoints(joints: Joint[], pose: Pose, ids: string[], points: Vec3[], weight: number) {
  // Solve full orientation first, then blend once, so partial influence does not
  // compound at each descendant and turn a requested half-weight into a full solve.
  const solved = clonePose(pose);
  for (let i = 0; i < ids.length - 1; i++) {
    const frames = creatureJointFrames(joints, solved),
      root = frames.get(ids[i])!.position;
    rotateJointWorld(
      joints,
      solved,
      ids[i],
      rotationBetween(sub(frames.get(ids[i + 1])!.position, root), sub(points[i + 1], points[i])),
    );
  }
  for (const id of ids.slice(0, -1)) {
    const before = pose.get(id) ?? identityPose(),
      after = solved.get(id)!;
    pose.set(id, {
      translation: [...before.translation],
      rotation: quatSlerp(before.rotation, after.rotation, clamp(weight)),
    });
  }
}

export function solveCreatureIK(
  joints: Joint[],
  input: Pose,
  chain: {
    id: string;
    joints: string[];
    target: Vec3;
    pole: Vec3;
    weight: number;
    iterations: number;
    tolerance: number;
  },
): { pose: Pose; diagnostic: CreatureSolveDiagnostic } {
  const pose = clonePose(input),
    original = chainPoints(joints, pose, chain.joints);
  const result =
    original.length === 3
      ? solveTwoBonePositions(original[0], original[1], original[2], chain.target, chain.pole)
      : solveChainPositions(original, chain.target, chain.iterations, chain.tolerance);
  applyChainPoints(joints, pose, chain.joints, result.points, chain.weight);
  const final = chainPoints(joints, pose, chain.joints),
    residual = distance(final[final.length - 1], chain.target);
  return {
    pose,
    diagnostic: {
      id: chain.id,
      kind: "ik",
      residual,
      status:
        residual <= chain.tolerance
          ? "satisfied"
          : result.residual > chain.tolerance
            ? "unreachable"
            : "limited",
    },
  };
}

export type CreaturePoseLayer = {
  pose: Pose;
  weight: number;
  mode: "additive" | "override";
  joints?: readonly string[];
};
/** The layer order is explicit and deterministic; masks never create unknown joints. */
export function layerCreaturePoses(base: Pose, layers: readonly CreaturePoseLayer[]): Pose {
  const result = clonePose(base);
  for (const layer of layers) {
    const weight = clamp(layer.weight),
      selected = layer.joints ? new Set(layer.joints) : undefined;
    if (!Number.isFinite(weight)) throw new RangeError("Pose layer weight must be finite");
    for (const [id, value] of layer.pose) {
      const before = result.get(id);
      if (!before || (selected && !selected.has(id))) continue;
      result.set(
        id,
        layer.mode === "additive"
          ? {
              translation: add(before.translation, mul(value.translation, weight)),
              rotation: quatMultiply(before.rotation, quatSlerp(quatIdentity(), value.rotation, weight)),
            }
          : {
              translation: add(before.translation, mul(sub(value.translation, before.translation), weight)),
              rotation: quatSlerp(before.rotation, value.rotation, weight),
            },
      );
    }
  }
  return result;
}

export type CreatureRuntimeState = {
  version: 1;
  contacts: Record<string, { target: Vec3; normal?: Vec3; cycle: number; motion: string }>;
  secondary: Record<string, { positions: Vec3[]; previous: Vec3[] }>;
  expressions: Record<string, number>;
};
export const createCreatureRuntimeState = (): CreatureRuntimeState => ({
  version: 1,
  contacts: {},
  secondary: {},
  expressions: {},
});
export type CreatureRuntimeContext = {
  position: Vec3;
  rotation: Quat;
  scale: number;
  time: number;
  motion?: Motion;
  /** Seconds since this motion started, not absolute session time. */
  motionTime: number;
  /** Authored character-space contact targets belong to the clip's base frame,
   * before physical root motion is extracted into the actor transform. */
  contactFrame?: { position: Vec3; rotation: Quat };
  ground?: (x: number, z: number) => { height: number; normal: Vec3 } | undefined;
  /** World-space proxies; callers exclude the chain's own attachment region. */
  colliders?: { center: Vec3; radius: number }[];
  capsules?: { a: Vec3; b: Vec3; radius: number }[];
};
const toWorld = (point: Vec3, context: CreatureRuntimeContext) =>
  add(context.position, rotateVector(context.rotation, mul(point, context.scale)));
const toLocal = (point: Vec3, context: CreatureRuntimeContext) =>
  mul(rotateVector(inverse(context.rotation), sub(point, context.position)), 1 / context.scale);

/** Projects onto authored sphere/capsule proxies; returns the point unchanged when clear. */
export function projectCreatureCollision(point: Vec3, radius: number, context: CreatureRuntimeContext): Vec3 {
  let result = point;
  const project = (cx: number, cy: number, cz: number, extent: number) => {
    const x = result[0] - cx,
      y = result[1] - cy,
      z = result[2] - cz,
      squared = x * x + y * y + z * z;
    const minimum = extent + radius;
    if (squared < minimum * minimum) {
      const scale = squared > 1e-16 ? minimum / Math.sqrt(squared) : 0;
      result = squared > 1e-16 ? [cx + x * scale, cy + y * scale, cz + z * scale] : [cx + minimum, cy, cz];
    }
  };
  for (const sphere of context.colliders ?? [])
    project(sphere.center[0], sphere.center[1], sphere.center[2], sphere.radius);
  for (const capsule of context.capsules ?? []) {
    const x = capsule.b[0] - capsule.a[0],
      y = capsule.b[1] - capsule.a[1],
      z = capsule.b[2] - capsule.a[2];
    const squared = x * x + y * y + z * z;
    const t =
      squared > 1e-12
        ? clamp(
            ((result[0] - capsule.a[0]) * x +
              (result[1] - capsule.a[1]) * y +
              (result[2] - capsule.a[2]) * z) /
              squared,
          )
        : 0;
    project(capsule.a[0] + x * t, capsule.a[1] + y * t, capsule.a[2] + z * t, capsule.radius);
  }
  return result;
}

/** Restores only bounded finite state; malformed saves never enter the solver. */
export function validateCreatureRuntimeState(
  value: unknown,
  source: CreatureDefinition,
): CreatureRuntimeState {
  if (!value || typeof value !== "object") throw new Error("Invalid creature runtime state");
  const state = value as CreatureRuntimeState;
  if (
    state.version !== 1 ||
    !state.contacts ||
    !state.secondary ||
    !state.expressions ||
    [state.contacts, state.secondary, state.expressions].some(
      (entry) => typeof entry !== "object" || Array.isArray(entry),
    )
  )
    throw new Error("Invalid creature runtime state version");
  const finiteVector = (v: unknown): v is Vec3 =>
    Array.isArray(v) && v.length === 3 && v.every((n) => typeof n === "number" && Number.isFinite(n));
  for (const [id, contact] of Object.entries(state.contacts)) {
    if (
      !source.contacts.some((c) => c.id === id) ||
      !finiteVector(contact.target) ||
      (contact.normal && !finiteVector(contact.normal)) ||
      !Number.isSafeInteger(contact.cycle) ||
      typeof contact.motion !== "string"
    )
      throw new Error(`Invalid saved creature contact ${id}`);
  }
  for (const [id, chain] of Object.entries(state.secondary)) {
    const authored = source.secondaryChains.find((c) => c.id === id);
    if (
      !authored ||
      !Array.isArray(chain.positions) ||
      !Array.isArray(chain.previous) ||
      chain.positions.length !== authored.joints.length ||
      chain.previous.length !== authored.joints.length ||
      !chain.positions.every(finiteVector) ||
      !chain.previous.every(finiteVector)
    )
      throw new Error(`Invalid saved creature secondary chain ${id}`);
  }
  for (const [id, weight] of Object.entries(state.expressions))
    if (!source.expressions.some((e) => e.id === id) || !Number.isFinite(weight) || weight < 0 || weight > 1)
      throw new Error(`Invalid saved creature expression ${id}`);
  return structuredClone(state);
}

/**
 * Ownership order: authored animation -> expressions -> authored IK -> contact IK
 * -> secondary chains. State advances only with dt > 0. A render-only call never
 * plants/releases contacts or integrates simulation, so capture frequency cannot
 * change the performance. World coordinates keep plants stable during root motion.
 */
export function evaluateCreaturePose(
  joints: Joint[],
  source: CreatureDefinition,
  input: Pose,
  state: CreatureRuntimeState,
  context: CreatureRuntimeContext,
  dt = 0,
  options: {
    stages?: readonly ("expression" | "ik" | "contacts" | "secondary")[];
    ownedJoints?: readonly string[];
  } = {},
): { pose: Pose; diagnostics: CreatureSolveDiagnostic[] } {
  if (!Number.isFinite(dt) || dt < 0 || dt > 0.1 || !Number.isFinite(context.scale) || context.scale <= 0)
    throw new RangeError("Invalid creature simulation step or scale");
  const stages = new Set(options.stages ?? ["expression", "ik", "contacts", "secondary"]);
  let pose = layerCreaturePoses(
    input,
    (stages.has("expression") ? source.expressions : []).map((expression) => ({
      pose: new Map(
        expression.weights.map((value) => [
          value.joint,
          { translation: value.translation, rotation: quatFromEuler(value.rotation) },
        ]),
      ),
      weight: own(state.expressions, expression.id) ?? expression.weight,
      mode: "additive" as const,
    })),
  );
  const diagnostics: CreatureSolveDiagnostic[] = [];
  const owned = new Set<string>(options.ownedJoints);
  for (const chain of stages.has("ik") ? source.ikChains : []) {
    if (chain.weight <= 0) continue;
    const solved = solveCreatureIK(joints, pose, chain);
    pose = solved.pose;
    diagnostics.push(solved.diagnostic);
    for (const id of chain.joints.slice(0, -1)) owned.add(id);
  }
  const active = new Set<string>();
  const contacts: {
    contact: CreatureDefinition["contacts"][number];
    chain: CreatureDefinition["ikChains"][number];
    planted: CreatureRuntimeState["contacts"][string];
    envelope: number;
  }[] = [];
  for (const contact of stages.has("contacts") ? source.contacts : []) {
    if (!context.motion || contact.motion !== context.motion.id) continue;
    const cycle = context.motion.loop ? Math.floor(context.motionTime / context.motion.duration) : 0;
    const time = context.motion.loop
      ? ((context.motionTime % context.motion.duration) + context.motion.duration) % context.motion.duration
      : context.motionTime;
    if (time < contact.start || time >= contact.end || contact.weight <= 0) continue;
    active.add(contact.id);
    const chain = source.ikChains.find((c) => c.joints[c.joints.length - 1] === contact.joint);
    if (!chain) {
      diagnostics.push({
        id: contact.id,
        kind: "contact",
        residual: 0,
        status: "unavailable",
        message: "No IK chain ends at this contact joint",
      });
      continue;
    }
    let planted = own(state.contacts, contact.id);
    if (!planted || planted.cycle !== cycle || planted.motion !== context.motion.id) {
      const target =
        contact.space === "world"
          ? ([...contact.target] as Vec3)
          : toWorld(contact.target, context.contactFrame ? { ...context, ...context.contactFrame } : context);
      const ground = contact.ground ? context.ground?.(target[0], target[2]) : undefined;
      if (contact.ground && context.ground && !ground) {
        diagnostics.push({
          id: contact.id,
          kind: "contact",
          residual: 0,
          status: "unavailable",
          message: "Contact collision region is not resident",
        });
        continue;
      }
      if (ground) target[1] = ground.height + contact.offset;
      planted = { target, normal: ground?.normal, cycle, motion: context.motion.id };
      if (dt > 0) setOwn(state.contacts, contact.id, planted);
    }
    if (contact.ground && context.ground) {
      const ground = context.ground(planted.target[0], planted.target[2]);
      if (!ground) {
        diagnostics.push({
          id: contact.id,
          kind: "contact",
          residual: 0,
          status: "unavailable",
          message: "Planted contact collision region is not resident",
        });
        continue;
      }
      planted = {
        ...planted,
        target: [planted.target[0], ground.height + contact.offset, planted.target[2]],
        normal: ground.normal,
      };
      if (dt > 0) setOwn(state.contacts, contact.id, planted);
    }
    const envelope = Math.min(
      contact.blendIn > 0 ? clamp((time - contact.start) / contact.blendIn) : 1,
      contact.blendOut > 0 ? clamp((contact.end - time) / contact.blendOut) : 1,
    );
    contacts.push({ contact, chain, planted, envelope });
  }
  if (source.pelvis && contacts.length && source.pelvis.weight > 0) {
    const controller = source.pelvis,
      pelvis = joints.find((joint) => joint.id === controller.joint);
    if (!pelvis) throw new Error(`Unknown pelvis control joint ${controller.joint}`);
    const byId = new Map(joints.map((joint) => [joint.id, joint]));
    const descendants = contacts.filter(({ chain }) => {
      let current = byId.get(chain.joints[0]);
      while (current) {
        if (current.id === pelvis.id) return true;
        current = current.parent ? byId.get(current.parent) : undefined;
      }
      return false;
    });
    const original = pose.get(pelvis.id) ?? identityPose();
    for (let iteration = 0; iteration < controller.iterations; iteration++) {
      const frames = creatureJointFrames(joints, pose);
      let correction: Vec3 = [0, 0, 0],
        weightSum = 0;
      for (const { chain, planted, contact, envelope } of descendants) {
        const points = chain.joints.map((id) => frames.get(id)!.position);
        const lengths = points.slice(1).map((point, i) => distance(point, points[i]));
        const reach = lengths.reduce((sum, length) => sum + length, 0);
        const minimum = Math.max(0, 2 * Math.max(...lengths) - reach);
        const delta = sub(toLocal(planted.target, context), points[0]),
          requested = Math.hypot(...delta);
        const error =
          requested > reach ? requested - reach + 1e-5 : requested < minimum ? requested - minimum - 1e-5 : 0;
        const weight = contact.weight * envelope;
        correction = add(correction, mul(unit(delta), error * weight));
        weightSum += weight;
      }
      if (weightSum <= 0 || Math.hypot(...correction) < 1e-7) break;
      correction = mul(correction, controller.weight / weightSum);
      const parent = pelvis.parent ? frames.get(pelvis.parent) : undefined;
      const parentDeform = parent
        ? quatMultiply(parent.rotation, inverse(parent.restRotation))
        : quatIdentity();
      const previous = pose.get(pelvis.id) ?? identityPose();
      const proposed = add(previous.translation, rotateVector(inverse(parentDeform), correction));
      const translation = proposed.map(
        (value, axis) =>
          original.translation[axis] +
          clamp(value - original.translation[axis], -controller.maxOffset[axis], controller.maxOffset[axis]),
      ) as Vec3;
      pose.set(pelvis.id, { translation, rotation: previous.rotation });
    }
    const frames = creatureJointFrames(joints, pose);
    let residual = 0;
    for (const { chain, planted } of descendants) {
      const points = chain.joints.map((id) => frames.get(id)!.position);
      const reach = points.slice(1).reduce((total, point, i) => total + distance(point, points[i]), 0);
      residual = Math.max(
        residual,
        (distance(points[0], toLocal(planted.target, context)) - reach) * context.scale,
      );
    }
    diagnostics.push({
      id: controller.joint,
      kind: "pelvis",
      residual,
      status: descendants.length === 0 ? "unavailable" : residual <= 0.001 ? "satisfied" : "limited",
      ...(descendants.length === 0
        ? { message: "No active contact chain descends from this pelvis joint" }
        : {}),
    });
  }
  for (const { contact, chain, planted, envelope } of contacts) {
    const solved = solveCreatureIK(joints, pose, {
      ...chain,
      id: contact.id,
      target: toLocal(planted.target, context),
      weight: contact.weight * envelope,
      tolerance: contact.tolerance / context.scale,
    });
    pose = solved.pose;
    if (planted.normal) {
      const frame = creatureJointFrames(joints, pose).get(contact.joint)!;
      const deform = quatMultiply(frame.rotation, inverse(frame.restRotation));
      rotateJointWorld(
        joints,
        pose,
        contact.joint,
        rotationBetween(
          rotateVector(deform, [0, 1, 0]),
          rotateVector(inverse(context.rotation), planted.normal),
        ),
        contact.weight * envelope,
      );
    }
    for (const id of chain.joints) owned.add(id);
    diagnostics.push({
      ...solved.diagnostic,
      kind: "contact",
      residual: solved.diagnostic.residual * context.scale,
    });
  }
  if (dt > 0 && stages.has("contacts"))
    for (const id of Object.keys(state.contacts)) if (!active.has(id)) delete state.contacts[id];
  for (const chain of stages.has("secondary") ? source.secondaryChains : []) {
    if (chain.weight <= 0) continue;
    if (chain.joints.slice(0, -1).some((id) => owned.has(id))) {
      diagnostics.push({
        id: chain.id,
        kind: "ownership",
        residual: 0,
        status: "conflict",
        message: "Secondary chain overlaps a joint owned by IK/contact; secondary solve skipped",
      });
      continue;
    }
    const rest = chainPoints(joints, pose, chain.joints).map((p) => toWorld(p, context));
    let simulation = own(state.secondary, chain.id);
    if (!simulation) simulation = { positions: rest.map((p) => [...p]), previous: rest.map((p) => [...p]) };
    if (dt > 0) {
      const prior = simulation.positions.map((p) => [...p] as Vec3),
        positions = simulation.positions.map((p) => [...p] as Vec3);
      positions[0] = [...rest[0]];
      const attenuation = Math.exp(-chain.damping * dt),
        acceleration = add(chain.gravity, chain.wind);
      for (let i = 1; i < positions.length; i++) {
        positions[i] = add(
          add(positions[i], mul(sub(positions[i], simulation.previous[i]), attenuation)),
          mul(acceleration, dt * dt),
        );
        // Implicit spring is stable for large authored stiffness values.
        positions[i] = add(
          positions[i],
          mul(sub(rest[i], positions[i]), 1 - 1 / (1 + chain.stiffness * dt * dt)),
        );
      }
      const radius = chain.collisionRadius * context.scale;
      for (let iteration = 0; iteration < 6; iteration++) {
        positions[0] = [...rest[0]];
        for (let i = 1; i < positions.length; i++) {
          const length = distance(rest[i], rest[i - 1]),
            preferred = unit(sub(rest[i], rest[i - 1]));
          let direction = unit(sub(positions[i], positions[i - 1]), preferred);
          const angle = Math.acos(clamp(dot(preferred, direction), -1, 1));
          if (angle > chain.maxAngle)
            direction = rotateVector(
              quatSlerp(quatIdentity(), rotationBetween(preferred, direction), chain.maxAngle / angle),
              preferred,
            );
          positions[i] = add(positions[i - 1], mul(direction, length));
          const ground = context.ground?.(positions[i][0], positions[i][2]);
          if (ground && positions[i][1] < ground.height + radius) positions[i][1] = ground.height + radius;
          positions[i] = projectCreatureCollision(positions[i], radius, context);
        }
      }
      simulation = { positions, previous: prior };
      setOwn(state.secondary, chain.id, simulation);
    }
    applyChainPoints(
      joints,
      pose,
      chain.joints,
      simulation.positions.map((p) => toLocal(p, context)),
      chain.weight,
    );
    const actual = chainPoints(joints, pose, chain.joints).map((p) => toWorld(p, context));
    let residual = 0;
    for (let i = 1; i < actual.length; i++) {
      residual = Math.max(residual, distance(actual[i], simulation.positions[i]));
      const ground = context.ground?.(actual[i][0], actual[i][2]);
      if (ground)
        residual = Math.max(residual, ground.height + chain.collisionRadius * context.scale - actual[i][1]);
    }
    diagnostics.push({
      id: chain.id,
      kind: "secondary",
      residual,
      status: residual <= 0.005 ? "satisfied" : "limited",
    });
    for (const id of chain.joints.slice(0, -1)) owned.add(id);
  }
  return { pose, diagnostics };
}
