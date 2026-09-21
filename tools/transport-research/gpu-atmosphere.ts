import { quadratureOpticalDepth, topDistance } from "./atmosphere";
import { errorStats, type GpuContext, type Work } from "./gpu-common";

export async function atmosphereExperiment(ctx: GpuContext, scale = 8) {
  const side = 256,
    queries = side * side,
    extinction = scale === 8 ? 0.012 : 0.04;
  const output = ctx.buffer(new Float32Array(queries * 4), GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC);
  const lutWidth = 256,
    lutHeight = 64,
    lut = new Float32Array(lutWidth * lutHeight);
  const lutStart = performance.now();
  for (let y = 0; y < lutHeight; y++)
    for (let x = 0; x < lutWidth; x++) {
      const height = (y / (lutHeight - 1)) ** 2 * 40,
        cosine = (x / (lutWidth - 1)) ** 2;
      const ray = { height, cosine, scale, length: topDistance(height, cosine) };
      lut[y * lutWidth + x] = Math.exp(-extinction * quadratureOpticalDepth(ray, 2048));
    }
  const lutBuildMs = performance.now() - lutStart,
    lutBuffer = ctx.buffer(lut);
  const scaleLiteral = Number.isInteger(scale) ? `${scale}.0` : `${scale}`;
  const declarations = `
    @group(0) @binding(0) var<storage,read_write> result:array<vec4f>;
    fn altitude(h:f32,mu:f32,s:f32)->f32 {let r=6371.0+h;let d=s*(2.0*r*mu+s);return h+d/(sqrt(r*r+d)+r);}
    fn logDensity(h:f32,mu:f32,s:f32)->f32 {return -altitude(h,mu,s)/${scaleLiteral};}
    fn integral(a:f32,b:f32,length:f32)->f32 {
      let d=abs(b-a);var factor=1.0-d*0.5+d*d/6.0;
      if(d>0.01){factor=(1.0-exp(-d))/d;}return length*exp(max(a,b))*factor;
    }
    fn segment(h:f32,mu:f32,start:f32,end:f32)->vec2f {
      let mid=(start+end)*0.5;let r=6371.0+h;let rm=sqrt(r*r+mid*(2.0*r*mu+mid));
      let lm=logDensity(h,mu,mid);let slope=-(r*mu+mid)/(${scaleLiteral}*rm);let halfLength=(end-start)*0.5;
      return vec2f(integral(logDensity(h,mu,start),logDensity(h,mu,end),end-start),integral(lm-slope*halfLength,lm+slope*halfLength,end-start));
    }
  `;
  const works: Record<string, Work> = {};
  for (const mode of [
    "midpoint16",
    "midpoint64",
    "midpoint256",
    "bounded4",
    "bounded8",
    "bounded16",
    "bounded32",
    "lut",
    "reference",
  ]) {
    let extra = "",
      body = "";
    if (mode === "lut") {
      extra = "@group(0) @binding(1) var<storage,read> lut:array<f32>;";
      body = `let uv=vec2f(sqrt(mu)*${lutWidth - 1}.0,sqrt(h/40.0)*${lutHeight - 1}.0);let ij=min(vec2u(uv),vec2u(${lutWidth - 2}u,${lutHeight - 2}u));let f=uv-vec2f(ij);
        let lo=mix(lut[ij.y*${lutWidth}u+ij.x],lut[ij.y*${lutWidth}u+ij.x+1u],f.x);
        let hi=mix(lut[(ij.y+1u)*${lutWidth}u+ij.x],lut[(ij.y+1u)*${lutWidth}u+ij.x+1u],f.x);
        let value=mix(lo,hi,f.y);result[id]=vec4f(value,value,value,0);`;
    } else if (mode.startsWith("bounded")) {
      const count = Number(mode.slice(7));
      body = `var bounds=vec2f(0);for(var i=0u;i<${count}u;i++){let a=f32(i)/${count}.0;let b=f32(i+1u)/${count}.0;bounds+=segment(h,mu,length*a*a,length*b*b);}
        let low=exp(-${extinction}*bounds.y);let high=exp(-${extinction}*bounds.x);let value=exp(-${extinction}*(bounds.x+bounds.y)*0.5);result[id]=vec4f(value,low,high,bounds.y-bounds.x);`;
    } else {
      const count = mode === "reference" ? 4096 : Number(mode.slice(8));
      body =
        mode === "reference"
          ? `var sum=exp(logDensity(h,mu,0.0))+exp(logDensity(h,mu,length));for(var i=1u;i<${count}u;i++){
        sum+=select(2.0,4.0,i%2u==1u)*exp(logDensity(h,mu,length*f32(i)/${count}.0));}let depth=sum*length/${count * 3}.0;`
          : `var sum=0.0;for(var i=0u;i<${count}u;i++){sum+=exp(logDensity(h,mu,length*(f32(i)+0.5)/${count}.0));}let depth=sum*length/${count}.0;`;
      body += `let value=exp(-${extinction}*depth);result[id]=vec4f(value,value,value,0);`;
    }
    const pipeline = await ctx.pipeline(`${declarations}${extra}
      @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) gid:vec3u){let id=gid.x;if(id>=${queries}u){return;}
        let u=(f32(id%${side}u)+0.37)/${side}.0;let v=(f32(id/${side}u)+0.61)/${side}.0;let mu=u*u;let h=v*v*40.0;
        let r=6371.0+h;let d=(100.0-h)*(12842.0+h);let length=d/(sqrt(r*r*mu*mu+d)+r*mu);${body}}`);
    const entries: GPUBindGroupEntry[] = [{ binding: 0, resource: { buffer: output } }];
    if (mode === "lut") entries.push({ binding: 1, resource: { buffer: lutBuffer } });
    const group = ctx.device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries });
    works[mode] = (encoder, stamps) => {
      const pass = encoder.beginComputePass({ ...(stamps ? { timestampWrites: stamps } : {}) });
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, group);
      pass.dispatchWorkgroups(queries / 64);
      pass.end();
    };
  }
  const { reference, ...timed } = works,
    times = await ctx.benchmark(timed);
  const truth4 = await ctx.values(reference, output, queries * 4),
    truth = truth4.filter((_, i) => i % 4 === 0);
  const accuracy: Record<string, unknown> = {};
  let referenceParityMax = 0;
  for (let i = 0; i < 64; i++) {
    const index = (i * 1031 + 19) % queries,
      cosine = (((index % side) + 0.37) / side) ** 2,
      height = ((Math.floor(index / side) + 0.61) / side) ** 2 * 40;
    const depth = quadratureOpticalDepth(
      { height, cosine, scale, length: topDistance(height, cosine) },
      32768,
    );
    referenceParityMax = Math.max(referenceParityMax, Math.abs(truth[index] - Math.exp(-extinction * depth)));
  }
  for (const [name, work] of Object.entries(timed)) {
    const values = await ctx.values(work, output, queries * 4),
      scalar = values.filter((_, i) => i % 4 === 0);
    let outsideBy = 0,
      maxWidth = 0,
      negativeWidths = 0;
    if (name.startsWith("bounded"))
      for (let i = 0; i < queries; i++) {
        outsideBy = Math.max(outsideBy, values[4 * i + 1] - truth[i], truth[i] - values[4 * i + 2]);
        maxWidth = Math.max(maxWidth, values[4 * i + 2] - values[4 * i + 1]);
        negativeWidths += Number(values[4 * i + 2] < values[4 * i + 1]);
      }
    accuracy[name] = { ...errorStats(scalar, truth), outsideBy, maxWidth, negativeWidths };
  }
  output.destroy();
  lutBuffer.destroy();
  return {
    scaleHeight: scale,
    extinction,
    queries,
    lutSize: [lutWidth, lutHeight],
    lutBuildMs,
    referenceParityMax,
    times,
    accuracy,
  };
}
