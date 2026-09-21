/** Hardware experiment: the production mesher's river stone vs exact analytic
 * lowering, with identical instancing, lighting, depth, camera, and viewport.
 * This isolates geometry realization; it is not a complete-world FPS claim. */
import { extractSurface } from "@wrela/compiler";
import { type Camera, type FieldDefinition, referenceProject } from "@wrela/model";
import { cameraRay, lookAt, multiply, perspective } from "../../packages/render-webgpu/src/math";
import { apply, lower } from "./local-program";

const code = /* wgsl */ `
struct Globals { vp:mat4x4f, invShape:mat4x4f, eye:vec4f, boundsMin:vec4f, boundsMax:vec4f };
struct Instance { shift:vec4f, rotation:vec4f };
@group(0) @binding(0) var<uniform> g:Globals;
@group(0) @binding(1) var<storage,read> instances:array<Instance>;
struct Varying { @builtin(position) clip:vec4f, @location(0) world:vec3f, @location(1) normal:vec3f, @location(2) @interpolate(flat) id:u32 };
fn rotate(v:vec3f,c:f32,s:f32)->vec3f {return vec3f(c*v.x+s*v.z,v.y,-s*v.x+c*v.z);}
fn worldPoint(p:vec3f,i:Instance)->vec3f {return rotate(p*i.shift.w,i.rotation.x,i.rotation.y)+i.shift.xyz;}
@vertex fn meshVertex(@location(0) p:vec3f,@location(1) n:vec3f,@builtin(instance_index) id:u32)->Varying {
  let i=instances[id]; var out:Varying; out.world=worldPoint(p,i); out.normal=rotate(n,i.rotation.x,i.rotation.y); out.clip=g.vp*vec4f(out.world,1); out.id=id; return out;
}
@vertex fn boxVertex(@builtin(vertex_index) vertex:u32,@builtin(instance_index) id:u32)->Varying {
  // Outward winding on the six faces of a conservative extraction bound.
  let corners=array<vec3f,8>(vec3f(0,0,0),vec3f(1,0,0),vec3f(1,1,0),vec3f(0,1,0),vec3f(0,0,1),vec3f(1,0,1),vec3f(1,1,1),vec3f(0,1,1));
  let ids=array<u32,36>(0,2,1,0,3,2,4,5,6,4,6,7,0,1,5,0,5,4,3,7,6,3,6,2,0,4,7,0,7,3,1,2,6,1,6,5);
  var out:Varying; out.world=worldPoint(mix(g.boundsMin.xyz,g.boundsMax.xyz,corners[ids[vertex]]),instances[id]); out.normal=vec3f(0,1,0); out.clip=g.vp*vec4f(out.world,1); out.id=id; return out;
}
fn shade(p:vec3f,normal:vec3f)->vec4f {
  let n=normalize(normal); let l=normalize(vec3f(-0.3,0.8,0.7)); let view=normalize(g.eye.xyz-p); let h=normalize(l+view);
  let base=mix(vec3f(0.055,0.17,0.19),vec3f(0.35,0.7,0.65),clamp(n.y*0.5+0.5,0.0,1.0));
  let rgb=base*(0.22+max(dot(n,l),0.0)*0.85)+vec3f(0.85,0.91,1)*pow(max(dot(n,h),0.0),90.0)*0.45;
  return vec4f(pow(rgb,vec3f(1.0/2.2)),1);
}
struct Hit { p:vec3f,n:vec3f };
fn analytic(v:Varying)->Hit {
  let i=instances[v.id]; let ray=normalize(v.world-g.eye.xyz);
  let eyeLocal=rotate((g.eye.xyz-i.shift.xyz)/i.shift.w,i.rotation.x,-i.rotation.y);
  let directionLocal=rotate(ray/i.shift.w,i.rotation.x,-i.rotation.y);
  let o=(g.invShape*vec4f(eyeLocal,1)).xyz;
  let d=(g.invShape*vec4f(directionLocal,0)).xyz;
  let a=dot(d,d); let closest=-dot(o,d)/a; let perpendicular=o+closest*d;
  let discriminant=1.0-dot(perpendicular,perpendicular);
  if(discriminant<0.0){discard;}
  let halfChord=sqrt(max(discriminant,0.0)/a);
  var t=closest-halfChord; if(t<0.0){t=closest+halfChord;} if(t<0.0){discard;}
  let p=g.eye.xyz+t*ray; let unit=o+t*d;
  let objectNormal=(transpose(g.invShape)*vec4f(unit,0)).xyz;
  var hit:Hit; hit.p=p; hit.n=normalize(rotate(objectNormal,i.rotation.x,i.rotation.y)); return hit;
}
struct Output { @location(0) color:vec4f,@builtin(frag_depth) depth:f32 };
@fragment fn meshFragment(v:Varying)->@location(0) vec4f {return shade(v.world,v.normal);}
@fragment fn analyticFragment(v:Varying)->Output {let h=analytic(v); let clip=g.vp*vec4f(h.p,1); var out:Output; out.color=shade(h.p,h.n); out.depth=clip.z/clip.w; return out;}
@fragment fn meshMetric(v:Varying)->@location(0) vec4f {return vec4f(distance(v.world,g.eye.xyz),normalize(v.normal));}
@fragment fn analyticMetric(v:Varying)->Output {let h=analytic(v); let clip=g.vp*vec4f(h.p,1); var out:Output; out.color=vec4f(distance(h.p,g.eye.xyz),h.n); out.depth=clip.z/clip.w; return out;}
`;

type Mode = "meshLod" | "mesh12" | "mesh24" | "analytic";
const modes: Mode[] = ["meshLod", "mesh12", "mesh24", "analytic"];
const summary = (v: number[]) => {
  const sorted = [...v].sort((a, b) => a - b);
  return {
    count: v.length,
    median: sorted[Math.floor(v.length / 2)],
    p95: sorted[Math.floor(v.length * 0.95)],
    min: sorted[0],
    max: sorted.at(-1),
    samples: v,
  };
};
export async function createExperiment(canvas: HTMLCanvasElement) {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
  if (!adapter) throw new Error("No WebGPU adapter");
  if (!adapter.features.has("timestamp-query"))
    throw new Error("Hardware GPU timestamps are required for this experiment");
  const info = adapter.info;
  if (/swiftshader|llvmpipe|software/i.test([info.vendor, info.architecture, info.description].join(" ")))
    throw new Error("A software adapter is not hardware evidence");
  const device = await adapter.requestDevice({ requiredFeatures: ["timestamp-query"] });
  const errors: string[] = [];
  device.addEventListener("uncapturederror", (event) => errors.push(event.error.message));
  const width = 1920,
    height = 1080;
  canvas.width = width;
  canvas.height = height;
  const candidateContext = canvas.getContext("webgpu");
  if (!candidateContext) throw new Error("No WebGPU canvas");
  const context = candidateContext;
  context.configure({
    device,
    format: "rgba8unorm",
    alphaMode: "opaque",
    usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_DST,
  });
  const doc = referenceProject().documents.find((d) => d.id === "river-stone");
  if (doc?.kind !== "object") throw new Error("Missing stone");
  const field: FieldDefinition = doc.field,
    expression = lower(field);
  if (expression.kind !== "leaf")
    throw new Error("Only a single-primitive field is eligible for this GPU prototype");
  const leaf = expression;
  const meshes = new Map<Mode, { vertex: GPUBuffer; index: GPUBuffer; count: number; bytes: number }>();
  const buffer = (data: Float32Array | Uint32Array, usage: GPUBufferUsageFlags) => {
    const out = device.createBuffer({ size: data.byteLength, usage: usage | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(out, 0, data as Float32Array<ArrayBuffer>);
    return out;
  };
  for (const resolution of [12, 24]) {
    const mesh = extractSurface({ ...field, resolution }, "review").mesh;
    const vertices = new Float32Array(mesh.positions.length * 2);
    for (let i = 0; i < mesh.positions.length / 3; i++) {
      vertices.set(mesh.positions.subarray(i * 3, i * 3 + 3), i * 6);
      vertices.set(mesh.normals.subarray(i * 3, i * 3 + 3), i * 6 + 3);
    }
    meshes.set(`mesh${resolution}` as Mode, {
      vertex: buffer(vertices, GPUBufferUsage.VERTEX),
      index: buffer(mesh.indices, GPUBufferUsage.INDEX),
      count: mesh.indices.length,
      bytes: vertices.byteLength + mesh.indices.byteLength,
    });
  }
  // Strong conventional control: generate a 120-triangle parametric ellipsoid
  // directly from the same source, avoiding volumetric extraction altogether.
  const lodVertices: number[] = [],
    lodIndices: number[] = [];
  for (let row = 0; row <= 6; row++)
    for (let col = 0; col <= 12; col++) {
      const phi = (row * Math.PI) / 6,
        theta = (col * Math.PI * 2) / 12;
      const unit = [Math.sin(phi) * Math.cos(theta), Math.cos(phi), Math.sin(phi) * Math.sin(theta)];
      const local = unit.map((v, i) => v * leaf.node.size[i] - leaf.transform.b[i]);
      const normal = unit.map((v, i) => v / leaf.node.size[i]);
      const m = leaf.transform.m,
        len = Math.hypot(...normal);
      for (let i = 0; i < 3; i++)
        lodVertices.push(m[i] * local[0] + m[3 + i] * local[1] + m[6 + i] * local[2]);
      for (let i = 0; i < 3; i++)
        lodVertices.push((m[i] * normal[0] + m[3 + i] * normal[1] + m[6 + i] * normal[2]) / len);
      if (row < 6 && col < 12) {
        const a = row * 13 + col,
          b = a + 1,
          c = a + 13,
          d = c + 1;
        if (row > 0) lodIndices.push(a, b, c);
        if (row < 5) lodIndices.push(b, d, c);
      }
    }
  meshes.set("meshLod", {
    vertex: buffer(new Float32Array(lodVertices), GPUBufferUsage.VERTEX),
    index: buffer(new Uint32Array(lodIndices), GPUBufferUsage.INDEX),
    count: lodIndices.length,
    bytes: (lodVertices.length + lodIndices.length) * 4,
  });
  const getMesh = (mode: Mode) => {
    const mesh = meshes.get(mode);
    if (!mesh) throw new Error(`No mesh for ${mode}`);
    return mesh;
  };
  const globals = device.createBuffer({ size: 176, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  const instanceBuffer = device.createBuffer({
    size: 4096 * 32,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
  });
  const layout = device.createBindGroupLayout({
    entries: [
      {
        binding: 0,
        visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
        buffer: { type: "uniform" },
      },
      {
        binding: 1,
        visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
        buffer: { type: "read-only-storage" },
      },
    ],
  });
  const group = device.createBindGroup({
    layout,
    entries: [
      { binding: 0, resource: { buffer: globals } },
      { binding: 1, resource: { buffer: instanceBuffer } },
    ],
  });
  const module = device.createShaderModule({ code });
  const diagnostics = await module.getCompilationInfo();
  if (diagnostics.messages.some((m) => m.type === "error"))
    throw new Error(diagnostics.messages.map((m) => `${m.lineNum}: ${m.message}`).join("\n"));
  const pipelineLayout = device.createPipelineLayout({ bindGroupLayouts: [layout] });
  const makePipeline = (analytic: boolean, metric: boolean) =>
    device.createRenderPipelineAsync({
      layout: pipelineLayout,
      vertex: {
        module,
        entryPoint: analytic ? "boxVertex" : "meshVertex",
        buffers: analytic
          ? []
          : [
              {
                arrayStride: 24,
                attributes: [
                  { shaderLocation: 0, offset: 0, format: "float32x3" },
                  { shaderLocation: 1, offset: 12, format: "float32x3" },
                ],
              },
            ],
      },
      fragment: {
        module,
        entryPoint: analytic
          ? metric
            ? "analyticMetric"
            : "analyticFragment"
          : metric
            ? "meshMetric"
            : "meshFragment",
        targets: [{ format: metric ? "rgba32float" : "rgba8unorm" }],
      },
      primitive: { topology: "triangle-list", cullMode: "back" },
      depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less" },
    });
  const [meshPipeline, analyticPipeline, meshMetric, analyticMetric] = await Promise.all([
    makePipeline(false, false),
    makePipeline(true, false),
    makePipeline(false, true),
    makePipeline(true, true),
  ]);
  const texture = (format: GPUTextureFormat) =>
    device.createTexture({
      size: [width, height],
      format,
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
    });
  const beauty = texture("rgba8unorm"),
    metric = texture("rgba32float"),
    depth = texture("depth32float");
  const query = device.createQuerySet({ type: "timestamp", count: 2 });
  const resolve = device.createBuffer({
    size: 16,
    usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC,
  });
  const read = device.createBuffer({ size: 16, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  let instanceCount = 1;
  let camera: Camera = { position: [4, 2.7, 6], target: [0, 0, 0], fov: 50 };
  function scene(count: number) {
    instanceCount = count;
    const cols = Math.ceil(Math.sqrt((count * width) / height)),
      rows = Math.ceil(count / cols);
    const instances = new Float32Array(count * 8);
    for (let id = 0; id < count; id++) {
      const angle = count === 1 ? 0 : id * 1.618,
        scale = count === 1 ? 1 : 0.85 + 0.15 * Math.sin(id * 4.12);
      instances.set(
        count === 1
          ? [0, -0.8, 0, 1, 1, 0, 0, 0]
          : [
              ((id % cols) - (cols - 1) / 2) * 3.2,
              (Math.floor(id / cols) - (rows - 1) / 2) * 2.5 - 0.8 * scale,
              0.5 * Math.cos(id),
              scale,
              Math.cos(angle),
              Math.sin(angle),
              0,
              0,
            ],
        id * 8,
      );
    }
    camera =
      count === 1
        ? { position: [4, 2.7, 6], target: [0, 0, 0], fov: 50 }
        : {
            position: [
              0,
              0,
              Math.max(rows * 1.4, (cols * 1.8 * height) / width) / Math.tan((25 * Math.PI) / 180) + 3,
            ],
            target: [0, 0, 0],
            fov: 50,
          };
    const data = new Float32Array(44);
    data.set(multiply(perspective(camera.fov, width / height), lookAt(camera.position, camera.target)));
    const m = leaf.transform.m,
      b = leaf.transform.b,
      s = leaf.node.size;
    for (let row = 0; row < 3; row++) {
      for (let col = 0; col < 3; col++) data[16 + col * 4 + row] = m[row * 3 + col] / s[row];
      data[16 + 12 + row] = b[row] / s[row];
    }
    data[31] = 1;
    data.set([...camera.position, 1], 32);
    data.set([...field.bounds.min, 0], 36);
    data.set([...field.bounds.max, 0], 40);
    device.queue.writeBuffer(globals, 0, data);
    device.queue.writeBuffer(instanceBuffer, 0, instances);
  }
  function render(
    encoder: GPUCommandEncoder,
    mode: Mode,
    metricPass = false,
    timestamps?: GPURenderPassTimestampWrites,
  ) {
    const pass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: (metricPass ? metric : beauty).createView(),
          loadOp: "clear",
          storeOp: "store",
          clearValue: metricPass ? [-1, 0, 0, 0] : [0.025, 0.035, 0.05, 1],
        },
      ],
      depthStencilAttachment: {
        view: depth.createView(),
        depthLoadOp: "clear",
        depthStoreOp: "store",
        depthClearValue: 1,
      },
      ...(timestamps ? { timestampWrites: timestamps } : {}),
    });
    pass.setPipeline(
      mode === "analytic"
        ? metricPass
          ? analyticMetric
          : analyticPipeline
        : metricPass
          ? meshMetric
          : meshPipeline,
    );
    pass.setBindGroup(0, group);
    if (mode === "analytic") pass.draw(36, instanceCount);
    else {
      const mesh = getMesh(mode);
      pass.setVertexBuffer(0, mesh.vertex);
      pass.setIndexBuffer(mesh.index, "uint32");
      pass.drawIndexed(mesh.count, instanceCount);
    }
    pass.end();
  }
  async function measure(mode: Mode, repetitions = 4) {
    const encoder = device.createCommandEncoder();
    for (let i = 0; i < repetitions; i++)
      render(
        encoder,
        mode,
        false,
        i === 0
          ? { querySet: query, beginningOfPassWriteIndex: 0 }
          : i === repetitions - 1
            ? { querySet: query, endOfPassWriteIndex: 1 }
            : undefined,
      );
    encoder.resolveQuerySet(query, 0, 2, resolve, 0);
    encoder.copyBufferToBuffer(resolve, 0, read, 0, 16);
    device.queue.submit([encoder.finish()]);
    await read.mapAsync(GPUMapMode.READ);
    const result = new BigUint64Array(read.getMappedRange());
    const ms = Number(result[1] - result[0]) / 1e6 / repetitions;
    read.unmap();
    return ms;
  }
  async function show(mode: Mode, count = 1) {
    scene(count);
    const encoder = device.createCommandEncoder();
    render(encoder, mode);
    encoder.copyTextureToTexture({ texture: beauty }, { texture: context.getCurrentTexture() }, [
      width,
      height,
    ]);
    device.queue.submit([encoder.finish()]);
    await device.queue.onSubmittedWorkDone();
  }
  async function benchmark(count: number) {
    scene(count);
    for (let i = 0; i < 3; i++) for (const mode of modes) await measure(mode);
    const times: Record<Mode, number[]> = { meshLod: [], mesh12: [], mesh24: [], analytic: [] };
    for (let i = 0; i < 9; i++)
      for (const mode of i % 2 ? [...modes].reverse() : modes) times[mode].push(await measure(mode));
    const results = Object.fromEntries(
      modes.map((mode) => [
        mode,
        {
          gpuMs: summary(times[mode]),
          submittedTriangles: mode === "analytic" ? 12 * count : (getMesh(mode).count / 3) * count,
          geometryBytes: mode === "analytic" ? 0 : getMesh(mode).bytes,
        },
      ]),
    ) as Record<
      Mode,
      { gpuMs: ReturnType<typeof summary>; submittedTriangles: number; geometryBytes: number }
    >;
    return {
      instances: count,
      results,
      speedupVsProductionMesh24: results.mesh24.gpuMs.median / results.analytic.gpuMs.median,
      speedupVsCoarseMesh12: results.mesh12.gpuMs.median / results.analytic.gpuMs.median,
      speedupVsParametricLod: results.meshLod.gpuMs.median / results.analytic.gpuMs.median,
    };
  }
  async function accuracy(mode: Mode) {
    scene(1);
    const encoder = device.createCommandEncoder();
    render(encoder, mode, true);
    const stride = Math.ceil((width * 16) / 256) * 256;
    const pixels = device.createBuffer({
      size: stride * height,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    encoder.copyTextureToBuffer({ texture: metric }, { buffer: pixels, bytesPerRow: stride }, [
      width,
      height,
    ]);
    device.queue.submit([encoder.finish()]);
    await pixels.mapAsync(GPUMapMode.READ);
    const data = new Float32Array(pixels.getMappedRange());
    let squaredDepth = 0,
      maxDepth = 0,
      squaredAngle = 0,
      maxAngle = 0,
      matched = 0,
      falsePositive = 0,
      falseNegative = 0;
    const m = leaf.transform.m,
      s = leaf.node.size;
    for (let y = 0; y < height; y++)
      for (let x = 0; x < width; x++) {
        const ray = cameraRay(
          camera,
          ((x + 0.5) / width) * 2 - 1,
          1 - ((y + 0.5) / height) * 2,
          width / height,
        );
        const o = apply(leaf.transform, [ray.origin[0], ray.origin[1] + 0.8, ray.origin[2]]).map(
          (v, i) => v / s[i],
        );
        const d = [0, 1, 2].map(
          (i) =>
            (m[i * 3] * ray.direction[0] +
              m[i * 3 + 1] * ray.direction[1] +
              m[i * 3 + 2] * ray.direction[2]) /
            s[i],
        );
        const a = d.reduce((sum, v) => sum + v * v, 0),
          b = 2 * d.reduce((sum, v, i) => sum + v * o[i], 0),
          c = o.reduce((sum, v) => sum + v * v, -1),
          discriminant = b * b - 4 * a * c;
        const t = discriminant >= 0 ? (-b - Math.sqrt(discriminant)) / (2 * a) : -1;
        const index = (y * stride) / 4 + x * 4,
          actual = data[index];
        if (t < 0 && actual >= 0) falsePositive++;
        else if (t >= 0 && actual < 0) falseNegative++;
        else if (t >= 0 && actual >= 0) {
          const delta = actual - t;
          squaredDepth += delta * delta;
          maxDepth = Math.max(maxDepth, Math.abs(delta));
          matched++;
          const unit = o.map((v, i) => (v + t * d[i]) / s[i]);
          const normal = [0, 1, 2].map((i) => m[i] * unit[0] + m[3 + i] * unit[1] + m[6 + i] * unit[2]);
          const length = Math.hypot(...normal),
            measuredLength = Math.hypot(data[index + 1], data[index + 2], data[index + 3]);
          const cosine = normal.reduce(
            (sum, v, i) => sum + ((v / length) * data[index + 1 + i]) / measuredLength,
            0,
          );
          const angle = (Math.acos(Math.max(-1, Math.min(1, cosine))) * 180) / Math.PI;
          squaredAngle += angle * angle;
          maxAngle = Math.max(maxAngle, angle);
        }
      }
    pixels.unmap();
    pixels.destroy();
    return {
      matchedPixels: matched,
      falsePositivePixels: falsePositive,
      falseNegativePixels: falseNegative,
      rmsRayDepthErrorMetres: Math.sqrt(squaredDepth / matched),
      maxRayDepthErrorMetres: maxDepth,
      rmsNormalErrorDegrees: Math.sqrt(squaredAngle / matched),
      maxNormalErrorDegrees: maxAngle,
    };
  }
  scene(1);
  return {
    benchmark,
    show,
    accuracy,
    errors,
    adapter: { vendor: info.vendor, architecture: info.architecture, description: info.description },
    resolution: [width, height],
  };
}
