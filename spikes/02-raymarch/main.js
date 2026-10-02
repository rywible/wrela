// Spike harness: builds the pipelines, poses the herd, traces the scenes, times all of it.
// This file plays the engine's role plus the measuring. It isn't compiler output.

import { makeInstance, writeInstance, STRIDE } from './pose.js';
import { terrainHeight, rng } from '../01-grazer/grazer.js';

const W = 1920, H = 1080, TX = W / 8, TY = H / 8, GRID = 128, BIN_WORDS = 33;
const FOVY = 40 * Math.PI / 180;
const U = GPUBufferUsage;

// Trace variants: pipeline-overridable constants (trace.wgsl).
const VARIANTS = {
  full: {},
  displaced: { DISPLACED: 1 },
  relax12: { RELAX: 1.2 },
  relax16: { RELAX: 1.6 },
  eps50: { EPS_SCALE: 0.5 },
  eps10: { EPS_SCALE: 0.1 },
  noshadow: { SHADOWS: 0 },
  terrain: { CREATURES: 0, SHADOWS: 0 },
  stats: { STATS: 1 },
  stats_displaced: { STATS: 1, DISPLACED: 1 },
  heat: { VIEW: 1 },
  heat_shadow: { VIEW: 2 },
  ref: { RELAX: 1, STEP_SCALE: 0.5, EPS_SCALE: 0.05, MAX_STEPS: 4000, SHADOW_STEPS: 2000, TERRAIN_STEPS: 4000 },
  ref_displaced: { DISPLACED: 1, RELAX: 1, STEP_SCALE: 0.5, EPS_SCALE: 0.05, MAX_STEPS: 4000, SHADOW_STEPS: 2000, TERRAIN_STEPS: 4000 },
};
const ROUNDS = 3;
const MEASURED = ['full', 'eps50', 'eps10', 'displaced', 'relax12', 'relax16', 'noshadow', 'terrain'];
const T_SNAP = 89 / 60;   // the time of spike 01's screenshots (the last frame of its measuring loop)
const STAT_NAMES = ['creature_px', 'terrain_px', 'sky_px', 'marches', 'steps', 'misses', 'caps', 'shadow_px',
  'shadow_marches', 'shadow_steps', 'terrain_steps', 'screen_overflow', 'light_overflow', 'candidate_overflow',
  'part_evals', 'detail_evals', 'tile_entries', 'relax_fails', 'hit_pixel_steps', 'shadow_caps'];

const $ = id => document.getElementById(id);
const log = (...a) => { $('log').textContent += a.join(' ') + '\n'; console.log(...a); };
const R = { started: new Date().toISOString(), device: {}, pipelines: {}, scenes: {}, frames: {}, stats: {}, quality: {}, paced: {} };
window.__results = R;

let device, ctx, canvasFormat;
const src = {}, L = {}, P = {}, T = {}, B = {};

const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[s.length >> 1] : NaN; };
const r3 = x => Math.round(x * 1000) / 1000;

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
  try { await fetch(`results/${name}`, { method: 'PUT', body }); } catch (e) { log('save failed', name, e.message); }
}

// ---- setup -----------------------------------------------------------------------------------

async function init() {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
  const want = ['timestamp-query'].filter(f => adapter.features.has(f));
  device = await adapter.requestDevice({ requiredFeatures: want });
  device.lost.then(i => log('DEVICE LOST:', i.message));
  device.addEventListener('uncapturederror', e => log('GPU ERROR:', e.error.message));
  const info = adapter.info || {};
  R.device = {
    vendor: info.vendor, architecture: info.architecture, description: info.description,
    userAgent: navigator.userAgent, timestampQuery: want.includes('timestamp-query'),
    pageVisibility: document.visibilityState,
  };
  if (!R.device.timestampQuery) throw new Error('timestamp-query is required');
  for (const f of ['common', 'field', 'bin', 'trace', 'blit']) src[f] = await (await fetch(`${f}.wgsl`)).text();

  const canvas = $('view');
  canvas.width = W;
  canvas.height = H;
  ctx = canvas.getContext('webgpu');
  canvasFormat = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format: canvasFormat, alphaMode: 'opaque' });

  const C = GPUShaderStage.COMPUTE;
  const e = (binding, type) => ({ binding, visibility: C, buffer: { type } });
  L.bin = device.createBindGroupLayout({ entries: [e(0, 'uniform'), e(1, 'read-only-storage'), e(2, 'storage'), e(3, 'storage')] });
  L.trace = device.createBindGroupLayout({
    entries: [e(0, 'uniform'), e(1, 'read-only-storage'), e(2, 'read-only-storage'), e(3, 'read-only-storage'),
      { binding: 4, visibility: C, storageTexture: { access: 'write-only', format: 'rgba16float' } }, e(5, 'storage')],
  });
  L.blit = device.createBindGroupLayout({
    entries: [{ binding: 0, visibility: GPUShaderStage.FRAGMENT, texture: {} }, { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: {} }],
  });
}

const salted = (code, salt) => salt == null ? code : code.replace('const SALT: f32 = 0.0;', `const SALT: f32 = ${salt.toFixed(4)};`);

async function checkModule(module, label) {
  const info = await module.getCompilationInfo();
  for (const m of info.messages) log(`${label} ${m.type} line ${m.lineNum}:${m.linePos} ${m.message}`);
  if (info.messages.some(m => m.type === 'error')) throw new Error(`${label}: WGSL errors`);
}

/** The scene's pipelines (bin_screen, bin_light, one trace), timed; plus every trace variant when
 *  `variants` is set. `salt` makes the source unique so no cache can serve it. */
async function pipelines(salt, parallel, variants) {
  const t0 = performance.now();
  const bm = device.createShaderModule({ code: salted(src.common, salt) + src.field + src.bin, label: 'bin' });
  const tm = device.createShaderModule({ code: salted(src.common, salt) + src.field + src.trace, label: 'trace' });
  const layout = bgl => device.createPipelineLayout({ bindGroupLayouts: [bgl] });
  const list = [
    ['bin_screen', () => device.createComputePipelineAsync({ layout: layout(L.bin), compute: { module: bm, entryPoint: 'bin_screen' } })],
    ['bin_light', () => device.createComputePipelineAsync({ layout: layout(L.bin), compute: { module: bm, entryPoint: 'bin_light' } })],
  ];
  for (const [name, constants] of Object.entries(VARIANTS)) {
    if (!variants && name !== 'full') continue;
    list.push([`trace_${name}`, () => device.createComputePipelineAsync({ layout: layout(L.trace), compute: { module: tm, entryPoint: 'trace', constants } })]);
  }
  const out = {}, ms = {};
  try {
    if (parallel) {
      const ps = await Promise.all(list.map(([, f]) => f()));
      list.forEach(([n], i) => { out[n] = ps[i]; });
    } else {
      for (const [n, f] of list) { const t = performance.now(); out[n] = await f(); ms[n] = r3(performance.now() - t); }
    }
  } catch (err) {
    await checkModule(bm, 'bin.wgsl');
    await checkModule(tm, 'trace.wgsl');
    throw err;
  }
  ms.total = r3(performance.now() - t0);
  return { out, ms };
}

async function blitPipeline() {
  const bm = device.createShaderModule({ code: src.blit, label: 'blit' });
  P.blit = await device.createRenderPipelineAsync({
    layout: device.createPipelineLayout({ bindGroupLayouts: [L.blit] }),
    vertex: { module: bm, entryPoint: 'blit_vs' },
    fragment: { module: bm, entryPoint: 'blit_fs', targets: [{ format: canvasFormat }] },
  });
}

function resources(maxInst) {
  T.frame = device.createTexture({ size: [W, H], format: 'rgba16float', usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_SRC });
  T.frameView = T.frame.createView();
  B.frame = buffer(160, U.UNIFORM | U.COPY_DST);
  B.inst = buffer(maxInst * STRIDE * 16, U.STORAGE | U.COPY_DST);
  B.tiles = buffer(TX * TY * BIN_WORDS * 4, U.STORAGE);
  B.lbins = buffer(GRID * GRID * BIN_WORDS * 4, U.STORAGE);
  B.stats = buffer(32 * 4, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  const res = b => ({ resource: { buffer: b } });
  B.binScreen = device.createBindGroup({ layout: L.bin, entries: [B.frame, B.inst, B.tiles, B.stats].map((b, i) => ({ binding: i, ...res(b) })) });
  B.binLight = device.createBindGroup({ layout: L.bin, entries: [B.frame, B.inst, B.lbins, B.stats].map((b, i) => ({ binding: i, ...res(b) })) });
  B.trace = device.createBindGroup({
    layout: L.trace,
    entries: [
      { binding: 0, ...res(B.frame) }, { binding: 1, ...res(B.inst) }, { binding: 2, ...res(B.tiles) },
      { binding: 3, ...res(B.lbins) }, { binding: 4, resource: T.frameView }, { binding: 5, ...res(B.stats) },
    ],
  });
  B.blit = device.createBindGroup({
    layout: L.blit,
    entries: [{ binding: 0, resource: T.frameView }, { binding: 1, resource: device.createSampler({ magFilter: 'linear', minFilter: 'linear' }) }],
  });
  R.memory_mb = {
    instances_per_frame: r3(maxInst * STRIDE * 16 / 2 ** 20),
    tile_lists: r3(TX * TY * BIN_WORDS * 4 / 2 ** 20),
    light_lists: r3(GRID * GRID * BIN_WORDS * 4 / 2 ** 20),
    frame_rgba16f: r3(W * H * 8 / 2 ** 20),
    meshes: 0,
  };
}

// ---- scenes ------------------------------------------------------------------------------------

/** Spike 01's herd layout, same generator and seed. */
function herdInstances(n = 40, cols = 8, dx = 4.0, dz = 4.6) {
  const r = rng(4242), out = [];
  const rows = Math.ceil(n / cols);
  for (let i = 0; i < n; i++) {
    const row = Math.floor(i / cols), col = i % cols;
    const x = (col - (cols - 1) / 2) * dx + (r() - 0.5) * 1.6;
    const z = (row - (rows - 1) / 2) * dz + (r() - 0.5) * 1.6;
    out.push({ x, z, y: terrainHeight(x, z), yaw: Math.PI / 2 + (r() - 0.5) * 0.7, phase: r(), headDown: r() < 0.4 ? 1 : 0.2 * r() });
  }
  return out;
}

const norm = a => { const l = Math.hypot(...a); return a.map(x => x / l); };
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sub = (a, b) => a.map((x, i) => x - b[i]);

function scene(name, ins, places, eye, target, extent) {
  const sc = { name, ins, places, n: ins.length, extent };
  setCamera(sc, eye, target);
  sc.instData = new Float32Array(sc.n * STRIDE * 4);
  return sc;
}

function setCamera(sc, eye, target) {
  const f = norm(sub(target, eye));
  const r = norm(cross(f, [0, 1, 0]));
  const u = cross(r, f);
  const ty = Math.tan(FOVY / 2), tx = ty * W / H;
  const sun = norm([-0.5, 0.75, -0.45]);
  const lz = sun, lx = norm(cross([0, 1, 0], lz)), ly = cross(lz, lx);   // spike 01's light lookAt basis
  const fr = new ArrayBuffer(160), fl = new Float32Array(fr), fu = new Uint32Array(fr);
  fl.set([...eye, 2 * ty / H], 0);
  fl.set([...f, 0], 4);
  fl.set([...r.map(x => x * tx), 0], 8);
  fl.set([...u.map(x => x * ty), 0], 12);
  fl.set([...sun, 0], 16);
  fl.set([target[0], 0, target[2], sc.extent], 20);
  fl.set([...lx, 0], 24);
  fl.set([...ly, 0], 28);
  fu.set([W, H, TX, sc.n], 32);
  fu.set([GRID, 0, 0, 0], 36);
  sc.frame = fr;
  sc.eye = eye;
  sc.target = target;
}

/** Poses every instance on the CPU and uploads it. Returns the CPU time it took. */
function update(sc, t) {
  const t0 = performance.now();
  for (let i = 0; i < sc.n; i++) writeInstance(sc.ins[i], sc.places[i], t, sc.instData, i * STRIDE * 4);
  const ms = performance.now() - t0;
  device.queue.writeBuffer(B.frame, 0, sc.frame);
  device.queue.writeBuffer(B.inst, 0, sc.instData);
  return ms;
}

function encodeFrame(enc, sc, variant, qs, q0) {
  const ts = i => qs ? { timestampWrites: { querySet: qs, beginningOfPassWriteIndex: q0 + 2 * i, endOfPassWriteIndex: q0 + 2 * i + 1 } } : {};
  const creatures = !('CREATURES' in VARIANTS[variant]);
  const pass = (i, pipe, bg, x, y) => {
    const p = enc.beginComputePass(ts(i));
    p.setPipeline(pipe);
    p.setBindGroup(0, bg);
    p.dispatchWorkgroups(x, y);
    p.end();
  };
  if (creatures) {
    pass(0, P.bin_screen, B.binScreen, Math.ceil(TX / 8), Math.ceil(TY / 8));
    pass(1, P.bin_light, B.binLight, GRID / 8, GRID / 8);
  }
  pass(2, P[`trace_${variant}`], B.trace, TX, TY);
}

function present() {
  const enc = device.createCommandEncoder();
  const p = enc.beginRenderPass({ colorAttachments: [{ view: ctx.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
  p.setPipeline(P.blit);
  p.setBindGroup(0, B.blit);
  p.draw(3);
  p.end();
  device.queue.submit([enc.finish()]);
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

/** GPU time per pass under sustained load: 30 untimed frames, then `frames` back to back. */
async function measure(sc, variant, frames = 90) {
  const Q = 6;
  const creatures = !('CREATURES' in VARIANTS[variant]);
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  for (let f = 0; f < 30; f++) {
    update(sc, f / 60);
    const enc = device.createCommandEncoder();
    encodeFrame(enc, sc, variant, null, 0);
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();
  const cpu = [];
  const t1 = performance.now();
  for (let f = 0; f < frames; f++) {
    cpu.push(update(sc, f / 60));
    const enc = device.createCommandEncoder();
    encodeFrame(enc, sc, variant, qs, f * Q);
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();
  const throughput = (performance.now() - t1) / frames;
  const ts = await resolveTimestamps(qs, frames * Q);
  const d = (f, i) => Number(ts[f * Q + 2 * i + 1] - ts[f * Q + 2 * i]) / 1e6;
  const all = [...Array(frames).keys()];
  const m = fn => r3(median(all.map(fn)));
  return {
    bin_screen: creatures ? m(f => d(f, 0)) : 0,
    bin_light: creatures ? m(f => d(f, 1)) : 0,
    trace: m(f => d(f, 2)),
    frame_gpu: m(f => Number(ts[f * Q + 5] - ts[f * Q + (creatures ? 0 : 4)]) / 1e6),
    throughput_ms: r3(throughput),
    cpu_pose_ms: r3(median(cpu)),
  };
}

/** The 60 fps half of the kill criterion, as in spike 01: frames submitted at 60 Hz by busy-wait. */
async function paced(sc, variant, frames = 180) {
  const Q = 6, period = 1000 / 60;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  const t0 = performance.now() + 20;
  for (let f = 0; f < frames; f++) {
    const deadline = t0 + f * period;
    while (performance.now() < deadline) { /* spin */ }
    update(sc, f / 60);
    const enc = device.createCommandEncoder();
    encodeFrame(enc, sc, variant, qs, f * Q);
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * Q);
  const keep = [...Array(frames).keys()].slice(30);
  const gpu = keep.map(f => Number(ts[f * Q + 5] - ts[f * Q]) / 1e6);
  const sorted = [...gpu].sort((a, b) => a - b);
  return {
    pacing: 'busy-wait at 60 Hz', frames: keep.length,
    frame_gpu_median: r3(median(gpu)), frame_gpu_p95: r3(sorted[Math.floor(sorted.length * 0.95)]),
    frame_gpu_max: r3(sorted[sorted.length - 1]), frame_gpu_over_16_7: gpu.filter(g => g > 16.7).length,
  };
}

async function stats(sc, t = 0, variant = 'stats') {
  update(sc, t);
  const enc = device.createCommandEncoder();
  enc.clearBuffer(B.stats);
  encodeFrame(enc, sc, variant, null, 0);
  device.queue.submit([enc.finish()]);
  const s = new Uint32Array(await readback([{ src: B.stats, size: 128 }]));
  const o = {};
  STAT_NAMES.forEach((n, i) => { o[n] = s[i]; });
  const px = o.creature_px;
  o.creature_share = r3(px / (W * H));
  o.steps_per_march = r3(o.steps / Math.max(o.marches, 1));
  o.steps_per_creature_px = r3(o.hit_pixel_steps / Math.max(px, 1));
  o.parts_per_eval = r3(o.part_evals / Math.max(o.steps + o.detail_evals, 1));
  o.march_miss_share = r3(o.misses / Math.max(o.marches, 1));
  o.cap_share_of_creature_px = r3(o.caps / Math.max(px, 1));
  o.shadow_steps_per_march = r3(o.shadow_steps / Math.max(o.shadow_marches, 1));
  o.terrain_steps_per_px = r3(o.terrain_steps / (W * H));
  o.entries_per_px = r3(o.tile_entries / (W * H));
  return o;
}

/** Renders one variant at time t and reads the frame back as 8-bit display values. */
async function grab(sc, variant, t = T_SNAP) {
  update(sc, t);
  const out = device.createBuffer({ size: W * H * 8, usage: U.COPY_DST | U.MAP_READ });
  const enc = device.createCommandEncoder();
  encodeFrame(enc, sc, variant, null, 0);
  enc.copyTextureToBuffer({ texture: T.frame }, { buffer: out, bytesPerRow: W * 8, rowsPerImage: H }, [W, H]);
  device.queue.submit([enc.finish()]);
  await out.mapAsync(GPUMapMode.READ);
  const half = new Uint16Array(out.getMappedRange().slice(0));
  out.unmap();
  out.destroy();
  const px = new Uint8Array(W * H * 3);
  for (let i = 0, j = 0; i < half.length; i += 4) for (let c = 0; c < 3; c++) px[j++] = toByte(half[i + c]);
  return px;
}

const BYTE = new Uint8Array(65536);
for (let h = 0; h < 65536; h++) {
  const s = h >> 15, e = (h >> 10) & 31, m = h & 1023;
  let v = e === 0 ? m * 2 ** -24 : e === 31 ? 1 : (1 + m / 1024) * 2 ** (e - 15);
  if (s) v = 0;
  BYTE[h] = Math.round(255 * Math.pow(Math.min(Math.max(v, 0), 1), 1 / 2.2));
}
const toByte = h => BYTE[h];

function diff(a, b) {
  let sum = 0, over8 = 0, any = 0, max = 0;
  for (let i = 0; i < a.length; i += 3) {
    let m = 0;
    for (let c = 0; c < 3; c++) { const d = Math.abs(a[i + c] - b[i + c]); sum += d; m = Math.max(m, d); }
    if (m > 8) over8++;
    if (m > 0) any++;
    max = Math.max(max, m);
  }
  const n = a.length / 3;
  return { mean_abs_255: r3(sum / a.length), share_over_8: r3(over8 / n * 100) / 100, share_any: r3(any / n), max };
}

async function snapshot(name) {
  present();
  const blob = await new Promise(r => $('view').toBlob(r, 'image/png'));
  if (blob) await save(`${name}.png`, blob);
}

async function draw(sc, variant, t = T_SNAP) {
  update(sc, t);
  const enc = device.createCommandEncoder();
  encodeFrame(enc, sc, variant, null, 0);
  device.queue.submit([enc.finish()]);
}

// ---- run -------------------------------------------------------------------------------------------

function makeScenes() {
  const herdIns = Array.from({ length: 160 }, (_, i) => makeInstance(i + 1));
  const close = [{ x: 0, z: 0, y: terrainHeight(0, 0), yaw: Math.PI / 2, phase: 0.3, headDown: 0.2 }];
  return [
    scene('herd', herdIns.slice(0, 40), herdInstances(), [2, 7.5, -27], [0, 0.8, 1], 26),
    scene('closeup', [herdIns[0]], close, [0.85, 1.4, -3.6], [0.85, 1.2, 0], 4),
    scene('fill', [herdIns[0]], close, [0.55, 1.25, -1.3], [0.55, 1.15, 0], 4),
    scene('herd160', herdIns, herdInstances(160, 16, 3.0, 3.4), [2, 11, -44], [0, 0.8, 3], 36),
  ];
}

async function runAll() {
  $('run').disabled = true;
  try {
    await init();
    log('device', JSON.stringify(R.device));
    const saltA = 1000 + Math.random() * 1000, saltB = 3000 + Math.random() * 1000;
    R.pipelines.cold_sequential = (await pipelines(saltA, false, false)).ms;
    R.pipelines.cold_parallel_total = (await pipelines(saltB, true, false)).ms.total;
    R.pipelines.warm_sequential = (await pipelines(saltA, false, false)).ms;
    Object.assign(P, (await pipelines(null, true, true)).out);
    R.pipelines.per_scene = 'bin_screen, bin_light, trace (+ blit) = 4';
    log('pipelines', JSON.stringify(R.pipelines));
    await blitPipeline();

    const scenes = makeScenes();
    resources(160);
    for (const sc of scenes) {
      R.scenes[sc.name] = { individuals: sc.n, eye: sc.eye, target: sc.target, shadow_extent: sc.extent };
      R.stats[sc.name] = await stats(sc);
      log(`${sc.name} stats`, JSON.stringify(R.stats[sc.name]));
      // Rounds interleave the variants, so a clock change mid-scene doesn't land on one variant only.
      const rounds = {};
      for (let round = 0; round < ROUNDS; round++) {
        for (const v of MEASURED) (rounds[v] ||= []).push(await measure(sc, v));
      }
      for (const v of MEASURED) {
        const rs = rounds[v];
        R.frames[`${sc.name}/${v}`] = Object.fromEntries(Object.keys(rs[0]).map(k => [k, r3(median(rs.map(x => x[k])))]));
        R.frames[`${sc.name}/${v}`].trace_rounds = rs.map(x => x.trace);
        log(`${sc.name}/${v}`, JSON.stringify(R.frames[`${sc.name}/${v}`]));
      }
      const terrain = R.frames[`${sc.name}/terrain`].trace;
      const creatures = v => { const f = R.frames[`${sc.name}/${v}`]; return r3(f.bin_screen + f.bin_light + f.trace - terrain); };
      R.scenes[sc.name].creatures_ms = Object.fromEntries(MEASURED.filter(v => v !== 'terrain').map(v => [v, creatures(v)]));
      R.scenes[sc.name].shadow_ms = r3(R.frames[`${sc.name}/full`].trace - R.frames[`${sc.name}/noshadow`].trace);
      R.scenes[sc.name].ns_per_creature_px = r3(creatures('full') * 1e6 / Math.max(R.stats[sc.name].creature_px, 1));
      log(`${sc.name} creatures`, JSON.stringify(R.scenes[sc.name]));

      // Correctness: the fast variants against a brute-force reference march.
      const ref = await grab(sc, 'ref');
      await snapshot(`${sc.name}-ref`);
      R.quality[sc.name] = {};
      for (const v of ['full', 'eps50', 'eps10', 'relax12', 'relax16']) {
        R.quality[sc.name][v] = diff(await grab(sc, v), ref);
        if (v === 'full') await snapshot(`${sc.name}-full`);
      }
      const refD = await grab(sc, 'ref_displaced');
      R.quality[sc.name].displaced = diff(await grab(sc, 'displaced'), refD);
      await snapshot(`${sc.name}-displaced`);
      R.stats[`${sc.name}/displaced`] = await stats(sc, 0, 'stats_displaced');
      log(`${sc.name} quality`, JSON.stringify(R.quality[sc.name]));
      await draw(sc, 'heat');
      await snapshot(`${sc.name}-heat`);
      await draw(sc, 'heat_shadow');
      await snapshot(`${sc.name}-heat-shadow`);
      if (sc.name === 'herd') {
        R.paced['herd/full'] = await paced(sc, 'full');
        R.paced['herd/full#2'] = await paced(sc, 'full');
        log('paced', JSON.stringify(R.paced));
      }
    }
    R.finished = new Date().toISOString();
    await save(`run-${R.started.replace(/[:.]/g, '-')}.json`, JSON.stringify(R, null, 2));
    log('done');
  } catch (e) {
    log('FAILED:', e.stack || e.message);
    R.error = String(e.stack || e.message);
  }
  $('run').disabled = false;
}

/** A fast smoke test: compile, draw one frame, expose helpers for poking at it. */
async function quick() {
  await init();
  Object.assign(P, (await pipelines(null, true, true)).out);
  await blitPipeline();
  const scenes = makeScenes();
  resources(160);
  const byName = Object.fromEntries(scenes.map(s => [s.name, s]));
  window.__show = async (name = 'herd', variant = 'full', t = 0) => { await draw(byName[name], variant, t); present(); };
  window.__stats = (name, v) => stats(byName[name], 0, v);
  window.__measure = (name, v) => measure(byName[name], v);
  window.__diff = async (name, v = 'full', ref = 'ref') => diff(await grab(byName[name], v), await grab(byName[name], ref));
  window.__cam = (name, eye, target) => setCamera(byName[name], eye, target);
  window.__diffimg = async (name, v = 'full', ref = 'ref', file = `diff-${name}-${v}`) => {
    const a = await grab(byName[name], v), b = await grab(byName[name], ref);
    const c = document.createElement('canvas');
    c.width = W; c.height = H;
    const g = c.getContext('2d'), img = g.createImageData(W, H);
    for (let i = 0, j = 0; i < a.length; i += 3, j += 4) {
      let m = 0;
      for (let k = 0; k < 3; k++) m = Math.max(m, Math.abs(a[i + k] - b[i + k]));
      const base = (a[i] + a[i + 1] + a[i + 2]) / 12;
      img.data[j] = m > 8 ? 255 : base; img.data[j + 1] = m > 8 ? 0 : (m > 2 ? 200 : base); img.data[j + 2] = base; img.data[j + 3] = 255;
    }
    g.putImageData(img, 0, 0);
    const blob = await new Promise(r => c.toBlob(r, 'image/png'));
    await save(`${file}.png`, blob);
    return diff(a, b);
  };
  window.__snap = snapshot;
  await window.__show('herd');
  log('quick done');
}

$('run').addEventListener('click', runAll);
if (location.hash === '#run') runAll();
if (location.hash === '#quick') quick().catch(e => log('FAILED:', e.stack || e.message));
