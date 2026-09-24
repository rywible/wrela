import { packRadianceLighting, radianceLightingBytes, radianceRelightWGSL } from "./radiance-lighting";
import type { EvaluatedScene, IndirectLightingField, MeshData, RadianceLightingField, Vec3 } from "@wrela/model";

import { createAtmosphereWGSL } from "./atmosphere";
import sceneSource from "./scene.wgsl" with { type: "text" };

/** Match source bytes and absolute pose after LOD/realization. Dynamic, analytic,
 * displaced or edited meshes retain general segment visibility. A bounding box
 * or a reused object id is not sufficient to consume a surface proof. */
const originalTriangleMeshes = new WeakMap<MeshData, MeshData>();
export function withIndirectReceivers(scene: EvaluatedScene): EvaluatedScene {
  // A retained realized scene must recover its original geometry before testing
  // identities again, including removal, edits and a different field revision.
  if (scene.surfaces.some((s) => s.mesh.indirectProofs))
    scene = {
      ...scene,
      surfaces: scene.surfaces.map((s) => {
        if (!s.mesh.indirectProofs) return s;
        let original = originalTriangleMeshes.get(s.mesh);
        if (!original) {
          // A copied/edited realized mesh still carries compiler-owned data.
          // Strip that data while retaining the edit, then recheck eligibility.
          const { indirectProofs: _, ...mesh } = s.mesh;
          original = mesh;
          originalTriangleMeshes.set(s.mesh, original);
        }
        return { ...s, mesh: original };
      }),
    };
  const triangles = new Map(scene.indirectLighting?.triangleCache?.sources.map((s) => [s.id, s]));
  const sources = new Map(scene.indirectLighting?.receivers?.map((s) => [s.id, s]));
  const charts = new Map(
    scene.indirectLighting?.surfaceCache?.sources.map((s, i) => [s.id, { source: s, index: i + 1 }]),
  );
  if (!sources.size && !charts.size && !triangles.size)
    return scene.surfaces.some((s) => s.staticIndirectReceiver || s.indirectSurfaceChart)
      ? {
          ...scene,
          surfaces: scene.surfaces.map((s) => ({
            ...s,
            staticIndirectReceiver: false,
            indirectSurfaceChart: undefined,
          })),
        }
      : scene;
  return {
    ...scene,
    surfaces: scene.surfaces.map((surface) => {
      const chart = charts.get(surface.id),
        triangle = triangles.get(surface.id);
      const source = sources.get(surface.id) ?? chart?.source ?? triangle;
      const eligible =
        !!source &&
        !surface.skin &&
        !surface.deformation &&
        !surface.wind &&
        !surface.mesh.wind &&
        !surface.water &&
        !surface.reliefAppearance &&
        !surface.mesh.reliefCoordinates &&
        !surface.mesh.shoots &&
        !("thinCoverage" in surface.mesh) &&
        surface.castsShadow !== false &&
        surface.material.appearance?.family !== "glass" &&
        surface.material.appearance?.family !== "foliage" &&
        surface.lightingMobility !== "dynamic" &&
        (!surface.selectedRenderProduct || surface.selectedRenderProduct.kind === "direct-mesh") &&
        source.positions === surface.mesh.positions &&
        source.indices === surface.mesh.indices &&
        source.start === (surface.drawRange?.start ?? 0) &&
        source.count === (surface.drawRange?.count ?? surface.mesh.indices.length) &&
        source.matrix.every(
          (v, i) => v === surface.matrix[i] + (i >= 12 && i < 15 ? (scene.origin?.[i - 12] ?? 0) : 0),
        );
      const cacheEligible =
        eligible &&
        chart &&
        chart.source.normals === surface.mesh.normals &&
        !surface.material.appearance &&
        !surface.material.creature &&
        surface.material.normalStrength === 0 &&
        !surface.material.layers?.length;
      const triangleEligible =
        eligible &&
        triangle &&
        triangle.positions === surface.mesh.positions &&
        triangle.indices === surface.mesh.indices &&
        triangle.normals === surface.mesh.normals &&
        triangle.colors === surface.mesh.colors &&
        triangle.sourceIds === surface.mesh.sourceIds &&
        triangle.materialCoordinates === surface.mesh.materialCoordinates;
      if (triangleEligible) originalTriangleMeshes.set(triangle.mesh, surface.mesh);
      return {
        ...surface,
        ...(triangleEligible ? { mesh: triangle.mesh } : {}),
        staticIndirectReceiver: eligible && sources.has(surface.id),
        indirectSurfaceChart: cacheEligible ? chart.index : undefined,
      };
    }),
  };
}

export function indirectLightingBytes(field: IndirectLightingField): number {
  return (
    64 +
    field.data.byteLength +
    (field.surfaceCache?.data.byteLength ?? 0) +
    (field.reflections?.data.byteLength ?? 0) +
    (field.reflections?.transfer?.byteLength ?? 0) +
    (field.visibility?.nodes.byteLength ?? 0) +
    (field.visibility?.triangles.byteLength ?? 0) +
    (field.visibility?.cells?.byteLength ?? 0) +
    (field.transfer?.byteLength ?? 0)
  );
}

/** Validate addresses before GPU storage access, including bounded allocations
 * from imported or independently constructed field products. */
function validSurfaceCache(field: IndirectLightingField): boolean {
  const cache = field.surfaceCache;
  if (!cache) return true;
  const d = cache.data;
  if (d.length < 8 || d.length % 4 || d.byteLength > 16_000_000 || !d.every(Number.isFinite)) return false;
  const integer = (v: number, lo: number, hi: number) => Number.isInteger(v) && v >= lo && v <= hi;
  const patches = d[0],
    samples = d[1],
    weights = d[2],
    flags = d[3],
    metadata = d[4],
    output = d[5];
  if (
    !integer(patches, 0, 256) ||
    patches !== cache.sources.length ||
    !integer(samples, 0, 65536) ||
    weights !== 2 + patches * 4 ||
    flags !== weights + samples * 2 ||
    !integer(metadata, flags, d.length / 4) ||
    (output !== 0 && output !== metadata + samples) ||
    d.length / 4 !== (output ? output + samples * 10 : metadata)
  )
    return false;
  let first = 0,
    tile = 0;
  for (let i = 0; i < patches; i++) {
    const h = 8 + i * 16,
      nx = d[h + 3],
      ny = d[h + 7];
    if (!integer(nx, 2, 65536) || !integer(ny, 2, 65536) || d[h + 11] !== first || d[h + 15] !== tile)
      return false;
    first += nx * ny;
    tile += (nx - 1) * (ny - 1);
  }
  if (first !== samples || metadata !== flags + Math.ceil(tile / 4)) return false;
  for (let i = 0; i < samples; i++) {
    let sum = 0;
    for (let c = 0; c < 8; c++) {
      const w = d[weights * 4 + i * 8 + c];
      if (w < 0 || w > 1) return false;
      sum += w;
    }
    if (sum > 1.00001) return false;
    if (output) {
      const index = d[metadata * 4 + i * 4 + 3],
        dims = field.dimensions;
      if (
        !integer(index, 0, field.totalProbes - 1) ||
        index % dims[0] >= dims[0] - 1 ||
        Math.floor(index / dims[0]) % dims[1] >= dims[1] - 1 ||
        Math.floor(index / (dims[0] * dims[1])) >= dims[2] - 1
      )
        return false;
    }
  }
  for (let i = 0; i < tile; i++) if (d[flags * 4 + i] !== 0 && d[flags * 4 + i] !== 1) return false;
  return true;
}

/** Fixed-size header plus bounded probe payload. The shader needs a disabled
 * fallback buffer even in scenes without an irradiance field. */
export function packIndirectLighting(
  field?: IndirectLightingField,
  renderOrigin: Vec3 = [0, 0, 0],
  includeVisibility = true,
): Float32Array {
  if (!field) return new Float32Array(16);
  const total = field.dimensions.reduce((a, b) => a * b, 1);
  if (
    total > 2048 ||
    field.dimensions.some((v) => !Number.isInteger(v) || v < 2) ||
    field.data.length !== total * 60 ||
    ![...field.origin, ...field.spacing, ...renderOrigin].every(Number.isFinite) ||
    field.spacing.some((v) => v <= 0) ||
    !field.data.every(Number.isFinite) ||
    !validSurfaceCache(field) ||
    (field.transfer && (field.transfer.length !== total * 360 || !field.transfer.every(Number.isFinite))) ||
    (field.reflections &&
      (field.reflections.data.length !== total * 36 ||
        !field.reflections.data.every(Number.isFinite) ||
        !!field.reflections.transfer !== !!field.transfer ||
        (field.reflections.transfer &&
          (field.reflections.transfer.length !== total * 360 ||
            !field.reflections.transfer.every(Number.isFinite)))))
  )
    throw Error("Invalid bounded indirect field payload");
  const nodes = field.visibility?.nodes,
    triangles = field.visibility?.triangles,
    cells = field.visibility?.cells;
  if (
    includeVisibility &&
    nodes &&
    triangles &&
    (nodes.length % 8 ||
      triangles.length % 12 ||
      triangles.length / 12 > 1_000_000 ||
      !nodes.every(Number.isFinite) ||
      !triangles.every(Number.isFinite) ||
      (cells && (cells.length % 4 || !cells.every(Number.isFinite))))
  )
    throw Error("Invalid indirect visibility payload");
  const packed = new Float32Array(
    includeVisibility
      ? indirectLightingBytes(field) / 4
      : 16 +
          field.data.length +
          (field.reflections?.data.length ?? 0) +
          (field.surfaceCache?.data.length ?? 0),
  );
  packed.set(
    field.origin.map((v, i) => v - renderOrigin[i]),
    0,
  );
  packed[3] = (field.transfer ? 2 : 1) + (field.reflections ? 4 : 0) + (field.surfaceCache ? 8 : 0);
  packed.set(field.spacing, 4);
  packed.set(field.dimensions, 8);
  packed[11] = field.completedProbes === field.totalProbes ? 1 : 0;
  packed.set(field.data, 16);
  if (field.reflections) packed.set(field.reflections.data, 16 + field.data.length);
  if (field.surfaceCache)
    packed.set(field.surfaceCache.data, 16 + field.data.length + (field.reflections?.data.length ?? 0));
  if (field.transfer) {
    const offset =
      indirectLightingBytes(field) / 4 - field.transfer.length - (field.reflections?.transfer?.length ?? 0);
    packed[7] = offset / 4;
    if (includeVisibility) packed.set(field.transfer, offset);
    if (includeVisibility && field.reflections?.transfer)
      packed.set(field.reflections.transfer, offset + field.transfer.length);
  }
  if (nodes?.length && triangles?.length) {
    const nodeOffset =
        16 +
        field.data.length +
        (field.reflections?.data.length ?? 0) +
        (field.surfaceCache?.data.length ?? 0),
      triangleOffset = nodeOffset + nodes.length;
    const cellOffset = triangleOffset + triangles.length;
    packed.set(
      [nodeOffset / 4, triangleOffset / 4, nodes.length / 8, cells?.length ? cellOffset / 4 : 0],
      12,
    );
    if (includeVisibility) {
      packed.set(nodes, nodeOffset);
      packed.set(triangles, triangleOffset);
      if (cells) packed.set(cells, cellOffset);
    }
  }
  return packed;
}
export class IndirectLightingGpu {
  buffer: GPUBuffer;
  private key = "";
  private payloadKey = "";
  private relightKey = "";
  private relightPipeline?: GPUComputePipeline;
  private activeTransfer = false;
  private activeRadiance = false;
  private radianceEmissionScale = 1;
  private radiancePipeline?: GPUComputePipeline;
  private surfaceSamples = 0;
  private surfacePipeline?: GPUComputePipeline;
  get radianceReady() { return this.activeRadiance; }
  get bytes() {
    return this.buffer.size;
  }
  constructor(private device: GPUDevice) {
    this.buffer = device.createBuffer({
      label: "Disabled indirect lighting",
      size: 64,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });
  }
  update(
    field: IndirectLightingField | undefined,
    origin: Vec3 | undefined,
    availableBytes: number,
    radiance?: RadianceLightingField,
  ): { uploaded: number; changed: boolean; rejected?: string } {
    if (!field && radiance) {
      const admitted = radianceLightingBytes(radiance) <= availableBytes;
      const key = `${radiance.key}/${radiance.revision}/${origin?.join(",")}/${admitted}`;
      const gain = Math.fround(radiance.emissionScale ?? 1);
      if (!Number.isFinite(gain) || gain < 0) throw Error("Invalid radiance emission gain");
      if (key === this.key) {
        if (admitted && gain !== this.radianceEmissionScale) {
          this.device.queue.writeBuffer(this.buffer, 48, new Float32Array([gain]));
          this.radianceEmissionScale = gain;
          this.relightKey = "";
          return { uploaded: 4, changed: false };
        }
        return { uploaded: 0, changed: false };
      }
      const data = admitted ? packRadianceLighting(radiance, origin) : new Float32Array(16);
      const changed = this.buffer.size !== data.byteLength;
      if (changed) { this.buffer.destroy(); this.buffer = this.device.createBuffer({ label: "Compiled local radiance", size: data.byteLength, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST }); }
      this.device.queue.writeBuffer(this.buffer, 0, data as Float32Array<ArrayBuffer>);
      this.key = key; this.payloadKey = "radiance"; this.relightKey = "";
      this.radianceEmissionScale = gain;
      this.activeRadiance = admitted; this.activeTransfer = false; this.surfaceSamples = 0;
      return { uploaded: data.byteLength, changed, ...(!admitted ? { rejected: "Compiled radiance exceeds remaining GPU memory budget" } : {}) };
    }
    this.activeRadiance = false;
    const admitted = !field || indirectLightingBytes(field) <= availableBytes;
    const key = field
      ? `${field.key}/${field.revision}/${origin?.join(",") ?? "0,0,0"}/${admitted}`
      : "disabled";
    if (key === this.key) return { uploaded: 0, changed: false };
    this.activeTransfer = !!(admitted && field?.transfer);
    this.surfaceSamples = admitted && field?.surfaceCache?.data[5] ? field.surfaceCache.data[1] : 0;
    const payloadKey = admitted ? (field?.key ?? "disabled") : "disabled";
    const size = admitted && field ? indirectLightingBytes(field) : 64;
    const completeUpload = payloadKey !== this.payloadKey || this.buffer.size !== size;
    const accepted = packIndirectLighting(admitted ? field : undefined, origin, completeUpload);
    let changed = false;
    if (this.buffer.size !== size) {
      this.buffer.destroy();
      this.buffer = this.device.createBuffer({
        label: "Static diffuse irradiance field",
        size,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
      });
      changed = true;
    }
    this.device.queue.writeBuffer(this.buffer, 0, accepted as Float32Array<ArrayBuffer>);
    let transferUploaded = 0;
    if (!completeUpload && admitted && field?.transfer) {
      this.device.queue.writeBuffer(
        this.buffer,
        size - field.transfer.byteLength - (field.reflections?.transfer?.byteLength ?? 0),
        field.transfer as Float32Array<ArrayBuffer>,
      );
      transferUploaded = field.transfer.byteLength;
      if (field.reflections?.transfer) {
        this.device.queue.writeBuffer(
          this.buffer,
          size - field.reflections.transfer.byteLength,
          field.reflections.transfer as Float32Array<ArrayBuffer>,
        );
        transferUploaded += field.reflections.transfer.byteLength;
      }
    }
    // Uploads overwrite resolved coefficients/texels, including an off/on
    // toggle back to a previously seen key. They must always relight again.
    this.relightKey = "";
    this.key = key;
    this.payloadKey = payloadKey;
    return {
      uploaded: accepted.byteLength + transferUploaded,
      changed,
      ...(!admitted
        ? {
            rejected:
              "Indirect field exceeds remaining GPU memory budget; diffuse environment fallback retained.",
          }
        : {}),
    };
  }
  encodeRelight(
    encoder: GPUCommandEncoder,
    inputs: {
      key: string;
      timestamps?: GPUComputePassTimestampWrites;
      globals: GPUBuffer;
      atmosphere: GPUBuffer;
      sky: GPUBuffer;
      cloudShadow: GPUTextureView;
      sampler: GPUSampler;
    },
  ): boolean {
    if (this.activeRadiance) {
      const key = `${this.key}/${inputs.key}`;
      if (key === this.relightKey) return false;
      this.radiancePipeline ??= this.device.createComputePipeline({ label: "Relight compiled local radiance", layout: "auto", compute: { module: this.device.createShaderModule({ code: radianceRelightWGSL }), entryPoint: "relight" } });
      const pass = encoder.beginComputePass({ label: "Compiled local radiance relighting", ...(inputs.timestamps ? { timestampWrites: inputs.timestamps } : {}) });
      pass.setPipeline(this.radiancePipeline);
      pass.setBindGroup(0, this.device.createBindGroup({ layout: this.radiancePipeline.getBindGroupLayout(0), entries: [
        { binding: 0, resource: { buffer: inputs.globals } }, { binding: 4, resource: { buffer: inputs.atmosphere } },
        { binding: 11, resource: inputs.sampler }, { binding: 12, resource: { buffer: inputs.sky } },
        { binding: 18, resource: inputs.cloudShadow }, { binding: 22, resource: { buffer: this.buffer } },
      ] }));
      pass.dispatchWorkgroups(8, 9); pass.end(); this.relightKey = key; return true;
    }
    if (!this.activeTransfer && !this.surfaceSamples) return false;
    const key = `${this.key}/${this.activeTransfer ? inputs.key : "constant"}`;
    if (key === this.relightKey) return false;
    if (this.activeTransfer) {
      this.relightPipeline ??= this.device.createComputePipeline({
        label: "Relight compiled static sky transport",
        layout: "auto",
        compute: {
          module: this.device.createShaderModule({ code: indirectRelightWGSL }),
          entryPoint: "relight",
        },
      });
      const pass = encoder.beginComputePass({
        label: "Compiled diffuse transport relighting",
        ...(inputs.timestamps
          ? {
              timestampWrites: {
                querySet: inputs.timestamps.querySet,
                beginningOfPassWriteIndex: inputs.timestamps.beginningOfPassWriteIndex,
                ...(!this.surfaceSamples
                  ? { endOfPassWriteIndex: inputs.timestamps.endOfPassWriteIndex }
                  : {}),
              },
            }
          : {}),
      });
      pass.setPipeline(this.relightPipeline);
      pass.setBindGroup(
        0,
        this.device.createBindGroup({
          layout: this.relightPipeline.getBindGroupLayout(0),
          entries: [
            { binding: 0, resource: { buffer: inputs.globals } },
            { binding: 4, resource: { buffer: inputs.atmosphere } },
            { binding: 11, resource: inputs.sampler },
            { binding: 12, resource: { buffer: inputs.sky } },
            { binding: 18, resource: inputs.cloudShadow },
            { binding: 22, resource: { buffer: this.buffer } },
          ],
        }),
      );
      pass.dispatchWorkgroups(32, 18);
      pass.end();
    }
    if (this.surfaceSamples) {
      this.surfacePipeline ??= this.device.createComputePipeline({
        label: "Resolve compiled surface lighting",
        layout: "auto",
        compute: {
          module: this.device.createShaderModule({ code: surfaceLightingResolveWGSL }),
          entryPoint: "resolveSurface",
        },
      });
      const pass = encoder.beginComputePass({
        label: "Surface lighting cache resolve",
        ...(inputs.timestamps
          ? {
              timestampWrites: {
                querySet: inputs.timestamps.querySet,
                ...(!this.activeTransfer
                  ? { beginningOfPassWriteIndex: inputs.timestamps.beginningOfPassWriteIndex }
                  : {}),
                endOfPassWriteIndex: inputs.timestamps.endOfPassWriteIndex,
              },
            }
          : {}),
      });
      pass.setPipeline(this.surfacePipeline);
      pass.setBindGroup(
        0,
        this.device.createBindGroup({
          layout: this.surfacePipeline.getBindGroupLayout(0),
          entries: [{ binding: 0, resource: { buffer: this.buffer } }],
        }),
      );
      pass.dispatchWorkgroups(Math.ceil(this.surfaceSamples / 64));
      pass.end();
    }
    this.relightKey = key;
    return true;
  }
  dispose() {
    this.buffer.destroy();
  }
}

/** Low-order incident sky transfer, with the current atmospheric sun attenuation
 * evaluated at each probe. Cloud attenuation is a local-volume approximation. */
export const indirectRelightWGSL = `${sceneSource.slice(0, sceneSource.indexOf("struct Wave"))}
@group(0) @binding(0) var<uniform> g:Globals;
${createAtmosphereWGSL(0, 4)}
@group(0) @binding(22) var<storage,read_write> field:array<vec4f>;
@compute @workgroup_size(64) fn relight(@builtin(global_invocation_id) id:vec3u) {
 let dims=vec3u(field[2].xyz);let count=dims.x*dims.y*dims.z;
 let reflections=(u32(field[0].w)&4u)!=0u;
 if(id.x>=count||(id.y>=9u&&!reflections)||(u32(field[0].w)&2u)==0u){return;}
 let reflected=id.y>=9u;let band=id.y%9u;
 let output=select(4u+id.x*15u+band,4u+count*15u+id.x*9u+band,reflected);let valid=field[output].w;
 let start=u32(field[1].w)+id.x*90u+band*10u+select(0u,count*90u,reflected);
 var result=vec3f(0);
 for(var input=0u;input<9u;input++){
  let band=select(select(1.0,0.6666666667,input>0u),0.25,input>3u);
  result+=field[start+input].xyz*skyIrradiance[input].xyz*(g.horizon.w/band);
 }
 let c=vec3u(id.x%dims.x,(id.x/dims.x)%dims.y,id.x/(dims.x*dims.y));
 let probe=4u+id.x*15u;
 let point=field[0].xyz+vec3f(c)*field[1].xyz+vec3f(field[probe+9u].zw,field[probe+10u].z);
 let uv=(point.xz-floor(g.camera.xz/125.0)*125.0)/32000.0+0.5;
 var cloud=0.0;
 if(g.cloud.x>0.001&&g.skyCycle.x<=0.5&&all(uv>=vec2f(0))&&all(uv<=vec2f(1))){
  cloud=textureSampleLevel(physicalCloudShadowTexture,physicalAtmosphereSampler,uv,0.0).x;
 }
 let sun=g.sunlight.xyz*g.sun.w*physicalSunTransmissionAt(physicalPlanetPoint(point),normalize(g.sun.xyz))*(1.0-cloud*0.8);
 result+=field[start+9u].xyz*sun;
 field[output]=vec4f(result,valid);
}
`;

/** Per-sample gather runs after atmospheric probe relighting, outside fragment
 * shading. RGB SH retains a low-frequency reflection approximation; moving
 * the nonnegative angular clamp after blending is measured against the original. */
export const surfaceLightingResolveWGSL = `
@group(0) @binding(0) var<storage,read_write> field:array<vec4f>;
fn basis(n:vec3f,k:u32)->f32 {
 switch(k){
 case 0u:{return 0.2820947918;}case 1u:{return 0.4886025119*n.y;}case 2u:{return 0.4886025119*n.z;}case 3u:{return 0.4886025119*n.x;}
 case 4u:{return 1.0925484306*n.x*n.y;}case 5u:{return 1.0925484306*n.y*n.z;}case 6u:{return 0.3153915653*(3.0*n.z*n.z-1.0);}
 case 7u:{return 1.0925484306*n.x*n.z;}default:{return 0.5462742153*(n.x*n.x-n.y*n.y);}
 }
}
@compute @workgroup_size(64) fn resolveSurface(@builtin(global_invocation_id) id:vec3u){
 let dims=vec3u(field[2].xyz);let count=dims.x*dims.y*dims.z;let reflections=(u32(field[0].w)&4u)!=0u;
 let start=4u+count*15u+select(0u,count*9u,reflections);let header=field[start];let extra=field[start+1u];
 if(id.x>=u32(header.y)){return;}
 let receiver=field[start+u32(extra.x)+id.x];let normal=receiver.xyz;let first=u32(receiver.w);
 let weights=start+u32(header.z)+id.x*2u;let output=start+u32(extra.y)+id.x*10u;
 var diffuse=vec3f(0);var sky=0.0;var reflected:array<vec4f,9>;
 for(var corner=0u;corner<8u;corner++){
   let weight=field[weights+corner/4u][corner%4u];if(weight<=0.0000000001){continue;}
   let probe=first+(corner&1u)+dims.x*((corner>>1u)&1u)+dims.x*dims.y*((corner>>2u)&1u);
   var irradiance=vec3f(0);
   for(var band=0u;band<9u;band++){
     irradiance+=field[4u+probe*15u+band].xyz*basis(normal,band);
     if(reflections){reflected[band]+=field[4u+count*15u+probe*9u+band]*weight;}
   }
   diffuse+=max(irradiance,vec3f(0))*weight;
   if(reflections){sky+=clamp(field[4u+count*15u+probe*9u].w*0.2820947918,0.0,1.0)*weight;}
 }
 field[output]=vec4f(diffuse,select(1.0,sky,reflections));
 for(var band=0u;band<9u;band++){field[output+1u+band]=reflected[band];}
}
`;
