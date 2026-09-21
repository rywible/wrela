import RAPIER from "@dimforge/rapier3d-compat";
import type { Bounds, CharacterCollider, MeshData, Quat, Vec3 } from "@wrela/model";
import { quatFromEuler, quatMultiply, rotateVector } from "./animation";
export type PhysicsMode = "kinematic" | "dynamic" | "motor";
export type BodyState = {
  position: Vec3;
  rotation: Quat;
  velocity: Vec3;
  angularVelocity: Vec3;
  mode: PhysicsMode;
};
export type BodyConfiguration = {
  rotation?: Quat;
  colliders?: CharacterCollider[];
  id: string;
  position: Vec3;
  radius: number;
  halfHeight: number;
  mass: number;
  restitution: number;
  friction: number;
  mode: PhysicsMode;
};
export type DiagnosticCollider = {
  instanceId: string;
  colliderId: string;
  position: Vec3;
  rotation: Quat;
} & (
  | { shape: "box"; size: Vec3 }
  | { shape: "sphere"; radius: number }
  | { shape: "capsule"; radius: number; halfHeight: number }
);
export type PhysicsCheckpoint = {
  snapshot: Uint8Array;
  bodies: [string, number, PhysicsMode][];
  terrain: number[];
  statics: [string, number[]][];
  staticTransforms: [string, { offset: Vec3; rotation: Quat }[]][];
  staticOffsets: [string, Vec3][];
  colliderIds: [number, string][];
  restoredKinematicVelocities: [string, { velocity: Vec3; angularVelocity: Vec3 }][];
  origin: Vec3;
};
let initialized: Promise<void> | undefined;
const vector = (v: Vec3) => ({ x: v[0], y: v[1], z: v[2] });
const quaternion = (q: Quat) => ({ x: q[0], y: q[1], z: q[2], w: q[3] });
export type PhysicsContactEvent = { a: string; b: string; started: boolean; sensor: boolean };
export type PhysicsRayHit = { id: string; point: Vec3; normal: Vec3; distance: number };
export class PhysicsAdapter {
  private bodies = new Map<string, { body: RAPIER.RigidBody; mode: PhysicsMode }>();
  private terrain: RAPIER.Collider[] = [];
  private statics = new Map<string, { collider: RAPIER.Collider; offset: Vec3; rotation: Quat }[]>();
  private events = new RAPIER.EventQueue(true);
  private contacts: PhysicsContactEvent[] = [];
  private staticOffsets = new Map<string, Vec3>();
  private colliderIds = new Map<number, string>();
  // Rapier position-based kinematics recompute velocity on step; preserve saved
  // transfer velocity until that step so an immediate mode change is lossless.
  private restoredKinematicVelocities = new Map<string, { velocity: Vec3; angularVelocity: Vec3 }>();
  private origin: Vec3 = [0, 0, 0];
  get coordinateOrigin(): Vec3 {
    return [...this.origin];
  }
  private disposed = false;
  private controller: RAPIER.KinematicCharacterController;
  private constructor(private world: RAPIER.World) {
    this.controller = this.makeController();
  }
  private makeController() {
    const controller = this.world.createCharacterController(0.015);
    controller.setSlideEnabled(true);
    controller.setMaxSlopeClimbAngle(Math.PI / 3);
    controller.enableAutostep(0.2, 0.1, false);
    return controller;
  }
  static async create(gravity: Vec3 = [0, -9.81, 0]) {
    initialized ??= RAPIER.init();
    await initialized;
    return new PhysicsAdapter(new RAPIER.World(vector(gravity)));
  }
  add(configuration: BodyConfiguration) {
    if (this.bodies.has(configuration.id)) throw new Error(`Duplicate body ${configuration.id}`);
    if (this.bodies.size >= 256) throw new Error("Physics body budget exceeded");
    const descriptor =
      configuration.mode === "kinematic"
        ? RAPIER.RigidBodyDesc.kinematicPositionBased()
        : RAPIER.RigidBodyDesc.dynamic();
    if (configuration.rotation) descriptor.setRotation(quaternion(configuration.rotation));
    descriptor.setTranslation(...(configuration.position.map((v, i) => v - this.origin[i]) as Vec3));
    const body = this.world.createRigidBody(descriptor);
    const colliders: CharacterCollider[] = configuration.colliders ?? [
      {
        id: "body",
        shape: "capsule",
        position: [0, 0, 0],
        rotation: [0, 0, 0],
        radius: configuration.radius,
        halfHeight: configuration.halfHeight,
      },
    ];
    for (const collider of colliders) {
      const shape =
        collider.shape === "box"
          ? RAPIER.ColliderDesc.cuboid(...collider.size)
          : collider.shape === "sphere"
            ? RAPIER.ColliderDesc.ball(collider.radius)
            : RAPIER.ColliderDesc.capsule(collider.halfHeight, collider.radius);
      const installed = this.world.createCollider(
        shape
          .setTranslation(...collider.position)
          .setRotation(quaternion(quatFromEuler(collider.rotation)))
          .setMass(configuration.mass / colliders.length)
          .setRestitution(configuration.restitution)
          .setFriction(configuration.friction)
          .setActiveEvents(RAPIER.ActiveEvents.COLLISION_EVENTS),
        body,
      );
      this.colliderIds.set(installed.handle, collider.id);
    }
    this.bodies.set(configuration.id, { body, mode: configuration.mode });
  }
  addStaticObject(
    id: string,
    shape: "box" | "sphere",
    bounds: Bounds,
    position: Vec3,
    rotation: Quat = [0, 0, 0, 1],
    scale = 1,
  ) {
    if (this.statics.has(id)) throw new Error(`Duplicate static object ${id}`);
    if (this.statics.size >= 256) throw new Error("Static collision budget exceeded");
    const center = bounds.min.map((v, i) => ((v + bounds.max[i]) / 2) * scale) as Vec3;
    const offset = rotateVector(rotation, center),
      halfSize = bounds.max.map((v, i) => Math.max(0.01, ((v - bounds.min[i]) / 2) * scale)) as Vec3;
    const descriptor =
      shape === "sphere"
        ? RAPIER.ColliderDesc.ball(Math.max(...halfSize))
        : RAPIER.ColliderDesc.cuboid(...halfSize);
    descriptor
      .setTranslation(...(position.map((v, i) => v + offset[i] - this.origin[i]) as Vec3))
      .setRotation(quaternion(rotation))
      .setFriction(0.8);
    this.statics.set(id, [
      { collider: this.world.createCollider(descriptor), offset: center, rotation: [0, 0, 0, 1] },
    ]);
    this.staticOffsets.set(id, center);
  }
  addStaticMesh(id: string, mesh: MeshData, position: Vec3, rotation: Quat = [0, 0, 0, 1], scale = 1) {
    if (this.statics.has(id)) throw new Error(`Duplicate static object ${id}`);
    if (this.statics.size >= 256) throw new Error("Static collision budget exceeded");
    const vertices = mesh.positions.map((value) => value * scale);
    const collider = this.world.createCollider(
      RAPIER.ColliderDesc.trimesh(vertices, mesh.indices.slice())
        .setTranslation(...(position.map((value, axis) => value - this.origin[axis]) as Vec3))
        .setRotation(quaternion(rotation))
        .setFriction(0.8),
    );
    this.statics.set(id, [{ collider, offset: [0, 0, 0], rotation: [0, 0, 0, 1] }]);
  }
  addStaticCompound(
    id: string,
    shapes: CharacterCollider[],
    position: Vec3,
    rotation: Quat = [0, 0, 0, 1],
    scale = 1,
  ) {
    if (this.statics.has(id)) throw new Error(`Duplicate static object ${id}`);
    if (!shapes.length || shapes.length > 32 || this.statics.size >= 256)
      throw new Error("Static collision budget exceeded");
    const parts: { collider: RAPIER.Collider; offset: Vec3; rotation: Quat }[] = [];
    try {
      for (const shape of shapes) {
        const descriptor =
          shape.shape === "box"
            ? RAPIER.ColliderDesc.cuboid(...(shape.size.map((v) => v * scale) as Vec3))
            : shape.shape === "sphere"
              ? RAPIER.ColliderDesc.ball(shape.radius * scale)
              : RAPIER.ColliderDesc.capsule(shape.halfHeight * scale, shape.radius * scale);
        const offset = shape.position.map((v) => v * scale) as Vec3;
        const localRotation = quatFromEuler(shape.rotation);
        const rotated = rotateVector(rotation, offset);
        const collider = this.world.createCollider(
          descriptor
            .setTranslation(...(position.map((v, i) => v + rotated[i] - this.origin[i]) as Vec3))
            .setRotation(quaternion(quatMultiply(rotation, localRotation)))
            .setFriction(0.8),
        );
        parts.push({ collider, offset, rotation: localRotation });
        this.colliderIds.set(collider.handle, shape.id);
      }
      this.statics.set(id, parts);
    } catch (error) {
      for (const part of parts) this.world.removeCollider(part.collider, true);
      throw error;
    }
  }
  updateStaticObject(id: string, enabled: boolean, position: Vec3, rotation: Quat) {
    const parts = this.statics.get(id);
    if (!parts) return false;
    for (const part of parts) {
      part.collider.setEnabled(enabled);
      const offset = rotateVector(rotation, part.offset);
      part.collider.setTranslation(vector(position.map((v, i) => v + offset[i] - this.origin[i]) as Vec3));
      part.collider.setRotation(quaternion(quatMultiply(rotation, part.rotation)));
    }
    return true;
  }
  private owner(collider: RAPIER.Collider): string {
    for (const [id, parts] of this.statics)
      if (parts.some((part) => part.collider.handle === collider.handle)) return id;
    // Use installed collider membership. Restored standalone collider wrappers may
    // expose a zero parent handle, which is also a valid first rigid-body handle.
    for (const [id, entry] of this.bodies)
      for (let index = 0; index < entry.body.numColliders(); index++)
        if (entry.body.collider(index).handle === collider.handle) return id;
    return "terrain";
  }
  raycast(origin: Vec3, direction: Vec3, maximum: number, excludeId?: string): PhysicsRayHit | null {
    if (
      !Number.isFinite(maximum) ||
      maximum <= 0 ||
      [...origin, ...direction].some((v) => !Number.isFinite(v))
    )
      throw new RangeError("Invalid physics ray");
    const length = Math.hypot(...direction);
    if (!length) return null;
    const unit = direction.map((v) => v / length) as Vec3;
    const ray = new RAPIER.Ray(vector(origin.map((v, i) => v - this.origin[i]) as Vec3), vector(unit));
    const hit = this.world.castRayAndGetNormal(
      ray,
      maximum,
      true,
      undefined,
      undefined,
      undefined,
      undefined,
      (collider) => collider.isEnabled() && this.owner(collider) !== excludeId,
    );
    if (!hit) return null;
    return {
      id: this.owner(hit.collider),
      distance: hit.timeOfImpact,
      point: origin.map((v, i) => v + unit[i] * hit.timeOfImpact) as Vec3,
      normal: [hit.normal.x, hit.normal.y, hit.normal.z],
    };
  }
  overlapSphere(position: Vec3, radius: number, excludeId?: string): string[] {
    if (!Number.isFinite(radius) || radius <= 0 || position.some((v) => !Number.isFinite(v)))
      throw new RangeError("Invalid overlap sphere");
    const ids = new Set<string>();
    this.world.intersectionsWithShape(
      vector(position.map((v, i) => v - this.origin[i]) as Vec3),
      quaternion([0, 0, 0, 1]),
      new RAPIER.Ball(radius),
      (collider) => {
        const id = this.owner(collider);
        if (id !== excludeId) ids.add(id);
        return true;
      },
    );
    return [...ids].sort();
  }
  setSensor(id: string, sensor: boolean) {
    const body = this.bodies.get(id)?.body;
    const colliders = body
      ? Array.from({ length: body.numColliders() }, (_, index) => body.collider(index))
      : this.statics.get(id)?.map((part) => part.collider);
    if (!colliders) throw new Error(`Unknown physics object ${id}`);
    for (const collider of colliders) collider.setSensor(sensor);
  }
  drainContactEvents(): PhysicsContactEvent[] {
    return this.contacts.splice(0);
  }
  setCoefficients(id: string, parameters: { mass: number; friction: number; restitution: number }) {
    const body = this.entry(id).body;
    for (let index = 0; index < body.numColliders(); index++) {
      const collider = body.collider(index);
      collider.setMass(parameters.mass / body.numColliders());
      collider.setFriction(parameters.friction);
      collider.setRestitution(parameters.restitution);
    }
    body.recomputeMassPropertiesFromColliders();
  }
  setEnabled(id: string, enabled: boolean) {
    this.entry(id).body.setEnabled(enabled);
  }
  setRotation(id: string, rotation: Quat) {
    const body = this.entry(id).body;
    body.setRotation(quaternion(rotation), true);
    if (body.isKinematic()) body.setNextKinematicRotation(quaternion(rotation));
  }
  remove(id: string) {
    const entry = this.bodies.get(id);
    if (entry) {
      for (let index = 0; index < entry.body.numColliders(); index++)
        this.colliderIds.delete(entry.body.collider(index).handle);
      this.world.removeRigidBody(entry.body);
      this.bodies.delete(id);
      this.restoredKinematicVelocities.delete(id);
    }
  }
  state(id: string): BodyState {
    const entry = this.entry(id),
      p = entry.body.translation(),
      q = entry.body.rotation(),
      v = entry.body.linvel(),
      w = entry.body.angvel();
    return {
      position: [p.x + this.origin[0], p.y + this.origin[1], p.z + this.origin[2]],
      rotation: [q.x, q.y, q.z, q.w],
      velocity: [...(this.restoredKinematicVelocities.get(id)?.velocity ?? [v.x, v.y, v.z])],
      angularVelocity: [...(this.restoredKinematicVelocities.get(id)?.angularVelocity ?? [w.x, w.y, w.z])],
      mode: entry.mode,
    };
  }
  private entry(id: string) {
    const entry = this.bodies.get(id);
    if (!entry) throw new Error(`Unknown physics body ${id}`);
    return entry;
  }
  restoreBodyState(id: string, state: BodyState) {
    this.setMode(id, state.mode, state.velocity);
    const body = this.entry(id).body;
    body.setTranslation(
      vector(state.position.map((value, index) => value - this.origin[index]) as Vec3),
      true,
    );
    body.setRotation(quaternion(state.rotation), true);
    body.setLinvel(vector(state.velocity), true);
    body.setAngvel(vector(state.angularVelocity), true);
    if (state.mode === "kinematic") {
      body.setNextKinematicTranslation(body.translation());
      body.setNextKinematicRotation(body.rotation());
      this.restoredKinematicVelocities.set(id, {
        velocity: [...state.velocity],
        angularVelocity: [...state.angularVelocity],
      });
    }
  }
  setMode(id: string, mode: PhysicsMode, velocity?: Vec3) {
    const entry = this.entry(id);
    const previous = this.state(id);
    this.restoredKinematicVelocities.delete(id);
    entry.body.setBodyType(
      mode === "kinematic" ? RAPIER.RigidBodyType.KinematicPositionBased : RAPIER.RigidBodyType.Dynamic,
      true,
    );
    entry.mode = mode;
    entry.body.setLinvel(vector(velocity ?? previous.velocity), true);
    entry.body.setAngvel(vector(previous.angularVelocity), true);
    if (mode === "kinematic") {
      entry.body.setNextKinematicTranslation(entry.body.translation());
      entry.body.setNextKinematicRotation(entry.body.rotation());
    }
  }
  target(id: string, position: Vec3, rotation: Quat, dt: number) {
    const entry = this.entry(id),
      target = position.map((v, i) => v - this.origin[i]) as Vec3;
    if (entry.mode === "kinematic") {
      const current = entry.body.translation();
      const ownHandles = new Set(
        Array.from({ length: entry.body.numColliders() }, (_, index) => entry.body.collider(index).handle),
      );
      let movement = { x: target[0] - current.x, y: target[1] - current.y, z: target[2] - current.z };
      // Every authored primitive constrains the same rigid displacement.
      for (let pass = 0; pass < 2; pass++)
        for (let index = 0; index < entry.body.numColliders(); index++) {
          this.controller.computeColliderMovement(
            entry.body.collider(index),
            movement,
            undefined,
            undefined,
            (collider) => !ownHandles.has(collider.handle),
          );
          movement = this.controller.computedMovement();
        }
      entry.body.setNextKinematicTranslation({
        x: current.x + movement.x,
        y: current.y + movement.y,
        z: current.z + movement.z,
      });
      entry.body.setNextKinematicRotation(quaternion(rotation));
    } else if (entry.mode === "motor") {
      const current = entry.body.translation(),
        velocity = entry.body.linvel(),
        mass = entry.body.mass();
      const delta: Vec3 = [target[0] - current.x, target[1] - current.y, target[2] - current.z];
      const impulse = delta.map(
        (v, i) =>
          Math.max(
            -80,
            Math.min(80, 60 * v - 12 * [velocity.x, velocity.y, velocity.z][i] + (i === 1 ? 9.81 : 0)),
          ) *
          mass *
          dt,
      ) as Vec3;
      entry.body.applyImpulse(vector(impulse), true);
      const q = entry.body.rotation(),
        sign = q.x * rotation[0] + q.y * rotation[1] + q.z * rotation[2] + q.w * rotation[3] < 0 ? -1 : 1,
        angular = entry.body.angvel();
      const torque: Vec3 = [
        rotation[0] * sign * q.w -
          rotation[3] * sign * q.x -
          rotation[1] * sign * q.z +
          rotation[2] * sign * q.y,
        rotation[1] * sign * q.w -
          rotation[3] * sign * q.y -
          rotation[2] * sign * q.x +
          rotation[0] * sign * q.z,
        rotation[2] * sign * q.w -
          rotation[3] * sign * q.z -
          rotation[0] * sign * q.y +
          rotation[1] * sign * q.x,
      ];
      entry.body.applyTorqueImpulse(
        vector(torque.map((v, i) => (v * 12 - [angular.x, angular.y, angular.z][i] * 2) * mass * dt) as Vec3),
        true,
      );
    }
  }
  /** One-point displaced-volume approximation; water height and velocity use the render wave program. */
  buoyancy(id: string, height: number, surfaceVelocity: Vec3, halfHeight: number, dt: number) {
    const entry = this.entry(id);
    if (entry.mode === "kinematic") return;
    const state = this.state(id),
      submerged = Math.max(0, Math.min(1, (height - state.position[1] + halfHeight) / (2 * halfHeight)));
    if (submerged === 0) return;
    const mass = entry.body.mass();
    const impulse = state.velocity.map(
      (v, i) => ((surfaceVelocity[i] - v) * 3 + (i === 1 ? 9.81 * 1.4 : 0)) * submerged * mass * dt,
    ) as Vec3;
    entry.body.applyImpulse(vector(impulse), true);
  }
  teleport(id: string, position: Vec3) {
    this.restoredKinematicVelocities.delete(id);
    const entry = this.entry(id);
    entry.body.setTranslation(vector(position.map((v, i) => v - this.origin[i]) as Vec3), true);
    entry.body.setLinvel(vector([0, 0, 0]), true);
    entry.body.setAngvel(vector([0, 0, 0]), true);
    if (entry.mode === "kinematic") entry.body.setNextKinematicTranslation(entry.body.translation());
  }
  installTerrain(patches: { mesh: MeshData; x: number; z: number }[]) {
    if (patches.length > 256) throw new Error("Collision terrain budget exceeded");
    const prepared: RAPIER.Collider[] = [];
    try {
      for (const patch of patches)
        prepared.push(
          this.world.createCollider(
            RAPIER.ColliderDesc.trimesh(patch.mesh.positions.slice(), patch.mesh.indices.slice())
              .setTranslation(patch.x - this.origin[0], -this.origin[1], patch.z - this.origin[2])
              .setFriction(0.9),
          ),
        );
    } catch (error) {
      for (const collider of prepared) this.world.removeCollider(collider, true);
      throw error;
    }
    for (const collider of this.terrain) this.world.removeCollider(collider, true);
    this.terrain = prepared;
  }
  addGround(height = 0) {
    this.terrain.push(
      this.world.createCollider(
        RAPIER.ColliderDesc.cuboid(2048, 1, 2048).setTranslation(
          -this.origin[0],
          height - 1 - this.origin[1],
          -this.origin[2],
        ),
      ),
    );
  }
  rebase(origin: Vec3) {
    const delta = origin.map((v, i) => v - this.origin[i]) as Vec3;
    for (const { body } of this.bodies.values()) {
      const p = body.translation();
      body.setTranslation({ x: p.x - delta[0], y: p.y - delta[1], z: p.z - delta[2] }, false);
      if (body.isKinematic()) body.setNextKinematicTranslation(body.translation());
    }
    for (const collider of [
      ...this.terrain,
      ...[...this.statics.values()].flatMap((parts) => parts.map((part) => part.collider)),
    ]) {
      const p = collider.translation();
      collider.setTranslation({ x: p.x - delta[0], y: p.y - delta[1], z: p.z - delta[2] });
    }
    this.origin = [...origin];
  }
  step(dt: number) {
    if (this.disposed) throw new Error("Physics disposed");
    this.world.timestep = dt;
    this.world.step(this.events);
    this.restoredKinematicVelocities.clear();
    this.events.drainCollisionEvents((a, b, started) => {
      const first = this.world.getCollider(a),
        second = this.world.getCollider(b);
      if (first && second)
        this.contacts.push({
          a: this.owner(first),
          b: this.owner(second),
          started,
          sensor: first.isSensor() || second.isSensor(),
        });
    });
    if (this.contacts.length > 4096) this.contacts.splice(0, this.contacts.length - 4096);
  }
  /** Installed primitives at the authoritative physics tick, in absolute world coordinates. */
  diagnosticColliders(): DiagnosticCollider[] {
    const result: DiagnosticCollider[] = [];
    const collect = (
      instanceId: string,
      colliderId: string,
      collider: RAPIER.Collider,
      body?: RAPIER.RigidBody,
    ) => {
      if (!collider.isEnabled()) return;
      const p = collider.translation(),
        q = collider.rotation();
      let position: Vec3 = [p.x, p.y, p.z],
        rotation: Quat = [q.x, q.y, q.z, q.w];
      const localPosition = collider.translationWrtParent(),
        localRotation = collider.rotationWrtParent();
      // Rapier updates cached collider world transforms on the next step. A
      // paused teleport/rebase must still show the current body's true shape.
      if (body && localPosition && localRotation) {
        const bp = body.translation(),
          bq = body.rotation(),
          bodyRotation: Quat = [bq.x, bq.y, bq.z, bq.w];
        const offset = rotateVector(bodyRotation, [localPosition.x, localPosition.y, localPosition.z]);
        position = [bp.x + offset[0], bp.y + offset[1], bp.z + offset[2]];
        rotation = quatMultiply(bodyRotation, [
          localRotation.x,
          localRotation.y,
          localRotation.z,
          localRotation.w,
        ]);
      }
      const common = {
        instanceId,
        colliderId,
        position: position.map((value, axis) => value + this.origin[axis]) as Vec3,
        rotation,
      };
      if (collider.shapeType() === RAPIER.ShapeType.Cuboid) {
        const half = collider.halfExtents();
        result.push({ ...common, shape: "box", size: [half.x, half.y, half.z] });
      } else if (collider.shapeType() === RAPIER.ShapeType.Ball) {
        result.push({ ...common, shape: "sphere", radius: collider.radius() });
      } else if (collider.shapeType() === RAPIER.ShapeType.Capsule) {
        result.push({
          ...common,
          shape: "capsule",
          radius: collider.radius(),
          halfHeight: collider.halfHeight(),
        });
      }
    };
    for (const [id, { body }] of this.bodies) {
      if (!body.isEnabled()) continue;
      for (let index = 0; index < body.numColliders(); index++) {
        const collider = body.collider(index);
        collect(id, this.colliderIds.get(collider.handle) ?? `collider-${index}`, collider, body);
      }
    }
    for (const [id, parts] of this.statics)
      for (const part of parts) collect(id, this.colliderIds.get(part.collider.handle) ?? id, part.collider);
    return result;
  }
  checkpoint(): PhysicsCheckpoint {
    return {
      snapshot: this.world.takeSnapshot(),
      bodies: [...this.bodies].map(([id, entry]) => [id, entry.body.handle, entry.mode]),
      terrain: this.terrain.map((c) => c.handle),
      statics: [...this.statics].map(([id, parts]) => [id, parts.map((part) => part.collider.handle)]),
      staticTransforms: [...this.statics].map(([id, parts]) => [
        id,
        parts.map((part) => ({ offset: [...part.offset], rotation: [...part.rotation] })),
      ]),
      staticOffsets: [...this.staticOffsets].map(([id, offset]) => [id, [...offset]]),
      colliderIds: [...this.colliderIds],
      restoredKinematicVelocities: structuredClone([...this.restoredKinematicVelocities]),
      origin: [...this.origin],
    };
  }
  restore(checkpoint: PhysicsCheckpoint) {
    this.world.free();
    this.world = RAPIER.World.restoreSnapshot(checkpoint.snapshot);
    this.controller = this.makeController();
    this.bodies.clear();
    for (const [id, handle, mode] of checkpoint.bodies)
      this.bodies.set(id, { body: this.world.getRigidBody(handle), mode });
    this.terrain = checkpoint.terrain.map((handle) => this.world.getCollider(handle));
    const transforms = new Map(checkpoint.staticTransforms);
    this.statics = new Map(
      checkpoint.statics.map(([id, handles]) => [
        id,
        handles.map((handle, index) => {
          const transform = transforms.get(id)?.[index];
          if (!transform) throw new Error("Missing static collision checkpoint transform");
          return { collider: this.world.getCollider(handle), ...transform };
        }),
      ]),
    );
    this.contacts = [];
    this.events.clear();
    this.staticOffsets = new Map(checkpoint.staticOffsets);
    this.colliderIds = new Map(checkpoint.colliderIds);
    this.restoredKinematicVelocities = new Map(structuredClone(checkpoint.restoredKinematicVelocities));
    this.origin = [...checkpoint.origin];
  }
  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this.world.free();
    this.events.free();
    this.contacts = [];
    this.bodies.clear();
    this.terrain = [];
    this.statics.clear();
    this.staticOffsets.clear();
    this.colliderIds.clear();
    this.restoredKinematicVelocities.clear();
  }
}
