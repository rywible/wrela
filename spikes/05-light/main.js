// Spike harness: scenes, pipelines, the per-frame passes, sweeps, timing, the path-traced reference
// and the comparison. It plays the engine's role plus the measuring. It isn't compiler output.

const FULL = [1920, 1080], HALF = [960, 540];
const TPR = 128, IRR_T = 8, DEP_T = 16, MAX_R = 64, MAX_PROBES = 16384, ROWS = MAX_PROBES / TPR;
const VOL0_DIMS = [192, 104, 300], VOL1_DIMS = [256, 32, 256];
const FRAME_BYTES = 512;
const HM_DIMS = [2048, 2048];
const PASSES = ['primary', 'sun', 'sky', 'temporal', 'ptrace', 'pirr', 'pdepth', 'gather', 'composite'];
const LIGHTING = PASSES.slice(1);
const Q = PASSES.length * 2 + 4;   // + the second halves of the primary and sun passes
const U = GPUBufferUsage, TU = GPUTextureUsage;
const deg = Math.PI / 180;
const BUDGET_MS = 3.5;
const REF_BUDGET_HALF_MS = 2500;

const $ = id => document.getElementById(id);
const log = (...a) => { $('log').textContent += a.join(' ') + '\n'; console.log(...a); };
const fail = (...a) => { $('log').textContent += 'ERROR ' + a.join(' ') + '\n'; console.error(...a); };

const R = { started: new Date().toISOString(), device: {}, pipelines: {}, scenes: {} };
window.__results = R;

let device, ctx, canvasFormat;
const src = {}, L = {}, P = {}, T = {}, B = {}, BG = {};
let lineMap = [];
let VOLFMT = 'rgba16float';

const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[s.length >> 1] : NaN; };
const pct = (a, q) => { const s = [...a].sort((x, y) => x - y); return s.length ? s[Math.min(s.length - 1, Math.floor(s.length * q))] : NaN; };
const r3 = x => Math.round(x * 1000) / 1000;
const r5 = x => Math.round(x * 100000) / 100000;
const norm = a => { const l = Math.hypot(...a); return a.map(x => x / l); };
const sub = (a, b) => a.map((x, i) => x - b[i]);
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sunDir = (az, el) => [Math.cos(el * deg) * Math.cos(az * deg), Math.sin(el * deg), Math.cos(el * deg) * Math.sin(az * deg)];
const sleep = ms => new Promise(r => setTimeout(r, ms));

function mulberry(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// ---- the landscape's terrain in JS (only to place its camera) ------------------------------------------

const hash2i = (x, y) => { let h = (Math.imul(x | 0, 0x8da6b343) ^ Math.imul(y | 0, 0xd8163841)) >>> 0; h = Math.imul(h ^ (h >>> 15), 0x2c1b3c6d) >>> 0; return (h ^ (h >>> 12)) >>> 0; };
const s11 = h => (h >>> 8) * (2 / 16777216) - 1;
function vnoise2(px, py) {
  const ix = Math.floor(px), iy = Math.floor(py), fx = px - ix, fy = py - iy;
  const ux = fx * fx * (3 - 2 * fx), uy = fy * fy * (3 - 2 * fy);
  const a = s11(hash2i(ix, iy)), b = s11(hash2i(ix + 1, iy)), c = s11(hash2i(ix, iy + 1)), d = s11(hash2i(ix + 1, iy + 1));
  return a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy;
}
function landH(x, z) {
  const xc = 70 * Math.sin(z * 0.0021) + 28 * Math.sin(z * 0.0057 + 1);
  const u = (x - xc) / 320;
  let h = 150 * (1 - Math.exp(-3 * u * u)) - 10, amp = 30, f = 1 / 260, ox = 0, oy = 0;
  for (let i = 0; i < 5; i++) { h += amp * vnoise2(x * f + ox, z * f + oy); amp *= 0.42; f *= 2.3; ox += 17.3; oy += 31.7; }
  return h;
}

// ---- scenes -----------------------------------------------------------------------------------------------

const SKY_DAY = { skyZ: [0.22, 0.38, 0.75], skyH: [0.58, 0.68, 0.82], aureole: 1.0, below: 0.35 };

const SCENES = [
  {
    name: 'forest', id: 0,
    eye: [2.0, 1.6, 0.0], target: [3.5, 5.0, 14.0], fovy: 60,
    sun: sunDir(115, 42), sunRadiusDeg: 0.3, sunE: [3.0, 2.8, 2.35], exposure: 2.9, ...SKY_DAY,
    fog: [0.34, 0.42, 0.40], fogDensity: 1 / 230,
    probe: { origin: [-30, -2, -8], extent: [60, 24, 60], spacing: { fine: 2, base: 3, coarse: 4 } },
    vol0: { origin: [-24, -2, -10], voxel: 0.25, dims: [192, 104, 208] },
    vol1: { origin: [-80, -2, -60], voxel: 1.0, dims: [160, 26, 200] },
    hm: { origin: [-64, -32], texel: 0.25, dims: [512, 512] },
    steps: { SUN_STEPS: 160, PSHADOW_STEPS: 96 },
    L: { short: 4, base: 12, long: 32, ao: 2 }, sunMax: 60, probeMax: 80, refBudget: 8000,
  },
  {
    name: 'tower', id: 1,
    eye: [-0.92, 1.6, -2.0], target: [3.0, 1.0, 0.4], fovy: 70,
    sun: sunDir(3, 26), sunRadiusDeg: 0.3, sunE: [3.0, 2.78, 2.4], exposure: 7, ...SKY_DAY,
    fog: [0, 0, 0], fogDensity: 0,
    probe: { origin: [-6, -0.5, -6], extent: [12, 7, 12], spacing: { fine: 0.5, base: 0.75, coarse: 1.5 } },
    vol0: { origin: [-6.4, -1.0, -6.4], voxel: 0.1, dims: [128, 100, 128] },
    vol1: { origin: [-40, -2, -40], voxel: 0.5, dims: [160, 24, 160] },
    hm: { origin: [-32, -32], texel: 0.125, dims: [512, 512] },
    steps: {},
    L: { short: 2, base: 6, long: 16, ao: 1 }, sunMax: 40, probeMax: 40, refBudget: 12000,
  },
  {
    name: 'landscape', id: 2,
    eye: [180, 0, -120], target: [-40, 0, 900], eyeAbove: 10, targetAbove: 0, fovy: 50,
    sun: sunDir(205, 7), sunRadiusDeg: 0.4, sunE: [3.4, 2.1, 0.95], exposure: 2.1,
    skyZ: [0.14, 0.22, 0.46], skyH: [0.82, 0.58, 0.40], aureole: 2.5, below: 0.4,
    fog: [0.52, 0.40, 0.34], fogDensity: 1 / 1500,
    probe: { origin: [-480, -40, -200], extent: [960, 320, 1280], spacing: { fine: 32, base: 48, coarse: 64 } },
    vol0: { origin: [-300, -40, -200], voxel: 4, dims: [150, 75, 300] },
    vol1: { origin: [-1536, -60, -800], voxel: 16, dims: [192, 20, 256] },
    hm: { origin: [-4096, -4096], texel: 4, dims: [2048, 2048] },
    steps: { PRIMARY_STEPS: 512, SUN_STEPS: 192, PSHADOW_STEPS: 128, PROBE_STEPS: 96 },
    L: { short: 40, base: 160, long: 600, ao: 8 }, sunMax: 2500, probeMax: 2500, refBudget: 6000,
  },
];
for (const sc of SCENES) {
  if (sc.eyeAbove != null) {
    sc.eye[1] = landH(sc.eye[0], sc.eye[2]) + sc.eyeAbove;
    sc.target[1] = landH(sc.target[0], sc.target[2]) + sc.targetAbove;
  }
}

// ---- configurations -----------------------------------------------------------------------------------------

const BASE = {
  skyHalf: true, K: 4, L: 'base', skySrc: 'cones', skyNear: 1,               // sky cones (near zone in fine voxels)
  field: 'hybrid', sunField: 'hybrid', sunTerrain: 'map', sunNear: 2.5, sunHalf: false,   // fields; sun rays
  spacing: 'base', R: 32, H: 16, vis: 'ddgi', baked: false,                    // probes
  inline: false, gatherHalf: true,                                             // gather at half resolution
};
const FIELDS = { analytic: 0, cooked: 1, hybrid: 2 };
const ALL = ['forest', 'tower', 'landscape'];
const SWEEP = [
  ['base', {}],
  ['K1', { K: 1 }], ['K2', { K: 2 }],
  ['L_short', { L: 'short' }], ['L_long', { L: 'long' }],
  ['sky_probes', { skySrc: 'probes' }],
  ['probes_ao', { skySrc: 'probes_ao', L: 'ao' }],
  ['spacing_fine', { spacing: 'fine' }, ['forest', 'tower']], ['spacing_coarse', { spacing: 'coarse' }],
  ['R64', { R: 64 }],
  ['H1', { H: 1 }], ['H64', { H: 64 }],
  ['vis_none', { vis: 'none' }, ['tower']],
  ['all_cooked', { field: 'cooked', sunField: 'cooked' }],
  ['all_analytic', { field: 'analytic', sunField: 'analytic' }, ['forest', 'tower']],
  ['sun_half', { sunHalf: true }],
  ['sun_near0', { sunNear: 0 }],
  ['sun_terrain_march', { sunTerrain: 'march' }, ['landscape']],
  // Combinations of the cheaper options: all cooked, half-resolution sun, probe sky with cone AO.
  ['fast', { field: 'cooked', sunField: 'cooked', sunHalf: true, skySrc: 'probes_ao', L: 'ao', K: 2, R: 16 }],
  ['fast_fullsun', { field: 'cooked', sunField: 'cooked', skySrc: 'probes_ao', L: 'ao', K: 2, R: 16 }],
  ['fallback', { baked: true, skySrc: 'probes' }],
  ['fallback_fast', { baked: true, skySrc: 'probes', sunField: 'cooked', sunHalf: true }],
  ['base_again', {}],
];
const cfgOf = name => SWEEP.find(e => e[0] === name)[1];
const HALF_SWEEP = [['base', {}], ['fast', cfgOf('fast')], ['fallback_fast', cfgOf('fallback_fast')]];
const SNAP_CONFIGS = new Set(['base', 'vis_none', 'fallback_fast', 'L_short', 'sky_probes', 'probes_ao', 'fast', 'all_cooked', 'all_analytic', 'H1']);
const COMPARE_CONFIGS = new Set(['fast', 'fallback_fast', 'probes_ao']);

// ---- GPU helpers -----------------------------------------------------------------------------------------------

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
  try {
    const r = await fetch(`results/${name}`, { method: 'PUT', body });
    if (!r.ok) fail('save failed', name, r.status);
  } catch (e) { fail('save failed', name, e.message); }
}

async function done() { await device.queue.onSubmittedWorkDone(); }

// ---- setup -----------------------------------------------------------------------------------------------------------

async function init() {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
  if (!adapter) throw new Error('no WebGPU adapter');
  const want = ['timestamp-query', 'texture-formats-tier1', 'float32-filterable'].filter(f => adapter.features.has(f));
  device = await adapter.requestDevice({
    requiredFeatures: want,
    requiredLimits: {
      maxStorageBufferBindingSize: adapter.limits.maxStorageBufferBindingSize,
      maxBufferSize: adapter.limits.maxBufferSize,
      maxStorageTexturesPerShaderStage: Math.min(8, adapter.limits.maxStorageTexturesPerShaderStage),
      maxStorageBuffersPerShaderStage: Math.min(10, adapter.limits.maxStorageBuffersPerShaderStage),
    },
  });
  device.lost.then(i => fail('DEVICE LOST:', i.message));
  device.addEventListener('uncapturederror', e => fail('GPU ERROR:', e.error.message));
  const info = adapter.info || {};
  R.device = {
    vendor: info.vendor, architecture: info.architecture, description: info.description,
    userAgent: navigator.userAgent, timestampQuery: want.includes('timestamp-query'),
    pageVisibility: document.visibilityState,
  };
  if (!R.device.timestampQuery) throw new Error('timestamp-query is required');
  if (!want.includes('float32-filterable')) throw new Error('float32-filterable is required (cooked height map)');
  R.device.volumeFormat = VOLFMT = want.includes('texture-formats-tier1') ? 'r16float' : 'rgba16float';
  for (const f of ['common', 'scenes', 'light', 'ref', 'blit']) src[f] = await (await fetch(`${f}.wgsl`)).text();

  const canvas = $('view');
  canvas.width = FULL[0];
  canvas.height = FULL[1];
  ctx = canvas.getContext('webgpu');
  canvasFormat = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format: canvasFormat, alphaMode: 'opaque' });

  const C = GPUShaderStage.COMPUTE;
  const E = {
    ub: { buffer: { type: 'uniform' } }, sb: { buffer: { type: 'storage' } }, rb: { buffer: { type: 'read-only-storage' } },
    tf: { texture: { sampleType: 'float' } }, tu: { texture: { sampleType: 'unfilterable-float' } },
    t3: { texture: { sampleType: 'float', viewDimension: '3d' } }, smp: { sampler: { type: 'filtering' } },
    w: format => ({ storageTexture: { access: 'write-only', format } }),
    w3: format => ({ storageTexture: { access: 'write-only', format, viewDimension: '3d' } }),
  };
  const layout = entries => device.createBindGroupLayout({
    entries: Object.entries(entries).map(([b, e]) => ({ binding: +b, visibility: C, ...e })),
  });
  L.g0 = layout({ 0: E.ub, 1: E.sb });
  const defs = {
    primary: { 0: E.w('r32float'), 1: E.w('rgba16float'), 2: E.w('rgba8unorm'), 26: E.smp, 39: E.tf },
    sun: { 3: E.tu, 4: E.tf, 6: E.w('r32float'), 26: E.smp, 27: E.t3, 38: E.t3, 39: E.tf, 41: E.tf },
    sky: { 3: E.tu, 4: E.tf, 8: E.w('rgba16float'), 26: E.smp, 27: E.t3, 38: E.t3, 39: E.tf, 41: E.tf },
    temporal: { 3: E.tu, 9: E.tf, 10: E.tf, 11: E.tu, 12: E.w('rgba16float'), 13: E.w('r32float') },
    ptrace: { 16: E.rb, 18: E.sb, 20: E.tf, 21: E.tf, 22: E.tf, 26: E.smp, 27: E.t3, 38: E.t3, 39: E.tf, 41: E.tf },
    pirr: { 16: E.rb, 19: E.rb, 20: E.tf, 21: E.tf, 23: E.w('rgba16float'), 24: E.w('rgba16float') },
    pdepth: { 16: E.rb, 19: E.rb, 22: E.tf, 25: E.w('rgba16float') },
    gather: { 3: E.tu, 4: E.tf, 16: E.rb, 20: E.tf, 21: E.tf, 22: E.tf, 26: E.smp, 29: E.w('rgba16float'), 30: E.w('rgba16float') },
    composite_g: { 3: E.tu, 4: E.tf, 5: E.tf, 7: E.tu, 14: E.tf, 15: E.tu, 16: E.rb, 20: E.tf, 21: E.tf, 22: E.tf, 26: E.smp, 31: E.tf, 32: E.tf, 33: E.w('rgba16float') },
    cook: { 28: E.w3(VOLFMT), 26: E.smp, 39: E.tf },
    canary: {},
    cook_height: { 40: E.w('rgba32float') },
    cook_shadow: { 26: E.smp, 39: E.tf, 42: E.w('rg32float') },
    relocate: { 17: E.sb, 26: E.smp, 39: E.tf },
    ref_trace: { 3: E.tu, 4: E.tf, 34: E.sb, 35: E.sb, 36: E.sb, 37: E.sb, 26: E.smp, 27: E.t3, 38: E.t3, 39: E.tf },
    ref_resolve: { 3: E.tu, 4: E.tf, 5: E.tf, 33: E.w('rgba16float'), 34: E.sb, 35: E.sb, 36: E.sb, 37: E.sb },
  };
  for (const [k, v] of Object.entries(defs)) L[k] = layout(v);
  L.blit = device.createBindGroupLayout({
    entries: [
      { binding: 0, visibility: GPUShaderStage.FRAGMENT, texture: {} },
      { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: {} },
      { binding: 2, visibility: GPUShaderStage.VERTEX, buffer: { type: 'uniform' } },
    ],
  });
}

function moduleSource() {
  let code = '';
  lineMap = [];
  for (const f of ['common', 'scenes', 'light', 'ref']) {
    lineMap.push([f, code.split('\n').length]);
    code += src[f].replaceAll('VOLFMT', VOLFMT) + '\n';
  }
  return code;
}
function where(line) {
  let name = '?', start = 1;
  for (const [f, s] of lineMap) if (line >= s) { name = f; start = s; }
  return `${name}.wgsl:${line - start + 1}`;
}

async function checkModule(module, label) {
  const info = await module.getCompilationInfo();
  for (const m of info.messages) {
    const msg = `${label} ${m.type} at ${where(m.lineNum)}:${m.linePos} ${m.message}`;
    if (m.type === 'error') fail(msg); else log(msg);
  }
  if (info.messages.some(m => m.type === 'error')) throw new Error(`${label}: WGSL errors`);
}

// [key, entry point, layout, constants]. Per-scene pipelines get SCENE; shared ones don't use it.
const PER_SCENE = [
  ['primary', 'primary', 'primary', {}],
  // Cooked and hybrid sun rays take terrain shadows from the shadow-height map; analytic ones march it.
  ...[0, 1, 2].flatMap(f => [
    [`sun_${f}`, 'sun', 'sun', { SUN_FIELD: f, SUN_TMAP: f !== 0 }],
    [`sky_${f}`, 'sky', 'sky', { FIELD: f }],
    [`ptrace_${f}`, 'probe_trace', 'ptrace', { FIELD: f, VIS: 2, SUN_TMAP: f !== 0 }],
    [`ptrace_naive_${f}`, 'probe_trace', 'ptrace', { FIELD: f, VIS: 0, SUN_TMAP: f !== 0 }],
  ]),
  ['sun_2_march', 'sun', 'sun', { SUN_FIELD: 2, SUN_TMAP: false }],
  ['sky_ao_1', 'sky', 'sky', { FIELD: 1, SKY_AO: true }],
  ['sky_ao_2', 'sky', 'sky', { FIELD: 2, SKY_AO: true }],
  ['cook_shadow', 'cook_shadow', 'cook_shadow', {}],
  ['cook', 'cook', 'cook', {}],
  ['cook_height', 'cook_height', 'cook_height', {}],
  ['relocate', 'relocate', 'relocate', {}],
  ['ref_sun', 'ref_trace', 'ref_trace', { REF_MODE: 0 }],
  ['ref_ind', 'ref_trace', 'ref_trace', { REF_MODE: 1 }],
];
const SHARED = [
  ['temporal', 'temporal', 'temporal', {}],
  ['pirr', 'probe_irr', 'pirr', {}],
  ['pdepth', 'probe_depth', 'pdepth', {}],
  ['gather_ddgi', 'gather', 'gather', { VIS: 2 }],
  ['gather_none', 'gather', 'gather', { VIS: 0 }],
  ['comp_cones', 'composite', 'composite_g', { SKY_SRC: 0 }],
  ['comp_probes', 'composite', 'composite_g', { SKY_SRC: 1 }],
  ['comp_ao', 'composite', 'composite_g', { SKY_SRC: 2 }],
  ['comp_cones_g', 'composite', 'composite_g', { SKY_SRC: 0, INLINE_GATHER: true, VIS: 2 }],
  ['comp_probes_g', 'composite', 'composite_g', { SKY_SRC: 1, INLINE_GATHER: true, VIS: 2 }],
  ['ref_resolve', 'ref_resolve', 'ref_resolve', {}],
  ['canary', 'canary', 'canary', {}],
];

let shaderModule = null;
const compiling = new Map();
let pipelineMs = 0;

/** Compiles pipelines on demand, at most four at a time (Metal shader compilers are heavy, and many
 *  at once starve the display). `keys` are shared keys or [sceneId, key] pairs. */
async function need(keys) {
  if (!shaderModule) {
    shaderModule = device.createShaderModule({ code: moduleSource(), label: 'spike05' });
    await checkModule(shaderModule, 'spike05');
  }
  const make = (key, entry, lay, constants) => device.createComputePipelineAsync({
    label: key,
    layout: device.createPipelineLayout({ bindGroupLayouts: [L.g0, L[lay]] }),
    compute: { module: shaderModule, entryPoint: entry, constants },
  }).catch(e => { throw new Error(`pipeline ${key}: ${e.message}`); });
  const jobs = [];
  for (const k of keys) {
    if (Array.isArray(k)) {
      const [id, key] = k;
      P[id] ||= {};
      if (P[id][key] || compiling.has(`${id}/${key}`)) continue;
      const def = PER_SCENE.find(d => d[0] === key);
      if (!def) throw new Error(`unknown pipeline ${key}`);
      const steps = SCENES.find(sc => sc.id === id).steps;
      jobs.push([`${id}/${key}`, () => make(key, def[1], def[2], { SCENE: id, ...steps, ...def[3] }).then(p => { P[id][key] = p; })]);
    } else {
      if (P[k] || compiling.has(k)) continue;
      const def = SHARED.find(d => d[0] === k);
      if (!def) throw new Error(`unknown pipeline ${k}`);
      jobs.push([k, () => make(k, def[1], def[2], { SCENE: 0, ...def[3] }).then(p => { P[k] = p; })]);
    }
  }
  const t0 = performance.now();
  for (let i = 0; i < jobs.length; i += 4) {
    const batch = jobs.slice(i, i + 4).map(([name, f]) => { const pr = f(); compiling.set(name, pr); return pr; });
    await Promise.all(batch);
  }
  pipelineMs += performance.now() - t0;
  R.pipelines.compiled = Object.keys(P).length;
  R.pipelines.compile_ms_total = r3(pipelineMs);
}

const ALL_SHARED = () => SHARED.map(d => d[0]);
const ALL_SCENE = id => PER_SCENE.map(d => [id, d[0]]);
const QUICK_SCENE = id => ['primary', 'sun_2', 'sky_2', 'ptrace_2', 'ptrace_0', 'sun_1', 'sky_ao_1', 'ptrace_1', 'cook', 'cook_height', 'cook_shadow', 'relocate', 'ref_sun', 'ref_ind'].map(k => [id, k]);
const QUICK_SHARED = ['temporal', 'pirr', 'pdepth', 'gather_ddgi', 'comp_cones', 'comp_probes', 'comp_ao', 'ref_resolve', 'canary'];

async function blitPipeline() {
  const bm = device.createShaderModule({ code: src.blit, label: 'blit' });
  await checkModule(bm, 'blit');
  P.blit = await device.createRenderPipelineAsync({
    layout: device.createPipelineLayout({ bindGroupLayouts: [L.blit] }),
    vertex: { module: bm, entryPoint: 'blit_vs' },
    fragment: { module: bm, entryPoint: 'blit_fs', targets: [{ format: canvasFormat }] },
  });
  R.pipelines.per_scene_frame = 'primary, sun, sky, temporal, probe_trace, probe_irr, probe_depth, gather, composite = 9';
}

function resources() {
  const [W, H] = FULL;
  const st = TU.STORAGE_BINDING | TU.TEXTURE_BINDING | TU.COPY_SRC;
  const tex = (w, h, format) => device.createTexture({ size: [w, h], format, usage: st });
  T.gt = tex(W, H, 'r32float');
  T.gn = tex(W, H, 'rgba16float');
  T.ga = tex(W, H, 'rgba8unorm');
  T.sun = tex(W, H, 'r32float');
  T.sky = tex(W, H, 'rgba16float');
  T.hist = [tex(W, H, 'rgba16float'), tex(W, H, 'rgba16float')];
  T.hd = [tex(W, H, 'r32float'), tex(W, H, 'r32float')];
  T.irrS = [0, 1].map(() => tex(TPR * IRR_T, ROWS * IRR_T, 'rgba16float'));
  T.irrB = [0, 1].map(() => tex(TPR * IRR_T, ROWS * IRR_T, 'rgba16float'));
  T.dep = [0, 1].map(() => tex(TPR * DEP_T, ROWS * DEP_T, 'rgba16float'));
  T.bounce = tex(W, H, 'rgba16float');
  T.skyp = tex(W, H, 'rgba16float');
  T.out = tex(W, H, 'rgba16float');
  T.vol0 = device.createTexture({ size: VOL0_DIMS, dimension: '3d', format: VOLFMT, usage: TU.STORAGE_BINDING | TU.TEXTURE_BINDING });
  T.vol1 = device.createTexture({ size: VOL1_DIMS, dimension: '3d', format: VOLFMT, usage: TU.STORAGE_BINDING | TU.TEXTURE_BINDING });
  T.hm = device.createTexture({ size: HM_DIMS, format: 'rgba32float', usage: TU.STORAGE_BINDING | TU.TEXTURE_BINDING });
  T.sh = device.createTexture({ size: HM_DIMS, format: 'rg32float', usage: TU.STORAGE_BINDING | TU.TEXTURE_BINDING });
  B.frame = buffer(FRAME_BYTES, U.UNIFORM | U.COPY_DST);
  B.stats = buffer(256, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  B.probes = buffer(MAX_PROBES * 16, U.STORAGE | U.COPY_SRC);
  B.rays = buffer(MAX_PROBES * MAX_R * 32, U.STORAGE);
  B.acc = [0, 1, 2, 3].map(() => buffer(W * H * 16, U.STORAGE | U.COPY_DST));
  B.blitScale = buffer(16, U.UNIFORM | U.COPY_DST);
  const lin = device.createSampler({ magFilter: 'linear', minFilter: 'linear', addressModeU: 'clamp-to-edge', addressModeV: 'clamp-to-edge', addressModeW: 'clamp-to-edge' });

  const v = t => t.createView();
  const res = x => (x instanceof GPUBuffer ? { buffer: x } : x instanceof GPUTexture ? x.createView() : x);
  const bg = (lay, entries) => device.createBindGroup({ layout: L[lay], entries: Object.entries(entries).map(([b, r]) => ({ binding: +b, resource: res(r) })) });
  BG.g0 = bg('g0', { 0: B.frame, 1: B.stats });
  BG.primary = bg('primary', { 0: T.gt, 1: T.gn, 2: T.ga, 26: lin, 39: v(T.hm) });
  BG.sun = bg('sun', { 3: T.gt, 4: T.gn, 6: T.sun, 26: lin, 27: v(T.vol0), 38: v(T.vol1), 39: v(T.hm), 41: v(T.sh) });
  BG.sky = bg('sky', { 3: T.gt, 4: T.gn, 8: T.sky, 26: lin, 27: v(T.vol0), 38: v(T.vol1), 39: v(T.hm), 41: v(T.sh) });
  BG.temporal = [0, 1].map(p => bg('temporal', { 3: T.gt, 9: T.sky, 10: T.hist[p], 11: T.hd[p], 12: T.hist[1 - p], 13: T.hd[1 - p] }));
  BG.ptrace = [0, 1].map(p => bg('ptrace', { 16: B.probes, 18: B.rays, 20: T.irrS[p], 21: T.irrB[p], 22: T.dep[p], 26: lin, 27: v(T.vol0), 38: v(T.vol1), 39: v(T.hm), 41: v(T.sh) }));
  BG.pirr = [0, 1].map(p => bg('pirr', { 16: B.probes, 19: B.rays, 20: T.irrS[p], 21: T.irrB[p], 23: T.irrS[1 - p], 24: T.irrB[1 - p] }));
  BG.pdepth = [0, 1].map(p => bg('pdepth', { 16: B.probes, 19: B.rays, 22: T.dep[p], 25: T.dep[1 - p] }));
  BG.gather = [0, 1].map(q => bg('gather', { 3: T.gt, 4: T.gn, 16: B.probes, 20: T.irrS[q], 21: T.irrB[q], 22: T.dep[q], 26: lin, 29: T.bounce, 30: T.skyp }));
  BG.compositeG = [0, 1].map(p => [0, 1].map(q => bg('composite_g', { 3: T.gt, 4: T.gn, 5: T.ga, 7: T.sun, 14: T.hist[1 - p], 15: T.hd[1 - p], 16: B.probes, 20: T.irrS[q], 21: T.irrB[q], 22: T.dep[q], 26: lin, 31: T.bounce, 32: T.skyp, 33: T.out })));
  BG.cook = [bg('cook', { 28: v(T.vol0), 26: lin, 39: v(T.hm) }), bg('cook', { 28: v(T.vol1), 26: lin, 39: v(T.hm) })];
  BG.canary = bg('canary', {});
  BG.cookHeight = bg('cook_height', { 40: v(T.hm) });
  BG.cookShadow = bg('cook_shadow', { 26: lin, 39: v(T.hm), 42: v(T.sh) });
  BG.relocate = bg('relocate', { 17: B.probes, 26: lin, 39: v(T.hm) });
  BG.ref = bg('ref_trace', { 3: T.gt, 4: T.gn, 34: B.acc[0], 35: B.acc[1], 36: B.acc[2], 37: B.acc[3], 26: lin, 27: v(T.vol0), 38: v(T.vol1), 39: v(T.hm) });
  BG.resolve = bg('ref_resolve', { 3: T.gt, 4: T.gn, 5: T.ga, 33: T.out, 34: B.acc[0], 35: B.acc[1], 36: B.acc[2], 37: B.acc[3] });
  BG.blit = device.createBindGroup({
    layout: L.blit,
    entries: [{ binding: 0, resource: T.out.createView() }, { binding: 1, resource: lin }, { binding: 2, resource: { buffer: B.blitScale } }],
  });
  R.memory_mb = {
    gbuffer_and_lighting_targets: r3((W * H * (4 + 8 + 4 + 4 + 8 + 16 + 8 + 8 + 8 + 8)) / 2 ** 20),
    probe_atlases: r3((2 * 2 * TPR * IRR_T * ROWS * IRR_T * 8 + 2 * TPR * DEP_T * ROWS * DEP_T * 8) / 2 ** 20),
    probe_rays: r3(MAX_PROBES * MAX_R * 32 / 2 ** 20),
    cooked_clipmap: r3((VOL0_DIMS[0] * VOL0_DIMS[1] * VOL0_DIMS[2] + VOL1_DIMS[0] * VOL1_DIMS[1] * VOL1_DIMS[2]) * (VOLFMT === 'r16float' ? 2 : 8) / 2 ** 20),
    reference_accumulators: r3(4 * W * H * 16 / 2 ** 20),
    note: 'allocated for the largest case (1080p, 16384 probes, 64 rays per probe, the largest clipmap levels)',
  };
}

// ---- state, frame uniform ------------------------------------------------------------------------------------------

const S = { sc: null, cfg: null, dims: FULL, cam: null, grid: null, frame: 0, sinceReset: 0, pp: 0, sp: 0, bakedIdx: -1, probeKey: '' };

function camera(sc, W, H) {
  const f = norm(sub(sc.target, sc.eye));
  const r = norm(cross(f, [0, 1, 0]));
  const u = cross(r, f);
  const ty = Math.tan(sc.fovy * deg / 2), tx = ty * W / H;
  return { eye: sc.eye, cf: f, cr: r.map(x => x * tx), cu: u.map(x => x * ty), pix: 2 * ty / H };
}

function probeGrid(sc, spacing) {
  const s = sc.probe.spacing[spacing];
  const n = sc.probe.extent.map(e => Math.floor(e / s + 1e-6) + 1);
  const count = n[0] * n[1] * n[2];
  if (count > MAX_PROBES) throw new Error(`${sc.name}: ${count} probes exceeds ${MAX_PROBES}`);
  return { origin: sc.probe.origin, s, n, count, spacing };
}

function randomRotation(r) {
  const u1 = r(), u2 = r(), u3 = r();
  const a = Math.sqrt(1 - u1), b = Math.sqrt(u1);
  const x = a * Math.sin(2 * Math.PI * u2), y = a * Math.cos(2 * Math.PI * u2), z = b * Math.sin(2 * Math.PI * u3), w = b * Math.cos(2 * Math.PI * u3);
  return [[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
    [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
    [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]];
}

function writeFrame(o = {}) {
  const sc = S.sc, cfg = S.cfg, [W, H] = S.dims, cam = S.cam, g = S.grid;
  const buf = new ArrayBuffer(FRAME_BYTES), f = new Float32Array(buf), u = new Uint32Array(buf);
  f.set([...cam.eye, cam.pix], 0);
  f.set([...cam.cf, 0], 4);
  f.set([...cam.cr, 0], 8);
  f.set([...cam.cu, 0], 12);
  f.set([...sc.sun, Math.tan(sc.sunRadiusDeg * deg)], 16);
  f.set([...sc.sunE, sc.exposure], 20);
  f.set([...sc.skyZ, sc.aureole], 24);
  f.set([...sc.skyH, sc.below], 28);
  f.set([...sc.fog, sc.fogDensity], 32);
  const half = cfg.skyHalf;
  u.set([W, H, half ? W / 2 : W, half ? H / 2 : H], 36);
  u.set([S.frame, cfg.H, o.R ?? cfg.R, cfg.K], 40);
  f.set([sc.L[cfg.L], sc.sunMax, sc.probeMax, 0], 44);
  u.set([o.stats ? 1 : 0, o.view ?? 0, o.reset ? 1 : 0, o.half ?? 0], 48);
  const rot = randomRotation(mulberry(S.frame * 7919 + 17));
  f.set([...rot[0], 0], 52);
  f.set([...rot[1], 0], 56);
  f.set([...rot[2], 0], 60);
  f.set([...g.origin, o.alpha ?? 1], 64);
  f.set([g.s, g.s, g.s, 4 * g.s], 68);
  u.set([...g.n, g.count], 72);
  f.set([...cam.eye, 0], 76);   // previous camera: the camera is static while measuring
  f.set([...cam.cf, 0], 80);
  f.set([...cam.cr, 0], 84);
  f.set([...cam.cu, 0], 88);
  f.set([...sc.vol0.origin, sc.vol0.voxel], 92);
  u.set([...sc.vol0.dims, 0], 96);
  f.set([...sc.vol1.origin, sc.vol1.voxel], 100);
  u.set([...sc.vol1.dims, 0], 104);
  f.set([...sc.hm.origin, sc.hm.texel, 0], 108);
  u.set([...sc.hm.dims, 0, 0], 112);
  u.set([o.refPair ?? 0, o.refSeed ?? 0, (o.refStride ?? 0) << 16, o.tile ?? 0], 116);
  const v0 = sc.vol0.voxel;
  f.set([0.2, 0.3, cfg.sunNear * v0, cfg.skyNear * v0], 120);
  u.set([cfg.sunHalf ? 2 : 1, cfg.gatherHalf ? 2 : 1, 0, 0], 124);
  device.queue.writeBuffer(B.frame, 0, buf);
}

async function readStats() {
  const s = new Uint32Array(await readback([{ src: B.stats, size: 256 }]));
  return s;
}

function clearStats() {
  const enc = device.createCommandEncoder();
  enc.clearBuffer(B.stats);
  device.queue.submit([enc.finish()]);
}

function computePass(enc, pipe, bg, x, y = 1, z = 1, ts) {
  const p = enc.beginComputePass(ts ? { timestampWrites: ts } : {});
  p.setPipeline(pipe);
  p.setBindGroup(0, BG.g0);
  p.setBindGroup(1, bg);
  p.dispatchWorkgroups(x, y, z);
  p.end();
}

/** One pass, one submission: no command buffer holds more than one pass (GPU safety rule). */
function submitPass(pipe, bg, x, y = 1, z = 1, ts) {
  const enc = device.createCommandEncoder();
  computePass(enc, pipe, bg, x, y, z, ts);
  device.queue.submit([enc.finish()]);
}

// ---- configuring a scene and a configuration --------------------------------------------------------------------

async function setupScene(sc) {
  S.sc = sc;
  S.cfg = { ...BASE };
  S.dims = FULL;
  S.cam = camera(sc, ...FULL);
  S.grid = probeGrid(sc, 'base');
  S.probeKey = '';
  const out = {};
  {
    writeFrame();
    const qs = device.createQuerySet({ type: 'timestamp', count: 2 });
    const enc = device.createCommandEncoder();
    computePass(enc, P[sc.id].cook_height, BG.cookHeight, Math.ceil(sc.hm.dims[0] / 8), Math.ceil(sc.hm.dims[1] / 8), 1, { querySet: qs, beginningOfPassWriteIndex: 0, endOfPassWriteIndex: 1 });
    device.queue.submit([enc.finish()]);
    const ts = await resolveTimestamps(qs, 2);
    out.cook_heightmap = { texel_m: sc.hm.texel, dims: sc.hm.dims, extent_m: sc.hm.dims.map(x => x * sc.hm.texel), mb: r3(sc.hm.dims[0] * sc.hm.dims[1] * 16 / 2 ** 20), gpu_ms: r3(Number(ts[1] - ts[0]) / 1e6) };
  }
  // The shadow-height map for this sun, in bands of rows (one submission each). Re-cooked when the
  // sun moves; a slow golden-hour sun could refresh it a band at a time.
  {
    const band = 128, n = Math.ceil(sc.hm.dims[1] / band);
    const qs = device.createQuerySet({ type: 'timestamp', count: 2 * n });
    for (let k = 0; k < n; k++) {
      writeFrame({ tile: (k * band) << 16 });
      submitPass(P[sc.id].cook_shadow, BG.cookShadow, Math.ceil(sc.hm.dims[0] / 8), band / 8, 1, { querySet: qs, beginningOfPassWriteIndex: 2 * k, endOfPassWriteIndex: 2 * k + 1 });
      await done();
    }
    const ts = await resolveTimestamps(qs, 2 * n);
    let total = 0, worst = 0;
    for (let k = 0; k < n; k++) { const d = Number(ts[2 * k + 1] - ts[2 * k]) / 1e6; total += d; worst = Math.max(worst, d); }
    out.cook_shadow_map = { dims: sc.hm.dims, bands: n, gpu_ms: r3(total), worst_band_ms: r3(worst), mb: r3(sc.hm.dims[0] * sc.hm.dims[1] * 8 / 2 ** 20) };
  }
  // Cook both levels of the distance clipmap, timed. A static world cooks once; a moving camera
  // re-cooks only the slabs it scrolls into (not measured here).
  for (const [level, vol] of [[0, sc.vol0], [1, sc.vol1]]) {
    writeFrame({ view: level });
    const qs = device.createQuerySet({ type: 'timestamp', count: 2 });
    const enc = device.createCommandEncoder();
    const d = vol.dims;
    computePass(enc, P[sc.id].cook, BG.cook[level], Math.ceil(d[0] / 4), Math.ceil(d[1] / 4), Math.ceil(d[2] / 4), { querySet: qs, beginningOfPassWriteIndex: 0, endOfPassWriteIndex: 1 });
    const t0 = performance.now();
    device.queue.submit([enc.finish()]);
    await done();
    const ts = await resolveTimestamps(qs, 2);
    const bytes = VOLFMT === 'r16float' ? 2 : 8;
    out[`cook_level${level}`] = { voxel_m: vol.voxel, dims: d, extent_m: d.map(x => r3(x * vol.voxel)), voxels: d[0] * d[1] * d[2], mb: r3(d[0] * d[1] * d[2] * bytes / 2 ** 20), gpu_ms: r3(Number(ts[1] - ts[0]) / 1e6), wall_ms: r3(performance.now() - t0) };
  }
  out.canary_ms = await canary();
  return out;
}

/** GPU time of a fixed arithmetic kernel: a gauge of how much other work shares the GPU. */
async function canary() {
  const qs = device.createQuerySet({ type: 'timestamp', count: 6 });
  const enc = device.createCommandEncoder();
  for (let i = 0; i < 3; i++) computePass(enc, P.canary, BG.canary, 4096, 1, 1, { querySet: qs, beginningOfPassWriteIndex: 2 * i, endOfPassWriteIndex: 2 * i + 1 });
  device.queue.submit([enc.finish()]);
  const ts = await resolveTimestamps(qs, 6);
  return r3(median([0, 1, 2].map(i => Number(ts[2 * i + 1] - ts[2 * i]) / 1e6)));
}

async function placeProbes() {
  const key = `${S.sc.name}/${S.cfg.spacing}/${S.cfg.vis}`;
  if (key === S.probeKey) return S.probeStats;
  S.grid = probeGrid(S.sc, S.cfg.spacing);
  clearStats();
  writeFrame({ stats: true, view: S.cfg.vis === 'none' ? 1 : 0 });
  const qs = device.createQuerySet({ type: 'timestamp', count: 2 });
  const enc = device.createCommandEncoder();
  computePass(enc, P[S.sc.id].relocate, BG.relocate, Math.ceil(S.grid.count / 64), 1, 1, { querySet: qs, beginningOfPassWriteIndex: 0, endOfPassWriteIndex: 1 });
  device.queue.submit([enc.finish()]);
  const ts = await resolveTimestamps(qs, 2);
  const s = await readStats();
  S.probeKey = key;
  S.probeStats = { spacing_m: S.grid.s, grid: S.grid.n, probes: S.grid.count, active: s[20], off_in_solid: s[21], off_far_from_surfaces: s[22], placement_gpu_ms: r3(Number(ts[1] - ts[0]) / 1e6) };
  return S.probeStats;
}

async function configure(cfg, dims = FULL) {
  S.cfg = { ...BASE, ...cfg };
  S.dims = dims;
  S.cam = camera(S.sc, ...dims);
  const probes = await placeProbes();
  S.sinceReset = 0;
  if (!S.cfg.baked) S.bakedIdx = -1;
  return probes;
}

// ---- the frame ---------------------------------------------------------------------------------------------------

/** The primary pass as two submissions (top and bottom halves of the image): the forest's and the
 *  landscape's analytic fields cost tens of milliseconds per frame, and no submission may run long. */
function primaryHalves(ts) {
  const [W, H] = S.dims, half = Math.ceil(H / 16);
  for (let k = 0; k < 2; k++) {
    writeFrame({ ...S.frameOpts, tile: (k * half * 8) << 16 });
    submitPass(P[S.sc.id].primary, BG.primary, Math.ceil(W / 8), half, 1, ts[k]);
  }
}

function compKey(cfg, inline) {
  if (cfg.skySrc === 'probes_ao') return 'comp_ao';
  return `comp_${cfg.skySrc === 'cones' ? 'cones' : 'probes'}${inline ? '_g' : ''}`;
}

/** Submits one frame, one pass per submission. */
function encodeFrame(qs, q0) {
  const cfg = S.cfg, id = S.sc.id, [W, H] = S.dims;
  const sw = cfg.skyHalf ? W / 2 : W, sh = cfg.skyHalf ? H / 2 : H;
  const ts = i => qs ? { querySet: qs, beginningOfPassWriteIndex: q0 + 2 * i, endOfPassWriteIndex: q0 + 2 * i + 1 } : null;
  const gx = Math.ceil(W / 8), gy = Math.ceil(H / 8);
  if (!S.skipPrimary) primaryHalves(qs ? [ts(0), { querySet: qs, beginningOfPassWriteIndex: q0 + 18, endOfPassWriteIndex: q0 + 19 }] : [null, null]);
  writeFrame(S.frameOpts);
  // The sun pass in two bands of rows (two submissions): the analytic variants are heavy.
  const ssc = cfg.sunHalf ? 2 : 1, sunPipe = P[id][`sun_${FIELDS[cfg.sunField]}${cfg.sunTerrain === 'march' && cfg.sunField === 'hybrid' ? '_march' : ''}`];
  const sunRows = Math.ceil(H / ssc / 16);
  for (let k = 0; k < 2; k++) {
    writeFrame({ ...S.frameOpts, tile: (k * sunRows * 8) << 16 });
    submitPass(sunPipe, BG.sun, Math.ceil(W / ssc / 8), sunRows, 1, k === 0 ? ts(1) : (qs ? { querySet: qs, beginningOfPassWriteIndex: q0 + 20, endOfPassWriteIndex: q0 + 21 } : null));
  }
  writeFrame(S.frameOpts);
  if (cfg.skySrc !== 'probes') {
    const skyPipe = cfg.skySrc === 'probes_ao' ? `sky_ao_${Math.max(1, FIELDS[cfg.field])}` : `sky_${FIELDS[cfg.field]}`;
    submitPass(P[id][skyPipe], BG.sky, Math.ceil(sw / 8), Math.ceil(sh / 8), 1, ts(2));
    submitPass(P.temporal, BG.temporal[S.sp], Math.ceil(sw / 8), Math.ceil(sh / 8), 1, ts(3));
  }
  let latest = S.bakedIdx;
  if (!cfg.baked) {
    const tr = `${cfg.vis === 'none' ? 'ptrace_naive' : 'ptrace'}_${FIELDS[cfg.field]}`;
    submitPass(P[id][tr], BG.ptrace[S.pp], Math.ceil(S.grid.count * cfg.R / 64), 1, 1, ts(4));
    const rows = Math.ceil(S.grid.count / TPR);
    submitPass(P.pirr, BG.pirr[S.pp], TPR, rows, 1, ts(5));
    submitPass(P.pdepth, BG.pdepth[S.pp], TPR, rows, 1, ts(6));
    latest = 1 - S.pp;
  }
  const inline = cfg.inline && cfg.vis !== 'none';
  const gsc = cfg.gatherHalf ? 2 : 1;
  if (!inline) submitPass(P[cfg.vis === 'none' ? 'gather_none' : 'gather_ddgi'], BG.gather[latest], Math.ceil(W / gsc / 8), Math.ceil(H / gsc / 8), 1, ts(7));
  submitPass(P[compKey(cfg, inline)], BG.compositeG[S.sp][latest], gx, gy, 1, ts(8));
  S.lastSp = S.sp;
  S.latest = latest;
  if (cfg.skySrc !== 'probes') S.sp ^= 1;
  if (!cfg.baked) S.pp ^= 1;
}

function submitFrame(qs, q0, extra = {}) {
  const reset = S.sinceReset === 0;
  const alpha = reset ? 1 : Math.max(1 / S.cfg.H, 1 / (S.sinceReset + 1));
  S.frameOpts = { reset, alpha, ...extra };
  encodeFrame(qs, q0);
  S.frame++;
  S.sinceReset++;
}

/** Re-runs only the composite (for component views) on the last frame's lighting. */
async function recomposite(view) {
  writeFrame({ view, alpha: 1 });
  const [W, H] = S.dims, cfg = S.cfg;
  const inline = cfg.inline && cfg.vis !== 'none';
  submitPass(P[compKey(cfg, inline)], BG.compositeG[S.lastSp][S.latest], Math.ceil(W / 8), Math.ceil(H / 8));
  await done();
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

function ranPasses(cfg) {
  return PASSES.filter(p => !((p === 'sky' || p === 'temporal') && cfg.skySrc === 'probes') && !((p === 'ptrace' || p === 'pirr' || p === 'pdepth') && cfg.baked)
    && !(p === 'gather' && cfg.inline && cfg.vis !== 'none'));
}

/** GPU time per pass under sustained load: `warm` untimed frames, then `frames` back to back. */
async function measure(frames = 90, warm = 30) {
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  for (let f = 0; f < warm; f++) { submitFrame(null, 0); if (f % 24 === 23) await done(); }
  await done();
  const t1 = performance.now();
  for (let f = 0; f < frames; f++) { submitFrame(qs, f * Q); if (f % 24 === 23) await done(); }
  await done();
  const throughput = (performance.now() - t1) / frames;
  const ts = await resolveTimestamps(qs, frames * Q);
  const ran = ranPasses(S.cfg).filter(p => p !== 'primary' || !S.skipPrimary);
  const d = (f, p) => {
    const i = PASSES.indexOf(p);
    const a = Number(ts[f * Q + 2 * i + 1] - ts[f * Q + 2 * i]) / 1e6;
    if (p === 'primary') return a + Number(ts[f * Q + 19] - ts[f * Q + 18]) / 1e6;
    if (p === 'sun') return a + Number(ts[f * Q + 21] - ts[f * Q + 20]) / 1e6;
    return a;
  };
  const all = [...Array(frames).keys()];
  const out = { passes_ms: {}, passes_p95_ms: {}, passes_min_ms: {} };
  for (const p of PASSES) {
    out.passes_ms[p] = ran.includes(p) ? r3(median(all.map(f => d(f, p)))) : 0;
    out.passes_p95_ms[p] = ran.includes(p) ? r3(pct(all.map(f => d(f, p)), 0.95)) : 0;
    out.passes_min_ms[p] = ran.includes(p) ? r3(Math.min(...all.map(f => d(f, p)))) : 0;
  }
  const lighting = all.map(f => LIGHTING.filter(p => ran.includes(p)).reduce((s, p) => s + d(f, p), 0));
  const first = S.skipPrimary ? 2 : 0;
  const frame = all.map(f => Number(ts[f * Q + 2 * 8 + 1] - ts[f * Q + first]) / 1e6);
  out.lighting_ms = r3(median(lighting));
  out.lighting_min_ms = r3(Math.min(...lighting));
  out.lighting_sum_of_pass_minimums_ms = r3(LIGHTING.filter(p => ran.includes(p)).reduce((s2, p) => s2 + out.passes_min_ms[p], 0));
  out.lighting_p95_ms = r3(pct(lighting, 0.95));
  out.frame_gpu_ms = r3(median(frame));
  out.frame_gpu_p95_ms = r3(pct(frame, 0.95));
  out.throughput_ms = r3(throughput);
  out.primary_included = !S.skipPrimary;
  out.frames = frames;
  out.warmup = warm;
  out.verdict_vs_slice = out.lighting_ms <= BUDGET_MS ? 'within 3.5 ms' : out.lighting_ms <= 2 * BUDGET_MS ? '3.5–7 ms' : 'over 7 ms';
  return out;
}

/** 60 Hz pacing with a busy-wait, as in spikes 01 and 02. */
async function paced(frames = 180) {
  const period = 1000 / 60;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  const t0 = performance.now() + 20;
  for (let f = 0; f < frames; f++) {
    const deadline = t0 + f * period;
    while (performance.now() < deadline) { /* spin */ }
    submitFrame(qs, f * Q);
  }
  await done();
  const ts = await resolveTimestamps(qs, frames * Q);
  const keep = [...Array(frames).keys()].slice(30);
  const ran = ranPasses(S.cfg);
  const first = S.skipPrimary ? 2 : 0;
  const gpu = keep.map(f => Number(ts[f * Q + 2 * 8 + 1] - ts[f * Q + first]) / 1e6);
  const lighting = keep.map(f => LIGHTING.filter(p => ran.includes(p)).reduce((s, p) => { const i = PASSES.indexOf(p); return s + Number(ts[f * Q + 2 * i + 1] - ts[f * Q + 2 * i]) / 1e6; }, 0) + Number(ts[f * Q + 21] - ts[f * Q + 20]) / 1e6);
  const sorted = [...gpu].sort((a, b) => a - b);
  return {
    pacing: 'busy-wait at 60 Hz', frames: keep.length, primary_included: !S.skipPrimary,
    frame_gpu_median: r3(median(gpu)), frame_gpu_p95: r3(pct(gpu, 0.95)), frame_gpu_max: r3(sorted[sorted.length - 1]),
    frame_gpu_over_16_7: gpu.filter(g => g > 16.7).length,
    lighting_median: r3(median(lighting)), lighting_p95: r3(pct(lighting, 0.95)),
  };
}

/** One frame with counters on: rays, steps and step-cap hits per kind of ray. */
async function rayStats() {
  clearStats();
  submitFrame(null, 0, { stats: true });
  const s = await readStats();
  const [W, H] = S.dims;
  const per = (a, b) => (s[b] ? r3(s[a] / s[b]) : 0);
  return {
    primary: { px: s[0], hits: s[3], steps_per_px: per(1, 0), cap_hits: s[2], cap_share: r5(s[2] / Math.max(s[0], 1)) },
    sun: { rays: s[4], steps_per_ray: per(5, 4), cap_hits: s[6], cap_share: r5(s[6] / Math.max(s[4], 1)) },
    sky: { cones: s[7], steps_per_cone: per(8, 7), cap_hits: s[9], cap_share: r5(s[9] / Math.max(s[7], 1)) },
    probe_rays: { rays: s[10], hits: s[13], backface: s[23], steps_per_ray: per(11, 10), cap_hits: s[12], cap_share: r5(s[12] / Math.max(s[10], 1)) },
    probe_shadow: { rays: s[14], steps_per_ray: per(15, 14), cap_hits: s[16], cap_share: r5(s[16] / Math.max(s[14], 1)) },
    pixels: W * H,
  };
}

// ---- the fallback: probes baked once at load --------------------------------------------------------------------

async function bake(frames = 48, rays = 64) {
  const cfg = S.cfg;
  S.cfg = { ...cfg, baked: false, R: rays, H: 1e9 };
  S.sinceReset = 0;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * 2 });
  const t0 = performance.now();
  for (let f = 0; f < frames; f++) {
    const reset = f === 0;
    writeFrame({ reset, alpha: reset ? 1 : 1 / (f + 1), R: rays });
    const rows = Math.ceil(S.grid.count / TPR);
    submitPass(P[S.sc.id].ptrace_0, BG.ptrace[S.pp], Math.ceil(S.grid.count * rays / 64), 1, 1, { querySet: qs, beginningOfPassWriteIndex: 2 * f, endOfPassWriteIndex: 2 * f + 1 });
    submitPass(P.pirr, BG.pirr[S.pp], TPR, rows);
    submitPass(P.pdepth, BG.pdepth[S.pp], TPR, rows, 1);
    S.pp ^= 1;
    S.frame++;
    if (f % 4 === 3) await done();
  }
  await done();
  const wall = performance.now() - t0;
  const ts = await resolveTimestamps(qs, frames * 2);
  let trace = 0;
  for (let f = 0; f < frames; f++) trace += Number(ts[2 * f + 1] - ts[2 * f]) / 1e6;
  S.bakedIdx = S.pp;   // the atlas written last
  S.cfg = { ...cfg };
  S.sinceReset = 0;
  return { frames, rays_per_probe_per_frame: rays, rays_per_probe: frames * rays, probe_trace_gpu_ms_total: r3(trace), wall_ms: r3(wall) };
}

// ---- reference --------------------------------------------------------------------------------------------------------

async function primaryOnly() {
  S.frameOpts = { reset: true };
  primaryHalves([null, null]);
  await done();
}

/** Progressive, tiled reference in two phases (see ref.wgsl): the sun at every pixel, then sky and
 *  bounce on a grid of one pixel in stride². Each submission adds one sample pair to every pixel of
 *  one tile and is awaited before the next, so no submission runs long (GPU safety rule). The tile
 *  size adapts between passes toward TILE_TARGET_MS. Each phase runs passes until its share of
 *  `budgetMs` is spent (at least `minPairs`, at most `maxPairs`). */
const TILE_TARGET_MS = 40, TILE_SIZES = [32, 64, 128, 256, 512];
const SUN_SHARE = 0.3;
const refStride = dims => (dims[0] === 1920 ? 4 : 2);

async function refPhase(key, gridW, gridH, stride, budgetMs, minPairs, maxPairs) {
  const t0 = performance.now();
  let pairs = 0, submits = 0, maxAll = 0, stats = null, ti = 1;
  while (pairs < maxPairs && (pairs < minPairs || performance.now() - t0 < budgetMs)) {
    const ts = TILE_SIZES[ti];
    let maxd = 0;
    for (let y0 = 0; y0 < gridH; y0 += ts) {
      for (let x0 = 0; x0 < gridW; x0 += ts) {
        const first = submits === 0;
        if (first) clearStats();
        writeFrame({ refPair: pairs, refSeed: 1234 + 97 * S.sc.id, refStride: stride, tile: (y0 << 16) | x0, stats: first });
        const td = performance.now();
        submitPass(P[S.sc.id][key], BG.ref, Math.ceil(Math.min(ts, gridW - x0) / 8), Math.ceil(Math.min(ts, gridH - y0) / 8));
        await done();
        maxd = Math.max(maxd, performance.now() - td);
        submits++;
        if (first) {
          const st = await readStats();
          stats = { rays_first_tile: st[17], steps_per_ray: r3(st[18] * 16 / Math.max(st[17], 1)), cap_hits_first_tile: st[19], cap_share_first_tile: r5(st[19] / Math.max(st[17], 1)) };
        }
      }
    }
    pairs++;
    maxAll = Math.max(maxAll, maxd);
    if (maxd > TILE_TARGET_MS && ti > 0) ti--;
    else if (maxd < TILE_TARGET_MS / 3 && ti < TILE_SIZES.length - 1) ti++;
  }
  return { pairs, spp: 2 * pairs, submissions: submits, final_tile: TILE_SIZES[ti], max_submission_wall_ms: r3(maxAll), wall_ms: r3(performance.now() - t0), ...stats };
}

async function reference(budgetMs, minPairs, maxPairs) {
  await primaryOnly();
  const enc = device.createCommandEncoder();
  for (const b of B.acc) enc.clearBuffer(b);
  device.queue.submit([enc.finish()]);
  await done();
  const [W, H] = S.dims, stride = refStride(S.dims);
  const sun = await refPhase('ref_sun', W, H, stride, budgetMs * SUN_SHARE, Math.min(minPairs, 4), Math.min(64, maxPairs));
  const gw = Math.ceil((W - stride / 4) / stride), gh = Math.ceil((H - stride / 4) / stride);
  const ind = await refPhase('ref_ind', gw, gh, stride, budgetMs * (1 - SUN_SHARE), minPairs, maxPairs);
  S.refSunPairs = sun.pairs;
  S.refStride = stride;
  return {
    budget_ms: budgetMs, grid_stride: stride, grid_pixels: gw * gh,
    sun: { ...sun, note: 'every pixel; spp counts disc samples (both halves)' },
    indirect: { ...ind, note: `sky + bounce on 1 pixel in ${stride * stride}; spp per grid pixel (both halves)` },
  };
}

async function resolveRef(view = 0, half = 0) {
  writeFrame({ view, half, refPair: S.refSunPairs, refStride: S.refStride });
  const [W, H] = S.dims;
  submitPass(P.ref_resolve, BG.resolve, Math.ceil(W / 8), Math.ceil(H / 8));
  await done();
}

// ---- images ---------------------------------------------------------------------------------------------------------------

const BYTE = new Uint8Array(65536);
for (let h = 0; h < 65536; h++) {
  const s = h >> 15, e = (h >> 10) & 31, m = h & 1023;
  let v = e === 0 ? m * 2 ** -24 : e === 31 ? 1 : (1 + m / 1024) * 2 ** (e - 15);
  if (s) v = 0;
  BYTE[h] = Math.round(255 * Math.pow(Math.min(Math.max(v, 0), 1), 1 / 2.2));
}

/** The output texture as 8-bit display values (gamma 2.2), RGB. */
async function grab() {
  const [W, H] = S.dims;
  const bpr = Math.ceil(W * 8 / 256) * 256;
  const out = device.createBuffer({ size: bpr * H, usage: U.COPY_DST | U.MAP_READ });
  const enc = device.createCommandEncoder();
  enc.copyTextureToBuffer({ texture: T.out }, { buffer: out, bytesPerRow: bpr, rowsPerImage: H }, [W, H]);
  device.queue.submit([enc.finish()]);
  await out.mapAsync(GPUMapMode.READ);
  const half = new Uint16Array(out.getMappedRange().slice(0));
  out.unmap();
  out.destroy();
  const px = new Uint8Array(W * H * 3);
  const row = bpr / 2;
  for (let y = 0, j = 0; y < H; y++) for (let x = 0; x < W; x++) { const i = y * row + x * 4; px[j++] = BYTE[half[i]]; px[j++] = BYTE[half[i + 1]]; px[j++] = BYTE[half[i + 2]]; }
  return px;
}

/** The G-buffer's ray distances (−1 for sky). */
async function grabDepth() {
  const [W, H] = S.dims;
  const bpr = Math.ceil(W * 4 / 256) * 256;
  const out = device.createBuffer({ size: bpr * H, usage: U.COPY_DST | U.MAP_READ });
  const enc = device.createCommandEncoder();
  enc.copyTextureToBuffer({ texture: T.gt }, { buffer: out, bytesPerRow: bpr, rowsPerImage: H }, [W, H]);
  device.queue.submit([enc.finish()]);
  await out.mapAsync(GPUMapMode.READ);
  const raw = new Float32Array(out.getMappedRange().slice(0));
  out.unmap();
  out.destroy();
  const t = new Float32Array(W * H);
  for (let y = 0; y < H; y++) t.set(raw.subarray(y * bpr / 4, y * bpr / 4 + W), y * W);
  return t;
}

function diff(a, b, mask) {
  let sum = 0, over8 = 0, n = 0, max = 0;
  for (let i = 0, k = 0; i < a.length; i += 3, k++) {
    if (mask && !mask[k]) continue;
    let m = 0;
    for (let c = 0; c < 3; c++) { const d = Math.abs(a[i + c] - b[i + c]); sum += d; if (d > m) m = d; }
    if (m > 8) over8++;
    if (m > max) max = m;
    n++;
  }
  return { mean_abs_255: r3(sum / Math.max(3 * n, 1)), share_over_8: r5(over8 / Math.max(n, 1)), max, pixels: n };
}

/** The same comparison on k×k block averages: low-frequency lighting error, with the reference's
 *  per-pixel noise averaged down (k² samples per block). */
function blockDiff(a, b, k) {
  const [W, H] = S.dims, bw = Math.floor(W / k), bh = Math.floor(H / k);
  let sum = 0, over8 = 0, max = 0;
  for (let by = 0; by < bh; by++) for (let bx = 0; bx < bw; bx++) {
    let m = 0;
    for (let c = 0; c < 3; c++) {
      let sa = 0, sb = 0;
      for (let y = by * k; y < by * k + k; y++) for (let x = bx * k; x < bx * k + k; x++) { const i = (y * W + x) * 3 + c; sa += a[i]; sb += b[i]; }
      const d = Math.abs(sa - sb) / (k * k);
      sum += d;
      if (d > m) m = d;
    }
    if (m > 8) over8++;
    if (m > max) max = m;
  }
  const n = bw * bh;
  return { block: k, mean_abs_255: r3(sum / (3 * n)), share_over_8: r5(over8 / n), max: r3(max), blocks: n };
}
const blockOf = dims => (dims[0] === 1920 ? 4 : 2);

/** Pixels of the reference's indirect grid: where the reference is a direct estimate, not a
 *  reconstruction. The image metrics use only these. */
function gridMask(dims) {
  const [W, H] = dims, s = refStride(dims), o = s >> 2;
  const m = new Uint8Array(W * H);
  for (let y = o; y < H; y += s) for (let x = o; x < W; x += s) m[y * W + x] = 1;
  return m;
}

/** RMS error against the truth, estimated without the reference's noise: with A and B independent
 *  unbiased estimates, E[(x − A)(x − B)] = (x − truth)², so its mean over pixels is unbiased. */
function rmseUnbiased(x, a, b, mask) {
  let s = 0, n = 0;
  for (let i = 0, k = 0; i < x.length; i += 3, k++) {
    if (mask && !mask[k]) continue;
    for (let c = 0; c < 3; c++) s += (x[i + c] - a[i + c]) * (x[i + c] - b[i + c]);
    n += 3;
  }
  return r3(Math.sqrt(Math.max(s / Math.max(n, 1), 0)));
}

const LIN = new Float32Array(256).map((_, i) => Math.pow(i / 255, 2.2));
function meanLum(a, mask) {
  let s = 0, n = 0;
  for (let i = 0, k = 0; i < a.length; i += 3, k++) {
    if (!mask[k]) continue;
    s += 0.2126 * LIN[a[i]] + 0.7152 * LIN[a[i + 1]] + 0.0722 * LIN[a[i + 2]];
    n++;
  }
  return n ? s / n : 0;
}

async function present() {
  const [W, H] = S.dims;
  const canvas = $('view');
  if (canvas.width !== W) { canvas.width = W; canvas.height = H; }
  device.queue.writeBuffer(B.blitScale, 0, new Float32Array([W / FULL[0], H / FULL[1], 0, 0]));
  const enc = device.createCommandEncoder();
  const p = enc.beginRenderPass({ colorAttachments: [{ view: ctx.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
  p.setPipeline(P.blit);
  p.setBindGroup(0, BG.blit);
  p.draw(3);
  p.end();
  device.queue.submit([enc.finish()]);
  await done();
}

async function snapshot(name) {
  await present();
  const blob = await new Promise(r => $('view').toBlob(r, 'image/png'));
  if (blob) await save(`${name}.png`, blob); else fail('snapshot failed', name);
}

async function savePixels(name, draw) {
  const [W, H] = S.dims;
  const c = document.createElement('canvas');
  c.width = W;
  c.height = H;
  const g = c.getContext('2d');
  const img = g.createImageData(W, H);
  draw(img.data, W, H);
  g.putImageData(img, 0, 0);
  return { canvas: c, g, done: async () => { const blob = await new Promise(r => c.toBlob(r, 'image/png')); await save(`${name}.png`, blob); } };
}

/** Left half real-time, right half reference, labelled. */
async function saveCompare(name, rt, ref) {
  const s = await savePixels(name, (d, W, H) => {
    for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
      const src = x < W / 2 ? rt : ref, i = (y * W + x) * 3, j = (y * W + x) * 4;
      const line = Math.abs(x - W / 2) < Math.max(1, W / 960);
      d[j] = line ? 255 : src[i]; d[j + 1] = line ? 255 : src[i + 1]; d[j + 2] = line ? 255 : src[i + 2]; d[j + 3] = 255;
    }
  });
  const fs = Math.round(S.dims[1] / 36);
  s.g.font = `${fs}px system-ui, sans-serif`;
  s.g.fillStyle = 'rgba(0,0,0,0.55)';
  s.g.fillRect(8, 8, fs * 6.2, fs * 1.5);
  s.g.fillRect(S.dims[0] / 2 + 8, 8, fs * 9.5, fs * 1.5);
  s.g.fillStyle = '#fff';
  s.g.fillText('real-time', 16, 8 + fs * 1.1);
  s.g.fillText('path-traced reference', S.dims[0] / 2 + 16, 8 + fs * 1.1);
  await s.done();
}

/** Red where any channel differs by more than 8/255, yellow over 2/255, over a dimmed image. */
async function saveDiff(name, a, b) {
  const s = await savePixels(name, (d, W, H) => {
    for (let k = 0, i = 0, j = 0; k < W * H; k++, i += 3, j += 4) {
      let m = 0;
      for (let c = 0; c < 3; c++) m = Math.max(m, Math.abs(a[i + c] - b[i + c]));
      const base = (b[i] + b[i + 1] + b[i + 2]) / 9;
      d[j] = m > 8 ? 255 : m > 2 ? 220 : base;
      d[j + 1] = m > 8 ? 30 : m > 2 ? 190 : base;
      d[j + 2] = m > 8 ? 30 : m > 2 ? 40 : base;
      d[j + 3] = 255;
    }
  });
  await s.done();
}

/** Tower only: interior pixels (inside the walls, below the ceiling) that the sun doesn't reach. */
function interiorShadowMask(depth, refSun, grid) {
  const [W, H] = S.dims, cam = S.cam;
  const mask = new Uint8Array(W * H);
  let n = 0;
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
    const k = y * W + x, t = depth[k];
    if (!(t > 0) || !grid[k]) continue;
    const u = (x + 0.5) / W * 2 - 1, v = 1 - (y + 0.5) / H * 2;
    const d = norm([0, 1, 2].map(i => cam.cf[i] + cam.cr[i] * u + cam.cu[i] * v));
    const p = [0, 1, 2].map(i => cam.eye[i] + d[i] * t);
    if (Math.hypot(p[0], p[2]) > 3.68 || p[1] > 4.95 || p[1] < -0.05) continue;
    const i = k * 3;
    if (refSun[i] + refSun[i + 1] + refSun[i + 2] > 3) continue;
    mask[k] = 1;
    n++;
  }
  return { mask, pixels: n };
}

// ---- per-scene run -----------------------------------------------------------------------------------------------------

const resName = dims => (dims[0] === 1920 ? '1080p' : '540p');

async function runReference(sc, dims, budgetMs, out, snap, minPairs = 16, maxPairs = 2048) {
  await configure({}, dims);
  const info = await reference(budgetMs, minPairs, maxPairs);
  const ref = { grid: gridMask(dims) };
  const tag = `${sc.name}-${resName(dims)}`;
  await resolveRef(0, 0); ref.final = await grab(); if (snap) await snapshot(`${tag}-ref`);
  await resolveRef(0, 1); ref.finalA = await grab();
  await resolveRef(0, 2); ref.finalB = await grab();
  await resolveRef(1, 1); const sa = await grab();
  await resolveRef(1, 2); const sb = await grab();
  info.noise_floor = {
    note: 'half A against half B (each half the samples); the full reference is about half as noisy as either half',
    grid: diff(ref.finalA, ref.finalB, ref.grid), sun_full: diff(sa, sb),
  };
  for (const [view, key] of [[1, 'sun'], [2, 'sky'], [3, 'bounce']]) {
    await resolveRef(view, 0);
    ref[key] = await grab();
    if (snap) await snapshot(`${tag}-ref-${key}`);
  }
  ref.depth = await grabDepth();
  out[`reference_${resName(dims)}`] = info;
  log(sc.name, resName(dims), 'reference', JSON.stringify(info));
  return ref;
}

async function runConfig(sc, name, cfg, dims, ref, out, opts = {}) {
  const key = `${resName(dims)}/${name}`;
  const probes = await configure(cfg, dims);
  const rec = { config: { ...S.cfg }, probes: { ...probes } };
  if (S.cfg.baked) rec.bake = await bake();
  rec.rays_per_frame = S.cfg.baked ? 0 : probes.active * S.cfg.R;
  rec.canary_before_ms = await canary();
  rec.timing = await measure();
  rec.stats = await rayStats();
  const img = await grab();
  rec.quality = {
    grid: diff(img, ref.final, ref.grid),
    rmse_unbiased_255: rmseUnbiased(img, ref.finalA, ref.finalB, ref.grid),
  };
  await recomposite(1);
  rec.quality.sun_full = diff(await grab(), ref.sun);
  await recomposite(0);
  if (sc.id === 1) {
    const m = interiorShadowMask(ref.depth, ref.sun, ref.grid);
    const lr = meanLum(img, m.mask), lf = meanLum(ref.final, m.mask);
    rec.leak = { interior_unlit_pixels: m.pixels, mean_lum_realtime: r5(lr), mean_lum_reference: r5(lf), ratio: r3(lr / Math.max(lf, 1e-9)) };
  }
  const tag = `${sc.name}-${resName(dims)}-${name}`;
  if (opts.snap || SNAP_CONFIGS.has(name)) await snapshot(tag);
  if (COMPARE_CONFIGS.has(name) && dims === FULL) await saveCompare(`${tag}-compare`, img, ref.final);
  rec.meets_bar = meetsBar(rec, sc.id);
  if (opts.components) {
    rec.components = {};
    for (const [view, k] of [[1, 'sun'], [2, 'sky'], [3, 'bounce']]) {
      await recomposite(view);
      const c = await grab();
      rec.components[k] = k === 'sun' ? diff(c, ref[k]) : diff(c, ref[k], ref.grid);
      await snapshot(`${tag}-${k}`);
      if (k === 'bounce') await saveCompare(`${tag}-${k}-compare`, c, ref[k]);
    }
    await recomposite(0);
    await saveCompare(`${tag}-compare`, img, ref.final);
    await saveDiff(`${tag}-diff`, img, ref.final);
  }
  out[key] = rec;
  const q = rec.quality;
  log(`${sc.name} ${key}`, `lighting ${rec.timing.lighting_ms} ms (p95 ${rec.timing.lighting_p95_ms}), frame ${rec.timing.frame_gpu_ms}`,
    `| grid mean ${q.grid.mean_abs_255}/255, >8: ${(q.grid.share_over_8 * 100).toFixed(2)}%, rmse ${q.rmse_unbiased_255}`,
    `| sun >8: ${(q.sun_full.share_over_8 * 100).toFixed(2)}%`, rec.leak ? `| leak ratio ${rec.leak.ratio}` : '',
    '| passes', JSON.stringify(rec.timing.passes_ms));
  return rec;
}

function meetsBar(rec, sceneId) {
  const q = rec.quality.grid;
  const ok = q.mean_abs_255 <= 3 && q.share_over_8 <= 0.10;
  return sceneId === 1 ? ok && rec.leak && rec.leak.ratio <= 1.25 : ok;
}

/** Per scene: the cheapest 1080p configuration meeting the quality bar, separately for the technique
 *  under test (per-frame lighting) and for the fallback (probes baked at load, only the sun per frame). */
function verdict(sc, out) {
  const full = Object.entries(out.configs).filter(([k]) => k.startsWith('1080p/') && !k.endsWith('base_again'));
  const judge = list => {
    const passing = list.filter(([, r]) => meetsBar(r, sc.id)).sort((a, b) => a[1].timing.lighting_ms - b[1].timing.lighting_ms);
    const best = passing[0];
    return {
      configs_meeting_bar: passing.map(([k, r]) => `${k} (${r.timing.lighting_ms} ms)`),
      judged: best ? best[0] : null,
      lighting_ms: best ? best[1].timing.lighting_ms : null,
      verdict: !best ? 'fail on correctness: no swept configuration meets the quality bar'
        : best[1].timing.lighting_ms <= BUDGET_MS ? 'pass' : best[1].timing.lighting_ms <= 2 * BUDGET_MS ? 'inconclusive' : 'fail',
    };
  };
  return {
    quality_bar: 'mean |diff| ≤ 3/255 and ≤ 10% of pixels over 8/255, on the reference grid pixels' + (sc.id === 1 ? '; interior leak ratio ≤ 1.25' : ''),
    technique: judge(full.filter(([, r]) => !r.config.baked)),
    fallback: judge(full.filter(([, r]) => r.config.baked)),
    base_lighting_ms: out.configs['1080p/base'] ? out.configs['1080p/base'].timing.lighting_ms : null,
    caveat: 'indicative: other spikes were being built on the same machine; the GPU lock serialized runs, but clocks and thermals vary',
  };
}

async function runScene(sc, full) {
  const out = { camera: { eye: sc.eye.map(r3), target: sc.target.map(r3), fovy: sc.fovy }, sun: { dir: sc.sun.map(r3), radius_deg: sc.sunRadiusDeg, irradiance: sc.sunE }, configs: {} };
  R.scenes[sc.name] = out;
  await need(full ? ALL_SCENE(sc.id) : QUICK_SCENE(sc.id));
  out.setup = await setupScene(sc);
  log(sc.name, 'setup', JSON.stringify(out.setup));
  if (!full) {
    // #quick: one short look per scene, a two-pair reference, and the side-by-side. About 5 s a scene.
    const ref = await runReference(sc, FULL, 400, out, false, 1, 2);
    await configure({});
    out.configs['1080p/base'] = { timing: await measure(20, 10) };
    const img = await grab();
    out.configs['1080p/base'].quality = { grid: diff(img, ref.final, ref.grid), rmse_unbiased_255: rmseUnbiased(img, ref.finalA, ref.finalB, ref.grid), note: 'against a short reference: noisy' };
    await snapshot(`quick-${sc.name}`);
    await saveCompare(`quick-${sc.name}-compare`, img, ref.final);
    await configure(cfgOf('fast'));
    out.configs['1080p/fast'] = { timing: await measure(10, 5) };
    await snapshot(`quick-${sc.name}-fast`);
    log(sc.name, 'quick fast', JSON.stringify(out.configs['1080p/fast'].timing.passes_ms), 'lighting', out.configs['1080p/fast'].timing.lighting_ms);
    log(sc.name, 'quick', JSON.stringify(out.configs['1080p/base'].timing.passes_ms), 'lighting', out.configs['1080p/base'].timing.lighting_ms);
    return;
  }
  for (const [dims, budget, sweep] of [[FULL, sc.refBudget, SWEEP.filter(e => !e[2] || e[2].includes(sc.name))], [HALF, REF_BUDGET_HALF_MS, HALF_SWEEP]]) {
    const ref = await runReference(sc, dims, budget, out, true, 16, 2048);
    // The whole frame once, primary visibility included (30 + 90 frames). The camera and the world
    // are static, so the sweeps then reuse its G-buffer and time the lighting passes alone.
    await configure({}, dims);
    S.skipPrimary = false;
    out[`frame_${resName(dims)}`] = await measure(30, 10);
    out[`frame_${resName(dims)}`].primary_stats = (await rayStats()).primary;
    log(sc.name, resName(dims), 'whole frame', JSON.stringify(out[`frame_${resName(dims)}`]));
    S.skipPrimary = true;
    for (const [name, cfg] of sweep) {
      await runConfig(sc, name, cfg, dims, ref, out.configs, { components: name === 'base' && dims === FULL, snap: dims === HALF });
      await save('progress.json', JSON.stringify(R, null, 1));
    }
    if (dims === FULL) {
      // 60 Hz pacing for the configuration the verdict judges (the cheapest that meets the bar), or
      // the fast combination if none does.
      const v = verdict(sc, out);
      const name = v.technique.judged ? v.technique.judged.split('/')[1] : 'fast';
      const entry = SWEEP.find(e => e[0] === name) || ['fast', cfgOf('fast')];
      await configure(entry[1], FULL);
      if (S.cfg.baked) await bake();
      out.paced = { config: name, ...(await paced()) };
      log(sc.name, 'paced (lighting passes only)', JSON.stringify(out.paced));
    }
    S.skipPrimary = false;
  }
  out.verdict = verdict(sc, out);
  out.canary_end_ms = await canary();
  log(sc.name, 'verdict', JSON.stringify(out.verdict));
}

async function runAll(full) {
  $('run').disabled = true;
  try {
    await init();
    log('device', JSON.stringify(R.device));
    resources();
    await blitPipeline();
    await need(full ? ALL_SHARED() : QUICK_SHARED);
    for (const sc of SCENES) await runScene(sc, full);
    R.finished = new Date().toISOString();
    R.elapsed_s = r3((Date.parse(R.finished) - Date.parse(R.started)) / 1000);
    await save(`${full ? 'run' : 'quick'}-${R.started.replace(/[:.]/g, '-')}.json`, JSON.stringify(R, null, 2));
    log('done in', R.elapsed_s, 's');
  } catch (e) {
    fail('FAILED:', e.stack || e.message);
    R.error = String(e.stack || e.message);
  }
  await save('DONE', R.error ? 'failed' : 'ok');
  $('run').disabled = false;
}

/** Development: render chosen scenes with the base configuration (or a named sweep entry), save the
 *  image and its components, and optionally a short reference. #look=forest,tower&ref=8&cfg=K8 */
async function look(params) {
  try {
    await init();
    resources();
    await blitPipeline();
    await need(ALL_SHARED());
    const names = (params.get('look') || 'forest,tower,landscape').split(',');
    const refPairs = +(params.get('ref') || 0);
    const cfgName = params.get('cfg') || 'base';
    const cfg = (SWEEP.find(([n]) => n === cfgName) || ['base', {}])[1];
    for (const sc of SCENES.filter(s => names.includes(s.name))) {
      await need(ALL_SCENE(sc.id));
      const setup = await setupScene(sc);
      log(sc.name, 'setup', JSON.stringify(setup));
      let ref = null;
      if (refPairs) ref = await runReference(sc, FULL, 1e9, R.scenes[sc.name] = {}, true, refPairs, refPairs);
      if (ref) log(sc.name, 'reference', JSON.stringify(R.scenes[sc.name][`reference_1080p`]));
      const probes = await configure(cfg);
      if (S.cfg.baked) log('bake', JSON.stringify(await bake()));
      const t = await measure(40, 20);
      log(sc.name, cfgName, 'probes', JSON.stringify(probes), 'timing', JSON.stringify(t));
      log(sc.name, 'stats', JSON.stringify(await rayStats()));
      const img = await grab();
      await snapshot(`look-${sc.name}-${cfgName}`);
      if (params.has('probe')) {
        const depth = await grabDepth();
        const [W, H] = S.dims, cam = S.cam;
        for (const [x, y] of [[960, 1000], [960, 800], [400, 900], [1500, 900], [960, 700]]) {
          const t = depth[y * W + x], u = (x + 0.5) / W * 2 - 1, v = 1 - (y + 0.5) / H * 2;
          const d = norm([0, 1, 2].map(i => cam.cf[i] + cam.cr[i] * u + cam.cu[i] * v));
          log('pixel', x, y, 't', t.toFixed(4), 'p', JSON.stringify([0, 1, 2].map(i => +(cam.eye[i] + d[i] * t).toFixed(4))), 'dir', JSON.stringify(d.map(x => +x.toFixed(4))));
        }
      }
      const dbg = (params.get('debug') || '').split(',').filter(Boolean);
      for (const k of dbg) {
        const view = { albedo: 4, normal: 5, depth: 6, sunvis: 7, skyirr: 8, bounceirr: 9, ids: 10, height: 11 }[k];
        await recomposite(view);
        await snapshot(`look-${sc.name}-${cfgName}-dbg-${k}`);
      }
      for (const [view, k] of [[1, 'sun'], [2, 'sky'], [3, 'bounce']]) {
        await recomposite(view);
        await snapshot(`look-${sc.name}-${cfgName}-${k}`);
        if (ref) log(sc.name, k, JSON.stringify(diff(await grab(), ref[k], k === 'sun' ? null : ref.grid)));
      }
      if (ref) {
        log(sc.name, 'final', JSON.stringify({ grid: diff(img, ref.final, ref.grid), rmse: rmseUnbiased(img, ref.finalA, ref.finalB, ref.grid) }));
        await saveCompare(`look-${sc.name}-${cfgName}-compare`, img, ref.final);
      }
    }
  } catch (e) { fail('FAILED:', e.stack || e.message); }
  await save('DONE', 'ok');
}

$('run').addEventListener('click', () => runAll(true));
const hashParams = new URLSearchParams(location.hash.slice(1));
if (hashParams.has('look')) look(hashParams);
if (location.hash === '#run') runAll(true);
if (location.hash === '#quick') runAll(false);
