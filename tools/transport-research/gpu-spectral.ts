import { errorStats, type GpuContext, type Work } from "./gpu-common";
import {
  compileSpectrum,
  evaluatePlan,
  type Footprint,
  integrateResponse,
  type Plan,
  planSpectrum,
  tau,
  transfer,
  water,
  waterResponse,
} from "./spectral";

const f = (n: number) => (Number.isInteger(n) ? `${n}.0` : `${n}`);
const vec = (v: number[]) => `vec${v.length}f(${v.map(f).join(",")})`;
const responseWgsl = `
fn response(phase:vec2f, attenuation:vec2f)->f32 {
  let cs=cos(phase)*attenuation;
  let slope=${vec(water.slopes[0])}*cs.x+${vec(water.slopes[1])}*cs.y;
  let n=normalize(vec3f(-slope.x,1,-slope.y));
  let l=${vec(water.light)}; let v=${vec(water.view)}; let h=normalize(l+v);
  let nv=max(dot(n,v),0.0); let nl=max(dot(n,l),0.0); let nh=max(dot(n,h),0.0); let vh=max(dot(v,h),0.0);
  let a2=${f(water.roughness ** 4)}; let d=nh*nh*(a2-1.0)+1.0;
  let distribution=a2/(3.141592653589793*d*d);
  let lambdaV=(sqrt(1.0+a2*(1.0-nv*nv)/(nv*nv))-1.0)*0.5;
  let lambdaL=(sqrt(1.0+a2*(1.0-nl*nl)/(nl*nl))-1.0)*0.5;
  let fresnel=${f(water.f0)}+${f(1 - water.f0)}*pow(1.0-vh,5.0);
  return distribution*fresnel/(4.0*nv*(1.0+lambdaV+lambdaL));
}
fn hash(value:u32)->u32 { var x=value; x=(x^(x>>16u))*0x7feb352du; x=(x^(x>>15u))*0x846ca68bu; return x^(x>>16u); }
fn random(value:u32)->f32 {return f32(hash(value)>>8u)/16777216.0;}
fn multiplyComplex(a:vec2f,b:vec2f)->vec2f {return vec2f(a.x*b.x-a.y*b.y,a.x*b.y+a.y*b.x);}
`;
function polynomial(plan: Plan) {
  const maxM = Math.max(...plan.modes.map((m) => m.m), 0),
    maxN = Math.max(...plan.modes.map((m) => Math.abs(m.n)), 0);
  const lines = [
    "let ax=vec2f(cos(phase.x),sin(phase.x));let by=vec2f(cos(phase.y),sin(phase.y));",
    "let a0=vec2f(1,0);let b0=vec2f(1,0);",
  ];
  for (let i = 1; i <= maxM; i++) lines.push(`let a${i}=multiplyComplex(a${i - 1},ax);`);
  for (let i = 1; i <= maxN; i++) lines.push(`let b${i}=multiplyComplex(b${i - 1},by);`);
  lines.push(`var value=${f(plan.dc)};`);
  plan.modes.forEach((mode, i) => {
    lines.push(
      `let p${i}=multiplyComplex(a${mode.m},b${Math.abs(mode.n)}${mode.n < 0 ? "*vec2f(1,-1)" : ""});`,
    );
    lines.push(`value+=dot(p${i},${vec([2 * mode.re, -2 * mode.im])});`);
  });
  return lines.join("\n");
}
function half(value: number) {
  const floats = new Float32Array([value]),
    bits = new Uint32Array(floats.buffer)[0];
  const sign = (bits >>> 16) & 0x8000,
    exponent = ((bits >>> 23) & 0xff) - 127 + 15,
    mantissa = bits & 0x7fffff;
  if (exponent <= 0) return exponent < -10 ? sign : sign | ((mantissa | 0x800000) >>> (14 - exponent));
  if (exponent >= 31) return sign | 0x7c00;
  return sign | ((exponent << 10) + ((mantissa + 0x1000) >>> 13));
}
function phaseTexture(ctx: GpuContext, samples: Float64Array, size: number) {
  const mipLevelCount = Math.log2(size) + 1;
  const texture = ctx.device.createTexture({
    size: [size, size],
    mipLevelCount,
    format: "r16float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
  });
  let current = samples,
    width = size;
  for (let level = 0; level < mipLevelCount; level++) {
    ctx.device.queue.writeTexture(
      { texture, mipLevel: level },
      new Uint16Array(current.map(half)),
      { bytesPerRow: width * 2 },
      [width, width],
    );
    const nextWidth = width / 2;
    if (nextWidth < 1) break;
    const next = new Float64Array(nextWidth * nextWidth);
    for (let y = 0; y < nextWidth; y++)
      for (let x = 0; x < nextWidth; x++)
        next[y * nextWidth + x] =
          (current[2 * y * width + 2 * x] +
            current[2 * y * width + 2 * x + 1] +
            current[(2 * y + 1) * width + 2 * x] +
            current[(2 * y + 1) * width + 2 * x + 1]) /
          4;
    current = next;
    width = nextWidth;
  }
  return texture;
}
export async function spectralExperiment(ctx: GpuContext, name: string, footprint: Footprint, count: number) {
  const compileStart = performance.now(),
    spectrum = compileSpectrum(waterResponse),
    plan = planSpectrum(spectrum, footprint, count);
  const spectrumMs = performance.now() - compileStart;
  const side = 256,
    queries = side * side,
    output = ctx.buffer(new Float32Array(queries), GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC);
  const modes = ctx.buffer(
    new Float32Array(plan.modes.flatMap((mode) => [mode.m, mode.n, mode.re * 2, mode.im * -2])),
  );
  const texture = phaseTexture(ctx, spectrum.samples, spectrum.size);
  const sampler = ctx.device.createSampler({
    addressModeU: "repeat",
    addressModeV: "repeat",
    minFilter: "linear",
    magFilter: "linear",
    mipmapFilter: "linear",
    maxAnisotropy: 16,
  });
  const works: Record<string, Work> = {},
    compileTimes: Record<string, number> = {};
  const controls = [
    "point",
    "meanNormal",
    "samples64",
    "samples256",
    "stochastic64",
    "phaseLut",
    "spectralStorage64",
    "spectralShared64",
    "spectralPhasor64",
    "spectralPhasor128",
    "spectralPhasor256",
    "reference",
  ];
  const attenuation = vec([transfer(1, 0, footprint), transfer(0, 1, footprint)]);
  for (const mode of controls) {
    const groupSize = mode.endsWith("128")
      ? 128
      : mode.endsWith("256") && mode.startsWith("spectral")
        ? 256
        : 64;
    let declarations = "",
      beforeGuard = "",
      body = "";
    if (mode === "phaseLut") {
      declarations =
        "@group(0) @binding(1) var responseTexture:texture_2d<f32>; @group(0) @binding(2) var responseSampler:sampler;";
      // FFT samples are at integer phases, whereas texture values are at texel centers.
      body = `let value=textureSampleGrad(responseTexture,responseSampler,phase/${f(tau)}+vec2f(${f(0.5 / spectrum.size)}),${vec(footprint.dx.map((x) => x / tau))},${vec(footprint.dy.map((x) => x / tau))}).r;`;
    } else if (mode.startsWith("spectralPhasor")) body = polynomial(plan);
    else if (mode.startsWith("spectral")) {
      declarations = "@group(0) @binding(1) var<storage,read> modes:array<vec4f>;";
      let source = "modes";
      if (mode.includes("Shared")) {
        declarations += `var<workgroup> sharedModes:array<vec4f,${plan.modes.length}>;`;
        beforeGuard = `for(var j=local.x;j<${plan.modes.length}u;j+=${groupSize}u){sharedModes[j]=modes[j];}workgroupBarrier();`;
        source = "sharedModes";
      }
      body = `var value=${f(plan.dc)};for(var j=0u;j<${plan.modes.length}u;j++){let c=${source}[j];let p=dot(c.xy,phase);value+=c.z*cos(p)+c.w*sin(p);}`;
    } else if (mode === "point" || mode === "meanNormal")
      body = `let value=response(phase,${mode === "point" ? "vec2f(1)" : attenuation});`;
    else {
      const sampleSide = mode === "reference" ? 256 : mode === "samples256" ? 16 : 8;
      const fp =
        mode === "reference" && name === "long-correlated" ? { ...footprint, dx: [tau, tau] } : footprint;
      body = `var value=0.0;for(var y=0u;y<${sampleSide}u;y++){var row=0.0;for(var x=0u;x<${sampleSide}u;x++){
        let jitter=${mode === "stochastic64" ? "vec2f(random(id*131u+x*17u+y*1031u),random(id*311u+x*131u+y*19u+41u))" : "vec2f(0.5)"};
        let uv=(vec2f(f32(x),f32(y))+jitter)/${f(sampleSide)}-vec2f(0.5);
        row+=response(phase+${vec(fp.dx)}*uv.x+${vec(fp.dy)}*uv.y,vec2f(1));}value+=row;}value/=${f(sampleSide * sampleSide)};`;
    }
    const shader = `@group(0) @binding(0) var<storage,read_write> result:array<f32>;${declarations}${responseWgsl}
      @compute @workgroup_size(${groupSize}) fn main(@builtin(global_invocation_id) gid:vec3u,@builtin(local_invocation_id) local:vec3u){
        ${beforeGuard}let id=gid.x;if(id>=${queries}u){return;}
        let phase=(vec2f(f32(id%${side}u),f32(id/${side}u))+vec2f(0.37,0.61))*${f(tau / side)};
        ${body}result[id]=value;}`;
    const start = performance.now(),
      pipeline = await ctx.pipeline(shader);
    compileTimes[mode] = performance.now() - start;
    const entries: GPUBindGroupEntry[] = [{ binding: 0, resource: { buffer: output } }];
    if (mode === "phaseLut")
      entries.push({ binding: 1, resource: texture.createView() }, { binding: 2, resource: sampler });
    else if (mode.startsWith("spectral") && !mode.startsWith("spectralPhasor"))
      entries.push({ binding: 1, resource: { buffer: modes } });
    const group = ctx.device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries });
    works[mode] = (encoder, stamps) => {
      const pass = encoder.beginComputePass({ ...(stamps ? { timestampWrites: stamps } : {}) });
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, group);
      pass.dispatchWorkgroups(Math.ceil(queries / groupSize));
      pass.end();
    };
  }
  const { reference, ...timed } = works;
  const times = await ctx.benchmark(timed);
  const truth = await ctx.values(reference, output, queries);
  const accuracy: Record<string, ReturnType<typeof errorStats>> = {},
    images: Record<string, number[]> = {};
  const cpuParity: number[] = [],
    referenceParity: number[] = [];
  const selected = await ctx.values(works.spectralPhasor64, output, queries);
  for (let i = 0; i < 24; i++) {
    const index = (i * 2741 + 419) % queries,
      a = (((index % side) + 0.37) * tau) / side,
      b = ((Math.floor(index / side) + 0.61) * tau) / side;
    cpuParity.push(selected[index] - evaluatePlan(plan, a, b));
    const referenceFootprint: Footprint =
      name === "long-correlated" ? { ...footprint, dx: [tau, tau] } : footprint;
    referenceParity.push(truth[index] - integrateResponse(waterResponse, a, b, referenceFootprint, 512));
  }
  images.reference = [...truth];
  for (const [name, work] of Object.entries(timed)) {
    const actual = await ctx.values(work, output, queries);
    accuracy[name] = errorStats(actual, truth);
    if (["point", "meanNormal", "samples64", "phaseLut", "spectralPhasor64", "stochastic64"].includes(name))
      images[name] = [...actual];
  }
  output.destroy();
  modes.destroy();
  texture.destroy();
  return {
    name,
    queries,
    side,
    footprint,
    retainedModes: plan.modes.length,
    omittedL1: plan.omittedL1,
    spectrumMs,
    compileTimes,
    times,
    accuracy,
    cpuParityMax: Math.max(...cpuParity.map(Math.abs)),
    referenceParityMax: Math.max(...referenceParity.map(Math.abs)),
    images,
  };
}
