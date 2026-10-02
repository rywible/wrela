// Spike harness: builds the pipelines, poses the wolf, traces and rasterizes the scenes, times it all.
// This file plays the engine's role plus the measuring. It isn't compiler output.

import { poseWolf, writePose, writePalette, PARTS, JOINT_JOBS, JOINTS, MARGIN_SLACK, P as PL, NB, J, WALK, apply } from './wolf.js';
import { Raster } from './raster.js';

const FULL = { w: 1920, h: 1080, name: '1080p' };
const HALF = { w: 960, h: 540, name: '540p' };
const FOVY = 38 * Math.PI / 180;
const SUN = norm([0.45, 0.50, -0.74]);
const FUR = { hin: 0.006, hout: 0.012, tile: 0.128 };
const LIGHT_CELLS = 64;
const VOL = { dims: [12, 14, 24], cell: 0.1, off: [-0.6, -0.05, -1.2] };
const FRAME_BYTES = 14 * 16;
const ALL_RIGID = (1 << 22) - 1;
const ALL_WARP = ALL_RIGID & ~((1 << 10) | (1 << 14) | (1 << 17) | (1 << 21));
const U = GPUBufferUsage;
const T_SNAP = 0.75;      // seconds: the middle of the measuring window

// Trace variants: pipeline-overridable constants (trace.wgsl).
const REF_K = { REF: 1, STEP_SCALE: 0.45, EPS_SCALE: 0.05, MAX_STEPS: 3000, SHADOW_STEPS: 1500, SHADOW_SCALE: 0.5 };
const VARIANTS = {
  env: { CREATURE: 0 },
  s: { FUR: 0 },
  v: { FUR: 1 },
  v_nolod: { FUR: 1, FUR_LOD: 0 },
  v6: { FUR: 1, FUR_STEPS: 6 },
  v24: { FUR: 1, FUR_STEPS: 24 },
  v_noao: { FUR: 1, AO: 0 },
  v_e3: { FUR: 1, FUR_EVAL: 3 },
  v_nosh: { FUR: 1, SHADOWS: 0 },
  s_nosh_noao: { FUR: 0, SHADOWS: 0, AO: 0 },
  stats_s: { FUR: 0, STATS: 1 },
  stats_v: { FUR: 1, STATS: 1 },
  stats_v_nolod: { FUR: 1, FUR_LOD: 0, STATS: 1 },
  heat_s: { FUR: 0, VIEW: 3 },
  heat_v: { FUR: 1, VIEW: 1 },
  ref_s: { FUR: 0, ...REF_K },
  ref_v: { FUR: 1, FUR_LOD: 0, FUR_STEPS: 192, ...REF_K },   // a 0.2 mm sweep per step: below a strand's width
};
const isFur = v => (VARIANTS[v].FUR ?? 1) > 0 && VARIANTS[v].CREATURE !== 0;
const STAT_NAMES = ['creature_px', 'steps', 'evals', 'caps', 'fur_steps', 'fur_caps', 'segments', 'shadow_steps',
  'shadow_caps', 'rays', 'ray_parts', 'tile_parts', 'surface_px', 'hit_steps', 'stops', 'jumps'];

const $ = id => document.getElementById(id);
const log = (...a) => { $('log').textContent += a.join(' ') + '\n'; console.log(...a); };
const err = (...a) => { $('log').textContent += 'ERROR ' + a.join(' ') + '\n'; console.error(...a); };
const R = { started: new Date().toISOString(), device: {}, setup: {}, coverage: {}, frames: {}, creature_ms: {}, stats: {}, quality: {}, paced: {}, raster: {} };
window.__results = R;

let device, ctx, canvasFormat;
const src = {}, L = {}, P = {}, B = {}, T = {}, RES = {};
let rest = null;      // startup boxes
let raster = null;

const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[s.length >> 1] : NaN; };
const pct = (a, q) => { const s = [...a].sort((x, y) => x - y); return s[Math.min(s.length - 1, Math.floor(s.length * q))]; };
const r3 = x => Math.round(x * 1000) / 1000;
function norm(a) { const l = Math.hypot(...a); return a.map(x => x / l); }
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sub = (a, b) => a.map((x, i) => x - b[i]);
const add = (a, b) => a.map((x, i) => x + b[i]);
const scale = (a, s) => a.map(x => x * s);

function buffer(size, usage, data) {
  const b = device.createBuffer({ size: Math.max(16, Math.ceil(size / 16) * 16), usage, mappedAtCreation: !!data });
  if (data) { new data.constructor(b.getMappedRange()).set(data); b.unmap(); }
  return b;
}

async function readback(copies) {
  const total = copies.reduce((s, c) => s + c.size, 0);
  const rb = device.createBuffer({ size: total, usage: U.MAP_READ | U.COPY_DST });
  const enc = device.createCommandEncoder();
  let off = 0;
  for (const c of copies) { enc.copyBufferToBuffer(c.src, 0, rb, off, c.size); off += c.size; }
  device.queue.submit([enc.finish()]);
  await rb.mapAsync(GPUMapMode.READ);
  const out = rb.getMappedRange().slice(0);
  rb.unmap();
  rb.destroy();
  return out;
}

async function save(name, body) {
  try { await fetch(`results/${name}`, { method: 'PUT', body }); } catch (e) { err('save failed', name, e.message); }
}

// ---- modules, with WGSL errors mapped back to file and line ------------------------------------------

function assemble(files, salt) {
  let code = '';
  const map = [];
  for (const f of files) {
    let s = src[f];
    if (f === 'common' && salt != null) s = s.replace('const SALT: f32 = 0.0;', `const SALT: f32 = ${salt.toFixed(4)};`);
    map.push({ f, start: code.split('\n').length });
    code += s + '\n';
  }
  return { code, map };
}

async function module(files, label, salt) {
  const { code, map } = assemble(files, salt);
  const m = device.createShaderModule({ code, label });
  const info = await m.getCompilationInfo();
  let bad = false;
  for (const msg of info.messages) {
    let where = `line ${msg.lineNum}`;
    for (let i = map.length - 1; i >= 0; i--) {
      if (msg.lineNum >= map[i].start) { where = `${map[i].f}.wgsl:${msg.lineNum - map[i].start + 1}:${msg.linePos}`; break; }
    }
    const line = `${label} ${msg.type} ${where} ${msg.message}`;
    if (msg.type === 'error') { err(line); bad = true; } else log(line);
  }
  if (bad) throw new Error(`${label}: WGSL errors`);
  return m;
}

// ---- setup -----------------------------------------------------------------------------------------

async function init() {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
  const want = ['timestamp-query'].filter(f => adapter.features.has(f));
  device = await adapter.requestDevice({
    requiredFeatures: want,
    requiredLimits: {
      maxStorageBufferBindingSize: adapter.limits.maxStorageBufferBindingSize,
      maxBufferSize: adapter.limits.maxBufferSize,
      maxStorageBuffersPerShaderStage: Math.min(10, adapter.limits.maxStorageBuffersPerShaderStage),
    },
  });
  device.lost.then(i => err('DEVICE LOST:', i.message));
  device.addEventListener('uncapturederror', e => err('GPU ERROR:', e.error.message));
  const info = adapter.info || {};
  R.device = {
    vendor: info.vendor, architecture: info.architecture, description: info.description,
    userAgent: navigator.userAgent, timestampQuery: want.includes('timestamp-query'),
    pageVisibility: document.visibilityState,
  };
  if (!R.device.timestampQuery) throw new Error('timestamp-query is required');
  for (const f of ['common', 'wolf', 'posed', 'bin', 'trace', 'bounds', 'blit', 'extract', 'raster']) {
    src[f] = await (await fetch(`${f}.wgsl`)).text();
  }
  const canvas = $('view');
  canvas.width = FULL.w;
  canvas.height = FULL.h;
  ctx = canvas.getContext('webgpu');
  canvasFormat = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format: canvasFormat, alphaMode: 'opaque' });

  const C = GPUShaderStage.COMPUTE;
  const e = (binding, type) => ({ binding, visibility: C, buffer: { type } });
  L.bin = device.createBindGroupLayout({ entries: [e(0, 'uniform'), e(1, 'read-only-storage'), e(2, 'storage'), e(3, 'storage'), e(4, 'storage')] });
  L.bounds = device.createBindGroupLayout({ entries: [e(0, 'uniform'), e(1, 'storage'), e(2, 'read-only-storage'), e(3, 'storage'), e(4, 'uniform')] });
  L.trace = device.createBindGroupLayout({
    entries: [e(0, 'uniform'), e(1, 'read-only-storage'), e(2, 'read-only-storage'), e(3, 'read-only-storage'), e(4, 'read-only-storage'),
      { binding: 5, visibility: C, storageTexture: { access: 'write-only', format: 'rgba16float' } }, e(6, 'storage'),
      { binding: 7, visibility: C, texture: { viewDimension: '2d-array' } }, { binding: 8, visibility: C, sampler: {} }, e(9, 'storage'), e(10, 'uniform')],
  });
  L.blit = device.createBindGroupLayout({
    entries: [{ binding: 0, visibility: GPUShaderStage.FRAGMENT, texture: {} }, { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: {} }],
  });
}

const rng = seed => { let s = seed >>> 0; return () => { s = (s + 0x6D2B79F5) >>> 0; let t = s; t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }; };

/** The strand texture: 16 layers (heights along a strand), 256² texels per 2.56 cm tile, full mips.
 *  R = coverage at that height; G = brightness / 2 (agouti banding: pale band, dark tip, per-strand
 *  variation). Strands taper toward their own random length and lean into clumps toward the tips. */
function strandTexture() {
  const N = 512, LAYERS = 16, MIPS = 10, CL = 16, CS = N / CL;
  const r = rng(7);
  const strands = [];
  for (let cy = 0; cy < CL; cy++) for (let cx = 0; cx < CL; cx++) {
    const cc = { x: (cx + 0.3 + 0.4 * r()) * CS, y: (cy + 0.3 + 0.4 * r()) * CS };
    const dark = r() < 0.25 ? 0.6 : 1.0;     // some locks are darker overall
    for (let k = 0; k < 22; k++) {
      const a = 2 * Math.PI * r(), d = 15 * Math.sqrt(r());
      strands.push({ x: cc.x + d * Math.cos(a), y: cc.y + d * Math.sin(a), cx: cc.x, cy: cc.y, conv: 0.7,
        r0: 1.0 + 0.7 * r(), len: 0.62 + 0.38 * r(), tint: dark * (0.8 + 0.4 * r()), guard: true });
    }
    for (let k = 0; k < 30; k++) {
      strands.push({ x: (cx + r()) * CS, y: (cy + r()) * CS, cx: cc.x, cy: cc.y, conv: 0.3,
        r0: 0.7 + 0.4 * r(), len: 0.3 + 0.45 * r(), tint: 0.85 + 0.3 * r(), guard: false });
    }
  }
  const sm = (a, b, x) => { const t = Math.min(Math.max((x - a) / (b - a), 0), 1); return t * t * (3 - 2 * t); };
  const tex = device.createTexture({
    size: [N, N, LAYERS], format: 'rg8unorm', mipLevelCount: MIPS,
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
  });
  for (let l = 0; l < LAYERS; l++) {
    const h = (l + 0.5) / LAYERS;
    let cov = new Float32Array(N * N), tint = new Float32Array(N * N).fill(0.5);
    for (const s of strands) {
      if (h >= s.len) continue;
      const hr = h / s.len;
      const rad = s.r0 * Math.pow(1 - hr, 0.6);
      const pull = s.conv * Math.pow(hr, 1.4);
      const x = s.x + (s.cx - s.x) * pull, y = s.y + (s.cy - s.y) * pull;
      const band = s.guard ? 0.8 + 0.35 * sm(0.25, 0.4, hr) * (1 - sm(0.66, 0.82, hr)) - 0.3 * sm(0.82, 1.0, hr) : 0.85;
      const b = Math.min(band * s.tint, 1.98);
      const Rr = Math.ceil(rad + 1);
      const bx = Math.floor(x), by = Math.floor(y);
      for (let dy = -Rr; dy <= Rr; dy++) for (let dx = -Rr; dx <= Rr; dx++) {
        const px = bx + dx, py = by + dy;
        const dd = Math.hypot(px + 0.5 - x, py + 0.5 - y);
        const a = Math.min(Math.max(rad + 0.5 - dd, 0), 1);
        if (a <= 0) continue;
        const i = (py & (N - 1)) * N + (px & (N - 1));
        if (a > cov[i]) { cov[i] = a; tint[i] = b / 2; }
      }
    }
    let n = N;
    for (let m = 0; m < MIPS; m++) {
      const data = new Uint8Array(n * n * 2);
      for (let i = 0; i < n * n; i++) { data[2 * i] = Math.round(255 * cov[i]); data[2 * i + 1] = Math.round(255 * tint[i]); }
      device.queue.writeTexture({ texture: tex, mipLevel: m, origin: [0, 0, l] }, data, { bytesPerRow: n * 2, rowsPerImage: n }, [n, n, 1]);
      if (n === 1) break;
      const h2 = n >> 1, c2 = new Float32Array(h2 * h2), t2 = new Float32Array(h2 * h2);
      for (let y = 0; y < h2; y++) for (let x = 0; x < h2; x++) {
        let cs = 0, ts = 0;
        for (const [ox, oy] of [[0, 0], [1, 0], [0, 1], [1, 1]]) { const i = (2 * y + oy) * n + 2 * x + ox; cs += cov[i]; ts += cov[i] * tint[i]; }
        c2[y * h2 + x] = cs / 4;
        t2[y * h2 + x] = cs > 0 ? ts / cs : 0.5;
      }
      cov = c2; tint = t2; n = h2;
    }
  }
  R.setup.strands = { count: strands.length, tile_m: FUR.tile, texels: N, layers: LAYERS, texture_mb: r3(N * N * LAYERS * 2 * 4 / 3 / 2 ** 20) };
  return tex;
}

// ---- startup bounds ------------------------------------------------------------------------------------

async function startupBounds() {
  const jobs = [];
  const grow = (v, d) => v.map(x => x + d);
  PARTS.forEach((p, f) => jobs.push({ fn: f, warped: 0, lo: grow(p.lo, -0.1), hi: grow(p.hi, 0.1), y: p.y, ma: p.k + MARGIN_SLACK, mb: p.k + MARGIN_SLACK + FUR.hout, name: p.name }));
  // Joint regions, for the refit's domain: margin with fur plus the refit's own cell slack (≤ 2 × 1.3 cm).
  JOINT_JOBS.forEach(j => {
    const m = PARTS[j.fn].k + MARGIN_SLACK + FUR.hout + 0.045;
    jobs.push({ fn: j.fn, warped: j.warped, lo: grow(j.lo, -0.1), hi: grow(j.hi, 0.1), y: j.y, ma: m, mb: m, name: `joint_${j.name}` });
  });
  const jd = new Float32Array(jobs.length * 16);
  jobs.forEach((j, i) => {
    jd.set([...j.lo, j.fn], i * 16);
    jd.set([...j.hi, j.warped], i * 16 + 4);
    jd.set([1, 0, 0, j.ma], i * 16 + 8);
    jd.set([...j.y, j.mb], i * 16 + 12);
  });
  const acc = new Uint32Array(jobs.length * 16);
  for (let i = 0; i < jobs.length; i++) for (const k of [0, 1, 2, 6, 7, 8]) acc[i * 16 + k] = 0xFFFFFFFF;
  const jb = buffer(jd.byteLength, U.STORAGE, jd);
  const ab = buffer(acc.byteLength, U.STORAGE | U.COPY_SRC | U.COPY_DST, acc);
  device.queue.writeBuffer(B.frame, 0, new Float32Array(FRAME_BYTES / 4).fill(1));
  const jobIndex = buffer(16, U.UNIFORM | U.COPY_DST);
  const bg = device.createBindGroup({ layout: L.bounds, entries: [B.frame, B.pose, jb, ab, jobIndex].map((b, i) => ({ binding: i, resource: { buffer: b } })) });
  const t0 = performance.now();
  // One submission per job (80³ samples each), awaited: GPU safety rules.
  for (let j = 0; j < jobs.length; j++) {
    const enc = device.createCommandEncoder();
    const p = enc.beginComputePass();
    p.setPipeline(P.bounds);
    p.setBindGroup(0, bg);
    p.dispatchWorkgroups(20, 20, 20);
    p.end();
    device.queue.writeBuffer(jobIndex, 0, new Uint32Array([j, 0, 0, 0]));
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
  }
  const out = new Uint32Array(await readback([{ src: ab, size: acc.byteLength }]));
  R.setup.bounds_ms = r3(performance.now() - t0);
  const unord = u => { const b = new Uint32Array(1); b[0] = (u & 0x80000000) ? (u & 0x7fffffff) : (~u >>> 0); return new Float32Array(b.buffer)[0]; };
  const box = (j, o) => {
    const lo = [0, 1, 2].map(k => unord(out[j * 16 + o + k])), hi = [0, 1, 2].map(k => unord(out[j * 16 + o + 3 + k]));
    const ax = [[1, 0, 0], jobs[j].y, cross([1, 0, 0], jobs[j].y)];
    const mid = [0, 1, 2].map(k => (lo[k] + hi[k]) / 2);
    const c = [0, 1, 2].map(a => ax[0][a] * mid[0] + ax[1][a] * mid[1] + ax[2][a] * mid[2]);
    return { c, ax, h: [0, 1, 2].map(k => Math.max((hi[k] - lo[k]) / 2, 1e-3)) };
  };
  rest = { parts: [], joints: [] };
  jobs.forEach((j, i) => {
    if (out[i * 16 + 12]) err(`bounds job ${j.name}: sublevel set touches the sampling domain (flags ${out[i * 16 + 12]})`);
    if (i < PARTS.length) rest.parts.push({ m0: box(i, 0), m1: box(i, 6) });
    else rest.joints.push(box(i, 0));
  });
  R.setup.rest_boxes = Object.fromEntries(jobs.map((j, i) => [j.name, (i < PARTS.length ? rest.parts[i].m1 : rest.joints[i - PARTS.length]).h.map(r3)]));
  jb.destroy();
  ab.destroy();
  jobIndex.destroy();
}

// ---- pipelines -------------------------------------------------------------------------------------------

/** `only`: the trace variants to build (all when omitted). Pipelines compile three at a time, so
 *  the shader compiler doesn't fan out across every core at once (GPU safety rules). */
async function pipelines(only) {
  const t0 = performance.now();
  const lay = bgl => device.createPipelineLayout({ bindGroupLayouts: [bgl] });
  const bm = await module(['common', 'wolf', 'posed', 'bin'], 'bin');
  const tm = await module(['common', 'wolf', 'posed', 'trace'], 'trace');
  const fm = await module(['common', 'wolf', 'posed', 'bounds'], 'bounds');
  const lm = await module(['blit'], 'blit');
  const list = [
    ['bin_screen', () => device.createComputePipelineAsync({ layout: lay(L.bin), compute: { module: bm, entryPoint: 'bin_screen' } })],
    ['bin_light', () => device.createComputePipelineAsync({ layout: lay(L.bin), compute: { module: bm, entryPoint: 'bin_light' } })],
    ['bin_volume', () => device.createComputePipelineAsync({ layout: lay(L.bin), compute: { module: bm, entryPoint: 'bin_volume' } })],
    ['bounds', () => device.createComputePipelineAsync({ layout: lay(L.bounds), compute: { module: fm, entryPoint: 'bounds' } })],
    ['refit_clear', () => device.createComputePipelineAsync({ layout: lay(L.bounds), compute: { module: fm, entryPoint: 'refit_clear' } })],
    ['refit_sample', () => device.createComputePipelineAsync({ layout: lay(L.bounds), compute: { module: fm, entryPoint: 'refit_sample' } })],
    ['refit_finalize', () => device.createComputePipelineAsync({ layout: lay(L.bounds), compute: { module: fm, entryPoint: 'refit_finalize' } })],
    ['blit', () => device.createRenderPipelineAsync({
      layout: lay(L.blit), vertex: { module: lm, entryPoint: 'blit_vs' },
      fragment: { module: lm, entryPoint: 'blit_fs', targets: [{ format: canvasFormat }] },
    })],
  ];
  for (const [name, constants] of Object.entries(VARIANTS)) {
    if (only && !only.includes(name)) continue;
    list.push([`trace_${name}`, () => device.createComputePipelineAsync({ layout: lay(L.trace), compute: { module: tm, entryPoint: 'trace', constants } })]);
  }
  for (let i = 0; i < list.length; i += 3) {
    const batch = list.slice(i, i + 3);
    const ps = await Promise.all(batch.map(([, f]) => f()));
    batch.forEach(([n], k) => { P[n] = ps[k]; });
  }
  R.setup.pipelines_ms = r3(performance.now() - t0);
  R.setup.trace_variants = only ? only.length : Object.keys(VARIANTS).length;
}

function resources() {
  B.frame = buffer(FRAME_BYTES, U.UNIFORM | U.COPY_DST);
  B.pose = buffer(PL.SIZE * 16, U.STORAGE | U.COPY_DST | U.COPY_SRC);
  B.lights = buffer(LIGHT_CELLS * LIGHT_CELLS * 4, U.STORAGE);
  B.volm = buffer(VOL.dims[0] * VOL.dims[1] * VOL.dims[2] * 4, U.STORAGE);
  B.stats = buffer(32 * 4, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  B.acc = buffer(NJ_ACC * 4, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  B.accum = buffer(FULL.w * FULL.h * 16, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  B.jobsDummy = buffer(64, U.STORAGE);
  T.strands = strandTexture();
  const strandView = T.strands.createView({ dimension: '2d-array' });
  const samp = device.createSampler({ magFilter: 'linear', minFilter: 'linear', mipmapFilter: 'linear', addressModeU: 'repeat', addressModeV: 'repeat', maxAnisotropy: 16 });
  const bilinear = device.createSampler({ magFilter: 'linear', minFilter: 'linear' });
  const res = b => ({ resource: { buffer: b } });
  for (const r of [FULL, HALF]) {
    const o = {};
    o.tx = Math.ceil(r.w / 8);
    o.ty = Math.ceil(r.h / 8);
    o.tex = device.createTexture({ size: [r.w, r.h], format: 'rgba16float', usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_SRC | GPUTextureUsage.RENDER_ATTACHMENT });
    o.view = o.tex.createView();
    o.tiles = buffer(o.tx * o.ty * 4, U.STORAGE);
    o.bin = device.createBindGroup({ layout: L.bin, entries: [B.frame, B.pose, o.tiles, B.lights, B.volm].map((b, i) => ({ binding: i, ...res(b) })) });
    o.traceEntries = [
      { binding: 0, ...res(B.frame) }, { binding: 1, ...res(B.pose) }, { binding: 2, ...res(o.tiles) },
      { binding: 3, ...res(B.lights) }, { binding: 4, ...res(B.volm) }, { binding: 5, resource: o.view },
      { binding: 6, ...res(B.stats) }, { binding: 7, resource: strandView }, { binding: 8, resource: samp },
      { binding: 9, ...res(B.accum) },
    ];
    o.bandSets = {};
    o.blit = device.createBindGroup({ layout: L.blit, entries: [{ binding: 0, resource: o.view }, { binding: 1, resource: bilinear }] });
    RES[r.name] = { ...r, ...o };
  }
  B.refit = device.createBindGroup({ layout: L.bounds, entries: [B.frame, B.pose, B.jobsDummy, B.acc, buffer(16, U.UNIFORM)].map((b, i) => ({ binding: i, ...res(b) })) });
  T.strandView = strandView;
  T.samp = samp;
}
const NJ_ACC = 5 * 16;

/** Row bands of a frame: each band is its own dispatch and its own queue submission, so no single
 *  submission runs long (spikes/README.md, GPU safety rules). Cached per resolution and count. */
function bandSet(rs, n, crop = null) {
  const key = crop ? `${n}/${crop.join(',')}` : n;
  if (rs.bandSets[key]) return rs.bandSets[key];
  const [cx, cy, cw, ch] = crop || [0, 0, rs.tx, rs.ty];
  const rows = Math.ceil(ch / n);
  const set = [];
  for (let y0 = cy; y0 < cy + ch; y0 += rows) {
    const ub = buffer(16, U.UNIFORM, new Uint32Array([y0, cx, 0, 0]));
    set.push({ y0, rows: Math.min(rows, cy + ch - y0), cols: cw, bg: device.createBindGroup({ layout: L.trace, entries: [...rs.traceEntries, { binding: 10, resource: { buffer: ub } }] }) });
  }
  rs.bandSets[key] = set;
  return set;
}
const MAX_BANDS = 16;
const QF = 2 * (2 + MAX_BANDS);     // timestamp slots per frame: refit, bins, then one per band

// ---- scenes ---------------------------------------------------------------------------------------------

/** A camera fixed relative to the walking wolf's root. */
function relCam(eyeOff, targetOff) {
  return root => ({ eye: add(root, eyeOff), target: add(root, targetOff) });
}
const COV_DIR = norm([0.94, 0.14, 0.30]);   // mostly side-on, so 100% coverage is the flank filling the frame
const COV_TARGET = [0.0, 0.58, -0.10];
const covCam = dist => relCam(add(COV_TARGET, scale(COV_DIR, dist)), COV_TARGET);

/** A camera that tracks a posed joint (bone b's proximal joint), for the joint close-ups. */
function jointCam(b, joint, off) {
  return (root, pose) => { const target = apply(pose.world[b], joint); return { eye: add(target, off), target }; };
}

/** The time in the walk cycle where a joint is bent furthest from rest in direction `sign`
 *  (−1: flexion for the elbow and hock; +1: the neck lowered). */
function maxFlexTime(key, sign) {
  let best = -Infinity, bt = 0;
  for (let i = 0; i < 108; i++) {
    const t = 0.5 + i * WALK.period / 108;
    const a = sign * poseWolf(t).info.alpha[key];
    if (a > best) { best = a; bt = t; }
  }
  return bt;
}

const SCENES = {
  hero: { cam: relCam([1.30, 0.42, 1.65], [0.0, 0.50, 0.02]) },
  portrait: { cam: relCam([0.62, 0.86, 1.08], [0.02, 0.76, 0.46]) },
  elbow: { cam: jointCam(10, J.ELBOW, [0.40, 0.05, 0.16]), t: maxFlexTime('elbow_L', -1) },
  hock: { cam: jointCam(14, J.HOCK, [0.40, 0.06, 0.02]), t: maxFlexTime('hock_L', -1) },
  neck: { cam: jointCam(3, J.NECK, [0.80, 0.10, -0.02]), t: maxFlexTime('neck', 1) },
};

/** Writes the frame uniform and pose buffer for one frame. Returns CPU time and warp factors. */
function update(cam, res, t, variant, mode, extra = {}) {
  const c0 = performance.now();
  const pose = poseWolf(t);
  const root = pose.root;
  const { eye, target } = cam(root, pose);
  const f = norm(sub(target, eye));
  const rr = norm(cross(f, [0, 1, 0]));
  const uu = cross(rr, f);
  const ty = Math.tan(FOVY / 2), tx = ty * res.w / res.h;
  const fr = new ArrayBuffer(FRAME_BYTES), fl = new Float32Array(fr), fu = new Uint32Array(fr);
  fl.set([...eye, 2 * ty / res.h], 0);
  fl.set([...f, 0], 4);
  fl.set([...scale(rr, tx), 0], 8);
  fl.set([...scale(uu, ty), 0], 12);
  fl.set([...SUN, t], 16);
  const lz = SUN, lx = norm(cross([0, 1, 0], lz)), ly = cross(lz, lx);
  fl.set([...add(root, [0, 0.5, 0]), 1.15], 20);
  fl.set([...lx, 0], 24);
  fl.set([...ly, 0], 28);
  fl.set([...add(root, VOL.off), VOL.cell], 32);
  fu.set([res.w, res.h, Math.ceil(res.w / 8), Math.ceil(res.h / 8)], 36);
  fu.set([LIGHT_CELLS, extra.sample ?? 0, extra.spp ?? 1, 0], 40);
  fu.set([...VOL.dims, mode === 'warp' ? ALL_WARP : ALL_RIGID], 44);
  const fur = isFur(variant);
  fl.set([FUR.hin, FUR.hout, FUR.tile, fur ? FUR.hout : 0], 48);
  fl.set([...root, 0], 52);
  const pd = new Float32Array(PL.SIZE * 4);
  const { L: Ls } = writePose(pd, pose, rest, mode, fur);
  const cpu = performance.now() - c0;
  device.queue.writeBuffer(B.frame, 0, fr);
  device.queue.writeBuffer(B.pose, 0, pd);
  if (raster) raster.update(pose, eye, target, res, fr);
  return { cpu, L: Ls.length ? Math.max(...Ls) : 1, pose };
}

/** Submits one traced frame: refit (warp mode) and the bins with the first band, then one
 *  submission per remaining band. Timestamp pairs: 0 refit, 1 bins, 2 + k band k. */
function submitTraced(res, variant, mode, { qs = null, q0 = 0, bands = 1 } = {}) {
  const ts = i => (qs ? { timestampWrites: { querySet: qs, beginningOfPassWriteIndex: q0 + 2 * i, endOfPassWriteIndex: q0 + 2 * i + 1 } } : {});
  const rs = RES[res.name];
  const creature = VARIANTS[variant].CREATURE !== 0;
  const set = bandSet(rs, bands);
  set.forEach((b, k) => {
    const enc = device.createCommandEncoder();
    if (k === 0 && creature) {
      if (mode === 'warp') {
        const p = enc.beginComputePass(ts(0));
        p.setBindGroup(0, B.refit);
        p.setPipeline(P.refit_clear);
        p.dispatchWorkgroups(2);
        p.setPipeline(P.refit_sample);
        p.dispatchWorkgroups(32 * 32 * 32 / 64, 5);
        p.setPipeline(P.refit_finalize);
        p.dispatchWorkgroups(1);
        p.end();
      }
      const p = enc.beginComputePass(ts(1));
      p.setBindGroup(0, rs.bin);
      p.setPipeline(P.bin_screen);
      p.dispatchWorkgroups(Math.ceil(rs.tx / 8), Math.ceil(rs.ty / 8));
      p.setPipeline(P.bin_light);
      p.dispatchWorkgroups(LIGHT_CELLS / 8, LIGHT_CELLS / 8);
      p.setPipeline(P.bin_volume);
      p.dispatchWorkgroups(Math.ceil(VOL.dims[0] / 4), Math.ceil(VOL.dims[1] / 4), Math.ceil(VOL.dims[2] / 4));
      p.end();
    }
    const p = enc.beginComputePass(ts(2 + k));
    p.setPipeline(P[`trace_${variant}`]);
    p.setBindGroup(0, b.bg);
    p.dispatchWorkgroups(rs.tx, b.rows);
    p.end();
    device.queue.submit([enc.finish()]);
  });
  return set.length;
}

/** Untimed frames (screenshots, stats, image grabs) always run in bands, awaiting each. */
const DRAW_BANDS = { '1080p': 8, '540p': 2 };
async function submitAwaited(res, variant, mode, bands = DRAW_BANDS[res.name]) {
  const rs = RES[res.name];
  const set = bandSet(rs, bands);
  // The first submission carries the bins; then band by band.
  for (let k = 0; k < set.length; k++) {
    const enc = device.createCommandEncoder();
    if (k === 0 && VARIANTS[variant].CREATURE !== 0) {
      if (mode === 'warp') {
        const p = enc.beginComputePass();
        p.setBindGroup(0, B.refit);
        p.setPipeline(P.refit_clear); p.dispatchWorkgroups(2);
        p.setPipeline(P.refit_sample); p.dispatchWorkgroups(32 * 32 * 32 / 64, 5);
        p.setPipeline(P.refit_finalize); p.dispatchWorkgroups(1);
        p.end();
      }
      const p = enc.beginComputePass();
      p.setBindGroup(0, rs.bin);
      p.setPipeline(P.bin_screen); p.dispatchWorkgroups(Math.ceil(rs.tx / 8), Math.ceil(rs.ty / 8));
      p.setPipeline(P.bin_light); p.dispatchWorkgroups(LIGHT_CELLS / 8, LIGHT_CELLS / 8);
      p.setPipeline(P.bin_volume); p.dispatchWorkgroups(Math.ceil(VOL.dims[0] / 4), Math.ceil(VOL.dims[1] / 4), Math.ceil(VOL.dims[2] / 4));
      p.end();
    }
    const p = enc.beginComputePass();
    p.setPipeline(P[`trace_${variant}`]);
    p.setBindGroup(0, set[k].bg);
    p.dispatchWorkgroups(rs.tx, set[k].rows);
    p.end();
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
  }
}

function present(res) {
  const enc = device.createCommandEncoder();
  const p = enc.beginRenderPass({ colorAttachments: [{ view: ctx.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
  p.setPipeline(P.blit);
  p.setBindGroup(0, RES[res.name].blit);
  p.draw(3);
  p.end();
  device.queue.submit([enc.finish()]);
}

async function snapshot(name, res = FULL) {
  present(res);
  const blob = await new Promise(r => $('view').toBlob(r, 'image/png'));
  if (blob) await save(`${name}.png`, blob);
}

async function draw(cam, res, variant, mode, t = T_SNAP) {
  update(cam, res, t, variant, mode);
  await submitAwaited(res, variant, mode);
}

async function resolveTimestamps(qs, count) {
  const buf = buffer(count * 8, U.QUERY_RESOLVE | U.COPY_SRC);
  const enc = device.createCommandEncoder();
  enc.resolveQuerySet(qs, 0, count, buf, 0);
  device.queue.submit([enc.finish()]);
  const raw = await readback([{ src: buf, size: count * 8 }]);
  buf.destroy();
  qs.destroy();
  return new BigUint64Array(raw, 0, count);
}

/** How many row bands a configuration needs so each submission stays near 30 ms: one probe frame
 *  in 8 awaited bands, timed with timestamps. */
async function chooseBands(cam, res, variant, mode) {
  const n = 8;
  const set = bandSet(RES[res.name], n);
  const qs = device.createQuerySet({ type: 'timestamp', count: QF });
  update(cam, res, T_SNAP, variant, mode);
  const rs = RES[res.name];
  for (let k = 0; k < set.length; k++) {
    const enc = device.createCommandEncoder();
    if (k === 0) { enc.clearBuffer(B.stats); }
    if (k === 0 && VARIANTS[variant].CREATURE !== 0) {
      const p = enc.beginComputePass();
      p.setBindGroup(0, rs.bin);
      p.setPipeline(P.bin_screen); p.dispatchWorkgroups(Math.ceil(rs.tx / 8), Math.ceil(rs.ty / 8));
      p.setPipeline(P.bin_light); p.dispatchWorkgroups(LIGHT_CELLS / 8, LIGHT_CELLS / 8);
      p.setPipeline(P.bin_volume); p.dispatchWorkgroups(Math.ceil(VOL.dims[0] / 4), Math.ceil(VOL.dims[1] / 4), Math.ceil(VOL.dims[2] / 4));
      p.end();
    }
    const p = enc.beginComputePass({ timestampWrites: { querySet: qs, beginningOfPassWriteIndex: 2 * k, endOfPassWriteIndex: 2 * k + 1 } });
    p.setPipeline(P[`trace_${variant}`]);
    p.setBindGroup(0, set[k].bg);
    p.dispatchWorkgroups(rs.tx, set[k].rows);
    p.end();
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
  }
  const ts = await resolveTimestamps(qs, 2 * set.length);
  let total = 0, worst = 0;
  for (let k = 0; k < set.length; k++) { const ms = Number(ts[2 * k + 1] - ts[2 * k]) / 1e6; total += ms; worst = Math.max(worst, ms * set.length); }
  // Bands are uneven (the creature isn't spread evenly), so size them by the worst band.
  return { bands: Math.min(MAX_BANDS, Math.max(1, Math.ceil(Math.max(total, worst) / 30))), ms: total };
}

/** GPU time per pass under sustained load: 30 untimed frames, then `frames` back to back. The wolf
 *  walks (t advances 1/60 s per frame) and the camera follows it. Each frame is split into as many
 *  row-band submissions as `chooseBands` asks for; trace time is the sum of the bands. */
async function measure(cam, res, variant, mode, frames = 90) {
  const probe = await chooseBands(cam, res, variant, mode);
  const bands = probe.bands;
  // Protocol: 30 warm-up frames then 90 timed. Frames over ~17 ms would make that minutes per
  // configuration, so heavy ones get ≥ 0.5 s of warm-up and ≥ 1.5 s (and ≥ 20 frames) timed instead.
  const warm = Math.min(30, Math.max(4, Math.ceil(500 / probe.ms)));
  frames = Math.min(frames, Math.max(20, Math.ceil(1500 / probe.ms)));
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * QF });
  const t0 = T_SNAP - frames / 120;
  for (let f = 0; f < warm; f++) {
    update(cam, res, t0 + f / 60, variant, mode);
    submitTraced(res, variant, mode, { bands });
  }
  await device.queue.onSubmittedWorkDone();
  const cpu = [], Ls = [];
  const w0 = performance.now();
  let nb = 1;
  for (let f = 0; f < frames; f++) {
    const u = update(cam, res, t0 + f / 60, variant, mode);
    cpu.push(u.cpu);
    Ls.push(u.L);
    nb = submitTraced(res, variant, mode, { qs, q0: f * QF, bands });
  }
  await device.queue.onSubmittedWorkDone();
  const wall = (performance.now() - w0) / frames;
  const ts = await resolveTimestamps(qs, frames * QF);
  const creature = VARIANTS[variant].CREATURE !== 0;
  const d = (f, i) => Number(ts[f * QF + 2 * i + 1] - ts[f * QF + 2 * i]) / 1e6;
  const all = [...Array(frames).keys()];
  const refit = all.map(f => (mode === 'warp' && creature ? d(f, 0) : 0));
  const bins = all.map(f => (creature ? d(f, 1) : 0));
  const trace = all.map(f => { let s = 0; for (let k = 0; k < nb; k++) s += d(f, 2 + k); return s; });
  const sum = all.map(f => refit[f] + bins[f] + trace[f]);
  return {
    refit: r3(median(refit)), bins: r3(median(bins)), trace: r3(median(trace)), trace_p95: r3(pct(trace, 0.95)),
    total: r3(median(sum)), total_p95: r3(pct(sum, 0.95)), wall_ms_per_frame: r3(wall), bands: nb, warmup_frames: warm, timed_frames: frames,
    cpu_pose_ms: r3(median(cpu)), warp_L_max: r3(Math.max(...Ls)), perFrame: sum,
  };
}

async function drawRaster(cam, res, shells, t = T_SNAP, creature = true) {
  update(cam, res, t, 's', 'rigid');
  const enc = device.createCommandEncoder();
  raster.encode(enc, res, RES[res.name].view, shells, null, 0, creature);
  device.queue.submit([enc.finish()]);
  await device.queue.onSubmittedWorkDone();
}

/** Raster frames under sustained load, as `measure`. Pairs: shadow, ground, mesh, shells. */
async function measureRaster(cam, res, shells, creature = true, frames = 90) {
  const Q = 8;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  const t0 = T_SNAP - frames / 120;
  for (let f = 0; f < 30; f++) {
    update(cam, res, t0 + f / 60, 's', 'rigid');
    const enc = device.createCommandEncoder();
    raster.encode(enc, res, RES[res.name].view, shells, null, 0, creature);
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();
  for (let f = 0; f < frames; f++) {
    update(cam, res, t0 + f / 60, 's', 'rigid');
    const enc = device.createCommandEncoder();
    raster.encode(enc, res, RES[res.name].view, shells, qs, f * Q, creature);
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * Q);
  const d = (f, i) => Number(ts[f * Q + 2 * i + 1] - ts[f * Q + 2 * i]) / 1e6;
  const all = [...Array(frames).keys()];
  const shadow = all.map(f => (creature ? d(f, 0) : 0));
  const ground = all.map(f => d(f, 1));
  const mesh = all.map(f => (creature ? d(f, 2) : 0));
  const shell = all.map(f => (creature && shells ? d(f, 3) : 0));
  const creatureMs = all.map(f => shadow[f] + mesh[f] + shell[f]);
  return {
    shadow: r3(median(shadow)), ground: r3(median(ground)), mesh: r3(median(mesh)), shells: r3(median(shell)),
    creature: r3(median(creatureMs)), creature_p95: r3(pct(creatureMs, 0.95)),
  };
}

/** The 60 Hz half of the protocol: frames submitted at 60 Hz by busy-wait (banded as measured). */
async function paced(cam, res, variant, mode, frames = 110) {   // 110 × 36 query slots < 4096
  const bands = (await chooseBands(cam, res, variant, mode)).bands;
  const period = 1000 / 60;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * QF });
  const t0 = performance.now() + 20;
  let nb = 1;
  for (let f = 0; f < frames; f++) {
    const deadline = t0 + f * period;
    while (performance.now() < deadline) { /* spin */ }
    update(cam, res, f / 60, variant, mode);
    nb = submitTraced(res, variant, mode, { qs, q0: f * QF, bands });
  }
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * QF);
  const keep = [...Array(frames).keys()].slice(30);
  const first = mode === 'warp' ? 0 : 1;
  const gpu = keep.map(f => Number(ts[f * QF + 2 * (2 + nb - 1) + 1] - ts[f * QF + 2 * first]) / 1e6);
  const sorted = [...gpu].sort((a, b) => a - b);
  return {
    pacing: 'busy-wait at 60 Hz', frames: keep.length, bands: nb,
    frame_gpu_median: r3(median(gpu)), frame_gpu_p95: r3(sorted[Math.floor(sorted.length * 0.95)]),
    frame_gpu_max: r3(sorted[sorted.length - 1]), frame_gpu_over_16_7: gpu.filter(g => g > 16.7).length,
  };
}

async function stats(cam, res, variant, mode, t = T_SNAP) {
  update(cam, res, t, variant, mode);
  const enc = device.createCommandEncoder();
  enc.clearBuffer(B.stats);
  device.queue.submit([enc.finish()]);
  await submitAwaited(res, variant, mode);
  const s = new Uint32Array(await readback([{ src: B.stats, size: 128 }]));
  const o = {};
  STAT_NAMES.forEach((n, i) => { o[n] = s[i]; });
  const px = o.creature_px;
  o.coverage = r3(px / (res.w * res.h));
  o.steps_per_creature_px = r3(o.hit_steps / Math.max(px, 1));
  o.fur_steps_per_creature_px = r3(o.fur_steps / Math.max(px, 1));
  o.part_evals_per_px = r3(o.evals / (res.w * res.h));
  o.parts_per_ray = r3(o.ray_parts / Math.max(o.rays, 1));
  o.tile_parts_per_px = r3(o.tile_parts / (res.w * res.h));
  o.cap_share_of_creature_px = o.caps / Math.max(px, 1);
  o.shadow_steps_per_px = r3(o.shadow_steps / (res.w * res.h));
  o.stops_per_creature_px = r3(o.stops / Math.max(px, 1));
  o.jumps_per_creature_px = r3(o.jumps / Math.max(px, 1));
  return o;
}

// ---- images and the reference --------------------------------------------------------------------------

const BYTE = new Uint8Array(65536);
for (let h = 0; h < 65536; h++) {
  const s = h >> 15, e = (h >> 10) & 31, m = h & 1023;
  let v = e === 0 ? m * 2 ** -24 : e === 31 ? 1 : (1 + m / 1024) * 2 ** (e - 15);
  if (s) v = 0;
  BYTE[h] = Math.round(255 * Math.pow(Math.min(Math.max(v, 0), 1), 1 / 2.2));
}
const toByteF = v => Math.round(255 * Math.pow(Math.min(Math.max(v, 0), 1), 1 / 2.2));

async function readFrame(res) {
  const rs = RES[res.name];
  const bpr = Math.ceil(res.w * 8 / 256) * 256;
  const out = device.createBuffer({ size: bpr * res.h, usage: U.COPY_DST | U.MAP_READ });
  const enc = device.createCommandEncoder();
  enc.copyTextureToBuffer({ texture: rs.tex }, { buffer: out, bytesPerRow: bpr, rowsPerImage: res.h }, [res.w, res.h]);
  device.queue.submit([enc.finish()]);
  await out.mapAsync(GPUMapMode.READ);
  const raw = new Uint16Array(out.getMappedRange().slice(0));
  out.unmap();
  out.destroy();
  const px = new Uint8Array(res.w * res.h * 3);
  for (let y = 0; y < res.h; y++) for (let x = 0; x < res.w; x++) {
    const i = (y * bpr) / 2 + x * 4, j = (y * res.w + x) * 3;
    for (let c = 0; c < 3; c++) px[j + c] = BYTE[raw[i + c]];
  }
  return px;
}

async function grab(cam, res, variant, mode, t = T_SNAP) {
  await draw(cam, res, variant, mode, t);
  return readFrame(res);
}

/** The brute-force reference: `spp` jittered samples (one centred sample when spp = 1). Every sample
 *  runs one tile row (8 px) at a time, one submission each, awaited, so no submission runs long
 *  (GPU safety rules). `crop` = [x, y, w, h] in tiles limits it to part of the frame. Accumulates in
 *  a float buffer; returns display bytes for the whole frame (zero outside the crop). */
async function reference(cam, res, variant, mode, spp, t = T_SNAP, crop = null) {
  const rs = RES[res.name];
  const [cx, cy, cw, ch] = crop || [0, 0, rs.tx, rs.ty];
  // Chunks of one tile row × at most 15 tiles: each its own submission, awaited.
  const sets = [];
  for (let x = cx; x < cx + cw; x += 15) sets.push(bandSet(rs, ch, [x, cy, Math.min(15, cx + cw - x), ch]));
  const enc0 = device.createCommandEncoder();
  enc0.clearBuffer(B.accum);
  device.queue.submit([enc0.finish()]);
  const t0 = performance.now();
  let worst = 0;
  for (let s = 0; s < spp; s++) {
    update(cam, res, t, variant, mode, { sample: s, spp });
    for (const set of sets) for (const b of set) {
      const w0 = performance.now();
      const enc = device.createCommandEncoder();
      const p = enc.beginComputePass();
      p.setPipeline(P[`trace_${variant}`]);
      p.setBindGroup(0, b.bg);
      p.dispatchWorkgroups(b.cols, b.rows);
      p.end();
      device.queue.submit([enc.finish()]);
      await device.queue.onSubmittedWorkDone();
      worst = Math.max(worst, performance.now() - w0);
    }
  }
  const ms = performance.now() - t0;
  if (worst > 100) err(`reference ${variant}: a band took ${worst.toFixed(0)} ms (wall clock, incl. latency)`);
  const f = new Float32Array(await readback([{ src: B.accum, size: res.w * res.h * 16 }]));
  const px = new Uint8Array(res.w * res.h * 3);
  for (let i = 0, j = 0; i < f.length; i += 4) for (let c = 0; c < 3; c++) px[j++] = toByteF(f[i + c]);
  return { px, ms, worst_band_wall_ms: worst };
}

/** Pixels of an image inside a crop (tiles), for comparisons limited to the crop. */
function cropPixels(px, res, crop) {
  const [cx, cy, cw, ch] = crop.map(v => v * 8);
  const out = new Uint8Array(cw * ch * 3);
  for (let y = 0; y < ch; y++) out.set(px.subarray(((cy + y) * res.w + cx) * 3, ((cy + y) * res.w + cx + cw) * 3), y * cw * 3);
  return out;
}

function diff(a, b, creaturePx) {
  let sum = 0, over8 = 0, max = 0;
  for (let i = 0; i < a.length; i += 3) {
    let m = 0;
    for (let c = 0; c < 3; c++) { const d = Math.abs(a[i + c] - b[i + c]); sum += d; m = Math.max(m, d); }
    if (m > 8) over8++;
    max = Math.max(max, m);
  }
  const n = a.length / 3;
  const o = { mean_abs_255: r3(sum / a.length), share_over_8: r3(over8 / n * 1e4) / 1e4, max };
  if (creaturePx) o.over_8_per_creature_px = r3(over8 / creaturePx * 1e4) / 1e4;
  return o;
}

async function savePixels(name, px, w, h) {
  const c = document.createElement('canvas');
  c.width = w; c.height = h;
  const g = c.getContext('2d'), img = g.createImageData(w, h);
  for (let i = 0, j = 0; i < px.length; i += 3, j += 4) { img.data[j] = px[i]; img.data[j + 1] = px[i + 1]; img.data[j + 2] = px[i + 2]; img.data[j + 3] = 255; }
  g.putImageData(img, 0, 0);
  const blob = await new Promise(r => c.toBlob(r, 'image/png'));
  await save(`${name}.png`, blob);
}

async function saveDiff(name, a, b, w, h) {
  const px = new Uint8Array(a.length);
  for (let i = 0; i < a.length; i += 3) {
    let m = 0;
    for (let k = 0; k < 3; k++) m = Math.max(m, Math.abs(a[i + k] - b[i + k]));
    const base = (a[i] + a[i + 1] + a[i + 2]) / 12;
    px[i] = m > 8 ? 255 : base; px[i + 1] = m > 8 ? 0 : (m > 2 ? 200 : base); px[i + 2] = base;
  }
  await savePixels(name, px, w, h);
}

// ---- coverage search ------------------------------------------------------------------------------------

async function coverageOf(dist, res = HALF, t = T_SNAP) {
  return (await stats(covCam(dist), res, 'stats_s', 'rigid', t)).coverage;
}

/** Camera distance (along COV_DIR) giving the target coverage. Coverage falls with distance. */
async function findDistance(target) {
  let lo = 0.2, hi = 30;
  for (let i = 0; i < 16; i++) {
    const mid = Math.sqrt(lo * hi);
    const c = await coverageOf(mid);
    if (c >= target) lo = mid; else hi = mid;
  }
  return lo;
}

// ---- run -----------------------------------------------------------------------------------------------

async function setup(only) {
  await init();
  log('device', JSON.stringify(R.device));
  resources();
  await pipelines(only);
  log('pipelines', JSON.stringify({ ms: R.setup.pipelines_ms, variants: R.setup.trace_variants }));
  await startupBounds();
  log('bounds', JSON.stringify(R.setup.rest_boxes));
  raster = new Raster(device, src, { FULL, HALF, FUR, module, buffer, readback, log, err, canvasFormat, strandView: T.strandView, samp: T.samp, frameBuffer: B.frame, NB });
  await raster.init();
  R.setup.raster = raster.info;
  log('raster', JSON.stringify(raster.info));
  R.setup.params = { fovy_deg: FOVY * 180 / Math.PI, sun: SUN, fur: FUR, light_cells: LIGHT_CELLS, ao_volume: VOL, cov_dir: COV_DIR, cov_target: COV_TARGET, T_SNAP };
}

/** A light smoke test (~10 s): a few scenes, one frame each, screenshots, a stats line. */
async function quick() {
  const t0 = performance.now();
  try {
    await setup(['s', 'v', 'stats_v']);
    const scenes = { hero: SCENES.hero, portrait: SCENES.portrait, hock: SCENES.hock, close: { cam: covCam(0.55) } };
    for (const [name, sc] of Object.entries(scenes)) {
      for (const [v, mode] of [['s', 'rigid'], ['v', 'warp']]) {
        await draw(sc.cam, FULL, v, mode, sc.t ?? T_SNAP);
        await snapshot(`quick-${name}-${mode}-${v}`);
      }
    }
    log('close stats_v warp', JSON.stringify(await stats(scenes.close.cam, FULL, 'stats_v', 'warp')));
    await drawRaster(SCENES.hero.cam, FULL, true);
    await snapshot('quick-hero-raster-shells');
    log(`quick done in ${((performance.now() - t0) / 1000).toFixed(1)} s`);
  } catch (e) {
    err('FAILED:', e.stack || e.message);
  }
  await save('DONE', 'quick');
}

/** Development probe: the heaviest frame and the reference's cost, to size #run. */
async function probe() {
  try {
    await setup(['v', 'ref_v']);
    const cam = covCam(0.45);
    const crop = [105, 59, 30, 17];
    const cut = px => cropPixels(px, FULL, crop);
    const r16 = await reference(cam, FULL, 'ref_v', 'warp', 16, T_SNAP, crop);
    const r1 = await reference(cam, FULL, 'ref_v', 'warp', 1, T_SNAP, crop);
    const fast = await grab(cam, FULL, 'v', 'warp');
    log('probe ref16', JSON.stringify({ ms: r3(r16.ms), worst_band_wall_ms: r3(r16.worst_band_wall_ms) }));
    log('probe ref1 vs ref16', JSON.stringify(diff(cut(r1.px), cut(r16.px))), 'fast vs ref16', JSON.stringify(diff(cut(fast), cut(r16.px))));
    await savePixels('probe-ref16', cut(r16.px), 240, 136);
    await savePixels('probe-ref1', cut(r1.px), 240, 136);
    await savePixels('probe-fast', cut(fast), 240, 136);
    await draw(cam, FULL, 'v', 'warp');
    await snapshot('probe-cov-warp-v');
  } catch (e) {
    err('FAILED:', e.stack || e.message);
  }
  await save('DONE', 'probe');
}

window.__api = { setup, draw, drawRaster, snapshot, stats, measure, measureRaster, grab, reference, diff, SCENES, covCam, FULL, HALF, update, poseWolf };

const runApi = () => ({ ...window.__api, R, log, err, save, paced, findDistance, coverageOf, saveDiff, savePixels, readFrame, cropPixels, T_SNAP });
$('run').addEventListener('click', () => import('./run.js').then(m => m.runAll(runApi())));
$('quick').addEventListener('click', quick);
if (location.hash === '#quick') quick();
if (location.hash === '#probe') probe();
if (location.hash === '#run') import('./run.js').then(m => m.runAll(runApi()));
