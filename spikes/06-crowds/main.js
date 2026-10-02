// Spike 06 harness: builds the pipelines, cooks the world, runs the crowd and the edit stream,
// times all of it, and compares the fast paths with a brute-force reference.
// This file plays the engine's role plus the measuring. It isn't compiler output.

import { makeCrowd, writeStates, poseJS, terrainHeight, STRIDE, S_FLOATS, P_FLOATS } from './crowd.js';
import { EditLog, EDIT_FLOATS, RDIM } from './edits.js';

const EIDX_CAP = 1 << 19;            // entries in the analytic world's per-cell edit lists
const MAX_INST = 1000, MAX_EDITS = 4096, CAP = 32768, NCELLS = RDIM[0] * RDIM[1] * RDIM[2], ATLAS = 288;
const FOVY = 50 * Math.PI / 180;
const U = GPUBufferUsage;
const PASSES = ['pose', 'cull', 'bin_screen', 'bin_light', 'classify', 'cook', 'trace'];
const QPF = PASSES.length * 2;
const T_SNAP = 4.0;                  // every screenshot and comparison is taken at this time (s)
const N_MAIN = 300;
const COUNTS = [50, 150, 300, 600, 1000];
const RATES = [0, 5, 10, 20, 40];
const PREHISTORY = 60;               // edit events applied before anything is measured
const LIGHT = { c: [4, 0, -8], e: 60 };
const norm = a => { const l = Math.hypot(...a); return a.map(x => x / l); };
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sub = (a, b) => a.map((x, i) => x - b[i]);
const SUN = norm([-0.62, 0.62, -0.30]);

const SCENES = {
  square: { eye: [-8, 15, -58], target: [3, 1.5, 4] },
  aerial: { eye: [-62, 52, -74], target: [4, 0, -2] },
  street: { eye: [-7, 1.75, -9], target: [3, 3.2, 18] },
  wall: { eye: [15, 3.0, 0], target: [32, 4.5, 19] },
};

// The reference: brute-force moving things (every instance, every part), the analytic world with
// every edit, half-size steps, a twentieth of the hit tolerance, step caps in the thousands.
// (Caps are per tile-sized dispatch, and the reference's cap hits are reported: they're zero.)
const REF_WORLD = { WORLD_ANALYTIC: 1, WA_STEP: 0.5, W_EPS: 0.0125, WORLD_STEPS: 3000, WSHADOW_STEPS: 2000, TERRAIN_STEPS: 3000 };
const REF = { ...REF_WORLD, REF: 1, C_STEP: 0.5, C_EPS: 0.0125, MAX_STEPS: 2000, SHADOW_STEPS: 1500 };
const VARIANTS = {
  full: {},
  nocrowd: { CREATURES: 0 },
  crowd_noshadow: { CREATURE_SHADOWS: 0 },
  world: { CREATURES: 0, SHADOWS: 0 },
  world_analytic: { CREATURES: 0, SHADOWS: 0, WORLD_ANALYTIC: 1 },
  world_noshade: { CREATURES: 0, SHADOWS: 0, SHADE: 0 },
  full_analytic: { WORLD_ANALYTIC: 1 },
  full_eps10: { C_EPS: 0.1 },           // moving things' hit tolerance at a tenth of a pixel
  crowd_check: REF_WORLD,               // fast moving things over the reference's world: isolates their path
  crowd_check_eps10: { ...REF_WORLD, C_EPS: 0.1 },
  ref: REF,
  stats: { STATS: 1 },
  stats_analytic: { STATS: 1, WORLD_ANALYTIC: 1 },
  stats_ref: { ...REF, STATS: 1 },
  stats_eps10: { STATS: 1, C_EPS: 0.1 },
  heat_world: { VIEW: 1 },
  heat_crowd: { VIEW: 2 },
  heat_shadow: { VIEW: 3 },
};
const STAT_NAMES = ['creature_px', 'world_px', 'sky_px', 'marches', 'steps', 'misses', 'caps', 'shadow_px',
  'sh_marches', 'sh_steps', 'sh_caps', 'w_steps', 'w_empty', 'w_caps', 'ws_steps', 'ws_caps', 't_steps', 'entries',
  'part_evals', 'hit_steps', 'screen_ovf', 'light_ovf', 'coarse_ovf', 'coarse_light_ovf', 'sphere_tests', 'cook_cells',
  'cook_bricks', 'alloc_fail', 'freed', 'far_px', 'ws_marches', 'unused31'];

const $ = id => document.getElementById(id);
const T_PAGE = performance.now();
const log = (...a) => { const s = `[${((performance.now() - T_PAGE) / 1000).toFixed(1)}s] ` + a.join(' '); $('log').textContent += s + '\n'; console.log(s); };
const err = (...a) => { const s = a.join(' '); $('log').textContent += 'ERROR ' + s + '\n'; console.error(s); };
const R = {
  started: new Date().toISOString(), device: {}, pipelines: {}, world: {}, edits: {}, cpu: {}, scenes: {},
  frames: {}, sweeps: {}, stats: {}, quality: {}, paced: {}, memory_mb: {}, notes: [],
};
window.__results = R;

let device;
const B = {}, P = {}, L = {}, T = {}, src = {}, RES = {};
let crowd, edlog, stateArr;
const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[s.length >> 1] : NaN; };
const pct = (a, p) => { const s = [...a].sort((x, y) => x - y); return s.length ? s[Math.min(s.length - 1, Math.floor(s.length * p))] : NaN; };
const r3 = x => Math.round(x * 1000) / 1000;

function buffer(size, usage) {
  return device.createBuffer({ size: Math.max(16, Math.ceil(size / 16) * 16), usage });
}

async function readback(src, size, offset = 0) {
  const rb = device.createBuffer({ size, usage: U.MAP_READ | U.COPY_DST });
  const enc = device.createCommandEncoder();
  enc.copyBufferToBuffer(src, offset, rb, 0, size);
  device.queue.submit([enc.finish()]);
  await rb.mapAsync(GPUMapMode.READ);
  const out = rb.getMappedRange().slice(0);
  rb.unmap();
  rb.destroy();
  return out;
}

async function save(name, body) {
  try {
    const r = await fetch(`results/${name}`, { method: 'PUT', body });
    if (!r.ok) err('save failed', name, r.status);
  } catch (e) { err('save failed', name, e.message); }
}

// ---- setup -------------------------------------------------------------------------------------------

const FILES = ['common', 'world', 'cache', 'creature', 'pose', 'bin', 'cook', 'trace'];
const MODULES = {
  pose: ['common', 'pose'],
  bin: ['common', 'creature', 'bin'],
  cook: ['common', 'world', 'cook'],
  trace: ['common', 'world', 'creature', 'cache', 'trace'],
};

async function init() {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
  if (!adapter) throw new Error('no WebGPU adapter');
  const want = ['timestamp-query', 'texture-formats-tier1'];
  for (const f of want) if (!adapter.features.has(f)) throw new Error(`${f} is required`);
  device = await adapter.requestDevice({ requiredFeatures: want, requiredLimits: { maxStorageBuffersPerShaderStage: 10 } });
  device.lost.then(i => err('DEVICE LOST:', i.message));
  device.addEventListener('uncapturederror', e => err('GPU ERROR:', e.error.message));
  const info = adapter.info || {};
  R.device = { vendor: info.vendor, architecture: info.architecture, description: info.description,
    userAgent: navigator.userAgent, features: want, pageVisibility: document.visibilityState };
  for (const f of FILES) src[f] = await (await fetch(`${f}.wgsl`)).text();
}

/** Concatenated source plus a map from line numbers back to files, for error messages. */
function moduleSource(name) {
  let code = '', line = 1;
  const map = [];
  for (const f of MODULES[name]) {
    map.push([line, f]);
    code += src[f] + '\n';
    line += src[f].split('\n').length;
  }
  return { code, where: n => { let w = map[0]; for (const m of map) if (m[0] <= n) w = m; return `${w[1]}.wgsl:${n - w[0] + 1}`; } };
}

async function compile(name) {
  const { code, where } = moduleSource(name);
  const module = device.createShaderModule({ code, label: name });
  const info = await module.getCompilationInfo();
  for (const m of info.messages) {
    const s = `${name} ${m.type} at ${where(m.lineNum)}:${m.linePos}: ${m.message}`;
    if (m.type === 'error') err(s); else log(s);
  }
  if (info.messages.some(m => m.type === 'error')) throw new Error(`${name}: WGSL errors`);
  return module;
}

function layouts() {
  const C = GPUShaderStage.COMPUTE;
  const b = (binding, type) => ({ binding, visibility: C, buffer: { type } });
  L.pose = device.createBindGroupLayout({ entries: [b(0, 'uniform'), b(1, 'storage'), b(2, 'read-only-storage'), b(3, 'read-only-storage')] });
  L.bin = device.createBindGroupLayout({ entries: [b(0, 'uniform'), b(1, 'read-only-storage'), b(2, 'storage'), b(3, 'storage'), b(4, 'storage'), b(5, 'storage'), b(6, 'storage')] });
  L.cook = device.createBindGroupLayout({
    entries: [b(0, 'uniform'), b(1, 'uniform'), b(2, 'storage'), b(3, 'storage'), b(4, 'storage'), b(5, 'storage'), b(6, 'storage'),
      b(7, 'read-only-storage'), b(8, 'read-only-storage'),
      { binding: 9, visibility: C, storageTexture: { access: 'write-only', format: 'rg16float', viewDimension: '3d' } }, b(10, 'storage')],
  });
  L.trace = device.createBindGroupLayout({
    entries: [b(0, 'uniform'), b(1, 'read-only-storage'), b(2, 'read-only-storage'), b(3, 'read-only-storage'), b(4, 'read-only-storage'),
      { binding: 5, visibility: C, texture: { sampleType: 'float', viewDimension: '3d' } },
      { binding: 6, visibility: C, sampler: { type: 'filtering' } }, b(7, 'read-only-storage'),
      { binding: 8, visibility: C, storageTexture: { access: 'write-only', format: 'rgba16float' } }, b(9, 'storage'),
      b(10, 'read-only-storage'), b(11, 'read-only-storage')],
  });
}

async function pipelines() {
  const t0 = performance.now();
  const M = {};
  for (const m of Object.keys(MODULES)) M[m] = await compile(m);
  const t1 = performance.now();
  const lay = l => device.createPipelineLayout({ bindGroupLayouts: [l] });
  const list = [
    ['pose', M.pose, L.pose, 'pose'], ['cull', M.bin, L.bin, 'cull'], ['bin_screen', M.bin, L.bin, 'bin_screen'],
    ['bin_light', M.bin, L.bin, 'bin_light'], ['begin_batch', M.cook, L.cook, 'begin_batch'], ['classify', M.cook, L.cook, 'classify'],
    ['cook', M.cook, L.cook, 'cook'], ['release', M.cook, L.cook, 'release'],
  ];
  // One at a time, so the shader compiler doesn't fan out across the machine (GPU safety rules).
  for (const [n, module, l, entryPoint, constants] of list) {
    try {
      P[n] = await device.createComputePipelineAsync({ label: n, layout: lay(l), compute: { module, entryPoint, constants } });
    } catch (e) { err(`pipeline ${n}: ${e.message}`); throw e; }
  }
  M_TRACE = M.trace;
  R.pipelines = { modules_ms: r3(t1 - t0), base_pipelines_ms: r3(performance.now() - t1), base_count: list.length, trace_variants_ms: {},
    note: 'created one at a time; trace variants on first use. A fresh profile has no pipeline cache, so these are cold.',
    per_frame: 'pose, cull, bin_screen, bin_light, trace (+ begin_batch, classify, cook, release on an edit frame) = 9 kernels' };
  log('pipelines', JSON.stringify(R.pipelines));
}

let M_TRACE = null;
let gridFor = -1;
/** The analytic world's per-cell edit lists, rebuilt when the log has grown since last time. */
function ensureGrid() {
  if (gridFor === edlog.prims.length) return;
  const g = edlog.grid();
  if (g.total > EIDX_CAP) throw new Error(`edit lists overflow: ${g.total}`);
  device.queue.writeBuffer(B.egrid, 0, g.info);
  device.queue.writeBuffer(B.eidx, 0, g.idx);
  gridFor = edlog.prims.length;
  R.edits.grid = { entries: g.total, max_per_cell: g.max, edits: gridFor };
}
/** Creates a trace variant's pipeline on first use (one at a time; see `pipelines`). */
async function ensure(variant) {
  const key = `trace_${variant}`;
  if (P[key]) return;
  const t0 = performance.now();
  P[key] = await device.createComputePipelineAsync({ label: key, layout: device.createPipelineLayout({ bindGroupLayouts: [L.trace] }),
    compute: { module: M_TRACE, entryPoint: 'trace', constants: VARIANTS[variant] } });
  R.pipelines.trace_variants_ms[variant] = r3(performance.now() - t0);
}

function resources() {
  const S = U.STORAGE;
  B.inst = buffer(MAX_INST * STRIDE * 16, S | U.COPY_SRC);
  B.state = buffer(MAX_INST * S_FLOATS * 4, S | U.COPY_DST);
  B.params = buffer(MAX_INST * P_FLOATS * 4, S | U.COPY_DST);
  B.ccount = buffer(1280 * 4, S | U.COPY_DST);
  B.clist = buffer(1280 * 512 * 4, S);
  B.lbins = buffer(128 * 128 * 33 * 4, S);
  B.bmap = buffer(NCELLS * 4, S | U.COPY_DST | U.COPY_SRC);
  B.ctrl = buffer(32, S | U.COPY_DST | U.COPY_SRC);
  B.cookArgs = buffer(16, U.INDIRECT | U.COPY_DST);
  B.freeslots = buffer(CAP * 4, S | U.COPY_DST);
  B.cooklist = buffer(CAP * 8, S);
  B.pending = buffer(CAP * 4, S);
  B.edits = buffer(MAX_EDITS * EDIT_FLOATS * 4, S | U.COPY_DST);
  B.relevant = buffer(MAX_EDITS * 4, S | U.COPY_DST);
  B.batch = buffer(48, U.UNIFORM | U.COPY_DST);
  B.stats = buffer(128, S | U.COPY_SRC | U.COPY_DST);
  B.egrid = buffer(48 * 20 * 48 * 8, S | U.COPY_DST);
  B.eidx = buffer(EIDX_CAP * 4, S | U.COPY_DST);
  T.atlas = device.createTexture({ dimension: '3d', size: [ATLAS, ATLAS, ATLAS], format: 'rg16float',
    usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING });
  T.sampler = device.createSampler({ magFilter: 'linear', minFilter: 'linear', addressModeU: 'clamp-to-edge', addressModeV: 'clamp-to-edge', addressModeW: 'clamp-to-edge' });
  device.queue.writeBuffer(B.ctrl, 0, new Uint32Array([0, 1, 1, 0, CAP, 0, 0, 0]));
  device.queue.writeBuffer(B.freeslots, 0, Uint32Array.from({ length: CAP }, (_, i) => i));
  device.queue.writeBuffer(B.params, 0, crowd.params);
  RES.full = makeRes(1920, 1080);
  RES.half = makeRes(960, 540);
  R.memory_mb = {
    brick_atlas_rg16f: r3(ATLAS ** 3 * 4 / 2 ** 20), brick_map: r3(NCELLS * 4 / 2 ** 20),
    posed_instances_1000: r3(MAX_INST * STRIDE * 16 / 2 ** 20), root_state_upload_per_frame_1000: r3(MAX_INST * S_FLOATS * 4 / 2 ** 20),
    coarse_lists: r3(1280 * 512 * 4 / 2 ** 20), tile_lists_1080p: r3(240 * 135 * 49 * 4 / 2 ** 20), light_lists: r3(128 * 128 * 33 * 4 / 2 ** 20),
    edit_log_4096: r3(MAX_EDITS * 32 / 2 ** 20), meshes: 0,
  };
}

function makeRes(W, H) {
  const r = { W, H, TX: Math.ceil(W / 8), TY: Math.ceil(H / 8) };
  r.CX = Math.ceil(r.TX / 8);
  r.CY = Math.ceil(r.TY / 8);
  r.frame = buffer(176, U.UNIFORM | U.COPY_DST);
  r.fbuf = new ArrayBuffer(176);
  r.tiles = buffer(r.TX * r.TY * 49 * 4, U.STORAGE);
  r.tex = device.createTexture({ size: [W, H], format: 'rgba16float', usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.COPY_SRC });
  const res = buf => ({ buffer: buf });
  r.bgPose = device.createBindGroup({ layout: L.pose, entries: [r.frame, B.inst, B.state, B.params].map((b, i) => ({ binding: i, resource: res(b) })) });
  r.bgBin = device.createBindGroup({ layout: L.bin, entries: [r.frame, B.inst, B.ccount, B.clist, r.tiles, B.lbins, B.stats].map((b, i) => ({ binding: i, resource: res(b) })) });
  r.bgCook = device.createBindGroup({
    layout: L.cook,
    entries: [
      ...[r.frame, B.batch, B.bmap, B.ctrl, B.freeslots, B.cooklist, B.pending, B.edits, B.relevant].map((b, i) => ({ binding: i, resource: res(b) })),
      { binding: 9, resource: T.atlas.createView() }, { binding: 10, resource: res(B.stats) },
    ],
  });
  r.bgTrace = device.createBindGroup({
    layout: L.trace,
    entries: [
      ...[r.frame, B.inst, r.tiles, B.lbins, B.bmap].map((b, i) => ({ binding: i, resource: res(b) })),
      { binding: 5, resource: T.atlas.createView() }, { binding: 6, resource: T.sampler },
      { binding: 7, resource: res(B.edits) }, { binding: 8, resource: r.tex.createView() }, { binding: 9, resource: res(B.stats) },
      { binding: 10, resource: res(B.egrid) }, { binding: 11, resource: res(B.eidx) },
    ],
  });
  return r;
}

// ---- per-frame state --------------------------------------------------------------------------------

function writeFrame(res, sc, n, t, nedits, row0 = 0, col0 = 0) {
  const f = norm(sub(sc.target, sc.eye));
  const rt = norm(cross(f, [0, 1, 0]));
  const up = cross(rt, f);
  const ty = Math.tan(FOVY / 2), tx = ty * res.W / res.H;
  const lz = SUN, lx = norm(cross([0, 1, 0], lz)), ly = cross(lz, lx);
  const fl = new Float32Array(res.fbuf), fu = new Uint32Array(res.fbuf);
  fl.set([...sc.eye, 2 * ty / res.H], 0);
  fl.set([...f, 0], 4);
  fl.set([...rt.map(x => x * tx), 0], 8);
  fl.set([...up.map(x => x * ty), 0], 12);
  fl.set([...SUN, t], 16);
  fl.set([...LIGHT.c, LIGHT.e], 20);
  fl.set([...lx, 0], 24);
  fl.set([...ly, 0], 28);
  fu.set([res.W, res.H, res.TX, n], 32);
  fu.set([128, res.CX, res.CY, nedits], 36);
  fu.set([row0, res.TY, col0, 0], 40);
  device.queue.writeBuffer(res.frame, 0, res.fbuf);
}

function uploadStates(n, t) {
  const t0 = performance.now();
  writeStates(crowd, n, t, stateArr);
  device.queue.writeBuffer(B.state, 0, stateArr, 0, n * S_FLOATS);
  return performance.now() - t0;
}

/** CPU side of an edit event: upload its primitives, the batch box and the overlapping entries.
 *  With `box` ({lo, size}), it's a full cook of every cell in that box instead. */
function prepareEdit(first, end, box = null) {
  const t0 = performance.now();
  const full = !!box;
  let b;
  if (full) {
    b = { lo: box.lo, size: box.size, cells: box.size[0] * box.size[1] * box.size[2], relevant: edlog.relevantFor(box.lo, box.size) };
  } else {
    b = edlog.batch(first, end);
  }
  if (end > first) device.queue.writeBuffer(B.edits, first * EDIT_FLOATS * 4, edlog.data, first * EDIT_FLOATS, (end - first) * EDIT_FLOATS);
  if (b.relevant.length) device.queue.writeBuffer(B.relevant, 0, new Uint32Array(b.relevant));
  const bt = new ArrayBuffer(48), bi = new Int32Array(bt), bu = new Uint32Array(bt);
  bi.set([b.lo[0], b.lo[1], b.lo[2], 0], 0);
  bu.set([b.size[0], b.size[1], b.size[2], b.cells, b.relevant.length, first, end, full ? 1 : 0], 4);
  device.queue.writeBuffer(B.batch, 0, bt);
  return { cells: b.cells, relevant: b.relevant.length, prims: end - first, cpu_ms: performance.now() - t0 };
}

function encodeEdit(enc, res, cells, ts) {
  let p = enc.beginComputePass(ts(4));
  p.setBindGroup(0, res.bgCook);
  p.setPipeline(P.begin_batch);
  p.dispatchWorkgroups(1);
  p.setPipeline(P.classify);
  p.dispatchWorkgroups(Math.ceil(cells / 64));
  p.end();
  enc.copyBufferToBuffer(B.ctrl, 0, B.cookArgs, 0, 16);
  p = enc.beginComputePass(ts(5));
  p.setBindGroup(0, res.bgCook);
  p.setPipeline(P.cook);
  p.dispatchWorkgroupsIndirect(B.cookArgs, 0);
  p.setPipeline(P.release);
  p.dispatchWorkgroups(Math.ceil(cells / 64));
  p.end();
}

const creaturesIn = variant => VARIANTS[variant].CREATURES !== 0;

function encodeFrame(enc, res, variant, { n, edit = null, qs = null, q0 = 0, rows = null, cols = null, crowdPasses = true, tracePass = true } = {}) {
  const ts = i => qs ? { timestampWrites: { querySet: qs, beginningOfPassWriteIndex: q0 + 2 * i, endOfPassWriteIndex: q0 + 2 * i + 1 } } : {};
  const pass = (i, pipe, bg, x, y = 1) => {
    const p = enc.beginComputePass(ts(i));
    p.setPipeline(pipe);
    p.setBindGroup(0, bg);
    if (x > 0 && y > 0) p.dispatchWorkgroups(x, y);
    p.end();
  };
  if (edit) encodeEdit(enc, res, edit.cells, ts);
  if (crowdPasses && creaturesIn(variant) && n > 0) {
    enc.clearBuffer(B.ccount);
    pass(0, P.pose, res.bgPose, Math.ceil(n / 64));
    pass(1, P.cull, res.bgBin, Math.ceil(n / 64));
    pass(2, P.bin_screen, res.bgBin, res.CX, res.CY);
    pass(3, P.bin_light, res.bgBin, 16, 16);
  }
  if (tracePass) pass(6, P[`trace_${variant}`], res.bgTrace, cols ? cols / 8 : res.TX, rows ? rows / 8 : res.TY);
}

async function resolveTimestamps(qs, count) {
  const buf = buffer(count * 8, U.QUERY_RESOLVE | U.COPY_SRC);
  const enc = device.createCommandEncoder();
  enc.resolveQuerySet(qs, 0, count, buf, 0);
  device.queue.submit([enc.finish()]);
  const raw = await readback(buf, count * 8);
  buf.destroy();
  qs.destroy();
  return new BigUint64Array(raw);
}

// ---- measuring ---------------------------------------------------------------------------------------

/** GPU time per pass under sustained load: `warm` untimed frames, then `frames` back to back.
 *  With editRate > 0, edit events fire at that rate (per simulated second at 60 Hz). */
async function measure(sc, res, variant, { frames = 90, warm = 30, n = N_MAIN, editRate = 0, nedits = null, t0 = 0 } = {}) {
  await ensure(variant);
  const crowdOn = creaturesIn(variant);
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * QPF });
  const cpuState = [], cpuEdit = [], edits = [];
  const frame = (f, q) => {
    const t = t0 + f / 60;
    const cs = crowdOn ? uploadStates(n, t) : 0;
    let edit = null;
    if (editRate > 0 && Math.floor((f + 1) * editRate / 60) > Math.floor(f * editRate / 60)) {
      const ev = edlog.next();
      edit = prepareEdit(ev.first, ev.end);
      edit.kind = ev.kind;
    }
    writeFrame(res, sc, n, t, nedits ?? edlog.prims.length);
    const enc = device.createCommandEncoder();
    encodeFrame(enc, res, variant, { n, edit, qs: q >= 0 ? qs : null, q0: q * QPF });
    device.queue.submit([enc.finish()]);
    return { cs, edit };
  };
  for (let f = 0; f < warm; f++) frame(f, -1);
  await device.queue.onSubmittedWorkDone();
  const t1 = performance.now();
  for (let f = 0; f < frames; f++) {
    const r = frame(warm + f, f);
    cpuState.push(r.cs);
    edits.push(r.edit);
    if (r.edit) cpuEdit.push(r.edit.cpu_ms);
  }
  await device.queue.onSubmittedWorkDone();
  const throughput = (performance.now() - t1) / frames;
  const ts = await resolveTimestamps(qs, frames * QPF);
  const at = (f, k) => Number(ts[f * QPF + k]) / 1e6;
  const dur = (f, i) => at(f, 2 * i + 1) - at(f, 2 * i);
  const all = [...Array(frames).keys()];
  const editFrames = all.filter(f => edits[f]);
  const out = { variant, n, res: `${res.W}x${res.H}`, frames };
  for (const [i, name] of PASSES.entries()) {
    if (i <= 3 && !crowdOn) { out[name] = 0; continue; }
    if (i === 4 || i === 5) continue;
    out[name] = r3(median(all.map(f => dur(f, i))));
    out[`${name}_p95`] = r3(pct(all.map(f => dur(f, i)), 0.95));
  }
  const first = f => edits[f] ? at(f, 8) : (crowdOn ? at(f, 0) : at(f, 12));
  const gpu = all.map(f => at(f, 13) - first(f));
  out.frame_gpu = r3(median(gpu));
  out.frame_gpu_p95 = r3(pct(gpu, 0.95));
  out.frame_gpu_max = r3(Math.max(...gpu));
  out.throughput_ms = r3(throughput);
  out.cpu_state_ms = r3(median(cpuState));
  if (editRate > 0) {
    const rc = editFrames.map(f => dur(f, 4) + dur(f, 5));
    out.edit_rate = editRate;
    out.edit_events = editFrames.length;
    out.classify_median = r3(median(editFrames.map(f => dur(f, 4))));
    out.cook_median = r3(median(editFrames.map(f => dur(f, 5))));
    out.recook_median = r3(median(rc));
    out.recook_max = r3(rc.length ? Math.max(...rc) : 0);
    out.recook_amortized = r3(rc.reduce((s, x) => s + x, 0) / frames);
    out.cells_classified_median = median(editFrames.map(f => edits[f].cells));
    out.relevant_median = median(editFrames.map(f => edits[f].relevant));
    out.cpu_edit_ms_median = r3(median(cpuEdit));
    out.frame_gpu_edit_frames = r3(median(editFrames.map(f => gpu[f])));
    out.frame_gpu_other_frames = r3(median(all.filter(f => !edits[f]).map(f => gpu[f])));
    const kinds = {};
    for (const f of editFrames) (kinds[edits[f].kind] ||= []).push(dur(f, 4) + dur(f, 5));
    out.recook_by_kind = Object.fromEntries(Object.entries(kinds).map(([k, v]) => [k, { events: v.length, median: r3(median(v)), max: r3(Math.max(...v)) }]));
  }
  return out;
}

/** 60 Hz pacing with a busy-wait (the page may be hidden, so timers and rAF are throttled). */
async function paced(sc, res, variant, { frames = 180, n = N_MAIN, editRate = 0 } = {}) {
  await ensure(variant);
  const period = 1000 / 60;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * QPF });
  const edits = [];
  const start = performance.now() + 20;
  for (let f = 0; f < frames; f++) {
    const deadline = start + f * period;
    while (performance.now() < deadline) { /* spin */ }
    const t = f / 60;
    uploadStates(n, t);
    let edit = null;
    if (editRate > 0 && Math.floor((f + 1) * editRate / 60) > Math.floor(f * editRate / 60)) {
      const ev = edlog.next();
      edit = prepareEdit(ev.first, ev.end);
    }
    edits.push(edit);
    writeFrame(res, sc, n, t, edlog.prims.length);
    const enc = device.createCommandEncoder();
    encodeFrame(enc, res, variant, { n, edit, qs, q0: f * QPF });
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * QPF);
  const at = (f, k) => Number(ts[f * QPF + k]) / 1e6;
  const keep = [...Array(frames).keys()].slice(30);
  const gpu = keep.map(f => at(f, 13) - (edits[f] ? at(f, 8) : at(f, 0)));
  const starts = keep.map(f => edits[f] ? at(f, 8) : at(f, 0));
  const spacing = starts.slice(1).map((s, i) => s - starts[i]);
  return {
    pacing: 'busy-wait at 60 Hz', frames: keep.length, edit_rate: editRate,
    frame_gpu_median: r3(median(gpu)), frame_gpu_p95: r3(pct(gpu, 0.95)), frame_gpu_max: r3(Math.max(...gpu)),
    frame_gpu_over_16_7: gpu.filter(g => g > 16.7).length,
    gpu_start_spacing_median: r3(median(spacing)), gpu_start_spacing_max: r3(Math.max(...spacing)),
  };
}

async function readStats() {
  const s = new Uint32Array(await readback(B.stats, 128));
  const o = {};
  STAT_NAMES.forEach((k, i) => { o[k] = s[i]; });
  return o;
}

async function stats(sc, res, variant = 'stats', n = N_MAIN, t = T_SNAP) {
  await ensure(variant);
  if (heavy(variant)) ensureGrid();
  uploadStates(n, t);
  writeFrame(res, sc, n, t, edlog.prims.length);
  const enc = device.createCommandEncoder();
  enc.clearBuffer(B.stats);
  encodeFrame(enc, res, variant, { n, tracePass: !heavy(variant) });
  device.queue.submit([enc.finish()]);
  if (heavy(variant)) await traceTiled(sc, res, variant, n, t);
  const o = await readStats();
  const px = res.W * res.H;
  o.creature_share = r3(o.creature_px / px);
  o.steps_per_creature_px = r3(o.hit_steps / Math.max(o.creature_px, 1));
  o.steps_per_march = r3(o.steps / Math.max(o.marches, 1));
  o.parts_per_eval = r3(o.part_evals / Math.max(o.steps, 1));
  o.march_miss_share = r3(o.misses / Math.max(o.marches, 1));
  o.creature_cap_share = o.caps / Math.max(o.creature_px, 1);
  o.world_steps_per_px = r3((o.w_steps + o.w_empty) / px);
  o.world_brick_steps_per_px = r3(o.w_steps / px);
  o.world_cap_share = o.w_caps / px;
  o.world_shadow_steps_per_march = r3(o.ws_steps / Math.max(o.ws_marches, 1));
  o.world_shadow_cap_share = o.ws_caps / Math.max(o.ws_marches, 1);
  o.creature_shadow_steps_per_march = r3(o.sh_steps / Math.max(o.sh_marches, 1));
  o.creature_shadow_cap_share = o.sh_caps / Math.max(o.sh_marches, 1);
  o.terrain_steps_per_px = r3(o.t_steps / px);
  o.entries_per_px = r3(o.entries / px);
  o.spheres_per_px = r3(o.sphere_tests / px);
  return o;
}

// ---- images --------------------------------------------------------------------------------------------

const BYTE = new Uint8Array(65536);
for (let h = 0; h < 65536; h++) {
  const s = h >> 15, e = (h >> 10) & 31, m = h & 1023;
  let v = e === 0 ? m * 2 ** -24 : e === 31 ? 1 : (1 + m / 1024) * 2 ** (e - 15);
  if (s) v = 0;
  BYTE[h] = Math.round(255 * Math.pow(Math.min(Math.max(v, 0), 1), 1 / 2.2));
}

// ---- GPU safety: long renders in tiles -----------------------------------------------------------
// No single submit may run longer than ~100 ms (spikes/README.md). Variants that evaluate the
// analytic world with every edit, or every instance with every part, run their trace pass in
// tiles: one submit each, awaited, growing or shrinking the tile so each takes about TILE_MS.

const heavy = v => !!(VARIANTS[v].WORLD_ANALYTIC || VARIANTS[v].REF);
const TILE_MS = 25;
let tileLevel = 0;   // tile = (8 · 2^max(0, level − 3)) rows × min(240 · 2^min(level, 3), width) columns; level −1: 120 columns

/** The trace pass in tiles. Returns the tiles' summed GPU time (timestamps) and the longest
 *  submit's wall time. */
async function traceTiled(sc, res, variant, n, t, nedits = null) {
  const ne = nedits ?? edlog.prims.length;
  let gpu = 0, worst = 0, tiles = 0;
  for (let row = 0; row < res.H;) {
    const rows = Math.min(8 << Math.max(0, tileLevel - 3), Math.ceil((res.H - row) / 8) * 8);
    const cols = tileLevel < 0 ? 120 : Math.min(240 << Math.min(tileLevel, 3), res.W);
    let slowest = 0;
    for (let col = 0; col < res.W; col += cols) {
      writeFrame(res, sc, n, t, ne, row, col);
      const qs = device.createQuerySet({ type: 'timestamp', count: QPF });
      const enc = device.createCommandEncoder();
      encodeFrame(enc, res, variant, { n, crowdPasses: false, rows, cols: Math.min(cols, res.W - col), qs });
      const w0 = performance.now();
      device.queue.submit([enc.finish()]);
      await device.queue.onSubmittedWorkDone();
      const wall = performance.now() - w0;
      const ts = await resolveTimestamps(qs, QPF);
      gpu += Number(ts[13] - ts[12]) / 1e6;
      slowest = Math.max(slowest, wall);
      tiles++;
    }
    worst = Math.max(worst, slowest);
    row += rows;
    if (slowest > TILE_MS && tileLevel > -1) tileLevel--;
    else if (slowest < TILE_MS / 3 && tileLevel < 8) tileLevel++;
  }
  writeFrame(res, sc, n, t, ne, 0, 0);
  if (worst > 100) err(`a tile of ${variant} took ${worst.toFixed(1)} ms (${sc.eye}, ${edlog.prims.length} edits)`);
  R.longest_tile_ms = Math.max(R.longest_tile_ms || 0, r3(worst));
  return { gpu, worst, tiles };
}

/** GPU time of a heavy variant's frame, summed over its tiles. Not back-to-back (each tile is
 *  awaited), so clocks may sit lower than in `measure`; used only for the analytic-world costs. */
async function measureTiled(sc, res, variant, { frames = 4, warm = 1, n = N_MAIN, nedits = null } = {}) {
  await ensure(variant);
  if (heavy(variant)) ensureGrid();
  const gpu = [], worst = [];
  for (let f = 0; f < warm + frames; f++) {
    const t = f / 60;
    uploadStates(n, t);
    writeFrame(res, sc, n, t, nedits ?? edlog.prims.length);
    if (creaturesIn(variant)) {
      const enc = device.createCommandEncoder();
      encodeFrame(enc, res, variant, { n, tracePass: false });
      device.queue.submit([enc.finish()]);
    }
    const r = await traceTiled(sc, res, variant, n, t, nedits);
    if (f >= warm) { gpu.push(r.gpu); worst.push(r.worst); }
  }
  return { variant, n, res: `${res.W}x${res.H}`, frames, trace: r3(median(gpu)), tiled: true, longest_submit_wall_ms: r3(Math.max(...worst)) };
}

/** Renders one variant at time t and reads the frame back as 8-bit RGB (heavy variants in tiles). */
async function render(sc, res, variant, { t = T_SNAP, n = N_MAIN } = {}) {
  await ensure(variant);
  if (heavy(variant)) ensureGrid();
  uploadStates(n, t);
  writeFrame(res, sc, n, t, edlog.prims.length);
  const enc0 = device.createCommandEncoder();
  encodeFrame(enc0, res, variant, { n, tracePass: !heavy(variant) });
  device.queue.submit([enc0.finish()]);
  if (heavy(variant)) await traceTiled(sc, res, variant, n, t);
  const out = device.createBuffer({ size: res.W * res.H * 8, usage: U.COPY_DST | U.MAP_READ });
  const enc = device.createCommandEncoder();
  enc.copyTextureToBuffer({ texture: res.tex }, { buffer: out, bytesPerRow: res.W * 8, rowsPerImage: res.H }, [res.W, res.H]);
  device.queue.submit([enc.finish()]);
  await out.mapAsync(GPUMapMode.READ);
  const half = new Uint16Array(out.getMappedRange().slice(0));
  out.unmap();
  out.destroy();
  const px = new Uint8Array(res.W * res.H * 3);
  for (let i = 0, j = 0; i < half.length; i += 4) { px[j++] = BYTE[half[i]]; px[j++] = BYTE[half[i + 1]]; px[j++] = BYTE[half[i + 2]]; }
  return { px, W: res.W, H: res.H };
}

function diff(a, b) {
  let sum = 0, over8 = 0, max = 0;
  for (let i = 0; i < a.px.length; i += 3) {
    let m = 0;
    for (let c = 0; c < 3; c++) { const d = Math.abs(a.px[i + c] - b.px[i + c]); sum += d; m = Math.max(m, d); }
    if (m > 8) over8++;
    max = Math.max(max, m);
  }
  const n = a.px.length / 3;
  return { mean_abs_255: r3(sum / a.px.length), share_over_8: Math.round(over8 / n * 1e5) / 1e5, max };
}

async function png(img, name) {
  const c = new OffscreenCanvas(img.W, img.H);
  const g = c.getContext('2d');
  const d = g.createImageData(img.W, img.H);
  for (let i = 0, j = 0; i < img.px.length; i += 3, j += 4) { d.data[j] = img.px[i]; d.data[j + 1] = img.px[i + 1]; d.data[j + 2] = img.px[i + 2]; d.data[j + 3] = 255; }
  g.putImageData(d, 0, 0);
  const blob = await c.convertToBlob({ type: 'image/png' });
  await save(`${name}.png`, blob);
  const view = $('view');
  if (view && img.W === 1920) { view.getContext('2d').putImageData(d, 0, 0); }
}

/** Where a and b differ: red over 8/255, yellow over 2/255, on a dimmed copy of a. */
function diffImage(a, b) {
  const px = new Uint8Array(a.px.length);
  for (let i = 0; i < a.px.length; i += 3) {
    let m = 0;
    for (let c = 0; c < 3; c++) m = Math.max(m, Math.abs(a.px[i + c] - b.px[i + c]));
    const base = (a.px[i] + a.px[i + 1] + a.px[i + 2]) / 9;
    px[i] = m > 2 ? 255 : base; px[i + 1] = m > 8 ? 0 : (m > 2 ? 210 : base); px[i + 2] = m > 2 ? 0 : base;
  }
  return { px, W: a.W, H: a.H };
}

// ---- the world -------------------------------------------------------------------------------------------

/** Cooks every cell from base + log, in 8×40×8-cell chunks: one submit each, awaited (GPU safety;
 *  a single dispatch over the region took ~190 ms with ~480 edits in the log). */
async function cookAll() {
  const CH = 8, chunks = [];
  for (let x = 0; x < RDIM[0]; x += CH) for (let z = 0; z < RDIM[2]; z += CH) chunks.push({ lo: [x, 0, z], size: [CH, RDIM[1], CH] });
  const qs = device.createQuerySet({ type: 'timestamp', count: chunks.length * 4 });
  const enc0 = device.createCommandEncoder();
  enc0.clearBuffer(B.stats);
  device.queue.submit([enc0.finish()]);
  let worst = 0;
  for (const [k, box] of chunks.entries()) {
    const e = prepareEdit(edlog.prims.length, edlog.prims.length, box);
    const enc = device.createCommandEncoder();
    encodeEdit(enc, RES.full, e.cells, i => ({ timestampWrites: { querySet: qs, beginningOfPassWriteIndex: 4 * k + 2 * (i - 4), endOfPassWriteIndex: 4 * k + 2 * (i - 4) + 1 } }));
    const w0 = performance.now();
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
    worst = Math.max(worst, performance.now() - w0);
  }
  const ts = await resolveTimestamps(qs, chunks.length * 4);
  let classify = 0, cook = 0;
  for (let k = 0; k < chunks.length; k++) {
    classify += Number(ts[4 * k + 1] - ts[4 * k]) / 1e6;
    cook += Number(ts[4 * k + 3] - ts[4 * k + 2]) / 1e6;
  }
  const st = await readStats();
  const ctrl = new Uint32Array(await readback(B.ctrl, 32));
  if (worst > 100) err(`a cook chunk took ${worst.toFixed(1)} ms`);
  return {
    classify_ms: r3(classify), cook_ms: r3(cook), chunks: chunks.length, longest_submit_wall_ms: r3(worst),
    edits_in_log: edlog.prims.length, cells: NCELLS, bricks: CAP - ctrl[4], bricks_cooked: st.cook_bricks, alloc_fail: st.alloc_fail,
  };
}

/** One edit event in its own submit, timed, with its brick counts. Isolated, so clocks may be low. */
async function editOnce() {
  const ev = edlog.next();
  const e = prepareEdit(ev.first, ev.end);
  const qs = device.createQuerySet({ type: 'timestamp', count: QPF });
  const enc = device.createCommandEncoder();
  enc.clearBuffer(B.stats);
  encodeEdit(enc, RES.full, e.cells, i => ({ timestampWrites: { querySet: qs, beginningOfPassWriteIndex: 2 * i, endOfPassWriteIndex: 2 * i + 1 } }));
  device.queue.submit([enc.finish()]);
  const ts = await resolveTimestamps(qs, QPF);
  const st = await readStats();
  return { kind: ev.kind, prims: e.prims, cells_in_box: e.cells, relevant: e.relevant, cells_classified: st.cook_cells,
    bricks_cooked: st.cook_bricks, freed: st.freed, alloc_fail: st.alloc_fail,
    classify_ms: Number(ts[9] - ts[8]) / 1e6, cook_ms: Number(ts[11] - ts[10]) / 1e6, cpu_ms: e.cpu_ms };
}

async function bricksInUse() { return CAP - new Uint32Array(await readback(B.ctrl, 32))[4]; }

/** The far terrain's slope bound (T_SLOPE in trace.wgsl) must hold outside the brick region. */
function checkSlope() {
  let max = 0;
  const e = 0.05;
  for (let x = -900; x <= 900; x += 1.7) for (let z = -900; z <= 900; z += 1.7) {
    if (Math.abs(x) < 47 && Math.abs(z) < 47) continue;
    const gx = (terrainHeight(x + e, z) - terrainHeight(x - e, z)) / (2 * e);
    const gz = (terrainHeight(x, z + e) - terrainHeight(x, z - e)) / (2 * e);
    max = Math.max(max, Math.hypot(gx, gz));
  }
  return r3(max);
}

// ---- posing -----------------------------------------------------------------------------------------------

/** The GPU pose pass against the JS one, for every instance at one time. */
async function checkPose(n = MAX_INST, t = T_SNAP) {
  uploadStates(n, t);
  writeFrame(RES.full, SCENES.square, n, t, edlog.prims.length);
  const enc = device.createCommandEncoder();
  const p = enc.beginComputePass();
  p.setPipeline(P.pose);
  p.setBindGroup(0, RES.full.bgPose);
  p.dispatchWorkgroups(Math.ceil(n / 64));
  p.end();
  device.queue.submit([enc.finish()]);
  const gpu = new Float32Array(await readback(B.inst, n * STRIDE * 16));
  const cpu = new Float32Array(n * STRIDE * 4);
  poseJS(crowd, stateArr, n, cpu);
  let maxPart = 0, maxBound = 0, headerMismatch = 0;
  for (let i = 0; i < n; i++) {
    const b = i * STRIDE * 4;
    for (let k = 0; k < 4; k++) if (gpu[b + k] !== Math.fround(cpu[b + k])) headerMismatch++;
    for (let k = 12; k < 16; k++) maxBound = Math.max(maxBound, Math.abs(gpu[b + k] - cpu[b + k]));
    const en = cpu[b + 2];
    for (let part = 0; part < 16; part++) {
      if (!((en >> part) & 1)) continue;
      for (let k = 0; k < 8; k++) maxPart = Math.max(maxPart, Math.abs(gpu[b + 16 + part * 8 + k] - cpu[b + 16 + part * 8 + k]));
    }
  }
  return { instances: n, max_abs_diff_parts_m: maxPart, max_abs_diff_bound_m: maxBound, header_mismatches: headerMismatch };
}

/** CPU cost per frame of the root state (what stays on the CPU with GPU posing) and of posing in
 *  JS. performance.now() is coarsened to 0.1 ms here, so each figure is a batch of 50 frames,
 *  divided; the median of 7 batches. */
function cpuPosing() {
  const out = {};
  const buf = new Float32Array(MAX_INST * STRIDE * 4);
  for (const n of COUNTS) {
    const st = [], ps = [];
    for (let b = 0; b < 8; b++) {
      let t0 = performance.now();
      for (let k = 0; k < 50; k++) writeStates(crowd, n, 1 + (b * 50 + k) / 60, stateArr);
      const s1 = (performance.now() - t0) / 50;
      t0 = performance.now();
      for (let k = 0; k < 50; k++) poseJS(crowd, stateArr, n, buf);
      const s2 = (performance.now() - t0) / 50;
      if (b > 0) { st.push(s1); ps.push(s2); }
    }
    out[n] = { root_state_ms: Math.round(median(st) * 1e4) / 1e4, pose_js_ms: Math.round(median(ps) * 1e4) / 1e4,
      upload_bytes: n * S_FLOATS * 4 };
  }
  return out;
}

/** The crowd passes are each below one timestamp quantum (~65 µs), so time 20 back-to-back
 *  repetitions of each (separate passes) and divide. */
async function gpuCrowdPasses(n) {
  const reps = 20, res = RES.full, sc = SCENES.square;
  uploadStates(n, T_SNAP);
  writeFrame(res, sc, n, T_SNAP, edlog.prims.length);
  const out = {};
  for (const [name, pipe, bg, x, y] of [['pose', P.pose, res.bgPose, Math.ceil(n / 64), 1], ['cull', P.cull, res.bgBin, Math.ceil(n / 64), 1],
    ['bin_screen', P.bin_screen, res.bgBin, res.CX, res.CY], ['bin_light', P.bin_light, res.bgBin, 16, 16]]) {
    const ms = [];
    for (let round = 0; round < 3; round++) {
      const qs = device.createQuerySet({ type: 'timestamp', count: 2 });
      const enc = device.createCommandEncoder();
      for (let k = 0; k < reps; k++) {
        if (name === 'cull') enc.clearBuffer(B.ccount);
        const tw = k === 0 ? { timestampWrites: { querySet: qs, beginningOfPassWriteIndex: 0 } }
          : k === reps - 1 ? { timestampWrites: { querySet: qs, endOfPassWriteIndex: 1 } } : {};
        const p = enc.beginComputePass(tw);
        p.setPipeline(pipe);
        p.setBindGroup(0, bg);
        p.dispatchWorkgroups(x, y);
        p.end();
      }
      device.queue.submit([enc.finish()]);
      const ts = await resolveTimestamps(qs, 2);
      ms.push(Number(ts[1] - ts[0]) / 1e6 / reps);
    }
    out[name] = r3(median(ms));
  }
  return out;
}

// ---- run ---------------------------------------------------------------------------------------------------

async function setup() {
  await init();
  log('device', JSON.stringify(R.device));
  crowd = makeCrowd(MAX_INST);
  stateArr = new Float32Array(MAX_INST * S_FLOATS);
  edlog = new EditLog();
  layouts();
  await pipelines();
  resources();
  R.world.full_cook = await cookAll();
  log('full cook', JSON.stringify(R.world.full_cook));
  const pre = [];
  for (let i = 0; i < PREHISTORY; i++) pre.push(await editOnce());
  R.edits.prehistory_events = pre.length;
  R.edits.prehistory_prims = edlog.prims.length;
  const byKind = {};
  for (const e of pre) (byKind[e.kind] ||= []).push(e);
  R.edits.isolated_by_kind = Object.fromEntries(Object.entries(byKind).map(([k, v]) => [k, {
    events: v.length, prims: median(v.map(e => e.prims)), cells_classified: median(v.map(e => e.cells_classified)),
    bricks_cooked: median(v.map(e => e.bricks_cooked)), relevant: median(v.map(e => e.relevant)),
    classify_ms: r3(median(v.map(e => e.classify_ms))), cook_ms: r3(median(v.map(e => e.cook_ms))),
    cpu_ms: r3(median(v.map(e => e.cpu_ms))),
  }]));
  R.edits.isolated_alloc_fail = pre.reduce((s, e) => s + e.alloc_fail, 0);
  R.world.bricks_after_prehistory = await bricksInUse();
  log('edits (isolated)', JSON.stringify(R.edits.isolated_by_kind));
  log('bricks in use', R.world.bricks_after_prehistory, 'of', CAP);
}

async function shots(name, sc, res, variants = ['full'], n = N_MAIN) {
  for (const v of variants) {
    const img = await render(sc, res, v, { n });
    await png(img, `${name}${res.W === 960 ? '-540p' : ''}${v === 'full' ? '' : '-' + v}`);
  }
}

async function runAll() {
  const tStart = performance.now();
  try {
    await setup();
    R.world.far_terrain_max_slope = checkSlope();
    R.world.far_terrain_slope_bound = 0.5;
    if (R.world.far_terrain_max_slope > 0.5) err('far terrain slope bound violated', R.world.far_terrain_max_slope);
    R.cpu.pose_check = await checkPose();
    log('pose check', JSON.stringify(R.cpu.pose_check));
    R.cpu.posing = cpuPosing();
    R.cpu.note = 'pose_js_ms: the same posing as the GPU pass, in JS on the main thread (these creatures are simpler than spike 02\'s: 8–14 round cones, closed-form limbs, no bone matrices)';
    log('cpu posing', JSON.stringify(R.cpu.posing));

    // Scenes at N = 300: decomposition of creature cost at both resolutions.
    const sq = SCENES.square;
    for (const name of ['square', 'aerial', 'street']) {
      const sc = SCENES[name];
      for (const rk of ['full', 'half']) {
        const res = RES[rk];
        const key = `${name}/${rk}`;
        R.stats[key] = await stats(sc, res);
        const rounds = name === 'square' && rk === 'full' ? 3 : 1;
        const acc = {};
        const vs = ['full', 'nocrowd', 'crowd_noshadow', ...(name !== 'aerial' && rk === 'full' ? ['full_eps10'] : [])];
        for (let k = 0; k < rounds; k++) for (const v of vs) (acc[v] ||= []).push(await measure(sc, res, v));
        for (const v of Object.keys(acc)) {
          const rs = acc[v];
          R.frames[`${key}/${v}`] = Object.fromEntries(Object.keys(rs[0]).map(k => [k, typeof rs[0][k] === 'number' ? r3(median(rs.map(x => x[k]))) : rs[0][k]]));
          if (rounds > 1) R.frames[`${key}/${v}`].trace_rounds = rs.map(x => x.trace);
        }
        R.scenes[key] = creatureCost(key);
        if (acc.full_eps10) R.scenes[key].creatures_ms_eps10 = creatureCost(key, 'full_eps10').creatures_ms;
        log(key, JSON.stringify(R.scenes[key]), 'stats', JSON.stringify({ share: R.stats[key].creature_share, ovf: [R.stats[key].screen_ovf, R.stats[key].light_ovf, R.stats[key].coarse_ovf] }));
        await shots(name, sc, res);
      }
    }

    // The world on its own: bricks against analytic, with the current log and with none.
    for (const rk of ['full', 'half']) {
      const res = RES[rk];
      R.frames[`square/${rk}/world`] = await measure(sq, res, 'world');
      R.frames[`square/${rk}/world_noshade`] = await measure(sq, res, 'world_noshade');
      R.frames[`square/${rk}/world_analytic`] = await measureTiled(sq, res, 'world_analytic');
      R.frames[`square/${rk}/world_analytic_no_edits`] = await measureTiled(sq, res, 'world_analytic', { nedits: 0 });
      log(`world ${rk}`, R.frames[`square/${rk}/world`].trace, 'analytic', R.frames[`square/${rk}/world_analytic`].trace,
        'analytic, no edits', R.frames[`square/${rk}/world_analytic_no_edits`].trace, `(${edlog.prims.length} edits in the log)`);
    }
    R.stats['square/full/world_analytic'] = await stats(sq, RES.full, 'stats_analytic');

    // Instance count sweep.
    R.sweeps.count = {};
    for (const rk of ['full', 'half']) {
      for (const n of COUNTS) {
        const key = `square/${rk}/n${n}`;
        R.stats[key] = await stats(sq, RES[rk], 'stats', n);
        // The baseline is measured next to each point, so a clock change between points doesn't
        // land on the subtraction.
        const full = await measure(sq, RES[rk], 'full', { n });
        const ns = await measure(sq, RES[rk], 'crowd_noshadow', { n });
        const nc = await measure(sq, RES[rk], 'nocrowd', { n });
        R.frames[`${key}/full`] = full;
        R.frames[`${key}/crowd_noshadow`] = ns;
        R.frames[`${key}/nocrowd`] = nc;
        const base = nc.trace;
        R.sweeps.count[key] = {
          n, creature_share: R.stats[key].creature_share,
          pose: full.pose, cull: full.cull, bin_screen: full.bin_screen, bin_light: full.bin_light,
          visibility: r3(ns.trace - base), shadows: r3(full.trace - ns.trace), trace_marginal: r3(full.trace - base),
          creatures_ms: r3(full.pose + full.cull + full.bin_screen + full.bin_light + full.trace - base),
          frame_gpu: full.frame_gpu, frame_gpu_p95: full.frame_gpu_p95, cpu_state_ms: full.cpu_state_ms,
          overflow: { screen: R.stats[key].screen_ovf, light: R.stats[key].light_ovf, coarse: R.stats[key].coarse_ovf, coarse_light: R.stats[key].coarse_light_ovf },
          steps_per_creature_px: R.stats[key].steps_per_creature_px, parts_per_eval: R.stats[key].parts_per_eval,
          creature_shadow_marches: R.stats[key].sh_marches, entries_per_px: R.stats[key].entries_per_px,
        };
        if (rk === 'full') R.sweeps.count[key].gpu_passes_repeated = await gpuCrowdPasses(n);
        log(key, JSON.stringify(R.sweeps.count[key]));
      }
    }
    await shots('square-n1000', sq, RES.full, ['full'], 1000);

    // Correctness: fast paths against the brute-force reference.
    for (const name of ['square', 'street', 'aerial', 'wall']) await quality(name);
    for (const v of ['heat_world', 'heat_crowd', 'heat_shadow']) await shots('square', sq, RES.full, [v]);

    // Edit-rate sweep (the world keeps changing from here on).
    R.sweeps.edits = {};
    for (const rate of RATES) {
      const m = await measure(sq, RES.full, 'full', { editRate: rate, frames: 120 });
      R.sweeps.edits[rate] = m;
      log(`edits ${rate}/s`, JSON.stringify(m));
    }
    R.world.bricks_after_sweep = await bricksInUse();
    R.edits.prims_after_sweep = edlog.prims.length;
    R.paced['square/full'] = await paced(sq, RES.full, 'full');
    R.paced['square/full+20edits'] = await paced(sq, RES.full, 'full', { editRate: 20 });
    log('paced', JSON.stringify(R.paced));
    // After all those edits: the cache must still match base + log.
    await quality('wall', 'after-sweep', { crowd: false });
    // And the incrementally re-cooked cache must match one cooked from scratch from the same log.
    const inc = { wall: await render(SCENES.wall, RES.full, 'full'), square: await render(sq, RES.full, 'full') };
    R.world.recook_from_scratch = await cookAll();
    R.world.incremental_vs_scratch = {
      wall: diff(inc.wall, await render(SCENES.wall, RES.full, 'full')),
      square: diff(inc.square, await render(sq, RES.full, 'full')),
    };
    log('incremental vs scratch', JSON.stringify(R.world.incremental_vs_scratch), JSON.stringify(R.world.recook_from_scratch));
    R.world.bricks_final = await bricksInUse();
    R.world.final_alloc_fail = (await stats(sq, RES.full)).alloc_fail;
  } catch (e) {
    err('FAILED:', e.stack || e.message);
    R.error = String(e.stack || e.message);
  }
  try { R.summary = summarize(); log('summary', JSON.stringify(R.summary)); } catch (e) { err('summary failed', e.message); }
  R.finished = new Date().toISOString();
  R.run_seconds = r3((performance.now() - tStart) / 1000);
  await save(`run-${R.started.replace(/[:.]/g, '-')}.json`, JSON.stringify(R, null, 2));
  log('done in', R.run_seconds, 's');
  await save('DONE', 'ok');
}

/** The kill criteria (README), evaluated from this run's numbers. */
function summarize() {
  const verdict = (x, pass, inc) => x <= pass ? 'pass' : x <= inc ? 'inconclusive' : 'fail';
  const scenes = ['square', 'street', 'aerial', 'wall'].filter(n => R.quality[n]);
  const crowdOk = key => scenes.every(n => {
    const q = R.quality[n], cap = key === 'creature_path' ? q.caps.creature_share_of_creature_px : q.caps.creature_share_of_creature_px_eps10;
    return q[key].mean_abs_255 <= 0.5 && q[key].share_over_8 <= 0.005 && cap <= 0.001;
  });
  const sq = R.scenes['square/full'] || {};
  const quarter = crowdOk('creature_path'), tenth = crowdOk('creature_path_eps10');
  const ms = quarter ? sq.creatures_ms : tenth ? sq.creatures_ms_eps10 : NaN;
  const e20 = (R.sweeps.edits || {})[20] || {};
  const editV = (e20.recook_amortized <= 0.3 && e20.recook_max <= 1.0) ? 'pass'
    : (e20.recook_amortized <= 0.6 && e20.recook_max <= 2.0) ? 'inconclusive' : 'fail';
  const world = R.frames['square/full/world'], nc = R.frames['square/full/nocrowd'];
  return {
    note: 'Timings are only meaningful from a run on a quiet GPU.',
    creatures: {
      tolerance_used: quarter ? '1/4 px' : tenth ? '1/10 px' : 'neither passes criterion 3',
      creatures_ms: ms, creatures_ms_quarter_px: sq.creatures_ms, creatures_ms_tenth_px: sq.creatures_ms_eps10,
      of_which_shadows_ms: sq.creature_shadows, verdict: Number.isFinite(ms) ? verdict(ms, 2.5, 5.0) : 'fail (correctness)',
    },
    edits_20_per_s: { recook_amortized_ms: e20.recook_amortized, recook_worst_frame_ms: e20.recook_max, verdict: editV },
    correctness: Object.fromEntries(scenes.map(n => [n, {
      creature_path_quarter_px: R.quality[n].creature_path, creature_path_tenth_px: R.quality[n].creature_path_eps10,
      brick_cache: R.quality[n].brick_cache,
      brick_cache_verdict: R.quality[n].brick_cache.mean_abs_255 <= 1.0 && R.quality[n].brick_cache.share_over_8 <= 0.02 ? 'pass' : 'fail',
    }])),
    world_context: world && nc ? {
      world_primary_and_shading_ms: world.trace, world_shadows_ms: r3(nc.trace - world.trace),
      plus_edits_amortized_ms: r3(world.trace + (e20.recook_amortized || 0)), slice_ms: 3.0,
    } : null,
  };
}

function creatureCost(key, variant = 'full') {
  const f = R.frames[`${key}/${variant}`], nc = R.frames[`${key}/nocrowd`], ns = R.frames[`${key}/crowd_noshadow`];
  return {
    individuals: N_MAIN, creature_share: R.stats[key].creature_share,
    pose: f.pose, cull: f.cull, bin_screen: f.bin_screen, bin_light: f.bin_light,
    trace_full: f.trace, trace_nocrowd: nc.trace, trace_marginal: r3(f.trace - nc.trace),
    creature_shadows: r3(f.trace - ns.trace),
    creatures_ms: r3(f.pose + f.cull + f.bin_screen + f.bin_light + f.trace - nc.trace),
    frame_gpu: f.frame_gpu, frame_gpu_p95: f.frame_gpu_p95, cpu_state_ms: f.cpu_state_ms,
  };
}

async function quality(name, suffix = '', { crowd = true, withShots = true } = {}) {
  const sc = SCENES[name], res = RES.full;
  const tag = suffix ? `${name}-${suffix}` : name;
  const ref = await render(sc, res, 'ref');
  const cc = crowd ? await render(sc, res, 'crowd_check') : null;
  const cc10 = crowd ? await render(sc, res, 'crowd_check_eps10') : null;
  const full = await render(sc, res, 'full');
  const fa = await render(sc, res, 'full_analytic');
  const q = {
    edits_in_log: edlog.prims.length,
    ...(crowd ? { creature_path: diff(cc, ref), creature_path_eps10: diff(cc10, ref) } : {}),
    brick_cache: diff(full, fa),
    everything: diff(full, ref),
    fast: await stats(sc, res, 'stats'),
    fast10: await stats(sc, res, 'stats_eps10'),
    ref: await stats(sc, res, 'stats_ref'),
  };
  q.caps = {
    creature_share_of_creature_px: q.fast.creature_cap_share, creature_share_of_creature_px_eps10: q.fast10.creature_cap_share,
    world_share_of_px: q.fast.world_cap_share,
    world_shadow_share: q.fast.world_shadow_cap_share, creature_shadow_share: q.fast.creature_shadow_cap_share,
    ref_creature: q.ref.caps, ref_world: q.ref.w_caps, ref_world_shadow: q.ref.ws_caps, ref_creature_shadow: q.ref.sh_caps,
  };
  delete q.fast; delete q.fast10; delete q.ref;
  R.quality[tag] = q;
  log(`quality ${tag}`, JSON.stringify(q));
  if (withShots) {
    await png(full, `${tag}-full`);
    await png(ref, `${tag}-ref`);
    await png(diffImage(full, ref), `${tag}-diff`);
    if (cc) await png(diffImage(cc, ref), `${tag}-diff-crowd`);
    await png(diffImage(full, fa), `${tag}-diff-cache`);
  }
}

/** Renders each scene once and saves screenshots (development). */
async function quick() {
  try {
    await setup();
    for (const name of Object.keys(SCENES)) {
      await shots(name, SCENES[name], RES.full);
      const s = await stats(SCENES[name], RES.full);
      log(`${name} stats`, JSON.stringify(s));
    }
    for (const v of ['heat_world', 'heat_crowd', 'heat_shadow']) await shots('square', SCENES.square, RES.full, [v]);
    if (location.hash.includes('stress')) {
      for (let k = 0; k < 230; k++) await editOnce();
      log('stress: edits in log', edlog.prims.length);
      for (const name of ['wall', 'square']) await quality(name, 'stress', { crowd: name === 'square' });
      log('longest tile', R.longest_tile_ms, 'grid', JSON.stringify(R.edits.grid));
      for (const v of ['world', 'world_analytic']) log(`time ${v}`, JSON.stringify(v === 'world' ? await measure(SCENES.square, RES.full, v, { frames: 30 }) : await measureTiled(SCENES.square, RES.full, v)));
    }
    if (location.hash.includes('quality')) {
      for (const name of (location.hash.includes('wallonly') ? ['wall'] : ['square', 'street', 'wall'])) await quality(name, 'q');
    }
    if (location.hash.includes('time')) {
      for (const v of ['full', 'nocrowd', 'crowd_noshadow', 'world', 'world_noshade']) log(`time square/${v}`, JSON.stringify(await measure(SCENES.square, RES.full, v, { frames: 60 })));
      log('time edits 20/s', JSON.stringify(await measure(SCENES.square, RES.full, 'full', { frames: 90, editRate: 20 })));
    }
    window.__render = async (name, v = 'full', o = {}) => png(await render(SCENES[name], RES[o.res || 'full'], v, o), `dev-${name}-${v}`);
    window.__measure = (name, v = 'full', o = {}) => measure(SCENES[name], RES[o.res || 'full'], v, o);
    window.__stats = (name, v = 'stats', n = N_MAIN) => stats(SCENES[name], RES.full, v, n);
  } catch (e) {
    err('FAILED:', e.stack || e.message);
  }
  log('quick done');
  await save('DONE', 'ok');
}

const mode = location.hash;
$('run').addEventListener('click', runAll);
if (mode === '#run') runAll();
else if (mode.startsWith('#quick')) quick();
