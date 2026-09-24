import type { Bounds, EvaluatedScene, RenderEnvironment, RenderSurface, Vec3 } from "@wrela/model";

import { lookAt, multiply, perspective } from "./math";
import { GLOBAL_FLOATS } from "./packing";
import { surfaceBounds } from "./visibility";
export type LocalLight = NonNullable<RenderEnvironment["pointLights"]>[number];
export function lightIntersectsBounds(bounds: Bounds, position: Vec3, radius: number): boolean {
  if (![...bounds.min, ...bounds.max, ...position, radius].every(Number.isFinite)) return true;
  let squared = 0;
  for (let axis = 0; axis < 3; axis++) {
    const d = Math.max(0, bounds.min[axis] - position[axis], position[axis] - bounds.max[axis]);
    squared += d * d;
  }
  return squared <= radius * radius;
}
/** An authored compact support makes whole-draw rejection exact. Unbounded
 * legacy lights are always retained; no arbitrary attenuation cutoff is hidden. */
export function compileLightMask(bounds: Bounds, lights: readonly LocalLight[]): number {
  return lights
    .slice(0, 8)
    .reduce(
      (mask, light, index) =>
        mask | (!light.range || lightIntersectsBounds(bounds, light.position, light.range) ? 1 << index : 0),
      0,
    );
}
const directions: Vec3[] = [
  [1, 0, 0],
  [-1, 0, 0],
  [0, 1, 0],
  [0, -1, 0],
  [0, 0, 1],
  [0, 0, -1],
];
/** Conservative AABB/cube-face rejection. A caster entirely behind a face or
 * outside its 90-degree cone cannot contribute to that face's depth map. */
export function pointShadowFaceIntersectsBounds(bounds: Bounds, position: Vec3, face: number): boolean {
  const axis = Math.floor(face / 2),
    sign = face % 2 ? -1 : 1;
  const far = sign > 0 ? bounds.max[axis] - position[axis] : position[axis] - bounds.min[axis];
  if (far <= 0) return false;
  for (let a = 0; a < 3; a++)
    if (a !== axis) {
      const nearest = Math.max(0, bounds.min[a] - position[a], position[a] - bounds.max[a]);
      if (nearest > far) return false;
    }
  return true;
}
const MAX_SHADOW_LIGHTS = 8;
const SHADOW_FACES = MAX_SHADOW_LIGHTS * 6;
export const POINT_SHADOW_PARAMETER_FLOATS = MAX_SHADOW_LIGHTS * 6 * 16 + 8 * 4;
export function pointShadowMatrices(position: Vec3, far: number): Float32Array[] {
  return directions.map((d) =>
    multiply(perspective(90, 1, 0.03, far), lookAt(position, position.map((v, a) => v + d[a]) as Vec3)),
  );
}
export type PointShadowPlan = {
  index: number;
  slot: number;
  far: number;
  casters: Set<RenderSurface>;
  matrices: Float32Array[];
  key: string;
  faceKeys: string[];
  faceCasters: Set<RenderSurface>[];
};
/** Geometry-dependent depth products survive stationary frames. Wind, skin,
 * deformation, transforms, coverage, source edits and origin changes invalidate. */
export class PointShadowsGpu {
  lastPasses = 0;
  lastDrawCalls = 0;
  texture: GPUTexture;
  view: GPUTextureView;
  readonly parameters: GPUBuffer;
  readonly bufferBytes: number;
  private faces: GPUTextureView[];
  private capacity = MAX_SHADOW_LIGHTS;
  private slots = new Map<number, number>();
  resolution: number;
  private readonly globals: GPUBuffer[];
  private readonly groups: GPUBindGroup[];
  private readonly keys = new Map<number, string>();
  private readonly ids = new WeakMap<object, number>();
  private nextId = 1;
  private casterStates = new Map<string, { key: string; revision: number }>();
  private casterRevision = 0;
  constructor(
    private device: GPUDevice,
    layout: GPUBindGroupLayout,
    private readonly baseResolution: number,
  ) {
    this.resolution = baseResolution;
    const resolution = baseResolution;
    this.texture = device.createTexture({
      label: "Cached local-light shadows",
      size: [resolution, resolution, SHADOW_FACES],
      format: "depth32float",
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
    });
    this.view = this.texture.createView({ dimension: "2d-array" });
    this.faces = Array.from({ length: SHADOW_FACES }, (_, layer) =>
      this.texture.createView({ dimension: "2d", baseArrayLayer: layer, arrayLayerCount: 1 }),
    );
    this.parameters = device.createBuffer({
      label: "Local-light shadow matrices",
      size: POINT_SHADOW_PARAMETER_FLOATS * 4,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    this.globals = Array.from({ length: SHADOW_FACES }, () =>
      device.createBuffer({
        label: "Local shadow frame",
        size: GLOBAL_FLOATS * 4,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
      }),
    );
    this.groups = this.globals.map((buffer) =>
      device.createBindGroup({ layout, entries: [{ binding: 0, resource: { buffer } }] }),
    );
    this.bufferBytes = this.parameters.size + this.globals.reduce((n, b) => n + b.size, 0);
  }
  get byteLength(): number {
    return this.resolution ** 2 * this.capacity * 6 * 4 + this.bufferBytes;
  }
  /** Redistribute the eight-light texel budget. Two lights get twice the linear
   * resolution; no light loses coverage and stationary maps remain cached. */
  private allocate(active: number[]) {
    const capacity = active.length <= 2 ? 2 : active.length <= 4 ? 4 : MAX_SHADOW_LIGHTS;
    if (capacity !== this.capacity) {
      const resolution =
        Math.floor((this.baseResolution * Math.sqrt(MAX_SHADOW_LIGHTS / capacity)) / 16) * 16;
      const texture = this.device.createTexture({
        label: "Cached local-light shadows",
        size: [resolution, resolution, capacity * 6],
        format: "depth32float",
        usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
      });
      const previous = this.texture;
      this.texture = texture;
      this.view = texture.createView({ dimension: "2d-array" });
      this.faces = Array.from({ length: capacity * 6 }, (_, layer) =>
        texture.createView({ dimension: "2d", baseArrayLayer: layer, arrayLayerCount: 1 }),
      );
      this.capacity = capacity;
      this.resolution = resolution;
      this.keys.clear();
      this.slots.clear();
      previous.destroy();
    }
    for (const index of this.slots.keys()) if (!active.includes(index)) this.slots.delete(index);
    for (const index of active) {
      if (this.slots.has(index)) continue;
      const used = new Set(this.slots.values());
      let slot = this.capacity === MAX_SHADOW_LIGHTS ? index : 0;
      while (used.has(slot)) slot++;
      this.slots.set(index, slot);
    }
  }
  private id(object: object) {
    let id = this.ids.get(object);
    if (id === undefined) {
      id = this.nextId++;
      this.ids.set(object, id);
    }
    return id;
  }
  prepare(scene: EvaluatedScene, lights: readonly LocalLight[]): PointShadowPlan[] {
    if (
      (scene.mode !== "beauty" && scene.mode !== "clay") ||
      !lights.some((light) => light.intensity > 0 && light.shadows !== false)
    )
      return [];
    const active = lights
      .slice(0, MAX_SHADOW_LIGHTS)
      .flatMap((light, index) => (light.intensity > 0 && light.shadows !== false ? [index] : []));
    this.allocate(active);
    const bounds = new Map(
      scene.surfaces
        .filter((s) => !s.water && s.castsShadow !== false)
        .map((s) => [s, surfaceBounds(s, scene.environment)]),
    );
    const revisions = new Map<RenderSurface, number>();
    const live = new Set<string>();
    for (const s of bounds.keys()) {
      live.add(s.id);
      const key = JSON.stringify([
        s.id,
        this.id(s.mesh.positions),
        this.id(s.mesh.indices),
        s.mesh.thinCoverage?.key,
        s.mesh.shoots ? this.id(s.mesh.shoots.transforms) : null,
        s.shootSelection ? this.id(s.shootSelection) : null,
        s.drawRange,
        s.shadowDrawRange,
        ...s.matrix,
        s.skin ? Array.from(s.skin.matrices) : null,
        s.deformation ? `${s.deformation.revision}/${scene.time}` : null,
        s.wind ? [s.wind, ...scene.environment.wind, scene.environment.windPhase, scene.time] : null,
      ]);
      let state = this.casterStates.get(s.id);
      if (state?.key !== key) {
        state = { key, revision: ++this.casterRevision };
        this.casterStates.set(s.id, state);
      }
      revisions.set(s, state.revision);
    }
    for (const id of this.casterStates.keys()) if (!live.has(id)) this.casterStates.delete(id);
    const plans: PointShadowPlan[] = [];
    // Stable light slots preserve shadow reuse when the camera moves. Every
    // supported local light has coverage; priority must never turn a wall transparent.
    for (let index = 0; index < Math.min(lights.length, MAX_SHADOW_LIGHTS); index++) {
      const light = lights[index];
      if (light.shadows === false || light.intensity <= 0) continue;
      const far = Math.max(
        0.1,
        light.range ??
          Math.max(
            80,
            ...[...bounds.values()].map((b) =>
              Math.hypot(
                ...b.min.map((v, a) =>
                  Math.max(Math.abs(v - light.position[a]), Math.abs(b.max[a] - light.position[a])),
                ),
              ),
            ),
          ),
      );
      const casters = new Set(
        [...bounds].filter(([, b]) => lightIntersectsBounds(b, light.position, far)).map(([s]) => s),
      );
      const geometry = [...casters].map((s) => revisions.get(s));
      const key = JSON.stringify([light.position, far, scene.origin, geometry]);
      const casterList = [...casters];
      const faceCasters = Array.from(
        { length: 6 },
        (_, face) =>
          new Set(
            casterList.filter((s) => {
              const bound = bounds.get(s);
              return !bound || pointShadowFaceIntersectsBounds(bound, light.position, face);
            }),
          ),
      );
      const faceKeys = faceCasters.map((set) =>
        JSON.stringify([
          light.position,
          far,
          scene.origin,
          geometry.filter((_, i) => set.has(casterList[i])),
        ]),
      );
      const slot = this.slots.get(index);
      if (slot === undefined) throw Error("Active light has no shadow allocation");
      plans.push({
        index,
        slot,
        far,
        casters,
        matrices: pointShadowMatrices(light.position, far),
        key,
        faceKeys,
        faceCasters,
      });
    }
    return plans;
  }
  encode(
    encoder: GPUCommandEncoder,
    plans: PointShadowPlan[],
    frame: Float32Array,
    complete: boolean,
    draw: (pass: GPURenderPassEncoder, casters: Set<RenderSurface>) => number,
  ): number {
    this.lastPasses = 0;
    this.lastDrawCalls = 0;
    const params = new Float32Array(POINT_SHADOW_PARAMETER_FLOATS);
    let uploaded = params.byteLength;
    if (complete)
      for (const plan of plans) {
        const { slot, index, matrices } = plan;
        matrices.forEach((matrix, face) => {
          params.set(matrix, (slot * 6 + face) * 16);
        });
        params.set([slot + 1, 0.03, plan.far, 0], SHADOW_FACES * 16 + index * 4);
        for (let face = 0; face < 6; face++) {
          if (this.keys.get(slot * 6 + face) === plan.faceKeys[face]) continue;
          const data = frame.slice();
          data.set(matrices[face], 16);
          this.device.queue.writeBuffer(this.globals[slot * 6 + face], 0, data);
          uploaded += data.byteLength;
          const pass = encoder.beginRenderPass({
            label: "Local-light shadow face",
            colorAttachments: [],
            depthStencilAttachment: {
              view: this.faces[slot * 6 + face],
              depthClearValue: 1,
              depthLoadOp: "clear",
              depthStoreOp: "store",
            },
          });
          pass.setBindGroup(0, this.groups[slot * 6 + face]);
          this.lastDrawCalls += draw(pass, plan.faceCasters[face]);
          pass.end();
          this.lastPasses++;
          this.keys.set(slot * 6 + face, plan.faceKeys[face]);
        }
      }
    this.device.queue.writeBuffer(this.parameters, 0, params);
    return uploaded;
  }
  invalidate() {
    this.keys.clear();
    this.casterStates.clear();
  }
  destroy() {
    this.invalidate();
    this.texture.destroy();
    this.parameters.destroy();
    for (const b of this.globals) b.destroy();
  }
}
export const localLightingWGSL = /* wgsl */ `
struct LocalShadowData { matrices:array<mat4x4f,48>, lights:array<vec4f,8> };
@group(0) @binding(22) var localShadowMap:texture_depth_2d_array;
@group(0) @binding(23) var<uniform> localShadows:LocalShadowData;
fn localLightAttenuation(distanceSquared:f32,rangeSquared:f32)->f32 {
 var window=1.0;
 if(rangeSquared>0.0){window=pow(max(0.0,1.0-pow(distanceSquared/rangeSquared,2.0)),2.0);}
 return window/(1.0+distanceSquared);
}
fn localLightVisibility(index:u32,world:vec3f,n:vec3f)->f32 {
 let info=localShadows.lights[index];if(info.x<0.5||LIGHTING_ABLATION==4u){return 1.0;}
 let offset=world-g.points[index].position.xyz;let distance=length(offset);
 if(distance<=info.y||distance>=info.z){return 1.0;}
 let a=abs(offset);var face=select(0u,1u,offset.x<0.0);
 if(a.y>a.x&&a.y>=a.z){face=select(2u,3u,offset.y<0.0);}
 else if(a.z>a.x&&a.z>a.y){face=select(4u,5u,offset.z<0.0);}
 let layer=(u32(info.x)-1u)*6u+face;
 let p=localShadows.matrices[layer]*vec4f(world+n*(0.001+distance*0.0001),1.0);
 let uv=p.xy/p.w*vec2f(0.5,-0.5)+0.5;let texel=1.0/vec2f(textureDimensions(localShadowMap));
 var visibility=0.0;
 for(var y=-1;y<=1;y++){for(var x=-1;x<=1;x++){
  visibility+=textureSampleCompareLevel(localShadowMap,shadowSampler,clamp(uv+vec2f(f32(x),f32(y))*texel,texel*0.5,vec2f(1)-texel*0.5),i32(layer),p.z/p.w-0.00002);
 }}
 return visibility/9.0;
}
`;
