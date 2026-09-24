import { queryWater } from "@wrela/compiler";
import type { AssemblyDefinition, MeshData, ObjectDefinition } from "@wrela/model";
import {
  type CharacterDefinition,
  type CompiledCharacter,
  type CreatureDetail,
  contentKey,
  type PersistentWaterState,
  persistentWaterStateSchema,
  type Quat,
  type RenderSurface,
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
} from "./animation";
import { AssemblyMotion, assemblyWorldMatrix } from "./assembly-motion";
import { decodeAssemblyState, encodeAssemblyJoints } from "./assembly-state";
import { FixedClock } from "./clock";
import { installObjectCollision } from "./collision-realization";
import {
  CreatureArticulationController,
  type CreatureArticulationSnapshot,
  resolveCreatureArticulationSnapshot,
  validateCreatureArticulationSnapshot,
} from "./creature-articulation";
import {
  type CreatureClothState,
  createCreatureClothState,
  creatureClothDiagnostics,
  creatureClothOffsets,
  stepCreatureCloth,
  validateCreatureClothState,
} from "./creature-cloth";
import {
  type CreatureDeformationCache,
  evaluateCreatureDeformation,
  mergeCreatureDeformation,
} from "./creature-deformation";
import {
  type CreatureGroomState,
  createCreatureGroomState,
  creatureGroomOffsets,
  stepCreatureGroom,
  validateCreatureGroomState,
} from "./creature-groom";
import {
  type CreatureRuntimeContext,
  type CreatureRuntimeState,
  type CreatureSolveDiagnostic,
  createCreatureRuntimeState,
  evaluateCreaturePose,
  validateCreatureRuntimeState,
} from "./creature-runtime";
import { planEntityResidency } from "./lifecycle";
import {
  compileMotionEvents,
  crossedMotionEvents,
  type MotionEventMarker,
  type RuntimeMotionEvent,
} from "./motion-events";
import { performanceMotionEvents, resolvePerformanceBlend, sampleCharacterPerformance } from "./performance";
import {
  type BodyConfiguration,
  type BodyState,
  PhysicsAdapter,
  type PhysicsCheckpoint,
  type PhysicsMode,
} from "./physics";
import { WaterBodyRuntime } from "./water-body";
import type { WaterSimulationSnapshot } from "./water-simulation";
import { WaterSimulation } from "./water-simulation";

export type RootMotionPolicy = "physical" | "visual";
export type CreatureSecondaryGroundPlane = {
  height: number;
  minX: number;
  maxX: number;
  minZ: number;
  maxZ: number;
};

type CharacterInstance = {
  id: string;
  artifact: CompiledCharacter;
  basePosition: Vec3;
  baseRotation: Quat;
  scale: number;
  rootMotionPolicy: RootMotionPolicy;
  locomotionSpeed?: number;
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
  creatureState?: CreatureRuntimeState;
  groomState?: CreatureGroomState;
  clothState?: CreatureClothState;
  articulationState?: CreatureArticulationSnapshot;
};
type Control = { tick: number; id: string } & (
  | { kind: "mode"; mode: PhysicsMode }
  | { kind: "motion"; motion: string; blend: number }
  | { kind: "target"; position: Vec3 }
  | { kind: "facing"; yaw: number }
  | { kind: "rootMotionPolicy"; policy: RootMotionPolicy }
  | { kind: "locomotionSpeed"; speed: number }
  | { kind: "expression"; expression: string; weight: number }
  | { kind: "ragdoll"; velocity?: Vec3 }
  | { kind: "articulationImpulse"; joint: string; impulse: Vec3; torque: Vec3 }
  | { kind: "recover"; duration: number }
  | { kind: "assemblyJoint"; joint: string; value: number | null }
  | { kind: "waterImpulse"; x: number; z: number; radius: number; strength: number }
);
export type RuntimeCheckpoint = {
  tick: number;
  physics: PhysicsCheckpoint;
  instances: CharacterInstance[];
  worldRevision: number;
  collisionKey: string;
  secondaryGroundPlane?: CreatureSecondaryGroundPlane;
  assemblyJoints: Record<string, Record<string, number>>;
  waters?: Record<string, WaterSimulationSnapshot>;
};
export class RuntimeSession {
  private staticObjects = new Map<string, { definition: ObjectDefinition; mesh: MeshData; scale: number }>();
  private assemblies = new Map<
    string,
    { motion: AssemblyMotion; matrix: Float32Array; collision: boolean; enabled: boolean; definition: string }
  >();
  /** Authored drives use the authoritative clock and participate in physics replay. */
  addAssembly(
    id: string,
    source: AssemblyDefinition,
    mesh: MeshData,
    matrix: Float32Array,
    collision: boolean,
    definition = id,
  ) {
    if (this.assemblies.has(id)) throw new Error(`Duplicate assembly ${id}`);
    const motion = new AssemblyMotion(source, mesh);
    if (collision) motion.installCollision(this.physics, id, matrix, this.clock.time);
    this.assemblies.set(id, { motion, matrix, collision, enabled: collision, definition });
    this.resetHistory();
  }
  addStaticObject(
    id: string,
    definition: ObjectDefinition,
    mesh: MeshData,
    position: Vec3,
    rotation: Quat,
    scale = 1,
  ): void {
    installObjectCollision(this.physics, id, definition, mesh, position, rotation, scale);
    this.staticObjects.set(id, { definition, mesh, scale });
  }
  assemblyParts(id: string, matrix: Float32Array) {
    return this.assemblies.get(id)?.motion.evaluate(this.clock.time, matrix);
  }
  /** Recompiling an unrelated document preserves operated joints, while changed
   * assemblies deliberately start from their newly authored rest state. */
  retainCompatibleAssemblyJoints(entities: readonly DormantEntity[]) {
    for (const entity of entities) {
      if (entity.state.runtime !== "wrela-assembly-1") continue;
      const assembly = this.assemblies.get(entity.id);
      if (
        !assembly ||
        entity.definition !== assembly.definition ||
        entity.state.sourceKey !== contentKey(assembly.motion.source)
      )
        continue;
      const { commands } = decodeAssemblyState(entity, assembly.motion.source, assembly.definition);
      assembly.motion.restoreJoints(commands);
      if (assembly.enabled)
        assembly.motion.teleportCollision(this.physics, entity.id, assembly.matrix, this.clock.time);
    }
    this.resetHistory();
  }
  /** Machinery commands apply on the next fixed tick and branch/replay like character input. */
  setAssemblyJoint(id: string, joint: string, value: number | null): void {
    const part = this.assemblies.get(id)?.motion.source.parts.find((part) => part.id === joint);
    if (!part?.joint) throw new Error(`Unknown assembly joint ${id}/${joint}`);
    if (value !== null && !Number.isFinite(value)) throw new Error("Joint command must be finite");
    this.enqueue({ kind: "assemblyJoint", id, joint, value, tick: this.clock.tick + 1 });
  }
  get assemblyBytes(): number {
    return [...this.assemblies.values()].reduce((sum, assembly) => sum + assembly.motion.byteLength, 0);
  }

  readonly clock = new FixedClock();
  private instances = new Map<string, CharacterInstance>();
  private controls = new Map<number, Map<string, Control>>();
  private controlCount = 0;
  private replayEndTick = 0;
  private checkpoints: RuntimeCheckpoint[] = [];
  private world?: WorldSession;
  private waterSources: { definition: WaterDefinition; evaluate?: (seconds: number) => WaterDefinition }[] =
    [];
  readonly waterBodies = new Map<string, WaterBodyRuntime>();
  private secondaryGroundPlane?: CreatureSecondaryGroundPlane;
  private motionEvents = new Map<string, readonly MotionEventMarker[]>();
  private animationEvents: RuntimeMotionEvent[] = [];
  private droppedAnimationEvents = 0;
  private focusId?: string;
  private physicalInterests = new Map<string, Vec3>();
  private creaturePoses = new Map<
    string,
    { time: number; pose: Pose; diagnostics: CreatureSolveDiagnostic[] }
  >();
  private articulations = new Map<string, CreatureArticulationController>();
  private creatureDetailViews = new WeakMap<CompiledCharacter, Map<string, CompiledCharacter>>();
  private creatureCombinedDeformations = new Map<
    string,
    { key: string; deformation: RenderSurface["deformation"] }
  >();
  private creatureDeformations = new Map<string, CreatureDeformationCache>();
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
      creatureState: artifact.creature ? createCreatureRuntimeState() : undefined,
      groomState: artifact.creatureGroom?.guides.length ? createCreatureGroomState() : undefined,
      clothState: artifact.creature?.cloth.length ? createCreatureClothState() : undefined,
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
    this.ensureArticulation(this.instances.get(id)!);
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
    this.articulations.get(id)?.dispose();
    this.articulations.delete(id);
    this.physics.remove(id);
    this.instances.delete(id);
    this.creaturePoses.delete(id);
    this.creatureDeformations.delete(id);
    this.creatureCombinedDeformations.delete(id);
    this.world?.removeInterest(`runtime:${id}`);
    this.physicalInterests.delete(id);
    if (this.focusId === id) this.focusId = this.instances.keys().next().value;
    this.resetHistory();
  }
  setWater(water?: WaterDefinition, evaluate?: (seconds: number) => WaterDefinition) {
    this.setWaters(water ? [{ definition: water, evaluate }] : []);
  }
  setWaters(sources: { definition: WaterDefinition; evaluate?: (seconds: number) => WaterDefinition }[]) {
    if (sources.length > 8 || new Set(sources.map((source) => source.definition.id)).size !== sources.length)
      throw new Error("A runtime supports at most eight distinct water bodies");
    const old = this.waterBodies,
      next = new Map<string, WaterBodyRuntime>();
    for (const { definition } of sources) {
      if (!definition.domain && !definition.spectrum) continue;
      const prior = old.get(definition.id);
      const compatible =
        prior &&
        contentKey(prior.definition.domain) === contentKey(definition.domain) &&
        contentKey(prior.definition.flow) === contentKey(definition.flow) &&
        prior.definition.level === definition.level;
      next.set(definition.id, compatible ? prior : new WaterBodyRuntime(definition));
    }
    let changed = false;
    for (const [id, body] of old)
      if (next.get(id) !== body) {
        this.physics.removeStaticObject(`water-bed:${id}`);
        old.delete(id);
        changed = true;
      }
    for (const [id, body] of next)
      if (old.get(id) !== body) {
        old.set(id, body);
        if (body.domain)
          this.physics.addStaticMesh(`water-bed:${id}`, body.domain.bed, [
            body.domain.min[0],
            0,
            body.domain.min[1],
          ]);
        changed = true;
      }
    this.waterSources = sources;
    if (changed) this.resetHistory();
  }
  disturbWater(id: string, x: number, z: number, radius = 1, strength = 1.5) {
    if (
      ![x, z, radius, strength].every(Number.isFinite) ||
      radius <= 0 ||
      radius > 20 ||
      Math.abs(strength) > 5
    )
      throw new RangeError("Invalid water disturbance");
    if (!this.waterBodies.get(id)?.definition.domain?.simulate)
      throw new Error("This water has no active local fluid simulation");
    this.enqueue({ kind: "waterImpulse", tick: this.clock.tick + 1, id, x, z, radius, strength });
  }
  /** Exact specialization for a known authored terrain plane. This applies only
   * to cloth/groom: foot contacts retain their full collision support query.
   * The caller must exclude unknown/static support geometry; outside the stated
   * rectangle, secondary queries fall back to the general collision world.
   */
  setCreatureSecondaryGroundPlane(plane?: CreatureSecondaryGroundPlane) {
    if (
      plane &&
      (this.world ||
        [plane.height, plane.minX, plane.maxX, plane.minZ, plane.maxZ].some(
          (value) => !Number.isFinite(value),
        ) ||
        plane.minX >= plane.maxX ||
        plane.minZ >= plane.maxZ)
    )
      throw new RangeError("Secondary ground plane requires finite bounds and a standalone authored stage");
    this.secondaryGroundPlane = plane ? { ...plane } : undefined;
    for (const instance of this.instances.values()) this.resetCreatureDynamics(instance);
    this.resetHistory();
  }
  get creatureGroundPolicy() {
    return {
      contacts: this.world ? ("resident-heightfield" as const) : ("physics-ray" as const),
      secondary: this.secondaryGroundPlane
        ? { mode: "authored-plane" as const, ...this.secondaryGroundPlane, fallback: "physics-ray" as const }
        : { mode: "general" as const },
    };
  }
  private secondaryContext(context: CreatureRuntimeContext): CreatureRuntimeContext {
    const plane = this.secondaryGroundPlane;
    if (!plane) return context;
    return {
      ...context,
      ground: (x, z) =>
        x >= plane.minX && x <= plane.maxX && z >= plane.minZ && z <= plane.maxZ
          ? { height: plane.height, normal: [0, 1, 0] }
          : context.ground?.(x, z),
    };
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
          this.suspendArticulation(instance);
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
        this.resetCreatureDynamics(instance);
        this.ensureArticulation(instance);
      }
    }
    if (changed) this.world.update();
  }
  attachWorld(world: WorldSession) {
    this.secondaryGroundPlane = undefined;
    this.creaturePoses.clear();
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
    this.creaturePoses.clear();
    this.checkpoints = [this.checkpoint()];
  }
  private syncOverrides() {
    if (!this.world) return;
    for (const instance of this.instances.values()) {
      const override = this.world.persistence.overrides.get(instance.id),
        key = contentKey(override ?? null);
      if (this.appliedOverrides.get(instance.id) === key) continue;
      if (override?.removed) this.suspendArticulation(instance);
      else this.ensureArticulation(instance);
      if (!instance.dormant)
        this.physics.setEnabled(
          instance.id,
          !override?.removed &&
            (this.articulations.get(instance.id)?.authority ?? "animation") === "animation",
        );
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
          this.resetCreatureDynamics(instance);
          this.resetArticulation(instance);
        }
      }
      this.appliedOverrides.set(instance.id, key);
    }
    for (const instance of this.world.world.instances) {
      if (this.instances.has(instance.id)) continue;
      const override = this.world.persistence.overrides.get(instance.id),
        key = contentKey(override ?? null);
      if (this.appliedOverrides.get(instance.id) === key) continue;
      const assembly = this.assemblies.get(instance.id);
      if (assembly) {
        const matrix = assemblyWorldMatrix(
          override?.position ?? instance.position,
          override?.rotation ?? instance.rotation,
          override?.scale ?? instance.scale,
        );
        assembly.matrix = matrix;
        const enabled = assembly.collision && !override?.removed;
        if (assembly.enabled && enabled)
          assembly.motion.teleportCollision(this.physics, instance.id, matrix, this.clock.time);
        else assembly.motion.setCollisionEnabled(this.physics, instance.id, enabled, matrix, this.clock.time);
        assembly.enabled = enabled;
        this.appliedOverrides.set(instance.id, key);
        continue;
      }
      const staticObject = this.staticObjects.get(instance.id);
      const scale = override?.scale ?? instance.scale;
      if (staticObject && staticObject.scale !== scale) {
        this.physics.removeStaticObject(instance.id);
        this.addStaticObject(
          instance.id,
          staticObject.definition,
          staticObject.mesh,
          override?.position ?? instance.position,
          quatFromEuler(override?.rotation ?? instance.rotation),
          scale,
        );
      }
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
    if (
      control.kind === "waterImpulse"
        ? !this.waterBodies.has(control.id)
        : control.kind === "assemblyJoint"
          ? !this.assemblies.has(control.id)
          : !this.instances.has(control.id)
    )
      throw new Error(`Unknown runtime input target ${control.id}`);
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
    const key = `${control.id}:${control.kind}${control.kind === "expression" ? `:${control.expression}` : control.kind === "articulationImpulse" || control.kind === "assemblyJoint" ? `:${control.joint}` : ""}`;
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
    const markers =
      this.motionEvents.get(JSON.stringify([instance.artifact.id, instance.motion])) ??
      performanceMotionEvents(instance.artifact.performance, instance.motion);
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
  playMotion(id: string, motion: string, blendSeconds?: number) {
    if (
      blendSeconds !== undefined &&
      (!Number.isFinite(blendSeconds) || blendSeconds < 0 || blendSeconds > 10)
    )
      throw new RangeError("Motion blend must be between 0 and 10 seconds");
    const instance = this.instances.get(id);
    if (!instance?.artifact.motions.some((m) => m.id === motion)) throw new Error(`Unknown motion ${motion}`);
    // Explicit blends are controller/editor overrides; omission uses the authored transition.
    const blend =
      blendSeconds ?? resolvePerformanceBlend(instance.artifact.performance, instance.motion, motion, 0.2);
    this.enqueue({ kind: "motion", id, motion, blend: Math.max(0, blend), tick: this.clock.tick + 1 });
  }
  /** Controller input is tick-recorded, replayable and independent of the authored preview speed. */
  setLocomotionSpeed(id: string, speed: number) {
    if (!Number.isFinite(speed) || speed < 0 || speed > 50)
      throw new RangeError("Locomotion speed must be between 0 and 50 metres per second");
    const instance = this.instances.get(id);
    if (!instance) throw new Error(`Unknown character ${id}`);
    if (!instance.artifact.performance?.locomotion)
      throw new Error(`Character ${id} has no authored locomotion blend`);
    this.enqueue({ kind: "locomotionSpeed", id, speed, tick: this.clock.tick + 1 });
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
    this.creaturePoses.delete(id);
  }
  clearPose(id?: string) {
    if (id) this.creaturePoses.delete(id);
    else this.creaturePoses.clear();
    if (id) this.poseOverrides.delete(id);
    else this.poseOverrides.clear();
  }
  getPose(id: string, jointId: string) {
    const pose = this.poseOverrides.get(id)?.get(jointId);
    return pose ? structuredClone(pose) : { rotation: [0, 0, 0] as Vec3, translation: [0, 0, 0] as Vec3 };
  }
  /** Expression changes participate in fixed-tick recording and replay. */
  setCreatureExpression(id: string, expression: string, weight: number) {
    const source = this.instances.get(id)?.artifact.creature;
    if (!source?.expressions.some((value) => value.id === expression))
      throw new Error(`Unknown creature expression ${expression} on ${id}`);
    if (!Number.isFinite(weight) || weight < 0 || weight > 1)
      throw new RangeError("Expression weight must be between zero and one");
    this.enqueue({ kind: "expression", id, expression, weight, tick: this.clock.tick + 1 });
  }
  /** Physical authority is explicit, replayable, and shared with the collision world. */
  enterCreatureRagdoll(id: string, velocity?: Vec3) {
    const instance = this.instances.get(id);
    if (!instance?.artifact.creature?.articulation) throw new Error(`No articulation on ${id}`);
    if (velocity?.some((value) => !Number.isFinite(value)))
      throw new RangeError("Ragdoll velocity must be finite");
    this.enqueue({
      kind: "ragdoll",
      id,
      velocity: velocity ? [...velocity] : undefined,
      tick: this.clock.tick + 1,
    });
  }
  applyCreatureImpulse(id: string, joint: string, impulse: Vec3, torque: Vec3 = [0, 0, 0]) {
    const instance = this.instances.get(id);
    if (!instance?.artifact.creature?.articulation?.bodies.some((body) => body.joint === joint))
      throw new Error(`No articulated joint ${joint} on ${id}`);
    if ([...impulse, ...torque].some((value) => !Number.isFinite(value)))
      throw new RangeError("Creature impulse must be finite");
    this.enqueue({
      kind: "articulationImpulse",
      id,
      joint,
      impulse: [...impulse],
      torque: [...torque],
      tick: this.clock.tick + 1,
    });
  }
  recoverCreature(id: string, duration = 0.75) {
    if (!this.instances.get(id)?.artifact.creature?.articulation) throw new Error(`No articulation on ${id}`);
    if (!Number.isFinite(duration) || duration < 0 || duration > 10)
      throw new RangeError("Invalid creature recovery duration");
    this.enqueue({ kind: "recover", id, duration, tick: this.clock.tick + 1 });
  }
  creatureAuthority(id: string) {
    const instance = this.instances.get(id);
    if (!instance) throw new Error(`Unknown character ${id}`);
    return this.articulations.get(id)?.authority ?? instance.articulationState?.mode ?? "animation";
  }
  private evaluateCreatureInstance(instance: CharacterInstance, time: number, dt = 0) {
    const cached = this.creaturePoses.get(instance.id);
    if (dt === 0 && cached?.time === time) return cached;
    const remember = (value: { pose: Pose; diagnostics: CreatureSolveDiagnostic[] }) => {
      const result = { ...value, time };
      this.creaturePoses.set(instance.id, result);
      return result;
    };
    const source = instance.artifact.creature,
      state = instance.creatureState,
      context = this.creatureContext(instance, time);
    const pose = this.authoredCreaturePose(instance, time),
      controller = this.articulations.get(instance.id);
    if (!source || !state) return remember({ pose, diagnostics: [] });
    if (!controller && !instance.articulationState)
      return remember(evaluateCreaturePose(instance.artifact.joints, source, pose, state, context, dt));
    const authority = controller?.authority ?? instance.articulationState?.mode ?? "animation";
    const authored = evaluateCreaturePose(instance.artifact.joints, source, pose, state, context, dt, {
      stages: authority === "ragdoll" ? ["expression"] : ["expression", "ik", "contacts"],
    });
    const physical = controller
      ? controller.resolvePose(authored.pose, context)
      : source.articulation && instance.articulationState
        ? resolveCreatureArticulationSnapshot(
            instance.artifact.joints,
            source.articulation,
            instance.articulationState,
            authored.pose,
            context,
          )
        : authored.pose;
    const secondary = evaluateCreaturePose(instance.artifact.joints, source, physical, state, context, dt, {
      stages: ["secondary"],
      ownedJoints: authority === "animation" ? [] : source.articulation?.bodies.map((body) => body.joint),
    });
    return remember({
      pose: secondary.pose,
      diagnostics: [...authored.diagnostics, ...secondary.diagnostics],
    });
  }
  private proceduralCreaturePose(instance: CharacterInstance, time: number): Pose {
    const pose = this.authoredCreaturePose(instance, time);
    return instance.artifact.creature && instance.creatureState
      ? evaluateCreaturePose(
          instance.artifact.joints,
          instance.artifact.creature,
          pose,
          instance.creatureState,
          this.creatureContext(instance, time),
          0,
          { stages: ["expression", "ik", "contacts"] },
        ).pose
      : pose;
  }
  private ensureArticulation(instance: CharacterInstance) {
    const definition = instance.artifact.creature?.articulation;
    if (
      !definition ||
      instance.dormant ||
      this.world?.persistence.overrides.get(instance.id)?.removed ||
      this.articulations.has(instance.id)
    )
      return;
    const context = this.creatureContext(instance, this.clock.time),
      pose = this.proceduralCreaturePose(instance, this.clock.time);
    const controller = new CreatureArticulationController(
      this.physics,
      instance.id,
      instance.artifact.joints,
      definition,
      context,
      pose,
      instance.articulationState,
    );
    instance.articulationState = undefined;
    this.articulations.set(instance.id, controller);
  }
  private suspendArticulation(instance: CharacterInstance) {
    const controller = this.articulations.get(instance.id);
    if (!controller) return;
    instance.articulationState = controller.snapshot();
    controller.dispose();
    this.articulations.delete(instance.id);
  }
  private resetArticulation(instance: CharacterInstance) {
    instance.articulationState = undefined;
    this.articulations
      .get(instance.id)
      ?.reset(
        this.proceduralCreaturePose(instance, this.clock.time),
        this.creatureContext(instance, this.clock.time),
      );
  }
  private placeCreatureRecovery(instance: CharacterInstance) {
    const context = this.creatureContext(instance, this.clock.time);
    const ground = context.ground?.(instance.position[0], instance.position[2]);
    if (!ground) return;
    const bounds = instance.artifact.mesh.bounds;
    const offset = rotateVector(instance.rotation, [
      0,
      ((bounds.min[1] + bounds.max[1]) / 2) * instance.scale,
      0,
    ]);
    const height = ground.height - bounds.min[1] * instance.scale + offset[1];
    const delta = Math.max(0, height - instance.position[1]);
    instance.position[1] += delta;
    instance.basePosition[1] += delta;
    this.physics.teleport(instance.id, instance.position);
    this.creaturePoses.delete(instance.id);
  }
  private resetCreatureDynamics(instance: CharacterInstance) {
    this.creatureCombinedDeformations.delete(instance.id);
    this.creaturePoses.delete(instance.id);
    if (!instance.artifact.creature) return;
    const expressions = instance.creatureState?.expressions ?? {};
    instance.creatureState = { ...createCreatureRuntimeState(), expressions: { ...expressions } };
    if (instance.groomState) instance.groomState = createCreatureGroomState();
    if (instance.clothState) instance.clothState = createCreatureClothState();
  }
  private creatureContext(instance: CharacterInstance, time: number): CreatureRuntimeContext {
    const height =
      ((instance.artifact.mesh.bounds.min[1] + instance.artifact.mesh.bounds.max[1]) / 2) * instance.scale;
    const offset = rotateVector(instance.rotation, [0, height, 0]);
    const colliders: NonNullable<CreatureRuntimeContext["colliders"]> = [];
    const capsules: NonNullable<CreatureRuntimeContext["capsules"]> = [];
    for (const collider of instance.configuration.colliders ?? []) {
      const moved = rotateVector(instance.rotation, collider.position);
      const center = instance.position.map((v, i) => v + moved[i]) as Vec3;
      if (collider.shape === "sphere") colliders.push({ center, radius: collider.radius });
      if (collider.shape === "capsule") {
        const direction = rotateVector(quatMultiply(instance.rotation, quatFromEuler(collider.rotation)), [
          0,
          collider.halfHeight,
          0,
        ]);
        capsules.push({
          a: center.map((v, i) => v - direction[i]) as Vec3,
          b: center.map((v, i) => v + direction[i]) as Vec3,
          radius: collider.radius,
        });
      }
    }
    const motion = instance.artifact.motions.find((motion) => motion.id === instance.motion);
    const cycle = motion?.loop ? Math.max(0, Math.floor((time - instance.motionStart) / motion.duration)) : 0;
    let cycleTranslation: Vec3 = [0, 0, 0];
    if (cycle > 0 && motion && instance.rootMotionPolicy === "physical") {
      const root = instance.artifact.joints.find((joint) => !joint.parent);
      if (root) {
        const atCycle = sampleCharacterPerformance(
          instance.artifact.joints,
          instance.artifact.motions,
          instance.artifact.performance,
          motion.id,
          cycle * motion.duration,
          true,
          instance.locomotionSpeed,
        ).get(root.id);
        const atStart = sampleCharacterPerformance(
          instance.artifact.joints,
          instance.artifact.motions,
          instance.artifact.performance,
          motion.id,
          0,
          true,
          instance.locomotionSpeed,
        ).get(root.id);
        if (atCycle && atStart)
          cycleTranslation = rotateVector(
            instance.baseRotation,
            atCycle.translation.map(
              (value, axis) => (value - atStart.translation[axis]) * instance.scale,
            ) as Vec3,
          );
      }
    }
    return {
      position: instance.position.map((v, i) => v - offset[i]) as Vec3,
      rotation: instance.rotation,
      scale: instance.scale,
      time,
      motion,
      motionTime: time - instance.motionStart,
      contactFrame:
        instance.rootMotionPolicy === "physical"
          ? {
              position: instance.basePosition.map(
                (v, i) => v + cycleTranslation[i] - rotateVector(instance.baseRotation, [0, height, 0])[i],
              ) as Vec3,
              rotation: instance.baseRotation,
            }
          : undefined,
      colliders,
      capsules,
      ground: (x, z) => {
        if (this.world) {
          const result = this.world.queryGround(x, z);
          return result.status === "ready" ? { height: result.height, normal: result.normal } : undefined;
        }
        const hit = this.physics.raycast([x, instance.position[1] + 100, z], [0, -1, 0], 200, instance.id);
        return hit ? { height: hit.point[1], normal: hit.normal } : undefined;
      },
    };
  }
  private authoredCreaturePose(instance: CharacterInstance, time: number): Pose {
    const pose = this.pose(instance, time),
      root = instance.artifact.joints.find((joint) => !joint.parent);
    if (root && instance.rootMotionPolicy === "physical")
      pose.set(root.id, { translation: [0, 0, 0], rotation: quatIdentity() });
    for (const [jointId, override] of this.poseOverrides.get(instance.id) ?? [])
      pose.set(jointId, { translation: override.translation, rotation: quatFromEuler(override.rotation) });
    return pose;
  }
  creatureDiagnostics(id: string): CreatureSolveDiagnostic[] {
    const instance = this.instances.get(id);
    if (!instance) throw new Error(`Unknown character ${id}`);
    if (!instance.artifact.creature || !instance.creatureState) return [];
    const evaluated = this.evaluateCreatureInstance(instance, this.clock.time);
    const diagnostics: CreatureSolveDiagnostic[] = [...evaluated.diagnostics];
    for (const value of this.articulations.get(id)?.diagnostics() ?? [])
      diagnostics.push({
        id: value.id,
        kind: "articulation",
        residual: value.residual,
        status: value.residual <= 0.01 ? "satisfied" : "limited",
      });
    if (instance.clothState) {
      const matrices = poseMatrices(instance.artifact.joints, evaluated.pose);
      for (const value of creatureClothDiagnostics(
        instance.artifact,
        matrices,
        instance.clothState,
        this.creatureContext(instance, this.clock.time),
      )) {
        const residual = Math.max(value.stretchResidual, value.maximumPinError);
        diagnostics.push({
          id: value.id,
          kind: "cloth",
          residual,
          status: residual <= 0.01 ? "satisfied" : "limited",
          message: `Maximum stretch ${value.maximumStretch.toFixed(4)}; pin error ${value.maximumPinError.toFixed(5)}m`,
        });
      }
    }
    return diagnostics;
  }

  private pose(instance: CharacterInstance, time: number): Pose {
    const current = sampleCharacterPerformance(
      instance.artifact.joints,
      instance.artifact.motions,
      instance.artifact.performance,
      instance.motion,
      time - instance.motionStart,
      instance.rootMotionPolicy === "physical",
      instance.locomotionSpeed,
    );
    if (!instance.previousMotion || time >= instance.blendStart + instance.blendDuration) return current;
    return blendPoses(
      sampleCharacterPerformance(
        instance.artifact.joints,
        instance.artifact.motions,
        instance.artifact.performance,
        instance.previousMotion,
        time - instance.previousStart,
        instance.rootMotionPolicy === "physical",
        instance.locomotionSpeed,
      ),
      current,
      (time - instance.blendStart) / instance.blendDuration,
    );
  }
  private tick(dt: number, tick: number, emitEvents = true) {
    const time = tick * dt;
    this.replayEndTick = Math.max(this.replayEndTick, tick);
    for (const control of this.controls.get(tick)?.values() ?? []) {
      if (control.kind === "waterImpulse") {
        this.waterBodies
          .get(control.id)
          ?.simulation?.disturb(control.x, control.z, control.radius, control.strength);
        continue;
      }
      if (control.kind === "assemblyJoint") {
        const assembly = this.assemblies.get(control.id);
        if (!assembly) throw new Error(`Unknown runtime assembly ${control.id}`);
        assembly.motion.setJoint(control.joint, control.value);
        continue;
      }
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
      else if (control.kind === "locomotionSpeed") {
        // A new blend changes accumulated root displacement. Preserve the existing
        // position at the last completed tick, then integrate the new stride rate.
        const root =
          instance.rootMotionPolicy === "physical"
            ? instance.artifact.joints.find((joint) => !joint.parent)
            : undefined;
        const before = root ? this.pose(instance, time - dt).get(root.id)?.translation : undefined;
        instance.locomotionSpeed = control.speed;
        const after = root ? this.pose(instance, time - dt).get(root.id)?.translation : undefined;
        if (before && after) {
          const correction = rotateVector(
            instance.baseRotation,
            before.map((value, axis) => (value - after[axis]) * instance.scale) as Vec3,
          );
          instance.basePosition = instance.basePosition.map(
            (value, axis) => value + correction[axis],
          ) as Vec3;
        }
      } else if (control.kind === "expression") {
        if (instance.creatureState)
          Object.defineProperty(instance.creatureState.expressions, control.expression, {
            value: control.weight,
            writable: true,
            enumerable: true,
            configurable: true,
          });
      } else if (
        control.kind === "ragdoll" ||
        control.kind === "articulationImpulse" ||
        control.kind === "recover"
      ) {
        this.ensureArticulation(instance);
        const articulation = this.articulations.get(instance.id);
        if (!articulation)
          throw new Error(
            `Creature articulation ${instance.id} is dormant; prepare its region before applying physical controls`,
          );
        if (control.kind === "ragdoll") {
          if (instance.creatureState) instance.creatureState.contacts = {};
          articulation.enterRagdoll(control.velocity);
        } else if (control.kind === "recover") {
          this.placeCreatureRecovery(instance);
          articulation.recover(control.duration);
        } else {
          if (instance.creatureState) instance.creatureState.contacts = {};
          articulation.impulse(control.joint, control.impulse, control.torque);
        }
      } else {
        instance.previousMotion = instance.motion;
        instance.previousStart = instance.motionStart;
        instance.motion = control.motion;
        instance.motionStart = time;
        instance.blendStart = time;
        instance.blendDuration = control.blend;
      }
    }
    for (const [id, assembly] of this.assemblies) {
      if (assembly.enabled) assembly.motion.syncCollision(this.physics, id, assembly.matrix, time);
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
      if ((this.articulations.get(instance.id)?.authority ?? "animation") !== "animation") continue;
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
    for (const body of this.waterBodies.values()) body.simulation?.step(dt);
    for (const source of this.waterSources) {
      const currentWater = source.evaluate?.(time) ?? source.definition;
      const waterState = this.waterBodies.get(currentWater.id)?.renderState(currentWater);
      for (const instance of this.instances.values()) {
        if (instance.dormant) continue;
        const state = this.bodyState(instance.id);
        const surface = queryWater(currentWater, state.position[0], state.position[2], time, waterState);
        const extent = Math.max(
          0.2,
          ((instance.artifact.mesh.bounds.max[1] - instance.artifact.mesh.bounds.min[1]) / 2) *
            instance.scale,
        );
        if (!surface.wet || state.position[1] + extent < surface.height - surface.depth) continue;
        const impulse = this.physics.buoyancy(instance.id, surface.height, surface.velocity, extent, dt);
        if (impulse && (impulse[0] || impulse[2]))
          this.waterBodies
            .get(currentWater.id)
            ?.simulation?.receiveBodyImpulse(
              state.position[0],
              state.position[2],
              Math.min(3, extent),
              -impulse[0],
              -impulse[2],
            );
      }
    }
    for (const instance of this.instances.values()) {
      const articulation = this.articulations.get(instance.id);
      if (articulation && !instance.dormant)
        articulation.updateTargets(
          this.proceduralCreaturePose(instance, time),
          this.creatureContext(instance, time),
          dt,
        );
    }
    this.physics.step(dt);
    for (const instance of this.instances.values()) {
      const state = this.bodyState(instance.id);
      instance.position = state.position;
      instance.rotation = state.rotation;
      const articulation = this.articulations.get(instance.id);
      if (articulation?.authority === "ragdoll") {
        const displacement = articulation.rootDisplacement(
          this.authoredCreaturePose(instance, time),
          this.creatureContext(instance, time),
        );
        instance.position = instance.position.map((value, axis) => value + displacement[axis]) as Vec3;
        instance.basePosition = instance.basePosition.map(
          (value, axis) => value + displacement[axis],
        ) as Vec3;
        this.physics.teleport(instance.id, instance.position);
        instance.position = this.physics.state(instance.id).position;
      }
      if (!instance.dormant && instance.artifact.creature && instance.creatureState) {
        const context = this.creatureContext(instance, time);
        const evaluated = this.evaluateCreatureInstance(instance, time, dt);
        const skinMatrices = poseMatrices(instance.artifact.joints, evaluated.pose);
        const secondaryContext = this.secondaryContext(context);
        if (instance.clothState)
          stepCreatureCloth(instance.artifact, skinMatrices, instance.clothState, secondaryContext, dt);
        if (instance.groomState)
          stepCreatureGroom(instance.artifact, skinMatrices, instance.groomState, secondaryContext, dt);
      }
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
  private creatureDetailView(artifact: CompiledCharacter, detail?: CreatureDetail): CompiledCharacter {
    if (!detail || detail.mesh === artifact.mesh) return artifact;
    const views = this.creatureDetailViews.get(artifact) ?? new Map<string, CompiledCharacter>();
    const cached = views.get(detail.label);
    if (cached) return cached;
    const view: CompiledCharacter = {
      ...artifact,
      key: `${artifact.key}:detail:${detail.label}`,
      mesh: detail.mesh,
      jointIndices: detail.jointIndices,
      weights: detail.weights,
      creatureCorrectives: detail.correctives ?? artifact.creatureCorrectives,
      creatureGroomDetail: detail.label,
    };
    views.set(detail.label, view);
    this.creatureDetailViews.set(artifact, views);
    return view;
  }
  evaluatedCharacters(
    time = this.clock.time,
    origin: Vec3 = [0, 0, 0],
    selectDetail?: (
      artifact: CompiledCharacter,
      position: Vec3,
      scale: number,
      instanceId: string,
      skinMatrices: Float32Array,
    ) => CreatureDetail | undefined,
  ) {
    return [...this.instances.values()]
      .filter((instance) => !this.world?.persistence.overrides.get(instance.id)?.removed)
      .map((instance) => {
        const context = this.creatureContext(instance, time);
        const pose = this.evaluateCreatureInstance(instance, time).pose;
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
        const skinMatrices = poseMatrices(instance.artifact.joints, pose);
        const artifact = this.creatureDetailView(
          instance.artifact,
          selectDetail?.(
            instance.artifact,
            position.map((value, axis) => value + origin[axis]) as Vec3,
            instance.scale,
            instance.id,
            skinMatrices,
          ),
        );
        const corrective = evaluateCreatureDeformation(
          artifact,
          pose,
          this.creatureDeformations.get(instance.id),
          !instance.groomState?.guides.length && !instance.clothState?.panels.length,
        );
        this.creatureDeformations.set(instance.id, corrective);
        const deformationKey = contentKey({
          artifact: artifact.key,
          corrective: corrective.key,
          groom: instance.groomState?.revision,
          cloth: instance.clothState?.revision,
          position: context.position,
          rotation: context.rotation,
          skinMatrices,
        });
        let combined = this.creatureCombinedDeformations.get(instance.id);
        if (combined?.key !== deformationKey) {
          const groom = instance.groomState
            ? creatureGroomOffsets(instance.artifact, skinMatrices, instance.groomState, context, artifact)
            : undefined;
          const cloth = instance.clothState
            ? creatureClothOffsets(
                artifact,
                skinMatrices,
                instance.clothState,
                this.creatureContext(instance, time),
              )
            : undefined;
          combined = {
            key: deformationKey,
            deformation: mergeCreatureDeformation(
              artifact,
              corrective.deformation,
              [groom, cloth],
              deformationKey,
            ),
          };
          this.creatureCombinedDeformations.set(instance.id, combined);
        }
        return {
          id: instance.id,
          deformation: combined.deformation,
          artifact,
          matrix,
          skinMatrices,
          mode: instance.mode,
          scale: instance.scale,
        };
      });
  }
  get waterBytes() {
    return (
      [...this.waterBodies.values()].reduce((sum, body) => sum + body.byteLength, 0) +
      this.checkpoints.reduce(
        (sum, checkpoint) =>
          sum +
          Object.values(checkpoint.waters ?? {}).reduce((total, water) => total + water.state.byteLength, 0),
        0,
      )
    );
  }
  snapshotWaters(): PersistentWaterState[] {
    return [...this.waterBodies].map(([id, body]) => ({
      id,
      key: body.domain?.key ?? body.renderState().spectrum.key,
      tick: this.clock.tick,
      state: Array.from(body.simulation?.state ?? []),
      exchangedVolume: body.simulation?.exchangedVolume ?? 0,
      elapsed: body.simulation?.elapsed,
      wetness: body.simulation ? Array.from(body.simulation.wetness) : undefined,
    }));
  }
  validateWaterStates(states?: PersistentWaterState[], entities: DormantEntity[] = []) {
    if (!states) return;
    if (
      states.length !== this.waterBodies.size ||
      new Set(states.map((state) => state.id)).size !== states.length
    )
      throw new Error("Saved water bodies do not match this world");
    const tick = states[0]?.tick;
    for (const saved of states) {
      persistentWaterStateSchema.parse(saved);
      const body = this.waterBodies.get(saved.id);
      if (
        !body ||
        saved.key !== (body.domain?.key ?? body.renderState().spectrum.key) ||
        saved.state.length !== (body.simulation?.state.length ?? 0)
      )
        throw new Error("Saved water bed or spectrum is incompatible");
      if (saved.wetness && saved.wetness.length !== (body.simulation?.wetness.length ?? 0))
        throw new Error("Saved water wetness is incompatible");
      if (saved.tick !== tick) throw new Error("Saved water clocks disagree");
      for (let i = 0; i < saved.state.length; i += 4)
        if (saved.state[i] < 0 || saved.state[i + 3] < 0 || saved.state[i + 3] > saved.state[i])
          throw new Error("Nonphysical water checkpoint");
    }
    if (
      states.length &&
      entities.some(
        (entity) =>
          typeof entity.state.runtime === "string" &&
          entity.state.runtime.startsWith("wrela-") &&
          entity.state.tick !== tick,
      )
    )
      throw new Error("Saved water and body clocks disagree");
  }
  private restoreWaters(states?: PersistentWaterState[]) {
    for (const [id, body] of this.waterBodies) {
      const saved = states?.find((state) => state.id === id);
      if (body.simulation && body.domain)
        body.simulation.restore(
          saved
            ? {
                state: new Float64Array(saved.state),
                exchangedVolume: saved.exchangedVolume,
                elapsed: saved.elapsed,
                wetness: saved.wetness ? new Float32Array(saved.wetness) : undefined,
              }
            : new WaterSimulation(body.domain, body.definition).snapshot(),
        );
    }
    if (states?.length) this.clock.reset(states[0].tick);
  }
  snapshotEntities(): DormantEntity[] {
    const characters: DormantEntity[] = [...this.instances.values()]
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
            ...(instance.locomotionSpeed !== undefined ? { locomotionSpeed: instance.locomotionSpeed } : {}),
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
            ...(instance.creatureState ? { creatureState: JSON.stringify(instance.creatureState) } : {}),
            ...(instance.groomState ? { groomState: JSON.stringify(instance.groomState) } : {}),
            ...(instance.clothState ? { clothState: JSON.stringify(instance.clothState) } : {}),
            ...((this.articulations.get(instance.id)?.snapshot() ?? instance.articulationState)
              ? {
                  articulationState: JSON.stringify(
                    this.articulations.get(instance.id)?.snapshot() ?? instance.articulationState,
                  ),
                }
              : {}),
          },
        };
      });
    const machinery: DormantEntity[] = [...this.assemblies]
      .filter(([id]) => !this.world?.persistence.overrides.get(id)?.removed)
      .map(([id, assembly]) => ({
        id,
        definition: assembly.definition,
        position: [assembly.matrix[12], assembly.matrix[13], assembly.matrix[14]],
        velocity: [0, 0, 0],
        state: {
          runtime: "wrela-assembly-1",
          tick: this.clock.tick,
          sourceKey: contentKey(assembly.motion.source),
          joints: encodeAssemblyJoints(assembly.motion.source, assembly.motion.snapshotJoints()),
        },
      }));
    return [...characters, ...machinery];
  }
  private validateAssemblyEntityStates(entities: DormantEntity[]) {
    const active = entities.filter((entity) => entity.state.runtime === "wrela-assembly-1");
    if (new Set(active.map((entity) => entity.id)).size !== active.length)
      throw new Error("Duplicate saved assemblies");
    return active.map((entity) => {
      const assembly = this.assemblies.get(entity.id);
      if (!assembly) throw new Error(`Unknown saved assembly ${entity.id}`);
      return { id: entity.id, ...decodeAssemblyState(entity, assembly.motion.source, assembly.definition) };
    });
  }
  validateEntityStates(entities: DormantEntity[]) {
    const machinery = this.validateAssemblyEntityStates(entities);
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
      const locomotionSpeed =
        entity.state.locomotionSpeed === undefined ? undefined : number("locomotionSpeed");
      if (
        locomotionSpeed !== undefined &&
        (locomotionSpeed < 0 || locomotionSpeed > 50 || !instance.artifact.performance?.locomotion)
      )
        throw new Error("Invalid saved locomotion speed");
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
      const creatureState = instance.artifact.creature
        ? entity.state.creatureState === undefined
          ? createCreatureRuntimeState()
          : typeof entity.state.creatureState === "string" && entity.state.creatureState.length <= 262144
            ? validateCreatureRuntimeState(JSON.parse(entity.state.creatureState), instance.artifact.creature)
            : (() => {
                throw new Error("Invalid saved creature state");
              })()
        : undefined;
      const groomState = instance.artifact.creatureGroom?.guides.length
        ? entity.state.groomState === undefined
          ? createCreatureGroomState()
          : typeof entity.state.groomState === "string" && entity.state.groomState.length <= 524288
            ? validateCreatureGroomState(JSON.parse(entity.state.groomState), instance.artifact)
            : (() => {
                throw new Error("Invalid saved groom state");
              })()
        : undefined;
      const clothState = instance.artifact.creature?.cloth.length
        ? entity.state.clothState === undefined
          ? createCreatureClothState()
          : typeof entity.state.clothState === "string" && entity.state.clothState.length <= 2097152
            ? validateCreatureClothState(JSON.parse(entity.state.clothState), instance.artifact)
            : (() => {
                throw new Error("Invalid saved cloth state");
              })()
        : undefined;
      const articulationState =
        instance.artifact.creature?.articulation && entity.state.articulationState !== undefined
          ? typeof entity.state.articulationState === "string" &&
            entity.state.articulationState.length <= 262144
            ? validateCreatureArticulationSnapshot(
                JSON.parse(entity.state.articulationState),
                instance.artifact.creature.articulation,
              )
            : (() => {
                throw new Error("Invalid saved articulation state");
              })()
          : undefined;
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
        creatureState,
        groomState,
        clothState,
        articulationState,
        active: entity.state.active !== false,
        rootMotionPolicy,
        locomotionSpeed,
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
    if (new Set([...decoded, ...machinery].map((value) => value.tick)).size > 1)
      throw new Error("Saved runtime entities disagree on simulation tick");
    return decoded;
  }
  restoreEntityStates(entities: DormantEntity[], waters?: PersistentWaterState[]) {
    this.validateWaterStates(waters, entities);
    const decoded = this.validateEntityStates(entities);
    const machinery = this.validateAssemblyEntityStates(entities);
    this.creatureCombinedDeformations.clear();
    this.creaturePoses.clear();
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
      this.articulations.get(value.instance.id)?.dispose();
      this.articulations.delete(value.instance.id);
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
        locomotionSpeed: value.locomotionSpeed,
        basePosition: value.basePosition,
        baseRotation: value.baseRotation,
        motion: value.motion,
        previousMotion: value.previousMotion,
        motionStart: value.motionStart,
        previousStart: value.previousStart,
        blendStart: value.blendStart,
        blendDuration: value.blendDuration,
        creatureState: value.creatureState,
        groomState: value.groomState,
        clothState: value.clothState,
        articulationState: value.articulationState,
      });
    }
    if (decoded.length || machinery.length) this.clock.reset((decoded[0] ?? machinery[0]).tick);
    const commands = new Map(machinery.map((value) => [value.id, value.commands]));
    for (const [id, assembly] of this.assemblies) {
      assembly.motion.restoreJoints(commands.get(id) ?? {});
      if (assembly.enabled)
        assembly.motion.teleportCollision(this.physics, id, assembly.matrix, this.clock.time);
    }
    for (const value of decoded) this.ensureArticulation(value.instance);
    this.restoreWaters(waters);
    this.resetHistory();
    return this.clock.time;
  }
  checkpoint(): RuntimeCheckpoint {
    return {
      tick: this.clock.tick,
      physics: this.physics.checkpoint(),
      waters: Object.fromEntries(
        [...this.waterBodies].flatMap(([id, body]) =>
          body.simulation ? [[id, body.simulation.snapshot()]] : [],
        ),
      ),
      instances: [...this.instances.values()].map((i) => ({
        ...i,
        basePosition: [...i.basePosition],
        position: [...i.position],
        previousPosition: [...i.previousPosition],
        dormant: i.dormant ? structuredClone(i.dormant) : undefined,
        configuration: structuredClone(i.configuration),
        creatureState: i.creatureState ? structuredClone(i.creatureState) : undefined,
        groomState: i.groomState ? structuredClone(i.groomState) : undefined,
        clothState: i.clothState ? structuredClone(i.clothState) : undefined,
        articulationState:
          this.articulations.get(i.id)?.snapshot() ??
          (i.articulationState ? structuredClone(i.articulationState) : undefined),
      })),
      worldRevision: this.worldRevision,
      collisionKey: this.collisionKey,
      secondaryGroundPlane: this.secondaryGroundPlane ? { ...this.secondaryGroundPlane } : undefined,
      assemblyJoints: Object.fromEntries(
        [...this.assemblies].map(([id, assembly]) => [id, assembly.motion.snapshotJoints()]),
      ),
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
    for (const [id, state] of Object.entries(checkpoint.waters ?? {}))
      this.waterBodies.get(id)?.simulation?.restore(state);
    this.creatureCombinedDeformations.clear();
    this.creaturePoses.clear();
    this.articulations.clear();
    this.physics.restore(checkpoint.physics);
    for (const [id, assembly] of this.assemblies)
      assembly.motion.restoreJoints(checkpoint.assemblyJoints[id] ?? {});
    this.secondaryGroundPlane = checkpoint.secondaryGroundPlane
      ? { ...checkpoint.secondaryGroundPlane }
      : undefined;
    this.appliedOverrides.clear();
    this.instances = new Map(
      checkpoint.instances.map((instance) => [
        instance.id,
        {
          ...instance,
          configuration: structuredClone(instance.configuration),
          creatureState: instance.creatureState ? structuredClone(instance.creatureState) : undefined,
          groomState: instance.groomState ? structuredClone(instance.groomState) : undefined,
          clothState: instance.clothState ? structuredClone(instance.clothState) : undefined,
          articulationState: instance.articulationState
            ? structuredClone(instance.articulationState)
            : undefined,
          dormant: instance.dormant ? structuredClone(instance.dormant) : undefined,
        },
      ]),
    );
    this.clock.reset(checkpoint.tick);
    for (const instance of this.instances.values()) this.ensureArticulation(instance);
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
    this.resetCreatureDynamics(instance);
    this.resetArticulation(instance);
    this.ensureArticulation(instance);
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
    for (const articulation of this.articulations.values()) articulation.dispose();
    this.articulations.clear();
    this.assemblies.clear();
    this.physics.dispose();
    this.instances.clear();
    this.secondaryGroundPlane = undefined;
    this.checkpoints = [];
    this.controls.clear();
    this.controlCount = 0;
    this.replayEndTick = this.clock.tick;
    this.poseOverrides.clear();
    this.creatureDeformations.clear();
    this.creatureCombinedDeformations.clear();
    this.creaturePoses.clear();
    this.motionEvents.clear();
    this.clearAnimationEvents();
  }
}
