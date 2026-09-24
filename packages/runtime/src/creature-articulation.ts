import type { CreatureArticulation, Joint, Quat, Vec3 } from "@wrela/model";

import { type Pose, quatIdentity, quatMultiply, quatSlerp, rotateVector } from "./animation";
import { type CreatureRuntimeContext, creatureJointFrames } from "./creature-runtime";
import type { BodyState, PhysicsAdapter } from "./physics";

const add = (a: Vec3, b: Vec3): Vec3 => a.map((value, axis) => value + b[axis]) as Vec3;
const sub = (a: Vec3, b: Vec3): Vec3 => a.map((value, axis) => value - b[axis]) as Vec3;
const scale = (a: Vec3, value: number): Vec3 => a.map((v) => v * value) as Vec3;
const inverse = (q: Quat): Quat => [-q[0], -q[1], -q[2], q[3]];
const clamp = (value: number) => Math.max(0, Math.min(1, value));
const blend = (a: Vec3, b: Vec3, weight: number): Vec3 => a.map((v, i) => v + (b[i] - v) * weight) as Vec3;
const angularVelocity = (before: Quat, after: Quat, dt: number): Vec3 => {
  let delta = quatMultiply(after, inverse(before));
  if (delta[3] < 0) delta = delta.map((value) => -value) as Quat;
  const length = Math.hypot(delta[0], delta[1], delta[2]);
  if (length < 1e-9 || dt <= 0) return [0, 0, 0];
  return scale([delta[0], delta[1], delta[2]], (2 * Math.atan2(length, delta[3])) / (length * dt));
};

export type CreatureArticulationSnapshot = {
  version: 1;
  mode: "animation" | "ragdoll" | "recovering";
  recoveryElapsed: number;
  recoveryDuration: number;
  bodies: { joint: string; state: BodyState }[];
};

/** Validate portable state before mutating a shared physics world or installing any bodies. */
export function validateCreatureArticulationSnapshot(
  value: unknown,
  definition: CreatureArticulation,
): CreatureArticulationSnapshot {
  if (!value || typeof value !== "object") throw new Error("Invalid articulation snapshot");
  const snapshot = value as CreatureArticulationSnapshot;
  if (
    snapshot.version !== 1 ||
    !["animation", "ragdoll", "recovering"].includes(snapshot.mode) ||
    !Number.isFinite(snapshot.recoveryElapsed) ||
    !Number.isFinite(snapshot.recoveryDuration) ||
    snapshot.recoveryElapsed < 0 ||
    snapshot.recoveryDuration < snapshot.recoveryElapsed ||
    snapshot.recoveryDuration > 10 ||
    (snapshot.mode === "recovering" && snapshot.recoveryDuration <= 0) ||
    !Array.isArray(snapshot.bodies) ||
    snapshot.bodies.length !== definition.bodies.length
  )
    throw new Error("Invalid articulation snapshot");
  const expected = new Set(definition.bodies.map((body) => body.joint));
  const restored = new Set<string>();
  const numeric = (array: unknown, count: number): array is number[] =>
    Array.isArray(array) &&
    array.length === count &&
    array.every((v) => typeof v === "number" && Number.isFinite(v));
  for (const body of snapshot.bodies) {
    const state = body?.state;
    if (
      !body ||
      !expected.has(body.joint) ||
      restored.has(body.joint) ||
      !state ||
      !numeric(state.position, 3) ||
      !numeric(state.rotation, 4) ||
      !numeric(state.velocity, 3) ||
      !numeric(state.angularVelocity, 3) ||
      Math.abs(Math.hypot(...state.rotation) - 1) > 0.001 ||
      state.mode !== (snapshot.mode === "animation" ? "kinematic" : "dynamic")
    )
      throw new Error("Invalid articulation body snapshot");
    restored.add(body.joint);
  }
  return structuredClone(snapshot);
}

/** Pure shared mapping for active bodies and dormant saved bodies. */
function resolveArticulationPose(
  joints: Joint[],
  definition: CreatureArticulation,
  mode: CreatureArticulationSnapshot["mode"],
  recoveryElapsed: number,
  recoveryDuration: number,
  bodyState: (joint: string) => BodyState | undefined,
  authored: Pose,
  context: CreatureRuntimeContext,
): Pose {
  if (mode === "animation") return authored;
  const elapsed = recoveryDuration > 0 ? clamp(recoveryElapsed / recoveryDuration) : 0;
  const weight = mode === "recovering" ? 1 - elapsed * elapsed * (3 - 2 * elapsed) : 1;
  const result: Pose = new Map(
    [...authored].map(([id, value]) => [
      id,
      { translation: [...value.translation], rotation: [...value.rotation] },
    ]),
  );
  const byId = new Map(joints.map((joint) => [joint.id, joint]));
  const done = new Set<string>();
  const solve = (joint: Joint) => {
    if (done.has(joint.id)) return;
    if (joint.parent) {
      const parent = byId.get(joint.parent);
      if (parent) solve(parent);
    }
    done.add(joint.id);
    const physical = bodyState(joint.id);
    const specification = definition.bodies.find((body) => body.joint === joint.id);
    if (!physical || !specification) return;
    const rotation = quatMultiply(inverse(context.rotation), physical.rotation);
    const jointWorld = sub(
      physical.position,
      rotateVector(physical.rotation, scale(specification.offset ?? [0, 0, 0], context.scale)),
    );
    const position = scale(
      rotateVector(inverse(context.rotation), sub(jointWorld, context.position)),
      1 / context.scale,
    );
    const frames = creatureJointFrames(joints, result);
    const frame = frames.get(joint.id);
    if (!frame) return;
    const parent = joint.parent ? frames.get(joint.parent) : undefined;
    const parentJoint = joint.parent ? byId.get(joint.parent) : undefined;
    const parentDeform = parent
      ? quatMultiply(parent.rotation, inverse(parent.restRotation))
      : quatIdentity();
    const translation = sub(
      rotateVector(inverse(parentDeform), sub(position, parent?.position ?? [0, 0, 0])),
      sub(joint.position, parentJoint?.position ?? [0, 0, 0]),
    );
    const localRotation = quatMultiply(inverse(quatMultiply(parentDeform, frame.restRotation)), rotation);
    const original = authored.get(joint.id) ?? { translation: [0, 0, 0] as Vec3, rotation: quatIdentity() };
    result.set(joint.id, {
      translation: blend(original.translation, translation, weight),
      rotation: quatSlerp(original.rotation, localRotation, weight),
    });
  };
  for (const joint of joints) solve(joint);
  return result;
}

/** Dormant creatures preserve their physical silhouette without reinstalling a collision world. */
export function resolveCreatureArticulationSnapshot(
  joints: Joint[],
  definition: CreatureArticulation,
  snapshot: CreatureArticulationSnapshot,
  authoredPose: Pose,
  context: CreatureRuntimeContext,
): Pose {
  const bodies = new Map(snapshot.bodies.map((body) => [body.joint, body.state]));
  return resolveArticulationPose(
    joints,
    definition,
    snapshot.mode,
    snapshot.recoveryElapsed,
    snapshot.recoveryDuration,
    (joint) => bodies.get(joint),
    authoredPose,
    context,
  );
}

/** Articulated bodies share the adapter's collision world and fixed clock. Rendering only reads this controller. */
export class CreatureArticulationController {
  private mode: CreatureArticulationSnapshot["mode"] = "animation";
  private recoveryElapsed = 0;
  private recoveryDuration = 0;
  private disposed = false;
  private targets = new Map<string, { position: Vec3; rotation: Quat }>();
  private readonly bodyIds = new Map<string, string>();
  private transfer = new Map<string, { velocity: Vec3; angularVelocity: Vec3 }>();
  private readonly constraintIds: string[] = [];

  constructor(
    private readonly physics: PhysicsAdapter,
    readonly id: string,
    private readonly joints: Joint[],
    private readonly definition: CreatureArticulation,
    context: CreatureRuntimeContext,
    pose: Pose,
    restore?: CreatureArticulationSnapshot,
  ) {
    if (!(context.scale > 0) || !Number.isFinite(context.scale))
      throw new Error("Invalid articulation scale");
    if (restore) restore = validateCreatureArticulationSnapshot(restore, definition);
    const frames = creatureJointFrames(joints, pose);
    const created: string[] = [];
    try {
      for (const body of definition.bodies) {
        const frame = frames.get(body.joint);
        if (!frame) throw new Error(`Unknown articulation joint ${body.joint}`);
        const bodyId = `${id}::articulation::${body.joint}`;
        this.bodyIds.set(body.joint, bodyId);
        const rotation = quatMultiply(context.rotation, frame.rotation);
        const position = add(
          context.position,
          rotateVector(
            context.rotation,
            scale(add(frame.position, rotateVector(frame.rotation, body.offset ?? [0, 0, 0])), context.scale),
          ),
        );
        this.targets.set(body.joint, { position, rotation });
        if (!physics.hasBody(bodyId)) {
          physics.add({
            id: bodyId,
            position,
            rotation,
            radius: body.radius * context.scale,
            halfHeight: (body.halfHeight ?? 0) * context.scale,
            mass: body.mass,
            friction: definition.friction ?? 0.8,
            restitution: definition.restitution ?? 0,
            mode: "kinematic",
            colliders: [
              {
                id: body.joint,
                shape: "capsule",
                position: [0, 0, 0],
                rotation: [0, 0, 0],
                radius: body.radius * context.scale,
                halfHeight: (body.halfHeight ?? 0) * context.scale,
              },
            ],
          });
          created.push(bodyId);
        }
        physics.setBodyOwner(bodyId, id);
        physics.setDamping(bodyId, 0.1, 0.3);
      }
      for (const joint of definition.joints) {
        const parent = this.bodyIds.get(joint.parent),
          child = this.bodyIds.get(joint.child);
        if (!parent || !child) throw new Error(`Missing articulation body for ${joint.id}`);
        const constraintId = `${id}::articulation-joint::${joint.id}`;
        this.constraintIds.push(constraintId);
        if (physics.hasConstraint(constraintId)) continue;
        const frame = frames.get(joint.child);
        if (!frame) throw new Error(`Unknown articulation child ${joint.child}`);
        const anchor = add(
          context.position,
          rotateVector(context.rotation, scale(frame.position, context.scale)),
        );
        const p = physics.state(parent),
          c = physics.state(child);
        if (joint.kind === "revolute") {
          const axis = joint.axis ?? [1, 0, 0];
          const pa = rotateVector(p.rotation, axis),
            ca = rotateVector(c.rotation, axis);
          const denominator = Math.hypot(...pa) * Math.hypot(...ca);
          const alignment = pa.reduce((total, value, index) => total + value * ca[index], 0) / denominator;
          if (!Number.isFinite(alignment) || alignment < 0.999)
            throw new Error(`Hinge ${joint.id} local axes are not aligned in its authored reference pose`);
        }
        physics.addConstraint({
          id: constraintId,
          parent,
          child,
          kind: joint.kind,
          anchorParent: rotateVector(inverse(p.rotation), sub(anchor, p.position)),
          anchorChild: rotateVector(inverse(c.rotation), sub(anchor, c.position)),
          axis: joint.axis,
          minimum: joint.minimum,
          maximum: joint.maximum,
        });
      }
      if (restore) this.restore(restore);
      else this.setEnabled(false);
    } catch (error) {
      for (const body of created) physics.remove(body);
      throw error;
    }
  }

  static restoreFromCheckpoint(
    physics: PhysicsAdapter,
    id: string,
    joints: Joint[],
    definition: CreatureArticulation,
    context: CreatureRuntimeContext,
    pose: Pose,
    state: CreatureArticulationSnapshot,
  ) {
    return new CreatureArticulationController(physics, id, joints, definition, context, pose, state);
  }

  get authority() {
    return this.mode;
  }
  private setEnabled(physical: boolean) {
    for (const body of this.bodyIds.values()) this.physics.setEnabled(body, physical);
    if (this.physics.hasBody(this.id)) this.physics.setEnabled(this.id, !physical);
  }
  /** Run once before the shared physics step. Animation targets retain finite-difference transfer velocities. */
  updateTargets(pose: Pose, context: CreatureRuntimeContext, dt: number) {
    if (!Number.isFinite(dt) || dt <= 0 || dt > 0.1)
      throw new Error("Articulation requires a bounded fixed timestep");
    const frames = creatureJointFrames(this.joints, pose);
    for (const body of this.definition.bodies) {
      const frame = frames.get(body.joint);
      const bodyId = this.bodyIds.get(body.joint);
      if (!frame || !bodyId) continue;
      const rotation = quatMultiply(context.rotation, frame.rotation);
      const position = add(
        context.position,
        rotateVector(
          context.rotation,
          scale(add(frame.position, rotateVector(frame.rotation, body.offset ?? [0, 0, 0])), context.scale),
        ),
      );
      const before = this.targets.get(body.joint) ?? { position, rotation };
      this.targets.set(body.joint, { position, rotation });
      if (this.mode === "animation") {
        const transfer = {
          velocity: scale(sub(position, before.position), 1 / dt),
          angularVelocity: angularVelocity(before.rotation, rotation, dt),
        };
        this.transfer.set(bodyId, transfer);
        this.physics.restoreBodyState(bodyId, { position, rotation, ...transfer, mode: "kinematic" });
      } else if (this.mode === "recovering") {
        const state = this.physics.state(bodyId);
        const stiffness = this.definition.recoveryStiffness ?? 40;
        const damping = this.definition.recoveryDamping ?? 10;
        const impulse = sub(scale(sub(position, state.position), stiffness), scale(state.velocity, damping));
        const correction = angularVelocity(state.rotation, rotation, 1);
        const torque = sub(scale(correction, stiffness * 0.15), scale(state.angularVelocity, damping * 0.15));
        // Bound acceleration and angular acceleration so a distant recovery target cannot explode the world.
        const bounded = (v: Vec3) =>
          scale(v, Math.min(1, 80 / Math.max(1e-8, Math.hypot(...v))) * body.mass * dt);
        this.physics.impulse(bodyId, bounded(add(impulse, [0, 9.81, 0])), bounded(torque));
      }
    }
    if (this.mode === "recovering") {
      this.recoveryElapsed = Math.min(this.recoveryDuration, this.recoveryElapsed + dt);
      if (this.recoveryElapsed >= this.recoveryDuration) {
        this.mode = "animation";
        this.transfer.clear();
        for (const [joint, bodyId] of this.bodyIds) {
          const target = this.targets.get(joint);
          if (target)
            this.physics.restoreBodyState(bodyId, {
              ...target,
              velocity: [0, 0, 0],
              angularVelocity: [0, 0, 0],
              mode: "kinematic",
            });
        }
        this.setEnabled(false);
      }
    }
  }
  enterRagdoll(velocity?: Vec3) {
    if (velocity?.some((v) => !Number.isFinite(v))) throw new Error("Ragdoll velocity must be finite");
    for (const body of this.bodyIds.values()) {
      const state = this.physics.state(body);
      const transfer = this.mode === "animation" ? this.transfer.get(body) : undefined;
      const inherited = transfer?.velocity ?? state.velocity;
      this.physics.restoreBodyState(body, {
        ...state,
        mode: "dynamic",
        velocity: velocity ? add(inherited, velocity) : inherited,
        angularVelocity: transfer?.angularVelocity ?? state.angularVelocity,
      });
    }
    this.mode = "ragdoll";
    this.recoveryElapsed = 0;
    this.recoveryDuration = 0;
    this.setEnabled(true);
  }
  impulse(joint: string, impulse: Vec3, torque: Vec3 = [0, 0, 0]) {
    const body = this.bodyIds.get(joint);
    if (!body) throw new Error(`Unknown articulated joint ${joint}`);
    if (this.mode === "animation") this.enterRagdoll();
    this.physics.impulse(body, impulse, torque);
  }
  recover(duration = 0.75) {
    if (!Number.isFinite(duration) || duration <= 0 || duration > 10)
      throw new Error("Recovery duration must be in (0,10] seconds");
    if (this.mode === "animation") return;
    this.mode = "recovering";
    this.recoveryElapsed = 0;
    this.recoveryDuration = duration;
  }
  /** Read-only mapping from physical world frames into the authored skeleton's local pose convention. */
  resolvePose(authored: Pose, context: CreatureRuntimeContext): Pose {
    return resolveArticulationPose(
      this.joints,
      this.definition,
      this.mode,
      this.recoveryElapsed,
      this.recoveryDuration,
      (joint) => {
        const id = this.bodyIds.get(joint);
        return id ? this.physics.state(id) : undefined;
      },
      authored,
      context,
    );
  }
  /** World translation required to keep the actor's reference frame with its
   * highest physical joint. Limbs remain in place; only the actor/camera origin moves. */
  rootDisplacement(authored: Pose, context: CreatureRuntimeContext): Vec3 {
    if (this.mode !== "ragdoll") return [0, 0, 0];
    const byId = new Map(this.joints.map((joint) => [joint.id, joint]));
    const depth = (id: string) => {
      let joint = byId.get(id),
        result = 0;
      while (joint?.parent) {
        result++;
        joint = byId.get(joint.parent);
      }
      return result;
    };
    const source = [...this.definition.bodies].sort((a, b) => depth(a.joint) - depth(b.joint))[0];
    const id = source ? this.bodyIds.get(source.joint) : undefined;
    if (!source || !id) return [0, 0, 0];
    const frame = creatureJointFrames(this.joints, authored).get(source.joint);
    if (!frame) return [0, 0, 0];
    const body = this.physics.state(id);
    const actual = sub(
      body.position,
      rotateVector(body.rotation, scale(source.offset ?? [0, 0, 0], context.scale)),
    );
    const target = add(
      context.position,
      rotateVector(context.rotation, scale(frame.position, context.scale)),
    );
    return sub(actual, target);
  }

  reset(pose: Pose, context: CreatureRuntimeContext) {
    this.mode = "animation";
    this.recoveryDuration = this.recoveryElapsed = 0;
    this.targets.clear();
    this.transfer.clear();
    this.updateTargets(pose, context, 1 / 60);
    this.setEnabled(false);
  }
  snapshot(): CreatureArticulationSnapshot {
    return {
      version: 1,
      mode: this.mode,
      recoveryElapsed: this.recoveryElapsed,
      recoveryDuration: this.recoveryDuration,
      bodies: [...this.bodyIds].map(([joint, id]) => ({
        joint,
        state: {
          ...this.physics.state(id),
          ...(this.mode === "animation" ? this.transfer.get(id) : undefined),
        },
      })),
    };
  }
  private restore(snapshot: CreatureArticulationSnapshot) {
    snapshot = validateCreatureArticulationSnapshot(snapshot, this.definition);
    for (const body of snapshot.bodies) {
      const id = this.bodyIds.get(body.joint);
      if (!id) throw new Error(`Unknown articulation body ${body.joint}`);
      this.physics.restoreBodyState(id, body.state);
      this.transfer.set(id, {
        velocity: [...body.state.velocity],
        angularVelocity: [...body.state.angularVelocity],
      });
    }
    this.mode = snapshot.mode;
    this.recoveryElapsed = snapshot.recoveryElapsed;
    this.recoveryDuration = snapshot.recoveryDuration;
    this.setEnabled(this.mode !== "animation");
  }
  diagnostics() {
    return this.constraintIds.map((id) => ({ id, residual: this.physics.constraintError(id) }));
  }
  dispose() {
    if (this.disposed) return;
    for (const joint of this.constraintIds) this.physics.removeConstraint(joint);
    for (const body of this.bodyIds.values()) this.physics.remove(body);
    if (this.physics.hasBody(this.id)) this.physics.setEnabled(this.id, true);
    this.disposed = true;
  }
}
