// The raster hybrid ("rasterize heroes, trace crowds"): extraction at startup, then per frame a
// shadow map, the ground, the skinned mesh and optional fur shells. Spike 01's path, simplified:
// a dense surface-nets grid instead of culled blocks and a QEF, extracted once at rest pose.

import { writePalette } from './wolf.js';

const U = GPUBufferUsage;
const CELL = 0.005;                 // 5 mm: a hero-resolution mesh
const ORIGIN = [-0.27, -0.012, -0.88];
const SIZE = [0.54, 1.14, 1.84];
const SHELLS = 16;
const SHADOW = 2048;

const mat = {
  mul(a, b) {
    const o = new Float32Array(16);
    for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k];
      o[c * 4 + r] = s;
    }
    return o;
  },
  lookAt(eye, target, up) {
    const sub = (a, b) => a.map((x, i) => x - b[i]);
    const norm = a => { const l = Math.hypot(...a); return a.map(x => x / l); };
    const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    const f = norm(sub(target, eye)), s = norm(cross(f, up)), u = cross(s, f);
    return new Float32Array([s[0], u[0], -f[0], 0, s[1], u[1], -f[1], 0, s[2], u[2], -f[2], 0, -dot(s, eye), -dot(u, eye), dot(f, eye), 1]);
  },
  perspective(fovy, aspect, near, far) {
    const f = 1 / Math.tan(fovy / 2);
    return new Float32Array([f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, far / (near - far), -1, 0, 0, near * far / (near - far), 0]);
  },
  ortho(l, r, b, t, n, f) {
    return new Float32Array([2 / (r - l), 0, 0, 0, 0, 2 / (t - b), 0, 0, 0, 0, 1 / (n - f), 0, -(r + l) / (r - l), -(t + b) / (t - b), n / (n - f), 1]);
  },
};

export class Raster {
  constructor(device, src, o) {
    this.device = device;
    this.src = src;
    this.o = o;
    this.info = {};
    this.res = {};
  }

  async init() {
    const { device, o } = this;
    const t0 = performance.now();
    const V = GPUShaderStage.VERTEX, Fs = GPUShaderStage.FRAGMENT, C = GPUShaderStage.COMPUTE;
    const em = await o.module(['common', 'wolf', 'posed', 'extract'], 'extract');
    const rm = await o.module(['common', 'wolf', 'raster'], 'raster');

    // ---- extraction ----
    const dims = SIZE.map(s => Math.ceil(s / CELL));
    const ncells = dims[0] * dims[1] * dims[2];
    const ncorners = (dims[0] + 1) * (dims[1] + 1) * (dims[2] + 1);
    const vcap = 400000, icap = 3000000;
    const g = new ArrayBuffer(48), gf = new Float32Array(g), gu = new Uint32Array(g);
    gf.set([...ORIGIN, CELL], 0);
    gu.set([...dims, vcap], 4);
    gu.set([icap, 0, 0, 0], 8);
    const gridBuf = o.buffer(48, U.UNIFORM | U.COPY_DST, gu);
    const corners = o.buffer(ncorners * 4, U.STORAGE);
    const cellv = o.buffer(ncells * 4, U.STORAGE);
    this.verts = o.buffer(vcap * 48, U.STORAGE | U.VERTEX);
    this.counts = o.buffer(32, U.STORAGE | U.INDIRECT | U.COPY_SRC | U.COPY_DST, new Uint32Array([0, 1, 0, 0, 0, 0, 0, 0]));
    this.indices = o.buffer(icap * 4, U.STORAGE | U.INDEX);
    const poseDummy = o.buffer(16, U.STORAGE);
    const e = (binding, type) => ({ binding, visibility: C, buffer: { type } });
    const exL = device.createBindGroupLayout({ entries: [e(0, 'uniform'), e(1, 'read-only-storage'), e(2, 'uniform'), e(3, 'storage'), e(4, 'storage'), e(5, 'storage'), e(6, 'storage'), e(7, 'storage')] });
    const exBG = device.createBindGroup({
      layout: exL,
      entries: [o.frameBuffer, poseDummy, gridBuf, corners, cellv, this.verts, this.counts, this.indices].map((b, i) => ({ binding: i, resource: { buffer: b } })),
    });
    const lay = device.createPipelineLayout({ bindGroupLayouts: [exL] });
    const [pc, pv, pq] = await Promise.all(['ex_corners', 'ex_cells', 'ex_quads'].map(entryPoint =>
      device.createComputePipelineAsync({ layout: lay, compute: { module: em, entryPoint } })));
    // Slabs of 16 cells along z, one submission each, awaited (GPU safety rules: the whole grid in
    // one pass took over 100 ms). GPU time is summed from timestamps per slab.
    const SLAB = 16;
    const nslab = Math.ceil((dims[2] + 1) / SLAB);
    const qs = device.createQuerySet({ type: 'timestamp', count: 2 * 3 * nslab });
    const gpu = [0, 0, 0];
    let worst = 0;
    for (const [i, p, d] of [[0, pc, dims.map(x => x + 1)], [1, pv, dims], [2, pq, dims]]) {
      for (let k = 0; k < nslab; k++) {
        const z0 = k * SLAB;
        if (z0 >= d[2]) break;
        gu[8] = icap; gu[9] = z0;
        device.queue.writeBuffer(gridBuf, 0, g);
        const enc = device.createCommandEncoder();
        const q = 2 * (i * nslab + k);
        const pass = enc.beginComputePass({ timestampWrites: { querySet: qs, beginningOfPassWriteIndex: q, endOfPassWriteIndex: q + 1 } });
        pass.setPipeline(p);
        pass.setBindGroup(0, exBG);
        pass.dispatchWorkgroups(Math.ceil(d[0] / 4), Math.ceil(d[1] / 4), Math.ceil(Math.min(SLAB, d[2] - z0) / 4));
        pass.end();
        device.queue.submit([enc.finish()]);
        await device.queue.onSubmittedWorkDone();
      }
    }
    const tsBuf = o.buffer(2 * 3 * nslab * 8, U.QUERY_RESOLVE | U.COPY_SRC);
    const enc = device.createCommandEncoder();
    enc.resolveQuerySet(qs, 0, 2 * 3 * nslab, tsBuf, 0);
    device.queue.submit([enc.finish()]);
    const raw = await o.readback([{ src: this.counts, size: 32 }, { src: tsBuf, size: 2 * 3 * nslab * 8 }]);
    const c = new Uint32Array(raw, 0, 8);
    const ts = new BigUint64Array(raw, 32, 2 * 3 * nslab);
    for (let i = 0; i < 3; i++) for (let k = 0; k < nslab; k++) {
      const q = 2 * (i * nslab + k);
      if (ts[q + 1] > ts[q]) { const ms = Number(ts[q + 1] - ts[q]) / 1e6; gpu[i] += ms; worst = Math.max(worst, ms); }
    }
    this.info = {
      cell_m: CELL, grid: dims, vertices: c[5], triangles: c[0] / 3, holes: c[7], flags: c[6],
      extract_gpu_ms: { corners: +gpu[0].toFixed(3), cells: +gpu[1].toFixed(3), quads: +gpu[2].toFixed(3), worst_slab: +worst.toFixed(3) },
      mesh_mb: +((c[5] * 48 + c[0] * 4) / 2 ** 20).toFixed(2),
    };
    if (c[6]) o.err('extraction overflow flags', c[6]);
    corners.destroy(); cellv.destroy(); gridBuf.destroy(); tsBuf.destroy(); qs.destroy();

    // ---- drawing ----
    this.view = o.buffer(192, U.UNIFORM | U.COPY_DST);
    this.palette = o.buffer(22 * 64, U.STORAGE | U.COPY_DST);
    this.shadowTex = device.createTexture({ size: [SHADOW, SHADOW], format: 'depth32float', usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING });
    this.shadowView = this.shadowTex.createView();
    const VF = V | Fs;
    this.L = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: VF, buffer: { type: 'uniform' } },
        { binding: 1, visibility: VF, buffer: { type: 'uniform' } },
        { binding: 2, visibility: Fs, texture: { sampleType: 'depth' } },
        { binding: 3, visibility: Fs, sampler: { type: 'comparison' } },
        { binding: 4, visibility: Fs, texture: { viewDimension: '2d-array' } },
        { binding: 5, visibility: Fs, sampler: {} },
        { binding: 6, visibility: V, buffer: { type: 'read-only-storage' } },
      ],
    });
    this.Ls = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: VF, buffer: { type: 'uniform' } },
        { binding: 1, visibility: VF, buffer: { type: 'uniform' } },
        { binding: 6, visibility: V, buffer: { type: 'read-only-storage' } },
      ],
    });
    this.bg = device.createBindGroup({
      layout: this.L,
      entries: [
        { binding: 0, resource: { buffer: o.frameBuffer } }, { binding: 1, resource: { buffer: this.view } },
        { binding: 2, resource: this.shadowView }, { binding: 3, resource: device.createSampler({ compare: 'less', magFilter: 'linear', minFilter: 'linear' }) },
        { binding: 4, resource: o.strandView }, { binding: 5, resource: o.samp }, { binding: 6, resource: { buffer: this.palette } },
      ],
    });
    this.bgs = device.createBindGroup({
      layout: this.Ls,
      entries: [{ binding: 0, resource: { buffer: o.frameBuffer } }, { binding: 1, resource: { buffer: this.view } }, { binding: 6, resource: { buffer: this.palette } }],
    });
    const vbuf = [{
      arrayStride: 48,
      attributes: [
        { shaderLocation: 0, offset: 0, format: 'float32x3' },
        { shaderLocation: 1, offset: 12, format: 'uint8x4' },
        { shaderLocation: 2, offset: 16, format: 'float32x3' },
        { shaderLocation: 3, offset: 28, format: 'unorm8x4' },
        { shaderLocation: 4, offset: 32, format: 'float32' },
        { shaderLocation: 5, offset: 36, format: 'unorm8x4' },
        { shaderLocation: 6, offset: 40, format: 'unorm8x4' },
      ],
    }];
    const pl = l => device.createPipelineLayout({ bindGroupLayouts: [l] });
    const color = { format: 'rgba16float' };
    const blend = { color: { srcFactor: 'one', dstFactor: 'one-minus-src-alpha' }, alpha: { srcFactor: 'one', dstFactor: 'one-minus-src-alpha' } };
    const [shadow, env, envNoShadow, mesh, shells] = await Promise.all([
      device.createRenderPipelineAsync({
        layout: pl(this.Ls), vertex: { module: rm, entryPoint: 'shadow_vs', buffers: vbuf },
        primitive: { topology: 'triangle-list', cullMode: 'none' },
        depthStencil: { format: 'depth32float', depthCompare: 'less', depthWriteEnabled: true, depthBias: 2, depthBiasSlopeScale: 2 },
      }),
      device.createRenderPipelineAsync({
        layout: pl(this.L), vertex: { module: rm, entryPoint: 'env_vs' },
        fragment: { module: rm, entryPoint: 'env_fs', targets: [color] },
        depthStencil: { format: 'depth32float', depthCompare: 'always', depthWriteEnabled: true },
      }),
      device.createRenderPipelineAsync({
        layout: pl(this.L), vertex: { module: rm, entryPoint: 'env_vs' },
        fragment: { module: rm, entryPoint: 'env_fs', targets: [color], constants: { ENV_SHADOW: 0 } },
        depthStencil: { format: 'depth32float', depthCompare: 'always', depthWriteEnabled: true },
      }),
      device.createRenderPipelineAsync({
        layout: pl(this.L), vertex: { module: rm, entryPoint: 'mesh_vs', buffers: vbuf },
        fragment: { module: rm, entryPoint: 'mesh_fs', targets: [color] },
        primitive: { topology: 'triangle-list', cullMode: 'back', frontFace: 'ccw' },
        depthStencil: { format: 'depth32float', depthCompare: 'less', depthWriteEnabled: true },
      }),
      device.createRenderPipelineAsync({
        layout: pl(this.L), vertex: { module: rm, entryPoint: 'shell_vs', buffers: vbuf },
        fragment: { module: rm, entryPoint: 'shell_fs', targets: [{ ...color, blend }] },
        primitive: { topology: 'triangle-list', cullMode: 'back', frontFace: 'ccw' },
        depthStencil: { format: 'depth32float', depthCompare: 'less', depthWriteEnabled: false },
      }),
    ]);
    this.P = { shadow, env, envNoShadow, mesh, shells };
    for (const r of [o.FULL, o.HALF]) {
      this.res[r.name] = device.createTexture({ size: [r.w, r.h], format: 'depth32float', usage: GPUTextureUsage.RENDER_ATTACHMENT }).createView();
    }
    this.info.setup_ms = +(performance.now() - t0).toFixed(1);
    this.info.shells = SHELLS;
    this.info.shadow_map = SHADOW;
  }

  /** Per frame: the bone palette, camera and light matrices, and the ground AO spheres. */
  update(pose, eye, target, res, frame) {
    const pal = new Float32Array(22 * 16);
    writePalette(pal, pose);
    this.device.queue.writeBuffer(this.palette, 0, pal);
    const cu = new Float32Array(frame, 48, 3);   // F.cu has length tan(fovy / 2)
    const ty = Math.hypot(cu[0], cu[1], cu[2]);
    const vp = mat.mul(mat.perspective(2 * Math.atan(ty), res.w / res.h, 0.05, 200), mat.lookAt(eye, target, [0, 1, 0]));
    const fl = new Float32Array(frame);
    const sun = [fl[16], fl[17], fl[18]];
    const c = [pose.root[0], 0.4, pose.root[2]];
    const le = c.map((x, i) => x + sun[i] * 4);
    const light = mat.mul(mat.ortho(-1.5, 1.5, -1.5, 1.5, 0.5, 8), mat.lookAt(le, c, [0, 1, 0]));
    const v = new Float32Array(48);
    v.set(vp, 0);
    v.set(light, 16);
    const at = (b, p) => { const m = pose.world[b]; return [0, 1, 2].map(r => m[r * 4] * p[0] + m[r * 4 + 1] * p[1] + m[r * 4 + 2] * p[2] + m[r * 4 + 3]); };
    v.set([...at(2, [0, 0.60, 0.10]), 0.17], 32);
    v.set([...at(0, [0, 0.62, -0.30]), 0.15], 36);
    v.set([...at(3, [0, 0.80, 0.42]), 0.11], 40);
    v.set([SHELLS, 0, 0, 0], 44);
    this.device.queue.writeBuffer(this.view, 0, v);
  }

  /** One raster frame. Timestamp pairs: 0 shadow, 1 ground, 2 mesh, 3 shells. */
  encode(enc, res, view, shells, qs, q0, creature = true) {
    const ts = i => (qs ? { timestampWrites: { querySet: qs, beginningOfPassWriteIndex: q0 + 2 * i, endOfPassWriteIndex: q0 + 2 * i + 1 } } : {});
    const depth = this.res[res.name];
    const drawMesh = (p, instances) => {
      p.setVertexBuffer(0, this.verts);
      p.setIndexBuffer(this.indices, 'uint32');
      if (instances === 1) p.drawIndexedIndirect(this.counts, 0);
      else for (let k = 0; k < instances; k++) p.drawIndexedIndirect(this.counts, 0);
    };
    if (creature) {
      const p = enc.beginRenderPass({ colorAttachments: [], depthStencilAttachment: { view: this.shadowView, depthClearValue: 1, depthLoadOp: 'clear', depthStoreOp: 'store' }, ...ts(0) });
      p.setPipeline(this.P.shadow);
      p.setBindGroup(0, this.bgs);
      drawMesh(p, 1);
      p.end();
    }
    let p = enc.beginRenderPass({
      colorAttachments: [{ view, loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }],
      depthStencilAttachment: { view: depth, depthClearValue: 1, depthLoadOp: 'clear', depthStoreOp: 'store' }, ...ts(1),
    });
    p.setPipeline(creature ? this.P.env : this.P.envNoShadow);
    p.setBindGroup(0, this.bg);
    p.draw(3);
    p.end();
    if (!creature) return;
    p = enc.beginRenderPass({
      colorAttachments: [{ view, loadOp: 'load', storeOp: 'store' }],
      depthStencilAttachment: { view: depth, depthLoadOp: 'load', depthStoreOp: 'store' }, ...ts(2),
    });
    p.setPipeline(this.P.mesh);
    p.setBindGroup(0, this.bg);
    drawMesh(p, 1);
    p.end();
    if (!shells) return;
    p = enc.beginRenderPass({
      colorAttachments: [{ view, loadOp: 'load', storeOp: 'store' }],
      depthStencilAttachment: { view: depth, depthLoadOp: 'load', depthStoreOp: 'store' }, ...ts(3),
    });
    p.setPipeline(this.P.shells);
    p.setBindGroup(0, this.bg);
    // Instanced: shell k is instance k. drawIndexedIndirect's instance count is 1, so the shells
    // are drawn with a direct indexed draw using the extracted index count (read back at startup).
    p.setVertexBuffer(0, this.verts);
    p.setIndexBuffer(this.indices, 'uint32');
    p.drawIndexed(this.info.triangles * 3, SHELLS);
    p.end();
  }
}
