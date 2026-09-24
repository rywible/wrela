import type { EvaluatedScene, MeshData, RenderSurface } from "@wrela/model";

import { cameraBasis } from "./math";

/** Source occurrences persist through selection, so neighboring shoots can use
 * different detail without duplicating geometry or discarding a shadow caster. */
export class ShootSelector {
  private history = new Map<
    string,
    {
      source: MeshData;
      camera: Uint8Array;
      light: Uint8Array;
      seen: number;
      near?: RenderSurface["shootSelection"];
      far?: RenderSurface["shootSelection"];
    }
  >();
  private geometry = new WeakMap<MeshData, Float64Array>();
  private geometryFor(mesh: MeshData): Float64Array {
    const cached = this.geometry.get(mesh);
    if (cached) return cached;
    const shoots = mesh.shoots;
    if (!shoots) throw Error("Missing shoot instances");
    const bounds = shoots.templateBounds;
    const center = bounds.min.map((v, a) => (v + bounds.max[a]) / 2),
      extent = bounds.max.map((v, a) => (v - bounds.min[a]) / 2);
    const out = new Float64Array(shoots.sourceIds.length * 5 + 2),
      m = shoots.transforms;
    for (let i = 0; i < shoots.sourceIds.length; i++) {
      const at = i * 16,
        q = i * 5;
      for (let a = 0; a < 3; a++)
        out[q + a] =
          m[at + a] * center[0] + m[at + 4 + a] * center[1] + m[at + 8 + a] * center[2] + m[at + 12 + a];
      let radius = 0;
      for (let corner = 0; corner < 8; corner++) {
        const x = extent[0] * (corner & 1 ? 1 : -1),
          y = extent[1] * (corner & 2 ? 1 : -1),
          z = extent[2] * (corner & 4 ? 1 : -1);
        radius = Math.max(
          radius,
          Math.hypot(
            m[at] * x + m[at + 4] * y + m[at + 8] * z,
            m[at + 1] * x + m[at + 5] * y + m[at + 9] * z,
            m[at + 2] * x + m[at + 6] * y + m[at + 10] * z,
          ),
        );
      }
      out[q + 3] = radius;
      // The vertex deformation scales travel by the composed model's first
      // column. Surface scale is applied later; include this occurrence now.
      const occurrenceScale = Math.hypot(m[at], m[at + 1], m[at + 2]);
      out[q + 4] = occurrenceScale * (0.6 + 0.25 * shoots.motion[i * 4] + 0.035 * shoots.motion[i * 4 + 1]);
      out[out.length - 2] = Math.max(out[out.length - 2], radius);
      out[out.length - 1] = Math.max(out[out.length - 1], out[q + 4]);
    }
    this.geometry.set(mesh, out);
    return out;
  }
  private frame = 0;
  begin() {
    this.frame++;
  }
  select(
    scene: EvaluatedScene,
    surface: RenderSurface,
    height: number,
    shadowTexel: number,
    aspect?: number,
  ): RenderSurface[] {
    const shoots = surface.mesh.shoots,
      detail = surface.details?.find((d) => d.label === "projected-shoot");
    if (!shoots || !detail) return [surface];
    const count = shoots.sourceIds.length,
      previous = this.history.get(surface.id);
    const state: NonNullable<ReturnType<typeof this.history.get>> =
      previous?.source === surface.mesh
        ? previous
        : {
            source: surface.mesh,
            camera: new Uint8Array(count),
            light: new Uint8Array(count),
            seen: this.frame,
          };
    state.seen = this.frame;
    this.history.set(surface.id, state);
    const near: number[] = [],
      far: number[] = [],
      nearMask: number[] = [],
      farMask: number[] = [];
    const m = surface.matrix,
      geometry = this.geometryFor(surface.mesh),
      basis = cameraBasis(scene.camera),
      forward = basis.forward;
    const plane = (v: number[]) => [
      v[0] * m[0] + v[1] * m[1] + v[2] * m[2],
      v[0] * m[4] + v[1] * m[5] + v[2] * m[6],
      v[0] * m[8] + v[1] * m[9] + v[2] * m[10],
      v[0] * (m[12] - scene.camera.position[0]) +
        v[1] * (m[13] - scene.camera.position[1]) +
        v[2] * (m[14] - scene.camera.position[2]),
    ];
    const depthPlane = plane(forward),
      rightPlane = plane(basis.right),
      upPlane = plane(basis.up);
    const tangent = Math.tan((scene.camera.fov * Math.PI) / 360),
      tangentX = tangent * (aspect ?? 1);
    const marginY = Math.hypot(1, tangent),
      marginX = Math.hypot(1, tangentX);
    // sqrt(max row sum of M^T M) bounds the largest singular value,
    // including shear; largest column length alone is not conservative.
    const columns = [
      [m[0], m[1], m[2]],
      [m[4], m[5], m[6]],
      [m[8], m[9], m[10]],
    ];
    const scale = Math.sqrt(
      Math.max(
        ...columns.map((a) =>
          columns.reduce((sum, b) => sum + Math.abs(a[0] * b[0] + a[1] * b[1] + a[2] * b[2]), 0),
        ),
      ),
    );
    const wind =
      (scale *
        Math.min(2, Math.max(0, surface.wind ?? 0)) *
        Math.min(10, Math.hypot(...scene.environment.wind))) /
      10;
    const focal = height / (2 * tangent);
    // Prove a complete template group is below both pass thresholds before
    // traversing individual shoots. This is a bound, not a periodic LOD update.
    const bounds = surface.mesh.bounds;
    let minimumDepth = depthPlane[3];
    for (let a = 0; a < 3; a++)
      minimumDepth += depthPlane[a] * (depthPlane[a] >= 0 ? bounds.min[a] : bounds.max[a]);
    const largest = scale * geometry[geometry.length - 2],
      envelope = wind * geometry[geometry.length - 1];
    // A finite local light cannot see casters outside its authored support.
    // Test the complete group's conservative transformed sphere first, then
    // the individual shoot below. Merely having a lamp somewhere in the world
    // must not force every tree into explicit-needle shadow geometry.
    const center = bounds.min.map((v, a) => (v + bounds.max[a]) * 0.5);
    const worldCenter = [0, 1, 2].map(
      (a) => m[a] * center[0] + m[4 + a] * center[1] + m[8 + a] * center[2] + m[12 + a],
    );
    const groupRadius = scale * Math.hypot(...bounds.max.map((v, a) => v - center[a])) + envelope;
    const localLights = (scene.environment.pointLights ?? []).filter(
      (light) =>
        light.intensity > 0 &&
        light.color.some((v) => v > 0) &&
        (!light.range ||
          worldCenter.reduce((sum, v, a) => sum + (v - light.position[a]) ** 2, 0) <=
            (groupRadius + light.range) ** 2),
    );
    if (
      !surface.shootSelection &&
      !localLights.length &&
      minimumDepth > envelope + largest &&
      (2 * largest * focal) / (minimumDepth - envelope - largest) < detail.maxProjectedDiameter * 0.85 &&
      shadowTexel > 0 &&
      (2 * largest) / shadowTexel < detail.maxProjectedDiameter * 0.85
    ) {
      state.camera.fill(1);
      state.light.fill(1);
      if (state.far?.indices.length !== count || state.far.masks.some((v) => v !== 3))
        state.far = {
          indices: Uint32Array.from({ length: count }, (_, i) => i),
          masks: new Uint8Array(count).fill(3),
        };
      return [
        {
          ...surface,
          id: `${surface.id}/projected`,
          mesh: detail.mesh,
          details: undefined,
          shootSelection: state.far,
        },
      ];
    }
    for (let selected = 0; selected < (surface.shootSelection?.indices.length ?? count); selected++) {
      const i = surface.shootSelection?.indices[selected] ?? selected;
      const q = i * 5,
        x = geometry[q],
        y = geometry[q + 1],
        z = geometry[q + 2];
      const radius = scale * geometry[q + 3];
      const depth = depthPlane[0] * x + depthPlane[1] * y + depthPlane[2] * z + depthPlane[3];
      // The conservative motion envelope is used for distance uncertainty, not
      // mistaken for geometric detail which would make a tiny twig a huge LOD.
      const motion = wind * geometry[q + 4];
      const diameter = depth > radius + motion ? (2 * radius * focal) / (depth - radius - motion) : Infinity;
      const threshold = detail.maxProjectedDiameter;
      state.camera[i] = Number(diameter < threshold * (state.camera[i] ? 1.15 : 0.85));
      state.light[i] = Number(
        shadowTexel > 0 && (2 * radius) / shadowTexel < threshold * (state.light[i] ? 1.15 : 0.85),
      );
      // The shared shadow packet still uses explicit needles wherever any
      // active local light could reach this shoot, including its wind envelope.
      if (state.light[i] && localLights.length) {
        const wx = m[0] * x + m[4] * y + m[8] * z + m[12],
          wy = m[1] * x + m[5] * y + m[9] * z + m[13],
          wz = m[2] * x + m[6] * y + m[10] * z + m[14];
        if (
          localLights.some(
            (light) =>
              !light.range ||
              (wx - light.position[0]) ** 2 + (wy - light.position[1]) ** 2 + (wz - light.position[2]) ** 2 <=
                (light.range + radius + motion) ** 2,
          )
        )
          state.light[i] = 0;
      }
      const expanded = radius + motion;
      const cameraVisible =
        !aspect ||
        (depth + expanded >= 0 &&
          Math.abs(rightPlane[0] * x + rightPlane[1] * y + rightPlane[2] * z + rightPlane[3]) <=
            depth * tangentX + expanded * marginX &&
          Math.abs(upPlane[0] * x + upPlane[1] * y + upPlane[2] * z + upPlane[3]) <=
            depth * tangent + expanded * marginY);
      const mask = (state.camera[i] || !cameraVisible ? 0 : 1) | (state.light[i] ? 0 : 2);
      const projectedMask = (state.camera[i] && cameraVisible ? 1 : 0) | (state.light[i] ? 2 : 0);
      if (mask) {
        near.push(i);
        nearMask.push(mask);
      }
      if (projectedMask) {
        far.push(i);
        farMask.push(projectedMask);
      }
    }
    const intern = (old: RenderSurface["shootSelection"], indices: number[], masks: number[]) =>
      old &&
      old.indices.length === indices.length &&
      indices.every((v, i) => v === old.indices[i] && masks[i] === old.masks[i])
        ? old
        : { indices: new Uint32Array(indices), masks: new Uint8Array(masks) };
    state.near = intern(state.near, near, nearMask);
    state.far = intern(state.far, far, farMask);
    const result: RenderSurface[] = [];
    if (near.length)
      result.push({
        ...surface,
        id: `${surface.id}/resolved`,
        details: undefined,
        shootSelection: state.near,
      });
    if (far.length)
      result.push({
        ...surface,
        id: `${surface.id}/projected`,
        mesh: detail.mesh,
        details: undefined,
        shootSelection: state.far,
      });
    return result;
  }
  end() {
    for (const [id, value] of this.history) if (this.frame - value.seen > 120) this.history.delete(id);
    while (this.history.size > 4096) {
      const oldest = this.history.keys().next().value;
      if (oldest === undefined) break;
      this.history.delete(oldest);
    }
  }
  clear() {
    this.history.clear();
  }
}
