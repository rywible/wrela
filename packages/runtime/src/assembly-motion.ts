import {
  type AssemblyDefinition,
  type AssemblyPart,
  assemblyJointValues,
  assemblyPartPoint,
  type CharacterCollider,
  dot,
  type MeshData,
  type Quat,
  rotateAssemblyPoint,
  sub,
  type Vec3,
} from "@wrela/model";

import type { PhysicsAdapter } from "./physics";

type Frame = { origin: Vec3; axes: [Vec3, Vec3, Vec3] };
function frame(
  source: AssemblyDefinition,
  part: AssemblyPart,
  values: Readonly<Record<string, number>> = {},
): Frame {
  const origin = assemblyPartPoint(source, part, [0, 0, 0], values);
  return {
    origin,
    axes: [0, 1, 2].map((axis) => {
      const p: Vec3 = [0, 0, 0];
      p[axis] = 1;
      return sub(assemblyPartPoint(source, part, p, values), origin);
    }) as Frame["axes"],
  };
}
function localPoint(point: Vec3, rest: Frame): Vec3 {
  const delta = sub(point, rest.origin);
  return rest.axes.map((axis) => dot(axis, delta)) as Vec3;
}
export function assemblyMatrixMultiply(a: Float32Array, b: Float32Array): Float32Array {
  const result = new Float32Array(16);
  for (let column = 0; column < 4; column++)
    for (let row = 0; row < 4; row++)
      for (let k = 0; k < 4; k++) result[column * 4 + row] += a[k * 4 + row] * b[column * 4 + k];
  return result;
}
function matrixOf(f: Frame): Float32Array {
  return new Float32Array([...f.axes[0], 0, ...f.axes[1], 0, ...f.axes[2], 0, ...f.origin, 1]);
}
function matrixQuat(matrix: Float32Array): Quat {
  const scale = Math.hypot(matrix[0], matrix[1], matrix[2]);
  const m = Array.from(matrix, (value) => value / scale);
  const trace = m[0] + m[5] + m[10];
  let q: Quat;
  if (trace > 0) {
    const s = Math.sqrt(trace + 1) * 2;
    q = [(m[6] - m[9]) / s, (m[8] - m[2]) / s, (m[1] - m[4]) / s, s / 4];
  } else if (m[0] > m[5] && m[0] > m[10]) {
    const s = Math.sqrt(1 + m[0] - m[5] - m[10]) * 2;
    q = [s / 4, (m[4] + m[1]) / s, (m[8] + m[2]) / s, (m[6] - m[9]) / s];
  } else if (m[5] > m[10]) {
    const s = Math.sqrt(1 + m[5] - m[0] - m[10]) * 2;
    q = [(m[4] + m[1]) / s, s / 4, (m[9] + m[6]) / s, (m[8] - m[2]) / s];
  } else {
    const s = Math.sqrt(1 + m[10] - m[0] - m[5]) * 2;
    q = [(m[8] + m[2]) / s, (m[9] + m[6]) / s, s / 4, (m[1] - m[4]) / s];
  }
  const length = Math.hypot(...q);
  return q.map((value) => value / length) as Quat;
}

/** Uniform-scale instance transform, using the same XYZ rotation convention as authoring. */
export function assemblyWorldMatrix(position: Vec3, rotation: Vec3, scale = 1): Float32Array {
  if (!Number.isFinite(scale) || scale <= 0) throw new Error("Assembly instance scale must be positive");
  const axes = [0, 1, 2].map((axis) => {
    const p: Vec3 = [0, 0, 0];
    p[axis] = scale;
    return rotateAssemblyPoint(p, rotation);
  }) as Frame["axes"];
  return matrixOf({ origin: position, axes });
}
export type AssemblyRenderPart = {
  id: string;
  mesh: MeshData;
  material?: string;
  matrix: Float32Array;
  dynamic: boolean;
};
/** Immutable part meshes are prepared once; playback changes transforms only. */
export class AssemblyMotion {
  private readonly parts: { part: AssemblyPart; mesh: MeshData; colliders: CharacterCollider[] }[];
  private commands: Record<string, number> = Object.create(null);
  private readonly dynamicParts = new Set<string>();
  private collisionInstances = new Map<string, { matrix: Float32Array; time: number; enabled: boolean }>();
  get byteLength(): number {
    return this.parts.reduce(
      (bytes, { mesh, colliders }) =>
        bytes +
        mesh.positions.byteLength +
        mesh.normals.byteLength +
        mesh.indices.byteLength +
        (mesh.colors?.byteLength ?? 0) +
        (mesh.materialCoordinates?.byteLength ?? 0) +
        (mesh.skyVisibility?.byteLength ?? 0) +
        (mesh.sourceIds?.length ?? 0) * 8 +
        colliders.length * 160,
      0,
    );
  }
  get meshes(): readonly MeshData[] {
    return this.parts.map((part) => part.mesh);
  }
  constructor(
    readonly source: AssemblyDefinition,
    mesh: MeshData,
  ) {
    if (!mesh.sourceIds) throw new Error("Assembly motion requires compiler part provenance");
    const byId = new Map(source.parts.map((part) => [part.id, part]));
    for (const part of source.parts) {
      let ancestor: AssemblyPart | undefined = part;
      const visited = new Set<string>();
      while (ancestor && !visited.has(ancestor.id)) {
        visited.add(ancestor.id);
        if (ancestor.joint) {
          this.dynamicParts.add(part.id);
          break;
        }
        ancestor = ancestor.parent ? byId.get(ancestor.parent) : undefined;
      }
    }
    const verticesByPart = new Map<string, number[]>();
    for (const vertex of mesh.indices) {
      const id = mesh.sourceIds[vertex];
      let vertices = verticesByPart.get(id);
      if (!vertices) {
        vertices = [];
        verticesByPart.set(id, vertices);
      }
      vertices.push(vertex);
    }
    this.parts = source.parts.map((part) => {
      const rest = frame(source, part),
        positions: number[] = [],
        normals: number[] = [],
        colors: number[] = [],
        materialCoordinates: number[] = [],
        skyVisibility: number[] = [],
        indices: number[] = [];
      const min: Vec3 = [Infinity, Infinity, Infinity],
        max: Vec3 = [-Infinity, -Infinity, -Infinity];
      for (const vertex of verticesByPart.get(part.id) ?? []) {
        const p = localPoint(
          [mesh.positions[vertex * 3], mesh.positions[vertex * 3 + 1], mesh.positions[vertex * 3 + 2]],
          rest,
        );
        const n: Vec3 = [
          mesh.normals[vertex * 3],
          mesh.normals[vertex * 3 + 1],
          mesh.normals[vertex * 3 + 2],
        ];
        indices.push(positions.length / 3);
        positions.push(...p);
        normals.push(...rest.axes.map((axis) => dot(axis, n)));
        if (mesh.colors) colors.push(...mesh.colors.slice(vertex * 3, vertex * 3 + 3));
        if (mesh.materialCoordinates)
          materialCoordinates.push(...mesh.materialCoordinates.subarray(vertex * 3, vertex * 3 + 3));
        if (mesh.skyVisibility) {
          const bent: Vec3 = [
            mesh.skyVisibility[vertex * 4],
            mesh.skyVisibility[vertex * 4 + 1],
            mesh.skyVisibility[vertex * 4 + 2],
          ];
          skyVisibility.push(...rest.axes.map((axis) => dot(axis, bent)), mesh.skyVisibility[vertex * 4 + 3]);
        }
        for (let axis = 0; axis < 3; axis++) {
          min[axis] = Math.min(min[axis], p[axis]);
          max[axis] = Math.max(max[axis], p[axis]);
        }
      }
      const perModule = positions.length / part.repeat.count;
      const colliders: CharacterCollider[] =
        part.collision === false
          ? []
          : Array.from({ length: part.repeat.count }, (_, repeat) => {
              const low: Vec3 = [Infinity, Infinity, Infinity],
                high: Vec3 = [-Infinity, -Infinity, -Infinity];
              for (let i = repeat * perModule; i < (repeat + 1) * perModule; i += 3)
                for (let axis = 0; axis < 3; axis++) {
                  low[axis] = Math.min(low[axis], positions[i + axis]);
                  high[axis] = Math.max(high[axis], positions[i + axis]);
                }
              return {
                id: `${part.id}-${repeat}`,
                shape: "box",
                rotation: [0, 0, 0],
                position: low.map((v, i) => (v + high[i]) / 2) as Vec3,
                size: low.map((v, i) => Math.max(0.005, (high[i] - v) / 2)) as Vec3,
              };
            });
      return {
        part,
        colliders,
        mesh: {
          positions: new Float32Array(positions),
          normals: new Float32Array(normals),
          indices: new Uint32Array(indices),
          colors: mesh.colors ? new Float32Array(colors) : undefined,
          materialCoordinates: mesh.materialCoordinates ? new Float32Array(materialCoordinates) : undefined,
          skyVisibility: mesh.skyVisibility ? new Float32Array(skyVisibility) : undefined,
          sourceIds: Array(indices.length).fill(part.id),
          bounds: { min, max },
        },
      };
    });
  }
  setJoint(id: string, value: number | null): void {
    const part = this.source.parts.find((p) => p.id === id);
    if (!part?.joint) throw new Error(`Unknown assembly joint ${id}`);
    if (value === null) delete this.commands[id];
    else {
      if (!Number.isFinite(value)) throw new Error("Joint command must be finite");
      this.commands[id] = Math.max(part.joint.minimum, Math.min(part.joint.maximum, value));
    }
  }
  snapshotJoints(): Record<string, number> {
    return { ...this.commands };
  }
  /** Validate the complete snapshot before replacing any currently active command. */
  restoreJoints(commands: Readonly<Record<string, number>>): void {
    const restored: Record<string, number> = Object.create(null);
    for (const [id, value] of Object.entries(commands)) {
      const joint = this.source.parts.find((part) => part.id === id)?.joint;
      if (!joint || !Number.isFinite(value) || value < joint.minimum || value > joint.maximum)
        throw new Error(`Invalid saved assembly joint ${id}`);
      restored[id] = value;
    }
    this.commands = restored;
  }
  evaluate(time: number, matrix: Float32Array): AssemblyRenderPart[] {
    const values = assemblyJointValues(this.source, time, this.commands);
    return this.parts.map(({ part, mesh }) => ({
      id: part.id,
      mesh,
      material: part.material,
      dynamic: this.dynamicParts.has(part.id),
      matrix: assemblyMatrixMultiply(matrix, matrixOf(frame(this.source, part, values))),
    }));
  }
  installCollision(physics: PhysicsAdapter, instance: string, matrix: Float32Array, time = 0): void {
    const installed: string[] = [],
      scale = Math.hypot(matrix[0], matrix[1], matrix[2]);
    try {
      for (const [index, rendered] of this.evaluate(time, matrix).entries()) {
        if (this.parts[index].part.collision === false) continue;
        const id = `${instance}/assembly/${rendered.id}`;
        physics.add({
          id,
          position: [rendered.matrix[12], rendered.matrix[13], rendered.matrix[14]],
          rotation: matrixQuat(rendered.matrix),
          mode: "kinematic",
          radius: 1,
          halfHeight: 1,
          mass: 1,
          restitution: 0,
          friction: 0.8,
          colliders: this.parts[index].colliders.map((c) =>
            c.shape === "box"
              ? {
                  ...c,
                  position: c.position.map((v) => v * scale) as Vec3,
                  size: c.size.map((v) => v * scale) as Vec3,
                }
              : c,
          ),
        });
        installed.push(id);
        physics.setBodyOwner(id, instance);
      }
      this.collisionInstances.set(instance, { matrix: matrix.slice(), time, enabled: true });
    } catch (error) {
      for (const id of installed) physics.remove(id);
      throw error;
    }
  }
  syncCollision(physics: PhysicsAdapter, instance: string, matrix: Float32Array, time: number): void {
    for (const [index, rendered] of this.evaluate(time, matrix).entries()) {
      if (this.parts[index].part.collision === false) continue;
      physics.targetKinematic(
        `${instance}/assembly/${rendered.id}`,
        [rendered.matrix[12], rendered.matrix[13], rendered.matrix[14]],
        matrixQuat(rendered.matrix),
      );
    }
  }
  /** Teleport is used for authored instance edits; fixed-tick drives use syncCollision. */
  teleportCollision(physics: PhysicsAdapter, instance: string, matrix: Float32Array, time: number): void {
    const previous = this.collisionInstances.get(instance);
    if (!previous?.enabled) {
      this.collisionInstances.set(instance, { matrix: matrix.slice(), time, enabled: false });
      return;
    }
    const oldScale = Math.hypot(previous.matrix[0], previous.matrix[1], previous.matrix[2]),
      newScale = Math.hypot(matrix[0], matrix[1], matrix[2]);
    if (Math.abs(oldScale - newScale) > 1e-6) {
      this.removeCollision(physics, instance);
      this.installCollision(physics, instance, matrix, time);
      return;
    }
    for (const [index, rendered] of this.evaluate(time, matrix).entries()) {
      if (this.parts[index].part.collision === false) continue;
      const id = `${instance}/assembly/${rendered.id}`,
        state = physics.state(id);
      physics.restoreBodyState(id, {
        ...state,
        position: [rendered.matrix[12], rendered.matrix[13], rendered.matrix[14]],
        rotation: matrixQuat(rendered.matrix),
        velocity: [0, 0, 0],
        angularVelocity: [0, 0, 0],
      });
    }
    this.collisionInstances.set(instance, { matrix: matrix.slice(), time, enabled: true });
  }
  setCollisionEnabled(
    physics: PhysicsAdapter,
    instance: string,
    enabled: boolean,
    matrix?: Float32Array,
    time?: number,
  ): void {
    const previous = this.collisionInstances.get(instance);
    if (previous?.enabled === enabled) return;
    if (!enabled) {
      this.removeCollision(physics, instance);
      return;
    }
    const transform = matrix ?? previous?.matrix;
    if (!transform) throw new Error("Enabling assembly collision requires an instance transform");
    this.installCollision(physics, instance, transform, time ?? previous?.time ?? 0);
  }
  removeCollision(physics: PhysicsAdapter, instance: string): void {
    for (const { part } of this.parts)
      if (part.collision !== false) physics.remove(`${instance}/assembly/${part.id}`);
    const previous = this.collisionInstances.get(instance);
    if (previous) previous.enabled = false;
  }
}
