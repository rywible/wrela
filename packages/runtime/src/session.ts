import { queryWater } from "@wrela/compiler";
import {
  type CharacterDefinition,
  type CompiledCharacter,
  contentKey,
  type Quat,
  transformMatrix,
  type Vec3,
  type WaterDefinition,
} from "@wrela/model";
import { CELL_SIZE, type DormantEntity, type WorldSession, worldPosition } from "@wrela/world";
import {
  blendPoses,
  type Pose,
  poseMatrices,
  quatFromEuler,
  quatIdentity,
  quatMultiply,
  quatSlerp,
  rotateVector,
  sampleMotion,
} from "./animation";
import { FixedClock } from "./clock";
import { planEntityResidency } from "./lifecycle";
import {
  compileMotionEvents,
  crossedMotionEvents,
  type MotionEventMarker,
  type RuntimeMotionEvent,
} from "./motion-events";
import {
  type BodyConfiguration,
  type BodyState,
  PhysicsAdapter,
  type PhysicsCheckpoint,
  type PhysicsMode,
} from "./physics";

export type RootMotionPolicy = "physical" | "visual";

type CharacterInstance = {
  id: string;
  artifact: CompiledCharacter;
  basePosition: Vec3;
  baseRotation: Quat;
  scale: number;
  rootMotionPolicy: RootMotionPolicy;
  motion?: string;
  motionStart: number;
  previousMotion?: string;
  previousStart: number;
  blendStart: number;
  blendDuration: number;
  mode: PhysicsMode;
  previousPosition: Vec3;
  position: Vec3;
  previousRotation: Quat;
  rotation: Quat;
  configuration: BodyConfiguration;
  dormant?: BodyState;
};
type Control = { tick: number; id: string } & (
  | { kind: "mode"; mode: PhysicsMode }
  | { kind: "motion"; motion: string; blend: number }
  | { kind: "target"; position: Vec3 }
  | { kind: "facing"; yaw: number }
  | { kind: "rootMotionPolicy"; policy: RootMotionPolicy }
);
export type RuntimeCheckpoint = {
  tick: number;
  physics: PhysicsCheckpoint;
  instances: CharacterInstance[];
  worldRevision: number;
  collisionKey: string;
};
export class RuntimeSession {
  readonly clock = new FixedClock();
  private instances = new Map<string, CharacterInstance>();
  private controls = new Map<number, Map<string, Control>>();
  private controlCount = 0;
  private replayEndTick = 0;
  private checkpoints: RuntimeCheckpoint[] = [];
  private world?: WorldSession;
  private water?: WaterDefinition;
  private motionEvents = new Map<string, readonly MotionEventMarker[]>();
  private animationEvents: RuntimeMotionEvent[] = [];
  private droppedAnimationEvents = 0;
  private focusId?: string;
  private physicalInterests = new Map<string, Vec3>();
  private poseOverrides = new Map<string, Map<string, { rotation: Vec3; translation: Vec3 }>>();
  private worldRevision = 0;
  private collisionKey = "";
  private appliedOverrides = new Map<string, string>();
  private seekEpoch = 0;
  private seeking = false;
  private seekPauseBase = false;
  private seekBaseline?: RuntimeCheckpoint;
  private seekBaselineCheckpoints: RuntimeCheckpoint[] = [];
  get isSeeking() {
    return this.seeking;
  }
  get earliestSeekTick() {
    return (
      this.checkpoints.find((checkpoint) => checkpoint.collisionKey === this.collisionKey)?.tick ??
      this.clock.tick
    );
  }
  paused = false;
  blockedReason: string | null = null;
  private constructor(readonly physics: PhysicsAdapter) {}
  static async create() {
    const session = new RuntimeSession(await PhysicsAdapter.create());
    session.resetHistory();
    return session;
  }
  addCharacter(
    id: string,
    artifact: CompiledCharacter,
    definition: CharacterDefinition,
    position: Vec3 = [0, 0, 0],
    rotation: Vec3 = [0, 0, 0],
    scale = 1,
  ) {
    if (!Number.isFinite(scale) || scale <= 0 || scale > 100) throw new RangeError("Invalid character scale");
    const bounds = artifact.mesh.bounds,
      radius = Math.max(
        0.1,
        Math.min(bounds.max[0] - bounds.min[0], bounds.max[2] - bounds.min[2]) * 0.35 * scale,
      ),
      halfHeight = Math.max(0, (bounds.max[1] - bounds.min[1]) * 0.5 * scale - radius);
    const baseRotation = quatFromEuler(rotation);
    const offset = rotateVector(baseRotation, [0, ((bounds.min[1] + bounds.max[1]) / 2) * scale, 0]);
    const center = position.map((v, i) => v + offset[i]) as Vec3;
    const configuration: BodyConfiguration = {
      id,
      position: center,
      rotation: baseRotation,
      radius,
      halfHeight,
      ...definition.physics,
      colliders: definition.physics.colliders?.map((collider) => ({
        ...collider,
        position: collider.position.map(
          (v, i) => (v - (i === 1 ? (bounds.min[1] + bounds.max[1]) / 2 : 0)) * scale,
        ) as Vec3,
        ...(collider.shape === "box"
          ? { size: collider.size.map((v) => v * scale) as Vec3 }
          : { radius: collider.radius * scale }),
        ...(collider.shape === "capsule" ? { halfHeight: collider.halfHeight * scale } : {}),
      })),
    };
    const dormant: BodyState | undefined =
      this.world && this.world.queryGround(center[0], center[2]).status !== "ready"
        ? {
            position: [...center],
            rotation: [...baseRotation],
            velocity: [0, 0, 0],
            angularVelocity: [0, 0, 0],
            mode: configuration.mode,
          }
        : undefined;
    if (!dormant) this.physics.add(configuration);
    this.focusId ??= id;
    this.instances.set(id, {
      configuration,
      dormant,
      id,
      artifact,
      basePosition: center,
      baseRotation,
      scale,
      position: center,
      previousPosition: center,
      rotation: baseRotation,
      previousRotation: baseRotation,
      mode: definition.physics.mode,
      rootMotionPolicy: "physical",
      motion: artifact.motions[0]?.id,
      motionStart: 0,
      previousStart: 0,
      blendStart: 0,
      blendDuration: 0,
    });
    this.resetHistory();
  }
  updateCharacterParameters(definition: CharacterDefinition) {
    for (const instance of this.instances.values())
      if (instance.artifact.id === definition.id) {
        Object.assign(instance.configuration, definition.physics);
        if (!instance.dormant) this.physics.setCoefficients(instance.id, definition.physics);
        if (instance.mode !== definition.physics.mode) this.setMode(instance.id, definition.physics.mode);
      }
  }
  resetReplay() {
    this.resetHistory();
  }
  private resetHistory() {
    this.controls.clear();
    this.controlCount = 0;
    this.replayEndTick = this.clock.tick;
    this.checkpoints = [this.checkpoint()];
  }
  removeCharacter(id: string) {
    this.physics.remove(id);
    this.instances.delete(id);
    this.world?.removeInterest(`runtime:${id}`);
    this.physicalInterests.delete(id);
    if (this.focusId === id) this.focusId = this.instances.keys().next().value;
    this.resetHistory();
  }
  setWater(water?: WaterDefinition) {
    this.water = water;
  }
  bodyState(id: string): BodyState {
    const instance = this.instances.get(id);
    if (!instance) throw new Error(`Unknown character ${id}`);
    return instance.dormant ? structuredClone(instance.dormant) : this.physics.state(id);
  }
  setFocusCharacter(id: string) {
    if (!this.instances.has(id)) throw new Error(`Unknown character ${id}`);
    this.focusId = id;
    this.synchronizeLifecycle();
  }
  get entityLifecycle() {
    return [...this.instances.values()].map((instance) => ({
      id: instance.id,
      state: instance.dormant ? ("dormant" as const) : ("active" as const),
    }));
  }
  /** Active actors own physical residency; distant actors retain exact dormant state. */
  synchronizeLifecycle() {
    if (!this.world || !this.focusId) return;
    const focus = this.instances.get(this.focusId);
    if (focus) this.rebasePhysicsNear(focus.position);
    const external = this.world.interestSources().filter((interest) => !interest.id.startsWith("runtime:"));
    const plan = planEntityResidency(
      [...this.instances.values()].map((instance) => ({
        id: instance.id,
        position: instance.position,
        dormant: !!instance.dormant,
        removed: !!this.world?.persistence.overrides.get(instance.id)?.removed,
      })),
      this.focusId,
      Math.max(0, 8 - external.length),
    );
    let changed = false;
    for (const id of this.physicalInterests.keys())
      if (!plan.regions.has(id)) {
        this.world.removeInterest(`runtime:${id}`);
        this.physicalInterests.delete(id);
        changed = true;
      }
    for (const [id, position] of plan.regions) {
      const previous = this.physicalInterests.get(id);
      if (!previous || Math.hypot(previous[0] - position[0], previous[2] - position[2]) >= 8) {
        this.world.setInterest({
          id: `runtime:${id}`,
          position,
          visualRadius: 48,
          collisionRadius: 40,
          priority: 2,
        });
        this.physicalInterests.set(id, [...position]);
        changed = true;
      }
    }
    for (const instance of this.instances.values()) {
      if (!plan.wanted.has(instance.id)) {
        if (!instance.dormant) {
          instance.dormant = this.physics.state(instance.id);
          this.physics.remove(instance.id);
        }
      } else if (
        instance.dormant &&
        this.world.queryGround(instance.position[0], instance.position[2]).status === "ready"
      ) {
        this.physics.add({ ...instance.configuration, position: instance.dormant.position });
        this.physics.restoreBodyState(instance.id, instance.dormant);
        instance.dormant = undefined;
      }
    }
    if (changed) this.world.update();
  }
  attachWorld(world: WorldSession) {
    this.world = world;
    this.syncWorld();
    this.resetHistory();
  }
  private currentCollisionKey() {
    return this.world
      ? contentKey(
          this.world
            .collisionPatches()
            .filter((patch) => patch.collision)
            .map((patch) => patch.key)
            .sort(),
        )
      : "";
  }
  private syncWorld() {
    if (!this.world) return;
    const revision = this.world.metrics.collisionRevision;
    if (revision === this.worldRevision) return;
    const key = this.currentCollisionKey();
    this.worldRevision = revision;
    // View residency is independent of physics history. Only a different actual
    // collider set establishes a new replay boundary.
    if (key === this.collisionKey) return;
    this.physics.installTerrain(this.world.collisionPatches().filter((patch) => patch.collision));
    this.collisionKey = key;
    this.checkpoints = [this.checkpoint()];
  }
  private syncOverrides() {
    if (!this.world) return;
    for (const instance of this.instances.values()) {
      const override = this.world.persistence.overrides.get(instance.id),
        key = contentKey(override ?? null);
      if (this.appliedOverrides.get(instance.id) === key) continue;
      if (!instance.dormant) this.physics.setEnabled(instance.id, !override?.removed);
      const authored = this.world.world.instances.find((i) => i.id === instance.id);
      if (override?.position || override?.rotation || this.appliedOverrides.has(instance.id)) {
        const rotation = quatFromEuler(override?.rotation ?? authored?.rotation ?? [0, 0, 0]);
        const origin = override?.position ?? authored?.position;
        if (origin) {
          const height =
            ((instance.artifact.mesh.bounds.min[1] + instance.artifact.mesh.bounds.max[1]) / 2) *
            instance.scale;
          const offset = rotateVector(rotation, [0, height, 0]);
          const center = origin.map((v, i) => v + offset[i]) as Vec3;
          if (instance.dormant) {
            instance.dormant.position = [...center];
            instance.dormant.rotation = rotation;
          } else {
            this.physics.teleport(instance.id, center);
            this.physics.setRotation(instance.id, rotation);
          }
          instance.position = [...center];
          instance.previousPosition = [...center];
          instance.basePosition = [...center];
          instance.rotation = rotation;
          instance.previousRotation = rotation;
          instance.baseRotation = rotation;
        }
      }
      this.appliedOverrides.set(instance.id, key);
    }
    for (const instance of this.world.world.instances) {
      if (this.instances.has(instance.id)) continue;
      const override = this.world.persistence.overrides.get(instance.id),
        key = contentKey(override ?? null);
      if (this.appliedOverrides.get(instance.id) === key) continue;
      this.physics.updateStaticObject(
        instance.id,
        !override?.removed,
        override?.position ?? instance.position,
        quatFromEuler(override?.rotation ?? instance.rotation),
      );
      this.appliedOverrides.set(instance.id, key);
    }
  }
  get replayUsage() {
    return {
      commands: this.controlCount,
      earliestTick: this.earliestSeekTick,
      latestTick: this.replayEndTick,
    };
  }
  private enqueue(control: Control) {
    if (!this.instances.has(control.id)) throw new Error(`Unknown character ${control.id}`);
    // Editing a replay creates a new deterministic branch, including its baseline.
    if (this.clock.tick < this.replayEndTick) {
      for (const [tick, commands] of this.controls)
        if (tick > this.clock.tick) {
          this.controlCount -= commands.size;
          this.controls.delete(tick);
        }
      this.checkpoints = this.checkpoints.filter((checkpoint) => checkpoint.tick <= this.clock.tick);
      this.checkpoints.push(this.checkpoint());
      this.replayEndTick = this.clock.tick;
    }
    const key = `${control.id}:${control.kind}`;
    const pending = this.controls.get(control.tick) ?? new Map<string, Control>();
    // Coalesce latest intent for an actor/action at a tick. Live input is bounded
    // independently of session duration; old replay commands retire with a baseline.
    if (!pending.has(key) && this.controlCount >= 4096) {
      this.checkpoints = [this.checkpoint()];
      for (const [tick, commands] of this.controls)
        if (tick <= this.clock.tick) {
          this.controlCount -= commands.size;
          this.controls.delete(tick);
        }
      if (this.controlCount >= 4096) throw new Error("Too many pending commands for one simulation tick");
    }
    if (!pending.has(key)) this.controlCount++;
    pending.set(key, control);
    this.controls.set(control.tick, pending);
  }
  /** Attach release-authored semantic markers to an immutable compiled clip. */
  registerMotionEvents(definition: string, motionId: string, markers: MotionEventMarker[]) {
    const motion = [...this.instances.values()]
      .find((instance) => instance.artifact.id === definition)
      ?.artifact.motions.find((clip) => clip.id === motionId);
    if (!motion) throw new Error(`Unknown motion ${definition}/${motionId}`);
    const key = JSON.stringify([definition, motionId]);
    if (!this.motionEvents.has(key) && this.motionEvents.size >= 256)
      throw new Error("Motion event track budget exceeded");
    this.motionEvents.set(key, compileMotionEvents(motion, markers));
  }
  /** Draining explicitly reports overflow; callers can reject incomplete gameplay event streams. */
  drainAnimationEvents(): { events: RuntimeMotionEvent[]; dropped: number } {
    const result = { events: this.animationEvents.splice(0), dropped: this.droppedAnimationEvents };
    this.droppedAnimationEvents = 0;
    return result;
  }
  private clearAnimationEvents() {
    this.animationEvents = [];
    this.droppedAnimationEvents = 0;
  }
  private emitMotionEvents(instance: CharacterInstance, time: number, dt: number, tick: number) {
    const motion = instance.artifact.motions.find((clip) => clip.id === instance.motion);
    const markers = this.motionEvents.get(JSON.stringify([instance.artifact.id, instance.motion]));
    if (!motion || !markers) return;
    const from = time - dt - instance.motionStart;
    for (const occurrence of crossedMotionEvents(
      motion,
      markers,
      tick === 1 && instance.motionStart === 0 ? -1e-8 : from,
      time - instance.motionStart,
    )) {
      if (this.animationEvents.length >= 1024) {
        this.droppedAnimationEvents++;
        continue;
      }
      this.animationEvents.push({
        entityId: instance.id,
        definition: instance.artifact.id,
        motion: motion.id,
        eventId: occurrence.marker.id,
        tick,
        cycle: occurrence.cycle,
        time: instance.motionStart + occurrence.time,
        payload: structuredClone(occurrence.marker.payload ?? {}),
      });
    }
  }
  playMotion(id: string, motion: string, blendSeconds = 0.2) {
    if (!this.instances.get(id)?.artifact.motions.some((m) => m.id === motion))
      throw new Error(`Unknown motion ${motion}`);
    this.enqueue({ kind: "motion", id, motion, blend: Math.max(0, blendSeconds), tick: this.clock.tick + 1 });
  }
  setMode(id: string, mode: PhysicsMode) {
    this.enqueue({ kind: "mode", id, mode, tick: this.clock.tick + 1 });
  }
  /** Controller-driven actors keep root animation visual; authored previews default to physical. */
  setRootMotionPolicy(id: string, policy: RootMotionPolicy) {
    if (policy !== "physical" && policy !== "visual") throw new RangeError("Invalid root motion policy");
    this.enqueue({ kind: "rootMotionPolicy", id, policy, tick: this.clock.tick + 1 });
  }
  setFacing(id: string, yaw: number) {
    if (!Number.isFinite(yaw)) throw new RangeError("Facing must be finite");
    this.enqueue({ kind: "facing", id, yaw, tick: this.clock.tick + 1 });
  }
  setTarget(id: string, position: Vec3) {
    if (position.some((v) => !Number.isFinite(v))) throw new RangeError("Target must be finite");
    if (this.world) {
      const previous = this.bodyState(id).position,
        distance = Math.hypot(position[0] - previous[0], position[2] - previous[2]);
      if (distance > 64) throw new RangeError("Movement exceeds the bounded sweep; use a prepared teleport");
      const samples = Math.max(1, Math.ceil(distance / 0.5));
      for (let index = 0; index <= samples; index++) {
        const t = index / samples,
          x = previous[0] + (position[0] - previous[0]) * t,
          z = previous[2] + (position[2] - previous[2]) * t;
        if (this.world.queryGround(x, z).status !== "ready")
          throw new Error(
            "Movement crosses unavailable collision data; prepare the swept region before moving",
          );
      }
    }
    this.enqueue({ kind: "target", id, position: [...position], tick: this.clock.tick + 1 });
  }
  setPose(id: string, jointId: string, rotation: Vec3, translation: Vec3) {
    const instance = this.instances.get(id),
      joint = instance?.artifact.joints.find((j) => j.id === jointId);
    if (!joint) throw new Error(`Unknown joint ${jointId} on ${id}`);
    if ([...rotation, ...translation].some((v) => !Number.isFinite(v)))
      throw new RangeError("Pose must be finite");
    const overrides = this.poseOverrides.get(id) ?? new Map();
    overrides.set(jointId, {
      rotation: rotation.map((v) => Math.max(joint.minimum, Math.min(joint.maximum, v))) as Vec3,
      translation: [...translation],
    });
    this.poseOverrides.set(id, overrides);
  }
  clearPose(id?: string) {
    if (id) this.poseOverrides.delete(id);
    else this.poseOverrides.clear();
  }
  getPose(id: string, jointId: string) {
    const pose = this.poseOverrides.get(id)?.get(jointId);
    return pose ? structuredClone(pose) : { rotation: [0, 0, 0] as Vec3, translation: [0, 0, 0] as Vec3 };
  }
  private pose(instance: CharacterInstance, time: number): Pose {
    const motion = instance.artifact.motions.find((m) => m.id === instance.motion);
    const current = sampleMotion(
      instance.artifact.joints,
      motion,
      time - instance.motionStart,
      instance.rootMotionPolicy === "physical",
    );
    if (!instance.previousMotion || time >= instance.blendStart + instance.blendDuration) return current;
    return blendPoses(
      sampleMotion(
        instance.artifact.joints,
        instance.artifact.motions.find((m) => m.id === instance.previousMotion),
        time - instance.previousStart,
        instance.rootMotionPolicy === "physical",
      ),
      current,
      (time - instance.blendStart) / instance.blendDuration,
    );
  }
  private tick(dt: number, tick: number, emitEvents = true) {
    const time = tick * dt;
    this.replayEndTick = Math.max(this.replayEndTick, tick);
    for (const control of this.controls.get(tick)?.values() ?? []) {
      const instance = this.instances.get(control.id);
      if (!instance) throw new Error(`Unknown runtime input target ${control.id}`);
      if (control.kind === "mode") {
        const state = this.bodyState(instance.id);
        if (control.mode === "kinematic") instance.basePosition = [...state.position];
        if (instance.dormant) instance.dormant.mode = control.mode;
        else this.physics.setMode(instance.id, control.mode);
        instance.configuration.mode = control.mode;
        instance.mode = control.mode;
      } else if (control.kind === "target") instance.basePosition = [...control.position];
      else if (control.kind === "facing") instance.baseRotation = quatFromEuler([0, control.yaw, 0]);
      else if (control.kind === "rootMotionPolicy") instance.rootMotionPolicy = control.policy;
      else {
        instance.previousMotion = instance.motion;
        instance.previousStart = instance.motionStart;
        instance.motion = control.motion;
        instance.motionStart = time;
        instance.blendStart = time;
        instance.blendDuration = control.blend;
      }
    }
    for (const instance of this.instances.values()) {
      if (instance.dormant) continue;
      if (emitEvents && !this.world?.persistence.overrides.get(instance.id)?.removed)
        this.emitMotionEvents(instance, time, dt, tick);
      instance.previousPosition = [...instance.position];
      instance.previousRotation = [...instance.rotation];
      const pose = this.pose(instance, time),
        root =
          instance.rootMotionPolicy === "physical"
            ? instance.artifact.joints.find((j) => !j.parent)
            : undefined;
      const translation = rotateVector(
        instance.baseRotation,
        (root ? (pose.get(root.id)?.translation ?? [0, 0, 0]) : [0, 0, 0]).map(
          (v) => v * instance.scale,
        ) as Vec3,
      );
      this.physics.target(
        instance.id,
        instance.basePosition.map((v, i) => v + translation[i]) as Vec3,
        quatMultiply(
          instance.baseRotation,
          root ? (pose.get(root.id)?.rotation ?? quatIdentity()) : quatIdentity(),
        ),
        dt,
      );
    }
    if (this.water)
      for (const instance of this.instances.values()) {
        if (instance.dormant) continue;
        const state = this.bodyState(instance.id);
        const surface = queryWater(this.water, state.position[0], state.position[2], time);
        const extent = Math.max(
          0.2,
          ((instance.artifact.mesh.bounds.max[1] - instance.artifact.mesh.bounds.min[1]) / 2) *
            instance.scale,
        );
        this.physics.buoyancy(instance.id, surface.height, surface.velocity, extent, dt);
      }
    this.physics.step(dt);
    for (const instance of this.instances.values()) {
      const state = this.bodyState(instance.id);
      instance.position = state.position;
      instance.rotation = state.rotation;
    }
    if (tick % 120 === 0) {
      this.checkpoints.push(this.checkpoint());
      if (this.checkpoints.length > 16) this.checkpoints.splice(1, 1);
    }
  }
  advance(seconds: number) {
    if (this.paused) return 0;
    this.synchronizeLifecycle();
    this.syncWorld();
    this.syncOverrides();
    this.blockedReason = null;
    if (this.world) {
      for (const instance of this.instances.values())
        if (
          !instance.dormant &&
          !this.world.persistence.overrides.get(instance.id)?.removed &&
          (this.world.queryGround(instance.position[0], instance.position[2]).status !== "ready" ||
            (instance.mode !== "dynamic" &&
              this.world.queryGround(instance.basePosition[0], instance.basePosition[2]).status !== "ready"))
        ) {
          this.blockedReason = "Required collision region is not resident";
          return 0;
        }
    }
    return this.clock.advance(seconds, (dt, tick) => this.tick(dt, tick));
  }
  evaluatedCharacters(time = this.clock.time, origin: Vec3 = [0, 0, 0]) {
    return [...this.instances.values()]
      .filter((instance) => !this.world?.persistence.overrides.get(instance.id)?.removed)
      .map((instance) => {
        const pose = this.pose(instance, time),
          root = instance.artifact.joints.find((j) => !j.parent);
        if (root && instance.rootMotionPolicy === "physical")
          pose.set(root.id, { translation: [0, 0, 0], rotation: quatIdentity() });
        for (const [jointId, override] of this.poseOverrides.get(instance.id) ?? [])
          pose.set(jointId, {
            translation: override.translation,
            rotation: quatFromEuler(override.rotation),
          });
        const height =
          ((instance.artifact.mesh.bounds.min[1] + instance.artifact.mesh.bounds.max[1]) / 2) *
          instance.scale;
        const rotation = quatSlerp(instance.previousRotation, instance.rotation, this.clock.alpha);
        const centerOffset = rotateVector(rotation, [0, height, 0]);
        const position = instance.previousPosition.map(
          (v, i) => v + (instance.position[i] - v) * this.clock.alpha - origin[i] - centerOffset[i],
        ) as Vec3;
        const matrix = transformMatrix(position);
        const rotationMatrix = poseMatrices(
          [
            {
              id: "body",
              name: "body",
              parent: null,
              position: [0, 0, 0],
              rotation: [0, 0, 0],
              radius: 1,
              minimum: -Math.PI,
              maximum: Math.PI,
            },
          ],
          new Map([["body", { translation: [0, 0, 0], rotation }]]),
        );
        for (let i = 0; i < 12; i++) matrix[i] = rotationMatrix[i] * instance.scale;
        return {
          id: instance.id,
          artifact: instance.artifact,
          matrix,
          skinMatrices: poseMatrices(instance.artifact.joints, pose),
          mode: instance.mode,
          scale: instance.scale,
        };
      });
  }
  snapshotEntities(): DormantEntity[] {
    return [...this.instances.values()]
      .filter((instance) => !this.world?.persistence.overrides.get(instance.id)?.removed)
      .map((instance) => {
        const body = this.bodyState(instance.id);
        return {
          id: instance.id,
          definition: instance.artifact.id,
          position: [...body.position],
          velocity: [...body.velocity],
          state: {
            runtime: "wrela-character-1",
            active: !instance.dormant,
            rootMotionPolicy: instance.rootMotionPolicy,
            artifactKey: instance.artifact.key,
            tick: this.clock.tick,
            mode: body.mode,
            scale: instance.scale,
            qx: body.rotation[0],
            qy: body.rotation[1],
            qz: body.rotation[2],
            qw: body.rotation[3],
            wx: body.angularVelocity[0],
            wy: body.angularVelocity[1],
            wz: body.angularVelocity[2],
            baseX: instance.basePosition[0],
            baseY: instance.basePosition[1],
            baseZ: instance.basePosition[2],
            baseQx: instance.baseRotation[0],
            baseQy: instance.baseRotation[1],
            baseQz: instance.baseRotation[2],
            baseQw: instance.baseRotation[3],
            motion: instance.motion ?? "",
            motionStart: instance.motionStart,
            previousMotion: instance.previousMotion ?? "",
            previousStart: instance.previousStart,
            blendStart: instance.blendStart,
            blendDuration: instance.blendDuration,
          },
        };
      });
  }
  validateEntityStates(entities: DormantEntity[]) {
    const active = entities.filter((entity) => entity.state.runtime === "wrela-character-1");
    if (active.length > 256) throw new Error("Active character save budget exceeded");
    const decoded = active.map((entity) => {
      const instance = this.instances.get(entity.id);
      if (
        !instance ||
        instance.artifact.id !== entity.definition ||
        instance.artifact.key !== entity.state.artifactKey
      )
        throw new Error(`Saved character ${entity.id} is incompatible with the prepared world`);
      const number = (key: string) => {
        const value = entity.state[key];
        if (typeof value !== "number" || !Number.isFinite(value))
          throw new Error(`Invalid saved character field ${key}`);
        return value;
      };
      const rotation: Quat = [number("qx"), number("qy"), number("qz"), number("qw")],
        baseRotation: Quat = [number("baseQx"), number("baseQy"), number("baseQz"), number("baseQw")];
      if (Math.abs(Math.hypot(...rotation) - 1) > 0.001 || Math.abs(Math.hypot(...baseRotation) - 1) > 0.001)
        throw new Error("Saved character rotation must be normalized");
      const tick = number("tick");
      if (!Number.isSafeInteger(tick) || tick < 0 || number("scale") !== instance.scale)
        throw new Error("Saved character tick or scale is incompatible");
      if (entity.state.active !== undefined && typeof entity.state.active !== "boolean")
        throw new Error("Invalid saved entity lifecycle");
      const rootMotionPolicy = entity.state.rootMotionPolicy ?? "physical";
      if (rootMotionPolicy !== "physical" && rootMotionPolicy !== "visual")
        throw new Error("Invalid saved root motion policy");
      const mode = entity.state.mode;
      if (mode !== "kinematic" && mode !== "dynamic" && mode !== "motor")
        throw new Error("Invalid saved physics mode");
      const motion = (key: string) => {
        const value = entity.state[key];
        if (
          typeof value !== "string" ||
          (value && !instance.artifact.motions.some((motion) => motion.id === value))
        )
          throw new Error(`Invalid saved motion ${key}`);
        return value || undefined;
      };
      const body: BodyState = {
        position: [...entity.position],
        velocity: [...entity.velocity],
        rotation,
        angularVelocity: [number("wx"), number("wy"), number("wz")],
        mode,
      };
      const motionStart = number("motionStart"),
        previousStart = number("previousStart"),
        blendStart = number("blendStart"),
        blendDuration = number("blendDuration");
      if (Math.min(motionStart, previousStart, blendStart, blendDuration) < 0)
        throw new Error("Saved motion times must be nonnegative");
      return {
        instance,
        entity,
        body,
        active: entity.state.active !== false,
        rootMotionPolicy,
        tick,
        baseRotation,
        basePosition: [number("baseX"), number("baseY"), number("baseZ")] as Vec3,
        motion: motion("motion"),
        previousMotion: motion("previousMotion"),
        motionStart,
        previousStart,
        blendStart,
        blendDuration,
      };
    });
    if (new Set(decoded.map((value) => value.tick)).size > 1)
      throw new Error("Saved characters disagree on simulation tick");
    return decoded;
  }
  restoreEntityStates(entities: DormantEntity[]) {
    const decoded = this.validateEntityStates(entities);
    this.clearAnimationEvents();
    this.cancelSeek();
    this.seekEpoch++;
    this.seeking = false;
    this.seekBaseline = undefined;
    this.seekBaselineCheckpoints = [];
    this.syncWorld();
    if (decoded.length) this.rebasePhysicsNear(decoded[0].body.position);
    this.appliedOverrides.clear();
    this.physicalInterests.clear();
    this.syncOverrides();
    for (const value of decoded) {
      if (
        !value.active ||
        this.world?.queryGround(value.body.position[0], value.body.position[2]).status === "not-resident"
      ) {
        this.physics.remove(value.instance.id);
        value.instance.dormant = structuredClone(value.body);
      } else {
        if (value.instance.dormant)
          this.physics.add({ ...value.instance.configuration, position: value.body.position });
        value.instance.dormant = undefined;
        this.physics.restoreBodyState(value.instance.id, value.body);
      }
      Object.assign(value.instance, {
        position: [...value.body.position],
        previousPosition: [...value.body.position],
        rotation: [...value.body.rotation],
        previousRotation: [...value.body.rotation],
        mode: value.body.mode,
        rootMotionPolicy: value.rootMotionPolicy,
        basePosition: value.basePosition,
        baseRotation: value.baseRotation,
        motion: value.motion,
        previousMotion: value.previousMotion,
        motionStart: value.motionStart,
        previousStart: value.previousStart,
        blendStart: value.blendStart,
        blendDuration: value.blendDuration,
      });
    }
    if (decoded.length) this.clock.reset(decoded[0].tick);
    this.resetHistory();
    return this.clock.time;
  }
  checkpoint(): RuntimeCheckpoint {
    return {
      tick: this.clock.tick,
      physics: this.physics.checkpoint(),
      instances: [...this.instances.values()].map((i) => ({
        ...i,
        basePosition: [...i.basePosition],
        position: [...i.position],
        previousPosition: [...i.previousPosition],
        dormant: i.dormant ? structuredClone(i.dormant) : undefined,
        configuration: structuredClone(i.configuration),
      })),
      worldRevision: this.worldRevision,
      collisionKey: this.collisionKey,
    };
  }
  private seekCheckpoint(tick: number) {
    if (!this.seeking) this.syncWorld();
    if (!Number.isInteger(tick) || tick < 0) throw new RangeError("Invalid seek tick");
    const checkpoint = [...this.checkpoints]
      .reverse()
      .find((c) => c.tick <= tick && c.collisionKey === this.collisionKey);
    if (!checkpoint)
      throw new RangeError(
        `Requested tick ${tick} predates the collision replay boundary; accessible replay starts at tick ${this.earliestSeekTick} (${(this.earliestSeekTick / 60).toFixed(3)} seconds). Reset runtime to start again from authored state.`,
      );
    if (tick - checkpoint.tick > 7200) throw new RangeError("Seek exceeds bounded replay work");
    return checkpoint;
  }
  private restoreCheckpoint(checkpoint: RuntimeCheckpoint) {
    this.physics.restore(checkpoint.physics);
    this.appliedOverrides.clear();
    this.instances = new Map(
      checkpoint.instances.map((instance) => [
        instance.id,
        {
          ...instance,
          configuration: structuredClone(instance.configuration),
          dormant: instance.dormant ? structuredClone(instance.dormant) : undefined,
        },
      ]),
    );
    this.clock.reset(checkpoint.tick);
  }
  seek(tick: number) {
    const checkpoint = this.seekCheckpoint(tick);
    this.clearAnimationEvents();
    this.seekEpoch++;
    if (this.seeking) {
      this.seeking = false;
      this.paused = this.seekPauseBase;
      this.seekBaseline = undefined;
    }
    this.restoreCheckpoint(checkpoint);
    while (this.clock.tick < tick)
      this.clock.advance(this.clock.stepSeconds, (dt, next) => this.tick(dt, next, false));
  }
  cancelSeek() {
    if (!this.seeking) return;
    this.seekEpoch++;
    if (this.seekBaseline) {
      this.restoreCheckpoint(this.seekBaseline);
      this.checkpoints = this.seekBaselineCheckpoints;
    }
    this.seeking = false;
    this.paused = this.seekPauseBase;
    this.seekBaseline = undefined;
    this.seekBaselineCheckpoints = [];
  }
  async seekAsync(
    tick: number,
    options: { onProgress?: (fraction: number) => void; signal?: AbortSignal } = {},
  ) {
    const checkpoint = this.seekCheckpoint(tick);
    if (options.signal?.aborted) throw new DOMException("Seek cancelled", "AbortError");
    const epoch = ++this.seekEpoch;
    this.clearAnimationEvents();
    if (!this.seeking) {
      this.seekPauseBase = this.paused;
      this.seekBaseline = this.checkpoint();
      this.seekBaselineCheckpoints = [...this.checkpoints];
    }
    this.seeking = true;
    this.paused = true;
    const start = checkpoint.tick;
    try {
      this.restoreCheckpoint(checkpoint);
      options.onProgress?.(0);
      while (this.clock.tick < tick) {
        if (epoch !== this.seekEpoch || options.signal?.aborted)
          throw new DOMException("Seek superseded or cancelled", "AbortError");
        if (this.currentCollisionKey() !== this.collisionKey)
          throw new Error("Collision geometry changed during replay; retry after world preparation");
        for (let step = 0; step < 120 && this.clock.tick < tick; step++)
          this.clock.advance(this.clock.stepSeconds, (dt, next) => this.tick(dt, next, false));
        options.onProgress?.((this.clock.tick - start) / Math.max(1, tick - start));
        if (this.clock.tick < tick) await new Promise<void>((resolve) => setTimeout(resolve, 0));
      }
      if (epoch !== this.seekEpoch || options.signal?.aborted)
        throw new DOMException("Seek superseded or cancelled", "AbortError");
      options.onProgress?.(1);
    } catch (error) {
      if (epoch === this.seekEpoch && this.seekBaseline) {
        this.restoreCheckpoint(this.seekBaseline);
        this.checkpoints = this.seekBaselineCheckpoints;
      }
      throw error;
    } finally {
      if (epoch === this.seekEpoch) {
        this.seeking = false;
        this.paused = this.seekPauseBase;
        this.seekBaseline = undefined;
        this.seekBaselineCheckpoints = [];
      }
    }
  }
  async teleport(id: string, position: Vec3) {
    const instance = this.instances.get(id);
    if (!instance) throw new Error(`Unknown character ${id}`);
    worldPosition(position);
    this.cancelSeek();
    if (this.world) {
      const ready = await this.world.teleport(position);
      if (!ready.ready) return ready;
      this.syncWorld();
    }
    this.rebasePhysicsNear(position);
    if (instance.dormant) {
      this.physics.add({ ...instance.configuration, position });
      this.physics.restoreBodyState(id, { ...instance.dormant, position });
      instance.dormant = undefined;
    }
    this.physics.teleport(id, position);
    instance.basePosition = [...position];
    instance.position = [...position];
    instance.previousPosition = [...position];
    this.resetHistory();
    return { ready: true, missing: [] };
  }
  private rebasePhysicsNear(position: Vec3) {
    if (
      position.every((value, axis) => Math.abs(value - this.physics.coordinateOrigin[axis]) <= CELL_SIZE * 2)
    )
      return;
    this.physics.rebase(worldPosition(position).cell.map((cell) => cell * CELL_SIZE) as Vec3);
    // Reinstall from absolute double-precision patch origins: shifting a collider
    // previously stored far away in Rapier would preserve its float rounding.
    if (this.world) {
      this.physics.installTerrain(this.world.collisionPatches().filter((patch) => patch.collision));
      this.appliedOverrides.clear();
      this.syncOverrides();
    }
  }
  dispose() {
    this.seekEpoch++;
    this.seeking = false;
    this.seekBaseline = undefined;
    this.seekBaselineCheckpoints = [];
    for (const id of this.physicalInterests.keys()) this.world?.removeInterest(`runtime:${id}`);
    this.physicalInterests.clear();
    this.physics.dispose();
    this.instances.clear();
    this.checkpoints = [];
    this.controls.clear();
    this.controlCount = 0;
    this.replayEndTick = this.clock.tick;
    this.poseOverrides.clear();
    this.motionEvents.clear();
    this.clearAnimationEvents();
  }
}
