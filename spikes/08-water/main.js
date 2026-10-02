// Spike harness: places the world, builds the pipelines, renders the views, times every pass,
// sweeps the parameters, and compares each fast path with a brute-force reference.
// This file plays the engine's role plus the measuring. It isn't compiler output.

const FOVY = 40 * Math.PI / 180;
const U = GPUBufferUsage, TU = GPUTextureUsage;
const RESOLUTIONS = { '1080p': [1920, 1080], '540p': [960, 540] };
const Q = 12;                       // timestamp slots per frame: six passes
const T_SNAP = 89 / 60;             // the time of every screenshot and comparison (the last measured frame)
const SS = 3;                       // supersampled reference: SS × SS samples per pixel

const $ = id => document.getElementById(id);
const log = (...a) => { $('log').textContent += a.join(' ') + '\n'; console.log(...a); };
const fail = (...a) => { $('log').textContent += 'ERROR ' + a.join(' ') + '\n'; console.error(...a); };
const R = {
  started: new Date().toISOString(),
  note: 'Timings taken while other GPU work ran are indicative only (spikes/README.md).',
  device: {}, pipelines: {}, probe: {}, views: {}, frames: {}, stats: {}, quality: {}, coverage: {}, paced: {}, analysis: {},
};
window.__results = R;

let device, ctx, canvasFormat, cur;
const src = {}, L = {}, B = {}, M = {}, P = {};
const pipeCache = new Map();
const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[s.length >> 1] : NaN; };
const pct = (a, q) => { const s = [...a].sort((x, y) => x - y); return s[Math.min(s.length - 1, Math.floor(s.length * q))]; };
const r3 = x => Math.round(x * 1000) / 1000;
const r5 = x => Math.round(x * 100000) / 100000;

// ---- the world's parameters that live on the CPU ------------------------------------------------------

const S_MOUTH = [15, -15], S_U = [Math.SQRT1_2, -Math.SQRT1_2], S_V = [Math.SQRT1_2, Math.SQRT1_2];
const meander = s => 1.4 * (Math.sin(0.13 * s + 0.4) - Math.sin(0.4));
const smooth = (a, b, x) => { const t = Math.min(Math.max((x - a) / (b - a), 0), 1); return t * t * (3 - 2 * t); };
const streamLevel = s => 0.045 * Math.max(s, 0) + 2.0 * smooth(7.7, 8.3, s);
const fromStream = (s, l) => { const m = l + meander(s); return [S_MOUTH[0] + s * S_U[0] + m * S_V[0], S_MOUTH[1] + s * S_U[1] + m * S_V[1]]; };

// Rocks. Stream rocks: [s, l, radius, sx, sy, sz, centre height above the water]; others:
// ['xz', x, z, radius, sx, sy, sz, centre height above the ground].
const ROCKS = [
  [8.55, -1.8, 0.85, 1.0, 0.75, 0.9, 0.0],     // the waterfall's lip
  [8.6, 1.75, 0.95, 0.9, 0.8, 1.0, -0.05],
  [8.45, 0.15, 0.5, 1.0, 0.6, 0.8, -0.18],
  [8.0, -2.15, 0.85, 0.9, 1.45, 0.9, -0.35],   // flanking the fall
  [7.95, 2.2, 0.9, 0.9, 1.4, 0.95, -0.4],
  [6.9, -1.9, 0.7, 1.0, 0.8, 1.0, 0.1],        // below the fall
  [6.3, 1.4, 0.5, 1.0, 0.8, 0.9, 0.05],
  [3.5, 0.8, 0.4, 1.0, 0.7, 1.0, 0.0],         // the run to the lake
  [1.6, -1.1, 0.45, 1.0, 0.7, 1.0, -0.05],
  [12.5, 0.7, 0.5, 1.0, 0.7, 1.0, 0.0],        // upstream
  [15.5, -1.3, 0.65, 1.0, 0.75, 1.0, 0.05],
  [19.0, 1.1, 0.55, 1.0, 0.7, 1.0, 0.0],
  [23.0, -0.4, 0.45, 1.0, 0.7, 1.0, -0.05],
  ['xz', ...fromStream(9.6, 3.4), 1.3, 1.0, 0.8, 1.1, 0.3],    // boulders on the banks
  ['xz', ...fromStream(7.0, -3.6), 1.1, 1.0, 0.85, 1.0, 0.2],
  ['xz', ...fromStream(10.6, -3.1), 1.0, 1.0, 0.8, 1.0, 0.3],
  ['xz', 2.5, 21.5, 1.0, 1.1, 0.75, 0.95, 0.25],              // the lake's shores
  ['xz', -6.0, 24.5, 0.6, 1.0, 0.8, 1.0, 0.15],
  ['xz', -7.0, -25.0, 1.4, 1.0, 0.7, 1.2, 0.3],
  ['xz', -16.0, -22.0, 0.9, 1.0, 0.8, 1.0, 0.2],
];

const norm = a => { const l = Math.hypot(...a); return a.map(x => x / l); };
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sub = (a, b) => a.map((x, i) => x - b[i]);
const sunDir = (az, el) => [Math.cos(el) * Math.cos(az), Math.sin(el), Math.cos(el) * Math.sin(az)];   // az: angle in xz from +x

// The views. Eye heights are relative to the ground (shore) or the water (sunset); place() resolves them.
const VIEWS = {
  shore: {
    eye: [6, 1.7, 27], ground: true, target: [7, 2.6, -20],
    sun: sunDir(Math.atan2(0.75, -0.66), 38 * Math.PI / 180),
    sunCol: [3.1, 2.9, 2.6], exposure: 1.0,
    zen: [0.17, 0.32, 0.68], cloud: 0.32,
    hor: [0.50, 0.60, 0.74], fog: 0.0018,
    horSun: [0.72, 0.72, 0.70], shadowLen: 45,
    wind: [0.6, 0.8], ripple: 1.0,
  },
  sunset: {
    eye: [16, 0.6, 14], ground: false, target: [-12, 4.2, -33],
    sun: null,   // set below: 12° left of the view, 13° up, just above the far treeline (12–15° from here)
    sunCol: [2.6, 1.1, 0.38], exposure: 0.95,
    zen: [0.04, 0.055, 0.15], cloud: 0.26,
    hor: [0.17, 0.10, 0.17], fog: 0.0014,
    horSun: [0.78, 0.30, 0.09], shadowLen: 90,
    wind: [0.6, 0.8], ripple: 0.3,
  },
};
{
  const v = VIEWS.sunset, f = norm([v.target[0] - v.eye[0], 0, v.target[2] - v.eye[2]]);
  const az = Math.atan2(f[2], f[0]) - 12 * Math.PI / 180;   // −angle in xz turns left when looking toward −z
  v.sun = sunDir(az, 13 * Math.PI / 180);
}

// A close look at the waterfall, for development only (not measured).
const DEV_VIEWS = {
  fall: { ...VIEWS.shore, eye: [12, 2.2, -4], ground: false, target: [21, 1.4, -21] },
};

for (const [k, v] of Object.entries({ ...VIEWS, ...DEV_VIEWS })) v.name = k;

// Configurations: what each pass does. Sweeps vary one thing from `full`.
const BASE = { rdiv: 1, qdiv: 1, len: 200, waves: 1, proxy: false, ref: false, dry: false, view: 0, stats: false };
const CONFIGS = {
  full: {},
  r2: { rdiv: 2 },
  r2q2: { rdiv: 2, qdiv: 2 },
  r4q2: { rdiv: 4, qdiv: 2 },
  proxy: { proxy: true },
  proxy_r2q2: { proxy: true, rdiv: 2, qdiv: 2 },
  len25: { len: 25 },
  len50: { len: 50 },
  len100: { len: 100 },
  waves_low: { waves: 0 },
  waves_high: { waves: 2 },
  dry: { dry: true },
  newton1: { newton: 1 },
  newton3: { newton: 3 },
  eps10: { eps: 0.1 },
  eps10n3: { eps: 0.1, newton: 3 },
  newton0: { newton: 0 },
  refl_unshaded: { profile: 1 },
  refl_notrace: { profile: 2 },
  refl_norocks: { profile: 3 },
  refl_terrain: { profile: 4 },
  ref: { ref: true, len: 400 },
  ref_low: { ref: true, len: 400, waves: 0 },
  ref_high: { ref: true, len: 400, waves: 2 },
  ref_proxy: { ref: true, len: 400, proxy: true },
};
const cfgOf = name => ({ ...BASE, ...CONFIGS[name], name });
const refFor = c => c.waves === 0 ? 'ref_low' : c.waves === 2 ? 'ref_high' : 'ref';

// ---- the wave tables ------------------------------------------------------------------------------------

function rng(seed) {
  let a = seed >>> 0;
  return () => { a = (a + 0x6D2B79F5) >>> 0; let t = a; t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
}

/** Three wave sets (low 6, mid 12, high 24 waves; 0, 2, 4 noise octaves), with the same RMS slope,
 *  from 6 m down to 15, 8 and 5 cm. Bounds (amplitude and slope sums) feed the reference march. */
function waveTables() {
  const levels = [[6, 0.15, 0], [12, 0.08, 2], [24, 0.05, 4]];
  const data = new Float32Array(42 * 8);
  const bounds = [];
  const windAng = Math.atan2(0.8, 0.6);
  let off = 0;
  for (const [n, lmin, oct] of levels) {
    const r = rng(77 + n);
    let amp = 0, slope = 0;
    for (let i = 0; i < n; i++) {
      const lam = 6.0 * Math.pow(lmin / 6.0, i / (n - 1));
      const k = 2 * Math.PI / lam;
      const steep = 0.06 * Math.sqrt(2 / n) * (lam > 2.0 ? 0.35 : 1.0);
      const a = steep / k;
      const ang = windAng + (r() - 0.5) * 1.8;
      const w = Math.sqrt(9.81 * k + 7.28e-5 * k * k * k);
      data.set([Math.cos(ang), Math.sin(ang), k, a, r() * 2 * Math.PI, w, 0, 0], (off + i) * 8);
      amp += a;
      slope += a * k;
    }
    let f = 6, na = 0.0026;
    for (let j = 0; j < oct; j++) { amp += na; slope += na * f * 3.75; f *= 2; na *= 0.55; }
    bounds.push({ waves: n, noise_octaves: oct, shortest_m: lmin, amp_sum_m: r5(amp), slope_sum: r3(slope) });
    off += n;
  }
  return { data, bounds };
}
const WAVES = waveTables();

// ---- GPU plumbing ------------------------------------------------------------------------------------------

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
  for (const c of copies) { enc.copyBufferToBuffer(c.src, c.offset || 0, rb, off, c.size); off += c.size; }
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
    if (!r.ok) fail('save', name, r.status);
  } catch (e) { fail('save failed', name, e.message); }
}

/** Concatenates WGSL files, remembering where each starts so errors can name file and line. */
function concat(names) {
  let code = '', line = 1;
  const map = [];
  for (const n of names) {
    map.push({ file: `${n}.wgsl`, start: line });
    code += src[n] + '\n';
    line += src[n].split('\n').length;
  }
  return { code, map };
}

async function checkModule(module, label, map) {
  const info = await module.getCompilationInfo();
  let errors = 0;
  for (const m of info.messages) {
    let where = `line ${m.lineNum}`;
    for (const f of map) if (m.lineNum >= f.start) where = `${f.file}:${m.lineNum - f.start + 1}`;
    const text = `${label} ${m.type} ${where}:${m.linePos} ${m.message}`;
    if (m.type === 'error') { errors++; fail(text); } else log(text);
  }
  if (errors) throw new Error(`${label}: ${errors} WGSL error(s)`);
}

async function init() {
  if (!navigator.gpu) throw new Error('WebGPU is not available');
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
  const want = ['timestamp-query', 'float32-filterable'].filter(f => adapter.features.has(f));
  if (!want.includes('float32-filterable')) throw new Error('float32-filterable is required (the heightmap is bilinear r32float)');
  device = await adapter.requestDevice({ requiredFeatures: want });
  device.lost.then(i => fail('DEVICE LOST:', i.message));
  device.addEventListener('uncapturederror', e => fail('GPU ERROR:', e.error.message));
  const info = adapter.info || {};
  R.device = {
    vendor: info.vendor, architecture: info.architecture, description: info.description,
    userAgent: navigator.userAgent, timestampQuery: want.includes('timestamp-query'), pageVisibility: document.visibilityState,
  };
  if (!R.device.timestampQuery) throw new Error('timestamp-query is required');
  for (const f of ['world', 'scene', 'water', 'passes', 'place', 'blit']) src[f] = await (await fetch(`${f}.wgsl`)).text();

  const canvas = $('view');
  canvas.width = 1920;
  canvas.height = 1080;
  ctx = canvas.getContext('webgpu');
  canvasFormat = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format: canvasFormat, alphaMode: 'opaque' });

  const C = GPUShaderStage.COMPUTE;
  const buf = (binding, type) => ({ binding, visibility: C, buffer: { type } });
  const st = (binding, format) => ({ binding, visibility: C, storageTexture: { access: 'write-only', format } });
  const tx = binding => ({ binding, visibility: C, texture: { sampleType: 'unfilterable-float' } });
  L.g0 = device.createBindGroupLayout({
    entries: [buf(0, 'uniform'), buf(1, 'storage'), buf(2, 'read-only-storage'), buf(3, 'uniform'),
      { binding: 4, visibility: C, texture: { sampleType: 'float' } }, { binding: 5, visibility: C, sampler: { type: 'filtering' } },
      { binding: 6, visibility: C, texture: { sampleType: 'float', viewDimension: '3d' } },
      { binding: 7, visibility: C, texture: { sampleType: 'float' } }, { binding: 8, visibility: C, sampler: { type: 'filtering' } },
      buf(9, 'uniform')],
  });
  const g1 = {
    water_surface: [st(0, 'r32float')],
    scene_pass: [tx(1), st(2, 'rgba16float'), st(3, 'r32float')],
    water_gbuf: [tx(4), st(9, 'rgba32float')],
    reflect_pass: [tx(4), st(5, 'rgba16float'), tx(10)],
    refract_pass: [tx(4), st(7, 'rgba16float'), tx(10)],
    composite: [tx(4), tx(6), tx(8), st(2, 'rgba16float'), tx(10)],
  };
  L.g1 = {};
  L.pl = {};
  for (const [k, entries] of Object.entries(g1)) {
    L.g1[k] = device.createBindGroupLayout({ entries });
    L.pl[k] = device.createPipelineLayout({ bindGroupLayouts: [L.g0, L.g1[k]] });
  }
  L.place = device.createBindGroupLayout({
    entries: [buf(0, 'storage'), buf(1, 'storage'), st(2, 'r32float'),
      { binding: 3, visibility: C, storageTexture: { access: 'write-only', format: 'r32float', viewDimension: '3d' } }, st(4, 'rgba32float')],
  });
  L.blit = device.createBindGroupLayout({
    entries: [{ binding: 0, visibility: GPUShaderStage.FRAGMENT, texture: {} }, { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: {} }],
  });

  const t0 = performance.now();
  const trace = concat(['world', 'scene', 'water', 'passes']);
  M.trace = device.createShaderModule({ code: trace.code, label: 'trace' });
  M.traceMap = trace.map;
  await checkModule(M.trace, 'trace', trace.map);
  const place = concat(['world', 'place']);
  M.place = device.createShaderModule({ code: place.code, label: 'place' });
  await checkModule(M.place, 'place', place.map);
  const blit = concat(['blit']);
  M.blit = device.createShaderModule({ code: blit.code, label: 'blit' });
  await checkModule(M.blit, 'blit', blit.map);
  R.pipelines.module_check_ms = r3(performance.now() - t0);

  B.frame = buffer(272, U.UNIFORM | U.COPY_DST);
  B.stats = buffer(48 * 4, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  B.objs = buffer(8336 * 16, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  B.waves = buffer(WAVES.data.byteLength, U.UNIFORM, WAVES.data);
  B.lip = buffer(32, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  B.rs = buffer(32 * 16, U.UNIFORM | U.COPY_DST);
  B.hmap = device.createTexture({ size: [HM_N, HM_N], format: 'r32float', usage: TU.STORAGE_BINDING | TU.TEXTURE_BINDING, label: 'heightmap' });
  B.n3 = device.createTexture({ size: [96, 96, 96], dimension: '3d', format: 'r32float', usage: TU.STORAGE_BINDING | TU.TEXTURE_BINDING, label: 'noise3' });
  B.n2 = device.createTexture({ size: [512, 512], format: 'rgba32float', usage: TU.STORAGE_BINDING | TU.TEXTURE_BINDING, label: 'noise2' });
  const rep = { magFilter: 'linear', minFilter: 'linear', addressModeU: 'repeat', addressModeV: 'repeat', addressModeW: 'repeat' };
  B.g0 = device.createBindGroup({
    layout: L.g0,
    entries: [...[B.frame, B.stats, B.objs, B.waves].map((b, binding) => ({ binding, resource: { buffer: b } })),
      { binding: 4, resource: B.hmap.createView() },
      { binding: 5, resource: device.createSampler({ magFilter: 'linear', minFilter: 'linear', addressModeU: 'clamp-to-edge', addressModeV: 'clamp-to-edge' }) },
      { binding: 6, resource: B.n3.createView({ dimension: '3d' }) }, { binding: 7, resource: B.n2.createView() },
      { binding: 8, resource: device.createSampler(rep) }, { binding: 9, resource: { buffer: B.rs } }],
  });
  P.blit = await device.createRenderPipelineAsync({
    layout: device.createPipelineLayout({ bindGroupLayouts: [L.blit] }),
    vertex: { module: M.blit, entryPoint: 'blit_vs' },
    fragment: { module: M.blit, entryPoint: 'blit_fs', targets: [{ format: canvasFormat }] },
  });
}

// ---- placement --------------------------------------------------------------------------------------------

const ROCK0 = 8192, TOWER_SLOT = 8256, PROBE0 = 8257, HM_N = 2048;
const STREAM_GROUP = 16;   // the first 16 rocks (the stream's and its banks') share a bounding box
let rockBox;

async function place() {
  const layout = device.createPipelineLayout({ bindGroupLayouts: [L.place] });
  const [pl, pr, pb, p3, p2, pk] = await Promise.all(['place', 'probe', 'bake', 'bake_n3', 'bake_n2', 'place_blocks'].map(entryPoint =>
    device.createComputePipelineAsync({ layout, compute: { module: M.place, entryPoint } })));
  // Rocks and probe points go in first; the place pass fills in heights.
  const rocks = new Float32Array(32 * 8);
  const r = rng(5);
  ROCKS.forEach((k, i) => {
    let x, z, rad, sx, sy, sz, y, flag = 0;
    if (k[0] === 'xz') { [, x, z, rad, sx, sy, sz, y] = k; }
    else {
      const [s, l, rr, a, b, c, h] = k;
      [x, z] = fromStream(s, l);
      [rad, sx, sy, sz] = [rr, a, b, c];
      y = streamLevel(s) + h;
      flag = 2;                         // absolute height: place() leaves it
    }
    rocks.set([x, y, z, rad, sx, sy, sz, r() + flag], i * 8);
  });
  const probes = new Float32Array(8 * 4);
  Object.values(VIEWS).forEach((v, i) => probes.set([v.eye[0], 0, v.eye[2], 0], i * 4));
  device.queue.writeBuffer(B.objs, ROCK0 * 16, rocks);
  device.queue.writeBuffer(B.objs, PROBE0 * 16, probes);
  const bg = device.createBindGroup({
    layout: L.place,
    entries: [{ binding: 0, resource: { buffer: B.objs } }, { binding: 1, resource: { buffer: B.lip } }, { binding: 2, resource: B.hmap.createView() },
      { binding: 3, resource: B.n3.createView({ dimension: '3d' }) }, { binding: 4, resource: B.n2.createView() }],
  });
  // One submission per stage, each awaited, so none runs long (spikes/README.md's GPU rules).
  const stage = async (pipe, ...wg) => {
    const enc = device.createCommandEncoder();
    const p = enc.beginComputePass();
    p.setPipeline(pipe);
    p.setBindGroup(0, bg);
    p.dispatchWorkgroups(...wg);
    p.end();
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
  };
  const t0 = performance.now();
  { const enc = device.createCommandEncoder(); enc.clearBuffer(B.lip); device.queue.submit([enc.finish()]); }
  await stage(pb, HM_N / 8, HM_N / 8);
  await stage(p3, 24, 24, 24);
  await stage(p2, 64, 64);
  await stage(pl, Math.ceil((4096 + 32 + 1 + 8) / 64));
  await stage(pk, 1);
  await stage(pr, 1024);
  R.world_cook_ms = r3(performance.now() - t0);
  const raw = await readback([{ src: B.objs, size: 8272 * 16 }, { src: B.lip, size: 32 }]);
  const o = new Float32Array(raw, 0, 8272 * 4);
  const lip = new Float32Array(raw, 8272 * 16, 8);
  // Absolute-height rocks were flagged by seed ≥ 2: strip the flag.
  const fix = new Float32Array(32 * 8), spheres = new Float32Array(32 * 4);
  let lo = [1e9, 1e9, 1e9], hi = [-1e9, -1e9, -1e9];
  for (let i = 0; i < ROCKS.length; i++) {
    const a = o.slice((ROCK0 + 2 * i) * 4, (ROCK0 + 2 * i) * 4 + 4), b = o.slice((ROCK0 + 2 * i + 1) * 4, (ROCK0 + 2 * i + 1) * 4 + 4);
    if (b[3] >= 2) b[3] -= 2;
    fix.set(a, i * 8);
    fix.set(b, i * 8 + 4);
    const ext = a[3] * (Math.max(b[0], b[1], b[2]) + 0.1225) + 0.01;
    spheres.set([a[0], a[1], a[2], ext], i * 4);
    if (i < STREAM_GROUP) for (let c = 0; c < 3; c++) { lo[c] = Math.min(lo[c], a[c] - ext); hi[c] = Math.max(hi[c], a[c] + ext); }
  }
  device.queue.writeBuffer(B.objs, ROCK0 * 16, fix);
  device.queue.writeBuffer(B.rs, 0, spheres);
  rockBox = { lo, hi };
  let trees = 0, birches = 0;
  for (let i = 0; i < 4096; i++) if (o[i * 8 + 3] > 0) { trees++; if (o[i * 8 + 5] > 0.5) birches++; }
  const tower = o.slice(TOWER_SLOT * 4, TOWER_SLOT * 4 + 3);
  Object.values(VIEWS).forEach((v, i) => {
    const g = o[(PROBE0 + i) * 4 + 1];
    v.groundY = g;
    v.rn = o[(PROBE0 + i) * 4 + 3];
    if (v.ground) v.eye = [v.eye[0], g + v.eye[1], v.eye[2]];
  });
  R.world = {
    trees, birches, rocks: ROCKS.length, tower_base: [...tower].map(r3),
    eyes: Object.fromEntries(Object.entries(VIEWS).map(([k, v]) => [k, { eye: v.eye.map(r3), ground: r3(v.groundY), lake_rn: r3(v.rn) }])),
    waves: WAVES.bounds,
  };
  R.probe = {
    terrain_slope_max_flat: r3(lip[0]), declared_flat: 0.72,
    terrain_slope_max_steep: r3(lip[1]), declared_steep: 1.6,
    noise3_grad_max: r3(lip[2]), noise2_grad_max: r3(lip[3]), declared_noise_slope: 2.5,
    samples: 1024 * 64 * 16, heightmap: '2048² r32float over 300 m, bilinear',
  };
  log('world', JSON.stringify(R.world));
  log('probe', JSON.stringify(R.probe));
  if (lip[0] > 0.72 || lip[1] > 1.6) fail('terrain slope exceeds its declared bound', JSON.stringify(R.probe));
}

// ---- pipelines -------------------------------------------------------------------------------------------

const PASSES = ['water_surface', 'scene_pass', 'water_gbuf', 'reflect_pass', 'refract_pass', 'composite'];
const WATER_PASSES = [0, 2, 3, 4, 5];   // every pass but the scene's
const PASS_CONSTS = {
  water_surface: c => ({ REF: c.ref, STATS: c.stats, WAVES: c.waves }),
  scene_pass: c => ({ REF: c.ref, STATS: c.stats, WATER: !c.dry, VIEW: c.view === 3 ? 3 : 0 }),
  water_gbuf: c => ({ REF: c.ref, STATS: c.stats, WAVES: c.waves }),
  // Reflection rays graze many crowns in a forest: they get twice the primary step budget.
  reflect_pass: c => ({ REF: c.ref, STATS: c.stats, WAVES: c.waves, PROXY: c.proxy, VIEW: c.view === 1 ? 1 : 0, PROFILE: c.profile || 0, ...(c.ref ? {} : { MAX_STEPS: 320 }) }),
  refract_pass: c => ({ REF: c.ref, STATS: c.stats, WAVES: c.waves, VIEW: c.view === 2 ? 2 : 0 }),
  composite: c => ({ REF: c.ref, STATS: c.stats, WAVES: c.waves, VIEW: c.view === 1 || c.view === 2 ? c.view : 0 }),
};
// Optional per-configuration overrides, passed to every pass that has them.
const extra = c => ({ ...(c.eps != null ? { EPS: c.eps } : {}), ...(c.newton != null ? { NEWTON: c.newton } : {}) });
const numify = o => Object.fromEntries(Object.entries(o).map(([k, v]) => [k, Number(v)]));
const constsFor = (pass, c) => numify({ ...PASS_CONSTS[pass](c), ...extra(c) });

function pipeFor(pass, c, module = M.trace) {
  const consts = constsFor(pass, c);
  const key = pass + JSON.stringify(consts) + (module === M.trace ? '' : module.label);
  if (!pipeCache.has(key)) {
    pipeCache.set(key, device.createComputePipelineAsync({ layout: L.pl[pass], compute: { module, entryPoint: pass, constants: consts } })
      .catch(async e => { await checkModule(module, 'trace', M.traceMap); throw e; }));
  }
  return pipeCache.get(key);
}

async function prepare(c) {
  const passes = c.dry ? ['scene_pass'] : PASSES;
  const ps = await Promise.all(passes.map(p => pipeFor(p, c)));
  return Object.fromEntries(passes.map((p, i) => [p, ps[i]]));
}

/** Cold creation of one configuration's five pipelines, with source made unique so no cache serves it. */
async function coldPipelines() {
  const salt = `\nconst SALT_${Math.floor(Math.random() * 1e9)}: f32 = 0.0;\n`;
  const m = device.createShaderModule({ code: concat(['world', 'scene', 'water', 'passes']).code + salt, label: 'cold' });
  const c = cfgOf('full');
  const t0 = performance.now();
  const ms = {};
  for (const p of PASSES) {
    const t = performance.now();
    await device.createComputePipelineAsync({ layout: L.pl[p], compute: { module: m, entryPoint: p, constants: constsFor(p, c) } });
    ms[p] = r3(performance.now() - t);
  }
  ms.total = r3(performance.now() - t0);
  return ms;
}

// ---- resources per resolution ------------------------------------------------------------------------------

const resCache = {};
function resources(name) {
  if (resCache[name]) return resCache[name];
  const [W, H] = RESOLUTIONS[name];
  const S = TU.STORAGE_BINDING | TU.TEXTURE_BINDING | TU.COPY_SRC;
  const r = { name, W, H };
  for (const [k, f] of [['wt', 'r32float'], ['out', 'rgba16float'], ['wm', 'r32float'], ['rt', 'rgba16float'], ['qt', 'rgba16float'], ['wg', 'rgba32float']]) {
    r[k] = device.createTexture({ size: [W, H], format: f, usage: S, label: k });
  }
  const v = k => r[k].createView();
  const bg = (pass, list) => device.createBindGroup({ layout: L.g1[pass], entries: list.map(([binding, k]) => ({ binding, resource: v(k) })) });
  r.bg = {
    water_surface: bg('water_surface', [[0, 'wt']]),
    scene_pass: bg('scene_pass', [[1, 'wt'], [2, 'out'], [3, 'wm']]),
    water_gbuf: bg('water_gbuf', [[4, 'wm'], [9, 'wg']]),
    reflect_pass: bg('reflect_pass', [[4, 'wm'], [5, 'rt'], [10, 'wg']]),
    refract_pass: bg('refract_pass', [[4, 'wm'], [7, 'qt'], [10, 'wg']]),
    composite: bg('composite', [[4, 'wm'], [6, 'rt'], [8, 'qt'], [2, 'out'], [10, 'wg']]),
  };
  r.blit = device.createBindGroup({
    layout: L.blit,
    entries: [{ binding: 0, resource: v('out') }, { binding: 1, resource: device.createSampler({ magFilter: 'linear', minFilter: 'linear' }) }],
  });
  resCache[name] = r;
  return r;
}

// ---- frames ------------------------------------------------------------------------------------------------

/** Writes the frame uniform for a view, configuration and time. `pitch` tilts the camera (degrees),
 *  for the coverage sweep. */
function update(view, c, t, jitter = [0, 0], fpScale = 1, tile = null) {
  const { W, H } = cur;
  const f = new Float32Array(68), u = new Uint32Array(f.buffer);
  const eye = view.eye;
  let target = view.target;
  if (view.pitch) {
    const fw0 = norm(sub(target, eye)), hz = Math.hypot(fw0[0], fw0[2]);
    const el = Math.atan2(fw0[1], hz) + view.pitch * Math.PI / 180;
    target = [eye[0] + fw0[0] / hz * Math.cos(el), eye[1] + Math.sin(el), eye[2] + fw0[2] / hz * Math.cos(el)];
  }
  const fw = norm(sub(target, eye)), rt = norm(cross(fw, [0, 1, 0])), up = cross(rt, fw);
  const ty = Math.tan(FOVY / 2), tx = ty * W / H;
  f.set([...eye, 2 * ty / H * fpScale], 0);
  f.set([...fw, 0], 4);
  f.set([...rt.map(x => x * tx), 0], 8);
  f.set([...up.map(x => x * ty), 0], 12);
  f.set([...view.sun, 0.0047], 16);
  f.set([...view.sunCol, view.exposure], 20);
  f.set([...view.zen, view.cloud], 24);
  f.set([...view.hor, view.fog], 28);
  f.set([...view.horSun, view.shadowLen], 32);
  u.set([W, H, c.rdiv, c.qdiv], 36);
  f.set([t, c.len, jitter[0], jitter[1]], 40);
  f.set([...view.wind, view.ripple, 0], 44);
  f.set([WAVES.bounds[c.waves].amp_sum_m, WAVES.bounds[c.waves].slope_sum, 0, 0], 48);
  u.set([ROCKS.length, STREAM_GROUP, 0, 0], 52);
  f.set([...rockBox.lo, 0], 56);
  f.set([...rockBox.hi, 0], 60);
  u.set(tile ? [tile[0], tile[1], tile[2], tile[3]] : [0, 0, W, H], 64);
  device.queue.writeBuffer(B.frame, 0, f);
}

/** Encodes one frame, or the part of it inside `tile` ([x0, y0, x1, y1]; the uniform must carry
 *  the same tile). `waterOnly` skips the scene pass, whose output (colour and the visible-water
 *  mask) doesn't change between frames of a fixed view, so the water passes can be timed alone. */
function encodeFrame(enc, c, pipes, qs, q0, tile = null, waterOnly = false) {
  const ts = i => qs ? { timestampWrites: { querySet: qs, beginningOfPassWriteIndex: q0 + 2 * i, endOfPassWriteIndex: q0 + 2 * i + 1 } } : {};
  const W = tile ? tile[2] - tile[0] : cur.W, H = tile ? tile[3] - tile[1] : cur.H;
  const g = (n, dv = 1) => Math.ceil(Math.ceil(n / dv) / 8) + (dv > 1 && tile ? 1 : 0);
  const run = (i, name, x, y) => {
    const p = enc.beginComputePass(ts(i));
    p.setPipeline(pipes[name]);
    p.setBindGroup(0, B.g0);
    p.setBindGroup(1, cur.bg[name]);
    p.dispatchWorkgroups(x, y);
    p.end();
  };
  if (c.dry) { run(1, 'scene_pass', g(W), g(H)); return; }
  run(0, 'water_surface', g(W), g(H));
  if (!waterOnly) run(1, 'scene_pass', g(W), g(H));
  run(2, 'water_gbuf', g(W), g(H));
  run(3, 'reflect_pass', g(W, c.rdiv), g(H, c.rdiv));
  run(4, 'refract_pass', g(W, c.qdiv), g(H, c.qdiv));
  run(5, 'composite', g(W), g(H));
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

/** GPU time per pass under sustained load: 30 untimed frames, then `frames` back to back.
 *  `whole`: full frames (scene pass included). Otherwise one full frame, then water-only frames:
 *  the scene pass's output doesn't change for a fixed view, and skipping it keeps sweeps short. */
async function measure(view, c, frames = 90, whole = false) {
  const pipes = await prepare(c);
  const waterOnly = !whole && !c.dry;
  if (waterOnly) {
    update(view, c, 0);
    const enc = device.createCommandEncoder();
    encodeFrame(enc, c, pipes, null, 0);
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
  }
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  for (let f = 0; f < 30; f++) {
    update(view, c, f / 60);
    const enc = device.createCommandEncoder();
    encodeFrame(enc, c, pipes, null, 0, null, waterOnly);
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();
  const t1 = performance.now();
  for (let f = 0; f < frames; f++) {
    update(view, c, f / 60);
    const enc = device.createCommandEncoder();
    encodeFrame(enc, c, pipes, qs, f * Q, null, waterOnly);
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();
  const throughput = (performance.now() - t1) / frames;
  const ts = await resolveTimestamps(qs, frames * Q);
  const d = (f, i) => Number(ts[f * Q + 2 * i + 1] - ts[f * Q + 2 * i]) / 1e6;
  const all = [...Array(frames).keys()];
  const stat = fn => { const v = all.map(fn); return { median: r3(median(v)), p95: r3(pct(v, 0.95)) }; };
  if (c.dry) {
    const s = stat(f => d(f, 1));
    return { scene: s.median, frame: s.median, frame_p95: s.p95, throughput_ms: r3(throughput) };
  }
  const water = stat(f => WATER_PASSES.reduce((s, i) => s + d(f, i), 0));
  const out = {
    frames: waterOnly ? 'water passes only' : 'whole frames',
    water_surface: stat(f => d(f, 0)).median, gbuf: stat(f => d(f, 2)).median,
    reflect: stat(f => d(f, 3)).median, refract: stat(f => d(f, 4)).median, composite: stat(f => d(f, 5)).median,
    water: water.median, water_p95: water.p95, throughput_ms: r3(throughput),
  };
  if (!waterOnly) {
    const frame = stat(f => Number(ts[f * Q + 11] - ts[f * Q]) / 1e6);
    Object.assign(out, { scene: stat(f => d(f, 1)).median, frame: frame.median, frame_p95: frame.p95 });
  }
  return out;
}

/** Frames submitted at 60 Hz by busy-wait (timers may be throttled), as in spikes 01 and 02. */
async function paced(view, c, frames = 180) {
  const pipes = await prepare(c);
  const period = 1000 / 60;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  const t0 = performance.now() + 20;
  for (let f = 0; f < frames; f++) {
    const deadline = t0 + f * period;
    while (performance.now() < deadline) { /* spin */ }
    update(view, c, f / 60);
    const enc = device.createCommandEncoder();
    encodeFrame(enc, c, pipes, qs, f * Q);
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * Q);
  const keep = [...Array(frames).keys()].slice(30);
  const gpu = keep.map(f => Number(ts[f * Q + 11] - ts[f * Q]) / 1e6);
  const water = keep.map(f => WATER_PASSES.reduce((s, i) => s + Number(ts[f * Q + 2 * i + 1] - ts[f * Q + 2 * i]) / 1e6, 0));
  return {
    pacing: 'busy-wait at 60 Hz', frames: keep.length,
    frame_gpu_median: r3(median(gpu)), frame_gpu_p95: r3(pct(gpu, 0.95)), frame_gpu_max: r3(Math.max(...gpu)),
    frame_gpu_over_16_7: gpu.filter(g => g > 16.7).length,
    water_median: r3(median(water)), water_p95: r3(pct(water, 0.95)),
  };
}

const STAT_NAMES = ['water_px', 'ws_steps', 'ws_caps', 'scene_steps', 'scene_caps', 'scene_tsteps', 'scene_cells', 'shadow_steps',
  'refl_rays', 'refl_steps', 'refl_caps', 'refl_tsteps', 'refl_cells', 'refl_sky', 'refl_terrain', 'refl_tree', 'refl_tower', 'refl_rock', 'refl_far',
  'refr_rays', 'refr_steps', 'refr_caps', 'refr_tsteps', 'refr_hit', 'glint_rays', 'glint_steps', 'stream_px', 'refl_wsteps', 'refr_wsteps',
  'scene_tcaps', 'refl_tcaps', 'refr_tcaps', 'sky_px', 'refl_fallback', 'gb_steps', 'shadow_caps'];

const TILE = 96;
R.gpu_safety = { max_submission_ms: 0, ref_tile_px: TILE };

/** Renders one frame at time t: in one submission, or for references tile by tile, one awaited
 *  submission per 96² tile, so no submission runs long (spikes/README.md's GPU rules). */
async function render(view, c, pipes, t = T_SNAP, jitter = [0, 0], fpScale = 1) {
  const { W, H } = cur;
  const tiles = [];
  if (!c.ref) tiles.push(null);
  else for (let y = 0; y < H; y += TILE) for (let x = 0; x < W; x += TILE) tiles.push([x, y, Math.min(x + TILE, W), Math.min(y + TILE, H)]);
  for (const tile of tiles) {
    update(view, c, t, jitter, fpScale, tile);
    const enc = device.createCommandEncoder();
    encodeFrame(enc, c, pipes, null, 0, tile);
    const t0 = performance.now();
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
    const ms = performance.now() - t0;
    if (ms > R.gpu_safety.max_submission_ms) {
      R.gpu_safety.max_submission_ms = r3(ms);
      R.gpu_safety.max_submission_what = `${view.name || ''} ${c.name}${tile ? ' tile' : ' frame'}`;
    }
  }
}

async function stats(view, c) {
  const cs = { ...c, stats: true };
  const pipes = await prepare(cs);
  const enc = device.createCommandEncoder();
  enc.clearBuffer(B.stats);
  device.queue.submit([enc.finish()]);
  await render(view, cs, pipes);
  const s = new Uint32Array(await readback([{ src: B.stats, size: 48 * 4 }]));
  const o = {};
  STAT_NAMES.forEach((n, i) => { o[n] = s[i]; });
  const px = cur.W * cur.H, per = (a, b) => r3(a / Math.max(b, 1));
  o.coverage = r3(o.water_px / px);
  o.stream_share_of_water = per(o.stream_px, o.water_px);
  o.ws_steps_per_px = per(o.ws_steps, px);
  o.scene_steps_per_px = per(o.scene_steps + o.scene_tsteps, px);
  o.refl_steps_per_ray = per(o.refl_steps + o.refl_tsteps, o.refl_rays);
  o.refl_object_steps_per_ray = per(o.refl_steps, o.refl_rays);
  o.refl_terrain_steps_per_ray = per(o.refl_tsteps, o.refl_rays);
  o.refl_cells_per_ray = per(o.refl_cells, o.refl_rays);
  o.refl_cap_share = r5((o.refl_caps + o.refl_tcaps) / Math.max(o.refl_rays, 1));
  o.refl_hits = Object.fromEntries(['sky', 'terrain', 'tree', 'tower', 'rock', 'far'].map(k => [k, per(o[`refl_${k}`], o.refl_rays)]));
  o.refr_steps_per_ray = per(o.refr_steps + o.refr_tsteps, o.refr_rays);
  o.refr_cap_share = r5((o.refr_caps + o.refr_tcaps) / Math.max(o.refr_rays, 1));
  o.refr_hit_share = per(o.refr_hit, o.refr_rays);
  o.glint_ray_share_of_water = per(o.glint_rays, o.water_px);
  o.glint_steps_per_ray = per(o.glint_steps, o.glint_rays);
  o.scene_cap_share = r5((o.scene_caps + o.scene_tcaps) / px);
  o.ws_cap_share = r5(o.ws_caps / px);
  o.secondary_cap_share_of_water = r5((o.refl_caps + o.refl_tcaps + o.refr_caps + o.refr_tcaps) / Math.max(o.water_px, 1));
  return o;
}

// ---- images --------------------------------------------------------------------------------------------------

const HALF = new Float32Array(65536);
for (let h = 0; h < 65536; h++) {
  const s = h >> 15, e = (h >> 10) & 31, m = h & 1023;
  let v = e === 0 ? m * 2 ** -24 : e === 31 ? 1 : (1 + m / 1024) * 2 ** (e - 15);
  HALF[h] = s ? 0 : Math.min(v, 1);
}
const GAMMA = new Uint8Array(4097);
for (let i = 0; i <= 4096; i++) GAMMA[i] = Math.round(255 * Math.pow(i / 4096, 1 / 2.2));
const toByte = v => GAMMA[Math.round(Math.min(Math.max(v, 0), 1) * 4096)];

/** Renders one configuration and reads back the display image (linear, tonemapped) and the water mask. */
async function grab(view, c, t = T_SNAP, jitter = [0, 0], fpScale = 1) {
  const pipes = await prepare(c);
  const { W, H } = cur;
  const out = device.createBuffer({ size: W * H * 8, usage: U.COPY_DST | U.MAP_READ });
  const wm = device.createBuffer({ size: W * H * 4, usage: U.COPY_DST | U.MAP_READ });
  const tr = performance.now();
  await render(view, c, pipes, t, jitter, fpScale);
  const renderMs = performance.now() - tr;
  const enc = device.createCommandEncoder();
  enc.copyTextureToBuffer({ texture: cur.out }, { buffer: out, bytesPerRow: W * 8, rowsPerImage: H }, [W, H]);
  enc.copyTextureToBuffer({ texture: cur.wm }, { buffer: wm, bytesPerRow: W * 4, rowsPerImage: H }, [W, H]);
  const t0 = performance.now();
  device.queue.submit([enc.finish()]);
  await Promise.all([out.mapAsync(GPUMapMode.READ), wm.mapAsync(GPUMapMode.READ)]);
  const ms = performance.now() - t0 + renderMs;
  const half = new Uint16Array(out.getMappedRange().slice(0));
  const mask = new Float32Array(wm.getMappedRange().slice(0));
  out.unmap(); out.destroy(); wm.unmap(); wm.destroy();
  const lin = new Float32Array(W * H * 3);
  for (let i = 0, j = 0; i < half.length; i += 4) { lin[j++] = HALF[half[i]]; lin[j++] = HALF[half[i + 1]]; lin[j++] = HALF[half[i + 2]]; }
  return { lin, mask, ms };
}

const bytes = lin => { const b = new Uint8Array(lin.length); for (let i = 0; i < lin.length; i++) b[i] = toByte(lin[i]); return b; };

/** The supersampled reference: SS×SS stratified jittered samples with 1/SS of the footprint. */
async function supersample(view, c) {
  const { W, H } = cur;
  const acc = new Float32Array(W * H * 3);
  let mask, ms = 0;
  for (let j = 0; j < SS; j++) for (let i = 0; i < SS; i++) {
    const g = await grab(view, c, T_SNAP, [(i + 0.5) / SS - 0.5, (j + 0.5) / SS - 0.5], 1 / SS);
    for (let k = 0; k < acc.length; k++) acc[k] += g.lin[k];
    if (!mask) mask = g.mask;
    ms += g.ms;
  }
  for (let k = 0; k < acc.length; k++) acc[k] /= SS * SS;
  return { lin: acc, mask, ms };
}

function diff(a, b, mask) {
  let sum = 0, over = 0, sumW = 0, overW = 0, nW = 0, max = 0;
  const n = a.length / 3;
  for (let i = 0, p = 0; i < a.length; i += 3, p++) {
    let m = 0, s = 0;
    for (let c = 0; c < 3; c++) { const d = Math.abs(a[i + c] - b[i + c]); s += d; if (d > m) m = d; }
    sum += s;
    if (m > 8) over++;
    if (m > max) max = m;
    if (mask[p] !== 0) { sumW += s; nW++; if (m > 8) overW++; }
  }
  return {
    mean_abs_255: r3(sum / a.length), share_over_8: r5(over / n), max,
    water_mean_abs_255: r3(sumW / Math.max(3 * nW, 1)), water_share_over_8: r5(overW / Math.max(nW, 1)), water_px: nW,
  };
}

async function putImage(name, rgb, W, H, diffAgainst) {
  const c = document.createElement('canvas');
  c.width = W;
  c.height = H;
  const g = c.getContext('2d'), img = g.createImageData(W, H);
  for (let i = 0, j = 0; i < rgb.length; i += 3, j += 4) {
    if (diffAgainst) {
      let m = 0;
      for (let k = 0; k < 3; k++) m = Math.max(m, Math.abs(rgb[i + k] - diffAgainst[i + k]));
      const base = (rgb[i] + rgb[i + 1] + rgb[i + 2]) / 9;
      img.data[j] = m > 8 ? 255 : base;
      img.data[j + 1] = m > 8 ? 30 : (m > 2 ? 160 : base);
      img.data[j + 2] = m > 8 ? 30 : base;
    } else {
      img.data[j] = rgb[i]; img.data[j + 1] = rgb[i + 1]; img.data[j + 2] = rgb[i + 2];
    }
    img.data[j + 3] = 255;
  }
  g.putImageData(img, 0, 0);
  const blob = await new Promise(r => c.toBlob(r, 'image/png'));
  if (blob) await save(`${name}.png`, blob);
}

function present() {
  const enc = device.createCommandEncoder();
  const p = enc.beginRenderPass({ colorAttachments: [{ view: ctx.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
  p.setPipeline(P.blit);
  p.setBindGroup(0, cur.blit);
  p.draw(3);
  p.end();
  device.queue.submit([enc.finish()]);
}

async function draw(view, c, t = T_SNAP) {
  await render(view, c, await prepare(c), t);
}

async function snapshot(name) {
  present();
  await device.queue.onSubmittedWorkDone();
  const blob = await new Promise(r => $('view').toBlob(r, 'image/png'));
  if (blob) await save(`${name}.png`, blob);
  else fail('snapshot', name, 'no blob');
}

// ---- the run ------------------------------------------------------------------------------------------------

const MEASURED = ['full', 'r2', 'r2q2', 'r4q2', 'proxy', 'proxy_r2q2', 'len25', 'len50', 'len100', 'waves_low', 'waves_high'];
const COMPARED = [...MEASURED, 'newton1', 'eps10'];   // quality only: one Newton step; a 0.1-footprint hit tolerance
const PROFILES = ['refl_notrace', 'refl_unshaded'];   // where reflection time goes (shore only)
const COVERAGE_PITCH = [8, 3, -2, -7, -13, -20, -28];
const COVERAGE_CFGS = ['full', 'r2', 'r2q2', 'r4q2'];
const SNAPS = ['full', 'r2q2', 'r4q2', 'proxy', 'len25', 'waves_low', 'waves_high'];
const ROUNDS = 2;

function fit(xs, ys) {
  const n = xs.length, mx = xs.reduce((a, b) => a + b) / n, my = ys.reduce((a, b) => a + b) / n;
  let sxy = 0, sxx = 0;
  for (let i = 0; i < n; i++) { sxy += (xs[i] - mx) * (ys[i] - my); sxx += (xs[i] - mx) ** 2; }
  const b = sxy / sxx, a = my - b * mx;
  return { intercept_ms: r3(a), ms_per_coverage: r3(b), at30: r3(a + b * 0.3), coverage_at_1ms: r3((1 - a) / b), coverage_at_2ms: r3((2 - a) / b) };
}

function perPixel(f, s) {
  f.ns_per_water_px = {
    water: r3(f.water * 1e6 / Math.max(s.water_px, 1)),
    reflect: r3(f.reflect * 1e6 / Math.max(s.water_px, 1)),
    refract: r3(f.refract * 1e6 / Math.max(s.water_px, 1)),
    gbuf_composite_surface: r3((f.gbuf + f.composite + f.water_surface) * 1e6 / Math.max(s.water_px, 1)),
  };
  f.ns_per_ray = {
    reflect: r3(f.reflect * 1e6 / Math.max(s.refl_rays, 1)),
    refract: r3(f.refract * 1e6 / Math.max(s.refr_rays, 1)),
  };
}

/** Times configurations in interleaved rounds (water-only frames) and keeps each one's round with
 *  the lower median, listing all rounds. */
async function timeRounds(view, names, frames, keyOf) {
  const rounds = {};
  for (let round = 0; round < ROUNDS; round++) {
    for (const name of names) (rounds[name] ||= []).push(await measure(view, cfgOf(name), frames));
  }
  for (const name of names) {
    const rs = rounds[name];
    const best = rs.reduce((a, b) => (b.water < a.water ? b : a));
    R.frames[keyOf(name)] = { ...best, water_rounds: rs.map(r => r.water) };
  }
}

async function viewSuite(vname, frames) {
  const view = VIEWS[vname];
  R.views[vname] = { eye: view.eye.map(r3), target: view.target, sun: view.sun.map(r3), exposure: view.exposure };
  const names = vname === 'shore' ? [...MEASURED, ...PROFILES] : MEASURED;
  for (const name of names) R.stats[`${cur.name}/${vname}/${name}`] = await stats(view, cfgOf(name));
  await timeRounds(view, names, frames, n => `${cur.name}/${vname}/${n}`);
  for (const name of names) {
    const key = `${cur.name}/${vname}/${name}`;
    perPixel(R.frames[key], R.stats[key]);
    log(key, JSON.stringify(R.frames[key]), 'coverage', R.stats[key].coverage);
  }
  // Whole frames (scene pass included) for the main configurations: the frame total, and the
  // marginal cost of water against a dry lake bed.
  R.stats[`${cur.name}/${vname}/dry`] = await stats(view, cfgOf('dry'));
  R.frames[`${cur.name}/${vname}/dry`] = await measure(view, cfgOf('dry'), frames);
  const dry = R.frames[`${cur.name}/${vname}/dry`].frame;
  for (const name of ['full', 'r2q2']) {
    const key = `${cur.name}/${vname}/${name}/whole`;
    R.frames[key] = await measure(view, cfgOf(name), frames, true);
    R.frames[key].marginal_vs_dry = r3(R.frames[key].frame - dry);
    log(key, JSON.stringify(R.frames[key]));
  }
}

async function qualitySuite(vname) {
  const view = VIEWS[vname];
  const q = R.quality[vname] = {};
  const refs = {};
  for (const rn of ['ref', 'ref_low', 'ref_high']) {
    refs[rn] = await grab(view, cfgOf(rn));
    q[`${rn}_frame_ms_incl_readback`] = r3(refs[rn].ms);
    R.stats[`${cur.name}/${vname}/${rn}`] = await stats(view, cfgOf(rn));
    log(`${vname} ${rn}`, r3(refs[rn].ms), 'ms', JSON.stringify(R.stats[`${cur.name}/${vname}/${rn}`]));
  }
  const refBytes = Object.fromEntries(Object.entries(refs).map(([k, v]) => [k, bytes(v.lin)]));
  await putImage(`${vname}-ref`, refBytes.ref, cur.W, cur.H);
  for (const name of COMPARED) {
    const c = cfgOf(name);
    const g = await grab(view, c);
    const b = bytes(g.lin);
    q[name] = diff(b, refBytes[refFor(c)], refs[refFor(c)].mask);
    if (SNAPS.includes(name)) await putImage(`${vname}-${name}`, b, cur.W, cur.H);
    if (name === 'full' || name === 'r2q2' || name === 'r4q2') await putImage(`${vname}-${name}-diff`, b, cur.W, cur.H, refBytes.ref);
    log(`${vname} quality ${name}`, JSON.stringify(q[name]));
  }
  // Filtering: the fast path and the reference against a 9-sample supersampled reference.
  const ss = await supersample(view, cfgOf('ref'));
  const ssb = bytes(ss.lin);
  await putImage(`${vname}-ss`, ssb, cur.W, cur.H);
  q.ss_ms_incl_readback = r3(ss.ms);
  q.full_vs_ss = diff(bytes((await grab(view, cfgOf('full'))).lin), ssb, ss.mask);
  q.ref_vs_ss = diff(refBytes.ref, ssb, ss.mask);
  log(`${vname} filtering`, JSON.stringify({ full_vs_ss: q.full_vs_ss, ref_vs_ss: q.ref_vs_ss }));
  // Heat maps.
  for (const [v, nm] of [[1, 'heat-reflect'], [2, 'heat-refract'], [3, 'heat-scene']]) {
    await draw(view, { ...cfgOf('full'), view: v });
    await snapshot(`${vname}-${nm}`);
  }
}

async function coverageSweep(frames) {
  const base = VIEWS.shore;
  const pts = [];
  for (const pitch of COVERAGE_PITCH) {
    const view = { ...base, pitch };
    const pt = { pitch };
    for (const name of COVERAGE_CFGS) {
      const c = cfgOf(name);
      const s = await stats(view, c);
      const m = await measure(view, c, frames);
      pt.coverage = s.coverage;
      pt[name] = { water: m.water, water_p95: m.water_p95, water_surface: m.water_surface, gbuf: m.gbuf, reflect: m.reflect, refract: m.refract, composite: m.composite, scene: m.scene, refl_steps_per_ray: s.refl_steps_per_ray };
    }
    log('coverage', JSON.stringify(pt));
    pts.push(pt);
    if (pitch === COVERAGE_PITCH[0] || pitch === COVERAGE_PITCH[COVERAGE_PITCH.length - 1]) {
      await draw(view, cfgOf('full'));
      await snapshot(`coverage-pitch${pitch}`);
    }
  }
  R.coverage.points = pts;
  R.coverage.fits = Object.fromEntries(COVERAGE_CFGS.map(n => [n, fit(pts.map(p => p.coverage), pts.map(p => p[n].water))]));
  log('coverage fits', JSON.stringify(R.coverage.fits));
}

/** The pre-registered verdict (README): at 30% coverage, the cheapest configuration that's correct. */
function analyse() {
  const bar = q => q && q.water_mean_abs_255 <= 1.0 && q.water_share_over_8 <= 0.01;
  const caps = key => { const s = R.stats[key]; return s ? s.secondary_cap_share_of_water <= 0.001 : false; };
  const out = { bar: 'water pixels: mean ≤ 1.0/255, ≤ 1% over 8/255; secondary-ray caps ≤ 0.1% of water pixels' };
  for (const vname of Object.keys(VIEWS)) {
    out[vname] = {};
    for (const name of COMPARED) {
      const q = R.quality[vname]?.[name], f = R.frames[`1080p/${vname}/${name}`];
      out[vname][name] = { water_ms: f?.water, correct: !!(bar(q) && caps(`1080p/${vname}/${name}`)), water_mean: q?.water_mean_abs_255, water_over8: q?.water_share_over_8 };
    }
  }
  if (R.coverage.fits) {
    out.at30 = {};
    for (const n of COVERAGE_CFGS) {
      const ms = R.coverage.fits[n].at30;
      out.at30[n] = { water_ms: ms, verdict: ms <= 1.0 ? 'pass' : ms <= 2.0 ? 'inconclusive' : 'fail', correct_in_shore_view: out.shore[n]?.correct };
    }
  }
  return out;
}

async function runAll() {
  $('run').disabled = true;
  const t0 = performance.now();
  try {
    await init();
    log('device', JSON.stringify(R.device));
    await place();
    // Pipelines are created lazily, six at a time (one configuration), not all variants at once:
    // dozens of concurrent Metal compiles helped starve the display on 2026-10-01.
    R.pipelines.cold_one_config = await coldPipelines();
    R.pipelines.per_frame = '6 compute (water_surface, scene, water_gbuf, reflect, refract, composite) + blit';
    log('pipelines', JSON.stringify(R.pipelines));

    cur = resources('1080p');
    for (const vname of Object.keys(VIEWS)) {
      await viewSuite(vname, 90);
      await qualitySuite(vname);
    }
    await coverageSweep(90);
    R.paced['1080p/shore/full'] = await paced(VIEWS.shore, cfgOf('full'));
    R.paced['1080p/shore/r2q2'] = await paced(VIEWS.shore, cfgOf('r2q2'));
    log('paced', JSON.stringify(R.paced));

    cur = resources('540p');
    for (const vname of Object.keys(VIEWS)) {
      const names = ['full', 'r2', 'r2q2'];
      for (const name of names) R.stats[`540p/${vname}/${name}`] = await stats(VIEWS[vname], cfgOf(name));
      await timeRounds(VIEWS[vname], names, 90, n => `540p/${vname}/${n}`);
      R.frames[`540p/${vname}/dry`] = await measure(VIEWS[vname], cfgOf('dry'), 90);
      R.frames[`540p/${vname}/full/whole`] = await measure(VIEWS[vname], cfgOf('full'), 90, true);
      for (const name of [...names, 'dry', 'full/whole']) log(`540p/${vname}/${name}`, JSON.stringify(R.frames[`540p/${vname}/${name}`]));
      await draw(VIEWS[vname], cfgOf('full'));
      await snapshot(`${vname}-540p`);
    }
    R.pipelines.created = pipeCache.size;
    R.analysis = analyse();
    log('analysis', JSON.stringify(R.analysis));
    R.finished = new Date().toISOString();
    R.run_seconds = r3((performance.now() - t0) / 1000);
    log('gpu safety', JSON.stringify(R.gpu_safety));
    await save(`run-${R.started.replace(/[:.]/g, '-')}.json`, JSON.stringify(R, null, 2));
    log('done in', R.run_seconds, 's');
  } catch (e) {
    fail('FAILED:', e.stack || e.message);
    R.error = String(e.stack || e.message);
    await save(`run-${R.started.replace(/[:.]/g, '-')}-failed.json`, JSON.stringify(R, null, 2));
  }
  await save('DONE', R.error ? 'failed' : 'ok');
  $('run').disabled = false;
}

/** Renders each view once and saves screenshots; logs stats and a short timing as a smoke test. */
async function quick() {
  try {
    await init();
    await place();
    cur = resources('1080p');
    for (const vname of Object.keys(VIEWS)) {
      const c = cfgOf('full');
      await draw(VIEWS[vname], c);
      await snapshot(`quick-${vname}`);
      log(`${vname} stats`, JSON.stringify(await stats(VIEWS[vname], c)));
      log(`${vname} timing (30+30 frames, indicative)`, JSON.stringify(await measure(VIEWS[vname], c, 30)));
    }
    const extra = (location.hash.split(':')[1] || '').split(',').filter(Boolean);
    for (const x of extra) {
      const [vname, cname, v] = x.split('/');
      const view = VIEWS[vname] || DEV_VIEWS[vname];
      await draw(view, { ...cfgOf(cname), view: Number(v || 0) });
      await snapshot(`quick-${vname}-${cname}${v ? '-v' + v : ''}`);
      if (!v) log(`${vname}/${cname} stats`, JSON.stringify(await stats(view, cfgOf(cname))));
    }
    log('quick done');
  } catch (e) {
    fail('FAILED:', e.stack || e.message);
  }
  window.__quick = { VIEWS, cfgOf, draw, snapshot, stats, measure, grab, diff, bytes };
  await save('DONE', 'quick');
}

/** Development: times the given configurations in both views (indicative), with stats. */
async function perf() {
  try {
    await init();
    await place();
    cur = resources('1080p');
    const names = (location.hash.split(':')[1] || 'full,r2q2,dry').split(',');
    for (const vname of Object.keys(VIEWS)) {
      const best = {};
      for (let round = 0; round < 3; round++) {
        for (const name of names) {
          const m = await measure(VIEWS[vname], cfgOf(name), 60);
          if (!best[name] || m.water < best[name].water) best[name] = m;
        }
      }
      for (const name of names) {
        const s = await stats(VIEWS[vname], cfgOf(name));
        const m = best[name];
        log(`${vname}/${name}`, `water ${m.water} ws ${m.water_surface} gb ${m.gbuf} refl ${m.reflect} refr ${m.refract} comp ${m.composite}`,
          '| cov', s.coverage, 'refl steps', s.refl_steps_per_ray, 'cells', s.refl_cells_per_ray, 'refr steps', s.refr_steps_per_ray, 'caps', s.secondary_cap_share_of_water);
      }
    }
    log('perf done');
  } catch (e) {
    fail('FAILED:', e.stack || e.message);
  }
  await save('DONE', 'perf');
}

/** Development: one view's reference against a few fast paths, with diff images. */
async function refcheck() {
  try {
    await init();
    await place();
    cur = resources('1080p');
    const args = (location.hash.split(':')[1] || 'shore').split(',');
    const view = VIEWS[args[0]];
    const ref = await grab(view, cfgOf('ref'));
    const rb = bytes(ref.lin);
    log('ref ms', r3(ref.ms), 'max submission', JSON.stringify(R.gpu_safety));
    await putImage(`check-${view.name}-ref`, rb, cur.W, cur.H);
    for (const arg of args.slice(1)) {
      const [name, vs] = arg.split('~');
      const g = await grab(view, cfgOf(name));
      const b = bytes(g.lin);
      if (vs) {
        const r2 = await grab(view, cfgOf(vs));
        log(`${name} vs ${vs}`, JSON.stringify(diff(b, bytes(r2.lin), r2.mask)));
        continue;
      }
      log(name, JSON.stringify(diff(b, rb, ref.mask)));
      await putImage(`check-${view.name}-${name}-diff`, b, cur.W, cur.H, rb);
    }
    const s = await stats(view, cfgOf('ref'));
    log('ref stats', JSON.stringify(s));
    log('refcheck done', JSON.stringify(R.gpu_safety));
  } catch (e) {
    fail('FAILED:', e.stack || e.message);
  }
  await save('DONE', 'refcheck');
}

$('run').addEventListener('click', runAll);
$('quick').addEventListener('click', quick);
if (location.hash === '#run') runAll();
if (location.hash.startsWith('#quick')) quick();
if (location.hash.startsWith('#perf')) perf();
if (location.hash.startsWith('#refcheck')) refcheck();
