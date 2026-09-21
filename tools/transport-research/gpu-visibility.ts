import type { GpuContext, Work } from "./gpu-common";
import { compileOccluders, crowdFixture, hidden } from "./visibility";

export async function visibilityExperiment(ctx: GpuContext, occluderFraction = 1, vertices = 1024) {
  const { grid, occluders: allOccluders, candidates } = crowdFixture();
  const occluders = allOccluders.slice(0, Math.ceil(allOccluders.length * occluderFraction));
  const { device } = ctx,
    tiles = grid.width * grid.height;
  const occluderBuffer = ctx.buffer(
    new Float32Array(
      (occluders.length ? occluders : [{ x: 0, y: 0, z: 1, radius: 0 }]).flatMap((s) => [
        s.x,
        s.y,
        s.z,
        s.radius,
      ]),
    ),
  );
  const candidateBuffer = ctx.buffer(new Float32Array(candidates.flatMap((s) => [s.x, s.y, s.z, s.radius])));
  const depths = ctx.buffer(new Uint32Array(tiles), GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC);
  const indirect = ctx.buffer(
    new Uint32Array([0, 1, 1, 0]),
    GPUBufferUsage.STORAGE | GPUBufferUsage.INDIRECT | GPUBufferUsage.COPY_SRC,
  );
  const survivors = ctx.buffer(
    new Uint32Array(candidates.length),
    GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  );
  const flags = ctx.buffer(
    new Uint32Array(candidates.length),
    GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  );
  const output = ctx.buffer(
    new Float32Array(candidates.length * vertices * 4),
    GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  );
  const palette = new Float32Array(candidates.length * 8 * 16);
  for (let id = 0; id < candidates.length; id++)
    for (let joint = 0; joint < 8; joint++) {
      const offset = (id * 8 + joint) * 16,
        angle = id * 0.031 + joint * 0.17,
        c = Math.cos(angle),
        s = Math.sin(angle);
      palette.set(
        [
          c,
          s,
          0,
          0,
          -s,
          c,
          0,
          0,
          0,
          0,
          1,
          0,
          candidates[id].x,
          candidates[id].y + (joint - 3.5) * 0.012,
          candidates[id].z,
          1,
        ],
        offset,
      );
    }
  const joints = ctx.buffer(palette),
    vertexData = new Float32Array(vertices * 4);
  for (let i = 0; i < vertices; i++)
    vertexData.set(
      [Math.cos(i * 0.13) * 0.06, (i / vertices - 0.5) * 0.1, Math.sin(i * 0.13) * 0.06, 1],
      i * 4,
    );
  const vertexBuffer = ctx.buffer(vertexData);
  const declarations = `
    @group(0) @binding(0) var<storage,read_write> depths:array<atomic<u32>>;
    @group(0) @binding(1) var<storage,read> occluders:array<vec4f>;
    @group(0) @binding(2) var<storage,read> candidates:array<vec4f>;
    @group(0) @binding(3) var<storage,read_write> indirect:array<atomic<u32>>;
    @group(0) @binding(4) var<storage,read_write> survivors:array<u32>;
    @group(0) @binding(5) var<storage,read_write> flags:array<u32>;
    fn project(s:vec4f)->vec4f {
      let denominator=s.z*s.z-s.w*s.w;
      let extent=s.w*sqrt(vec2f(s.z*s.z)+s.xy*s.xy-vec2f(s.w*s.w));
      return vec4f((s.xy*s.z-extent)/denominator,(s.xy*s.z+extent)/denominator);
    }
    fn range(r:vec4f)->vec4i {
      let scale=vec2f(${grid.width}.0,${grid.height}.0)*0.5;
      let halfSize=vec2f(${grid.halfWidth},${grid.halfHeight});
      return vec4i(max(vec2i(floor((r.xy/halfSize+vec2f(1))*scale)),vec2i(0)),
        min(vec2i(floor((r.zw/halfSize+vec2f(1))*scale)),vec2i(${grid.width - 1},${grid.height - 1})));
    }
    fn front(s:vec4f,uv:vec2f)->f32 {
      let c=dot(s.xyz,s.xyz)-s.w*s.w;let dp=dot(s.xyz,vec3f(uv,1));let d=dp*dp-c*(dot(uv,uv)+1.0);
      if(d<=0.0001 || dp<=0.0){return 3.402823e38;}return c/(dp+sqrt(d));
    }
  `;
  async function compute(body: string, buffers: { binding: number; buffer: GPUBuffer }[]) {
    const pipeline = await ctx.pipeline(declarations + body);
    const group = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: buffers.map((b) => ({ binding: b.binding, resource: { buffer: b.buffer } })),
    });
    return { pipeline, group };
  }
  const init = await compute(
    `@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) gid:vec3u){
    if(gid.x<${tiles}u){atomicStore(&depths[gid.x],0x7f7fffffu);}if(gid.x==0u){atomicStore(&indirect[0],0u);}}`,
    [
      { binding: 0, buffer: depths },
      { binding: 3, buffer: indirect },
    ],
  );
  const build = await compute(
    `@compute @workgroup_size(64) fn main(@builtin(workgroup_id) group:vec3u,@builtin(local_invocation_id) local:vec3u){
    var s=occluders[group.x];s.w=max(0.0,s.w-0.0001);if(s.z<=s.w){return;}let bounds=range(project(s));
    let width=bounds.z-bounds.x+1;let height=bounds.w-bounds.y+1;if(width<=0 || height<=0){return;}
    for(var i=i32(local.x);i<width*height;i+=64){let xy=bounds.xy+vec2i(i%width,i/width);
      let lo=(vec2f(xy)/vec2f(${grid.width}.0,${grid.height}.0)*2.0-vec2f(1))*vec2f(${grid.halfWidth},${grid.halfHeight});
      let hi=(vec2f(xy+vec2i(1))/vec2f(${grid.width}.0,${grid.height}.0)*2.0-vec2f(1))*vec2f(${grid.halfWidth},${grid.halfHeight});
      let depth=max(max(front(s,lo),front(s,hi)),max(front(s,vec2f(lo.x,hi.y)),front(s,vec2f(hi.x,lo.y))))+0.002;
      atomicMin(&depths[u32(xy.y*${grid.width}+xy.x)],bitcast<u32>(depth));}}
    `,
    [
      { binding: 0, buffer: depths },
      { binding: 1, buffer: occluderBuffer },
    ],
  );
  const cull = await compute(
    `@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) gid:vec3u){
    let id=gid.x;if(id>=${candidates.length}u){return;}let s=candidates[id];var isHidden=false;
    if(s.z>s.w){let r=project(s)+vec4f(-0.00002,-0.00002,0.00002,0.00002);let bounds=range(r);
      isHidden=bounds.x<=bounds.z && bounds.y<=bounds.w;
      for(var y=bounds.y;y<=bounds.w;y++){for(var x=bounds.x;x<=bounds.z;x++){
        if(s.z-s.w-0.002<=bitcast<f32>(atomicLoad(&depths[u32(y*${grid.width}+x)]))){isHidden=false;break;}}
        if(!isHidden){break;}}
    }
    flags[id]=select(0u,1u,isHidden);if(!isHidden){let slot=atomicAdd(&indirect[0],1u);survivors[slot]=id;}}
    `,
    [
      { binding: 0, buffer: depths },
      { binding: 2, buffer: candidateBuffer },
      { binding: 3, buffer: indirect },
      { binding: 4, buffer: survivors },
      { binding: 5, buffer: flags },
    ],
  );
  async function skin(compacted: boolean) {
    const pipeline = await ctx.pipeline(`
      @group(0) @binding(0) var<storage,read> joints:array<mat4x4f>;
      @group(0) @binding(1) var<storage,read> vertices:array<vec4f>;
      @group(0) @binding(2) var<storage,read_write> output:array<vec4f>;
      ${compacted ? "@group(0) @binding(3) var<storage,read> survivors:array<u32>;" : ""}
      @compute @workgroup_size(64) fn main(@builtin(workgroup_id) group:vec3u,@builtin(local_invocation_id) local:vec3u){
      let id=${compacted ? "survivors[group.x]" : "group.x"};
      for(var i=local.x;i<${vertices}u;i+=64u){let p=vertices[i];let joint=(i%5u)+id*8u;
        let skinned=(joints[joint]*p)*0.4+(joints[joint+1u]*p)*0.3+(joints[joint+2u]*p)*0.2+(joints[joint+3u]*p)*0.1;
        output[id*${vertices}u+i]=skinned;}}`);
    const entries: GPUBindGroupEntry[] = [
      { binding: 0, resource: { buffer: joints } },
      { binding: 1, resource: { buffer: vertexBuffer } },
      { binding: 2, resource: { buffer: output } },
    ];
    if (compacted) entries.push({ binding: 3, resource: { buffer: survivors } });
    return { pipeline, group: device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries }) };
  }
  const fullSkin = await skin(false),
    visibleSkin = await skin(true);
  function dispatch(
    encoder: GPUCommandEncoder,
    kernel: typeof init,
    count: number | null,
    stamps?: GPUComputePassTimestampWrites,
  ) {
    const pass = encoder.beginComputePass({ ...(stamps ? { timestampWrites: stamps } : {}) });
    pass.setPipeline(kernel.pipeline);
    pass.setBindGroup(0, kernel.group);
    if (count === null) pass.dispatchWorkgroupsIndirect(indirect, 0);
    else pass.dispatchWorkgroups(count);
    pass.end();
  }
  const baseline: Work = (encoder, stamps) => dispatch(encoder, fullSkin, candidates.length, stamps);
  const certificate: Work = (encoder, stamps) => {
    dispatch(
      encoder,
      init,
      Math.ceil(tiles / 64),
      stamps?.beginningOfPassWriteIndex !== undefined
        ? { querySet: stamps.querySet, beginningOfPassWriteIndex: stamps.beginningOfPassWriteIndex }
        : undefined,
    );
    if (occluders.length) dispatch(encoder, build, occluders.length);
    dispatch(encoder, cull, Math.ceil(candidates.length / 64));
    dispatch(
      encoder,
      visibleSkin,
      null,
      stamps?.endOfPassWriteIndex !== undefined
        ? { querySet: stamps.querySet, endOfPassWriteIndex: stamps.endOfPassWriteIndex }
        : undefined,
    );
  };
  const times = await ctx.benchmark({ baseline, certificate });
  const bits = new Uint32Array((await ctx.values(certificate, flags, candidates.length)).buffer);
  const cpuDepths = compileOccluders(occluders, grid);
  const cpu = candidates.map((s) => hidden(s, grid, cpuDepths));
  const missed = cpu.filter((c, i) => c && !bits[i]).length,
    unsafe = cpu.filter((c, i) => !c && bits[i]).length;
  const allSkin = await ctx.values(baseline, output, candidates.length * vertices * 4);
  // Poison output so stale baseline results cannot make compaction look correct.
  const clearEncoder = device.createCommandEncoder();
  clearEncoder.clearBuffer(output);
  device.queue.submit([clearEncoder.finish()]);
  const culledSkin = await ctx.values(certificate, output, candidates.length * vertices * 4);
  let maximumPosedRadius = 0;
  for (let id = 0; id < candidates.length; id++)
    for (let j = 0; j < vertices; j++) {
      const offset = (id * vertices + j) * 4,
        center = candidates[id];
      maximumPosedRadius = Math.max(
        maximumPosedRadius,
        Math.hypot(
          allSkin[offset] - center.x,
          allSkin[offset + 1] - center.y,
          allSkin[offset + 2] - center.z,
        ),
      );
    }
  if (maximumPosedRadius >= 0.16) throw new Error("Skin fixture escaped its conservative culling bound");
  let survivorMaxError = 0,
    missingSurvivors = 0;
  for (let id = 0; id < candidates.length; id++)
    if (!bits[id]) {
      if (culledSkin[id * vertices * 4 + 3] === 0) missingSurvivors++;
      for (let j = 0; j < vertices * 4; j++)
        survivorMaxError = Math.max(
          survivorMaxError,
          Math.abs(allSkin[id * vertices * 4 + j] - culledSkin[id * vertices * 4 + j]),
        );
    }
  for (const buffer of [
    occluderBuffer,
    candidateBuffer,
    depths,
    indirect,
    survivors,
    flags,
    output,
    joints,
    vertexBuffer,
  ])
    buffer.destroy();
  return {
    occluders: occluders.length,
    candidates: candidates.length,
    verticesPerInstance: vertices,
    culled: [...bits].reduce((a, b) => a + b, 0),
    cpuCulled: cpu.filter(Boolean).length,
    conservativeMisses: missed,
    unsafeDisagreements: unsafe,
    survivorMaxError,
    maximumPosedRadius,
    missingSurvivors,
    times,
  };
}
