// Spike 04 harness: the clipmap manager (the engine's role), pipelines, views, timing, and the
// reference comparison. It isn't compiler output.

const FOVY = 45 * Math.PI / 180;
const U = GPUBufferUsage, TU = GPUTextureUsage;
const FULL = [1920, 1080], HALF = [960, 540];
const Q = 12;                       // timestamps per frame: cook, hcook, geometry, light, sky, deferred normal
const FRAME_BYTES = 160 + 14 * 48;
const RF_BYTES = 96 + 16 * 48;
const INF = 1e30;
const MAX_BOXES = 64;
const SLOTS_XY = 64;                // atlas slots per row and per column (576 texels)

// Constants mirrored from field.wgsl, for the per-level error bounds.
const T_K = 0.3162278, T_AMP_MAX = 960, T_L0 = 2000, T_ROUGH = 0.08, R_K = 0.5555556;
const ROCK_KINDS = [{ rmin: 1.2, rmax: 5.0, amp: 0.16, crag: false }, { rmin: 16, rmax: 40, amp: 0.12, crag: true }];

const SUN = (() => { const az = 75 * Math.PI / 180, el = 14 * Math.PI / 180; return [Math.cos(el) * Math.sin(az), Math.sin(el), Math.cos(el) * Math.cos(az)]; })();

const MARCH = {
  analytic: { MODE: 0, MAX_STEPS: 1500 },
  cache: { MODE: 1 },
  cache_norelax: { MODE: 1, RELAX: 1.0 },
  cache_fn: { MODE: 1, FIELD_NORMALS: 1 },
  hybrid: { MODE: 2, MAX_STEPS: 2000 },
  ref: { MODE: 0, STEP_SCALE: 0.5, EPS_SCALE: 0.05, MAX_STEPS: 6000 },
  reftol: { MODE: 0, STEP_SCALE: 0.5, EPS_SCALE: 0.25, MAX_STEPS: 6000 },
};
const STAT_NAMES = ['hit', 'sky', 'steps', 'field_evals', 'empty_steps', 'caps', 'out', 'stone', 'sh_steps', 'sh_rays'];

const $ = id => document.getElementById(id);
const log = (...a) => { $('log').textContent += a.join(' ') + '\n'; console.log(...a); };
const fail = (...a) => { $('log').textContent += a.join(' ') + '\n'; console.error(...a); };
const R = { started: new Date().toISOString(), device: {}, pipelines: {}, config: {}, world: {}, views: {}, teleport: {}, frames: {}, stats: {}, quality: {}, precision: {}, shimmer: {}, paced: {}, sweeps: {}, memory_mb: {} };
window.__results = R;

let device, ctx, canvasFormat;
const src = {}, L = {}, P = {}, B = {};
const TG = {};                      // render targets per resolution

const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[s.length >> 1] : NaN; };
const pct = (a, q) => { const s = [...a].sort((x, y) => x - y); return s.length ? s[Math.min(s.length - 1, Math.floor(s.length * q))] : NaN; };
const r3 = x => Math.round(x * 1000) / 1000;
const norm = a => { const l = Math.hypot(...a); return a.map(x => x / l); };
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sub = (a, b) => a.map((x, i) => x - b[i]);
const add = (a, b) => a.map((x, i) => x + b[i]);
const mul = (a, s) => a.map(x => x * s);
const dot = (a, b) => a.reduce((s, x, i) => s + x * b[i], 0);
const smoothstep = (a, b, x) => { const t = Math.min(Math.max((x - a) / (b - a), 0), 1); return t * t * (3 - 2 * t); };

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
    if (!r.ok) fail('save failed', name, r.status);
  } catch (e) { fail('save failed', name, e.message); }
}

// ---- setup -------------------------------------------------------------------------------------

async function init() {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
  const need = ['timestamp-query', 'texture-formats-tier1', 'float32-filterable'];
  const missing = need.filter(f => !adapter.features.has(f));
  if (missing.length) throw new Error(`missing features: ${missing}`);
  device = await adapter.requestDevice({
    requiredFeatures: need,
    requiredLimits: {
      maxStorageBufferBindingSize: adapter.limits.maxStorageBufferBindingSize,
      maxBufferSize: adapter.limits.maxBufferSize,
      maxStorageBuffersPerShaderStage: Math.min(10, adapter.limits.maxStorageBuffersPerShaderStage),
    },
  });
  device.lost.then(i => fail('DEVICE LOST:', i.message));
  device.addEventListener('uncapturederror', e => fail('GPU ERROR:', e.error.message));
  const info = adapter.info || {};
  R.device = {
    vendor: info.vendor, architecture: info.architecture, description: info.description,
    userAgent: navigator.userAgent, pageVisibility: document.visibilityState,
  };
  for (const f of ['common', 'field', 'cache', 'march', 'light', 'sky', 'cook', 'raster', 'tools', 'blit', 'defer']) {
    src[f] = await (await fetch(`${f}.wgsl`)).text();
  }
  const canvas = $('view');
  canvas.width = FULL[0];
  canvas.height = FULL[1];
  ctx = canvas.getContext('webgpu');
  canvasFormat = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format: canvasFormat, alphaMode: 'opaque' });

  const C = GPUShaderStage.COMPUTE, V = GPUShaderStage.VERTEX, Fr = GPUShaderStage.FRAGMENT;
  const buf = (binding, type, vis = C) => ({ binding, visibility: vis, buffer: { type } });
  const stex = (binding, format, dim = '2d') => ({ binding, visibility: C, storageTexture: { access: 'write-only', format, viewDimension: dim } });
  const tex = (binding, sampleType, dim = '2d', vis = C) => ({ binding, visibility: vis, texture: { sampleType, viewDimension: dim } });
  const bgl = entries => device.createBindGroupLayout({ entries });
  L.world = bgl([buf(0, 'uniform'), buf(1, 'read-only-storage'), tex(2, 'float', '3d'), { binding: 3, visibility: C, sampler: { type: 'filtering' } }, buf(4, 'read-only-storage'), buf(5, 'storage'), buf(6, 'read-only-storage')]);
  L.march = bgl([stex(0, 'rgba16float'), stex(1, 'r32float')]);
  L.light = bgl([tex(0, 'unfilterable-float'), tex(1, 'unfilterable-float'), stex(2, 'rgba16float')]);
  L.defer = bgl([tex(0, 'unfilterable-float'), stex(1, 'rgba16float')]);
  L.sky0 = bgl([buf(0, 'uniform')]);
  L.sky = bgl([tex(0, 'unfilterable-float'), tex(1, 'unfilterable-float'), stex(2, 'rgba8unorm')]);
  L.cook = bgl([buf(0, 'uniform'), buf(4, 'read-only-storage'), buf(6, 'read-only-storage'), buf(10, 'storage'), stex(11, 'r16float', '3d'), buf(12, 'read-only-storage'), buf(13, 'storage'), buf(14, 'storage'), buf(15, 'storage'), buf(16, 'storage'), buf(17, 'uniform'), buf(26, 'storage')]);
  L.args = bgl([buf(13, 'storage'), buf(17, 'uniform'), buf(18, 'storage')]);
  L.refine = bgl([buf(0, 'uniform'), buf(4, 'read-only-storage'), buf(6, 'read-only-storage'), buf(10, 'storage'), buf(13, 'storage'), buf(14, 'storage'), buf(15, 'storage'), buf(16, 'storage'), buf(17, 'uniform'), buf(26, 'storage')]);
  L.hcook = bgl([buf(0, 'uniform'), buf(12, 'read-only-storage'), buf(17, 'uniform'), stex(19, 'r32float', '2d-array'), stex(20, 'rgba16float', '2d-array')]);
  L.raster = bgl([buf(21, 'uniform', V | Fr), tex(22, 'unfilterable-float', '2d-array', V), tex(23, 'float', '2d-array', Fr), { binding: 24, visibility: Fr, sampler: { type: 'filtering' } }, buf(25, 'read-only-storage', V)]);
  L.tools0 = bgl([buf(0, 'uniform'), buf(4, 'read-only-storage'), buf(6, 'read-only-storage')]);
  L.place = bgl([buf(0, 'uniform'), buf(4, 'read-only-storage'), buf(27, 'storage')]);
  L.tools1 = bgl([buf(0, 'read-only-storage'), buf(1, 'storage'), stex(2, 'rgba8unorm')]);
  L.blit = bgl([{ binding: 0, visibility: Fr, texture: {} }, { binding: 1, visibility: Fr, sampler: {} }]);
}

const salted = (code, salt) => salt == null ? code : code.replace('const SALT: f32 = 0.0;', `const SALT: f32 = ${salt.toFixed(4)};`);

/** A module from several files; compilation messages are mapped back to file:line. */
async function module(files, label, salt) {
  const parts = files.map(f => src[f]);
  const code = salted(parts.join('\n'), salt);
  const m = device.createShaderModule({ code, label });
  const info = await m.getCompilationInfo();
  const starts = [];
  let line = 1;
  for (let i = 0; i < files.length; i++) { starts.push(line); line += parts[i].split('\n').length; }
  for (const msg of info.messages) {
    let fi = 0;
    while (fi + 1 < starts.length && msg.lineNum >= starts[fi + 1]) fi++;
    const where = `${files[fi]}.wgsl:${msg.lineNum - starts[fi] + 1}:${msg.linePos}`;
    (msg.type === 'error' ? fail : log)(`[${label}] ${msg.type} ${where} ${msg.message}`);
  }
  if (info.messages.some(m => m.type === 'error')) throw new Error(`${label}: WGSL errors`);
  return m;
}

const MODS = {
  march: ['common', 'field', 'cache', 'march'],
  light: ['common', 'field', 'cache', 'light'],
  sky: ['common', 'sky'],
  cook: ['common', 'field', 'cook'],
  raster: ['common', 'field', 'raster'],
  tools: ['common', 'field', 'tools'],
  defer: ['common', 'field', 'cache', 'defer'],
};

async function pipelines() {
  const t0 = performance.now();
  const m = {};
  for (const [k, files] of Object.entries(MODS)) m[k] = await module(files, k);
  m.blit = await module(['blit'], 'blit');
  const pl = (...g) => device.createPipelineLayout({ bindGroupLayouts: g });
  const comp = (mod, entryPoint, layout, constants = {}) => device.createComputePipelineAsync({ layout, compute: { module: mod, entryPoint, constants } });
  const list = [];
  for (const [name, c] of Object.entries(MARCH)) {
    for (const t of [false, true]) {
      for (const s of [false, true]) {
        const consts = { ...c, ...(t ? { PARTS: 1 } : {}), ...(s ? { STATS: 1 } : {}) };
        list.push([`m_${name}${t ? '_t' : ''}${s ? '+s' : ''}`, () => comp(m.march, 'march', pl(L.world, L.march), consts)]);
      }
    }
  }
  list.push(['light', () => comp(m.light, 'light', pl(L.world, L.light))]);
  list.push(['light+s', () => comp(m.light, 'light', pl(L.world, L.light), { LSTATS: 1 })]);
  list.push(['light_nosh', () => comp(m.light, 'light', pl(L.world, L.light), { SHADOWS: 0 })]);
  list.push(['sky', () => comp(m.sky, 'sky', pl(L.sky0, L.sky))]);
  for (const t of [false, true]) {
    const c = t ? { PARTS: 1 } : {};
    list.push([`classify${t ? '_t' : ''}`, () => comp(m.cook, 'classify', pl(L.cook), c)]);
    list.push([`refine${t ? '_t' : ''}`, () => comp(m.cook, 'refine', pl(L.refine), c)]);
    list.push([`cook${t ? '_t' : ''}`, () => comp(m.cook, 'cook', pl(L.cook), c)]);
    list.push([`cook${t ? '_t' : ''}_strict`, () => comp(m.cook, 'cook', pl(L.cook), { ...c, NORMALIZE: 0 })]);
  }
  list.push(['allocate', () => comp(m.cook, 'allocate', pl(L.cook))]);
  list.push(['args_refine', () => comp(m.cook, 'args_refine', pl(L.args))]);
  list.push(['args_alloc', () => comp(m.cook, 'args_alloc', pl(L.args))]);
  list.push(['args_cook', () => comp(m.cook, 'args_cook', pl(L.args))]);
  list.push(['hcook', () => comp(m.cook, 'hcook', pl(L.hcook))]);
  list.push(['place_rocks', () => comp(m.cook, 'place_rocks', pl(L.place))]);
  list.push(['probe', () => comp(m.tools, 'probe', pl(L.tools0, L.tools1))]);
  list.push(['map', () => comp(m.tools, 'map', pl(L.tools0, L.tools1))]);
  // Raster terrain: back faces culled (the grid's triangles wind clockwise seen from above);
  // raster_nocull is the ablation; raster_fn shades with the field's normal per pixel.
  const rp = (fs, cullMode) => () => device.createRenderPipelineAsync({
    layout: pl(L.raster),
    vertex: { module: m.raster, entryPoint: 'rvs' },
    fragment: { module: m.raster, entryPoint: fs, targets: [{ format: 'rgba16float' }, { format: 'r32float' }] },
    primitive: { topology: 'triangle-list', cullMode, frontFace: 'cw' },
    depthStencil: { format: 'depth32float', depthCompare: 'greater', depthWriteEnabled: true },
  });
  list.push(['raster', rp('rfs', 'back')]);
  list.push(['raster_nocull', rp('rfs', 'none')]);
  list.push(['raster_fn', rp('rfs_fn', 'back')]);
  list.push(['defnormal', () => comp(m.defer, 'defnormal', pl(L.world, L.defer))]);
  list.push(['blit', () => device.createRenderPipelineAsync({
    layout: pl(L.blit),
    vertex: { module: m.blit, entryPoint: 'blit_vs' },
    fragment: { module: m.blit, entryPoint: 'blit_fs', targets: [{ format: canvasFormat }] },
  })]);
  // GPU safety: a few at a time, so the system doesn't spawn dozens of shader compilers at once.
  for (let i = 0; i < list.length; i += 4) {
    const batch = list.slice(i, i + 4);
    const ps = await Promise.all(batch.map(([, f]) => f()));
    batch.forEach(([n], k) => { P[n] = ps[k]; });
  }
  P.raster_dn = P.raster;   // raster visibility; the deferred pass (defnormal) replaces its normals
  R.pipelines.all_batched_ms = r3(performance.now() - t0);
  R.pipelines.count_total = list.length;
}

/** Cold and warm creation of one scene's pipelines (the cache path): unique source per run. */
async function pipelineTiming() {
  const pl = (...g) => device.createPipelineLayout({ bindGroupLayouts: g });
  const once = async salt => {
    const t0 = performance.now();
    const mm = await module(MODS.march, 'march-salted', salt);
    const ml = await module(MODS.light, 'light-salted', salt);
    const ms = await module(MODS.sky, 'sky-salted', salt);
    const mc = await module(MODS.cook, 'cook-salted', salt);
    const comp = (mod, ep, layout, c = {}) => device.createComputePipelineAsync({ layout, compute: { module: mod, entryPoint: ep, constants: c } });
    const ps = [
      comp(mm, 'march', pl(L.world, L.march), MARCH.hybrid), comp(ml, 'light', pl(L.world, L.light)), comp(ms, 'sky', pl(L.sky0, L.sky)),
      comp(mc, 'classify', pl(L.cook)), comp(mc, 'allocate', pl(L.cook)), comp(mc, 'cook', pl(L.cook)),
      comp(mc, 'args_alloc', pl(L.args)), comp(mc, 'args_cook', pl(L.args)),
    ];
    await Promise.all(ps);    // 8 at once: the scene's own set, as a loading screen would create it
    return r3(performance.now() - t0);
  };
  const salt = 1000 + Math.random() * 1000;
  R.pipelines.scene_set = 'march (hybrid), light, sky, classify, allocate, cook, 2 args (+ blit) = 9';
  R.pipelines.scene_cold_parallel_ms = await once(salt);
  R.pipelines.scene_warm_parallel_ms = await once(salt);
}

function makeTargets([w, h]) {
  const t = { w, h };
  const mk = (format, usage) => device.createTexture({ size: [w, h], format, usage });
  t.gbuf = mk('rgba16float', TU.STORAGE_BINDING | TU.TEXTURE_BINDING | TU.RENDER_ATTACHMENT);
  t.gdepth = mk('r32float', TU.STORAGE_BINDING | TU.TEXTURE_BINDING | TU.RENDER_ATTACHMENT | TU.COPY_SRC);
  t.hdr = mk('rgba16float', TU.STORAGE_BINDING | TU.TEXTURE_BINDING);
  t.final = mk('rgba8unorm', TU.STORAGE_BINDING | TU.TEXTURE_BINDING | TU.COPY_SRC);
  t.zbuf = mk('depth32float', TU.RENDER_ATTACHMENT);
  const v = k => t[k].createView();
  t.v = { gbuf: v('gbuf'), gdepth: v('gdepth'), hdr: v('hdr'), final: v('final'), zbuf: v('zbuf') };
  t.marchBG = device.createBindGroup({ layout: L.march, entries: [{ binding: 0, resource: t.v.gbuf }, { binding: 1, resource: t.v.gdepth }] });
  t.lightBG = device.createBindGroup({ layout: L.light, entries: [{ binding: 0, resource: t.v.gbuf }, { binding: 1, resource: t.v.gdepth }, { binding: 2, resource: t.v.hdr }] });
  t.deferBG = device.createBindGroup({ layout: L.defer, entries: [{ binding: 0, resource: t.v.gdepth }, { binding: 1, resource: t.v.gbuf }] });
  t.skyBG = device.createBindGroup({ layout: L.sky, entries: [{ binding: 0, resource: t.v.gdepth }, { binding: 1, resource: t.v.hdr }, { binding: 2, resource: t.v.final }] });
  t.blitBG = device.createBindGroup({ layout: L.blit, entries: [{ binding: 0, resource: t.v.final }, { binding: 1, resource: device.createSampler({ magFilter: 'linear', minFilter: 'linear' }) }] });
  return t;
}

function globalResources() {
  B.frame = buffer(FRAME_BYTES, U.UNIFORM | U.COPY_DST);
  B.frameH = buffer(FRAME_BYTES, U.UNIFORM | U.COPY_DST);
  B.stats = buffer(256, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  B.ruins = buffer(1024 * 16, U.STORAGE | U.COPY_DST);
  B.rockbase = buffer((172 * 172 + 36 * 36) * 4, U.STORAGE);
  B.sampler = device.createSampler({ magFilter: 'linear', minFilter: 'linear', addressModeU: 'clamp-to-edge', addressModeV: 'clamp-to-edge', addressModeW: 'clamp-to-edge' });
  B.skyBG0 = device.createBindGroup({ layout: L.sky0, entries: [{ binding: 0, resource: { buffer: B.frame } }] });
  B.tools0 = device.createBindGroup({ layout: L.tools0, entries: [{ binding: 0, resource: { buffer: B.frame } }, { binding: 4, resource: { buffer: B.ruins } }, { binding: 6, resource: { buffer: B.rockbase } }] });
  TG.full = makeTargets(FULL);
  TG.half = makeTargets(HALF);
}

// ---- the clipmap (CPU side: the engine's job) --------------------------------------------------------

function levelsFor(N, c0, V) {
  let l = 0;
  while ((N / 2 - 1) * 8 * c0 * 2 ** l < V) l++;
  return l + 1;
}

/** Amplitude of the octaves a cook at this cell size leaves out (mirrors field.wgsl). */
function omittedAmp(cell) {
  const w = (fp, wl) => 1 - smoothstep(0.125, 0.25, 0.5 * fp / wl);   // octave_weight at the cook's footprint, cell / 2
  let at = 0, b = 1, wl = T_L0;
  for (let i = 0; i < 15; i++) {
    if (w(cell, wl) < 1) at += Math.max(b * T_AMP_MAX, T_ROUGH * wl);
    b *= i < 3 ? 0.5 : 0.42;
    wl *= 0.5;
  }
  let worst = at * T_K;
  for (const k of ROCK_KINDS) {
    for (let i = 0; i <= 8; i++) {
      const s = k.rmin + (k.rmax - k.rmin) * i / 8;
      let a = 0.5, wl2 = 0.6 * s, sum = 0;
      for (let o = 0; o < 7; o++) {
        if (wl2 < 0.1) break;
        if (w(cell, wl2) < 1) sum += a;
        a *= 0.5;
        wl2 *= 0.5;
      }
      let amp = k.amp * s * sum;
      if (k.crag && w(cell, 3.1) < 1) amp += 0.25 * k.amp * s;
      worst = Math.max(worst, amp * R_K);
    }
  }
  return worst;
}

class Clipmap {
  constructor({ N, c0, V, parts = 7, capK = 6, strict = false }) {
    this.strict = strict;
    this.N = N;
    this.c0 = c0;
    this.V = V;
    this.parts = parts;
    this.nlev = levelsFor(N, c0, V);
    if (this.nlev > 14) throw new Error('too many levels');
    if (N & (N - 1)) throw new Error('N must be a power of two');
    this.org = new Array(this.nlev).fill(null);
    this.C = Math.ceil(capK * N * N * this.nlev / 4096) * 4096;
    this.SZ = this.C / 4096;
    if (9 * this.SZ > 2048) throw new Error(`atlas too deep: ${this.SZ} layers of slots`);
    this.A = [];
    this.delta = [];
    for (let l = 0; l < this.nlev; l++) {
      const cell = this.cell(l);
      this.A.push(omittedAmp(cell));
      this.delta.push(Math.sqrt(3) * cell + this.A[l] + 0.01 * 8 * cell);
    }
    this.lagged = 0;
  }
  cell(l) { return this.c0 * 2 ** l; }
  b(l) { return this.c0 * 8 * 2 ** l; }
  halfdiag(l) { return Math.sqrt(3) * 0.5 * this.b(l); }
  target(l, cam) { const b = this.b(l); return cam.map(c => Math.floor(c / b) - this.N / 2); }
  label() { return `N${this.N}-c${this.c0}-V${this.V / 1000}km${this.parts === 7 ? '' : '-terrain'}${this.strict ? '-strict' : ''}`; }

  /** Boxes of bricks to classify so every level is centred on cam. Levels past the budget wait. */
  plan(cam, budget = Infinity) {
    const N = this.N, boxes = [];
    let jobs = 0;
    for (let l = 0; l < this.nlev; l++) {
      const to = this.target(l, cam), from = this.org[l], lb = [];
      if (!from || to.some((t, a) => Math.abs(t - from[a]) >= N)) {
        lb.push({ l, min: to, size: [N, N, N] });
      } else {
        const cur = from.slice();
        for (let a = 0; a < 3; a++) {
          const d = to[a] - cur[a];
          if (!d) continue;
          const min = cur.slice(), size = [N, N, N];
          if (d > 0) { min[a] = cur[a] + N; size[a] = d; } else { min[a] = to[a]; size[a] = -d; }
          lb.push({ l, min, size });
          cur[a] = to[a];
        }
      }
      const n = lb.reduce((s, b) => s + b.size[0] * b.size[1] * b.size[2], 0);
      if (n && jobs > 0 && jobs + n > budget) { this.lagged++; continue; }
      boxes.push(...lb);
      jobs += n;
      this.org[l] = to;
    }
    if (boxes.length > MAX_BOXES) throw new Error('too many boxes');
    return { boxes, jobs };
  }

  gpu() {
    const N = this.N;
    this.page = buffer(N * N * N * this.nlev * 4, U.STORAGE);
    this.atlas = device.createTexture({ size: [9 * SLOTS_XY, 9 * SLOTS_XY, 9 * this.SZ], dimension: '3d', format: 'r16float', usage: TU.STORAGE_BINDING | TU.TEXTURE_BINDING });
    const free = new Uint32Array(this.C);
    for (let i = 0; i < this.C; i++) free[i] = this.C - 1 - i;
    this.free = buffer(this.C * 4, U.STORAGE, free);
    this.need = buffer(this.C * 16, U.STORAGE);
    this.cookl = buffer(this.C * 16, U.STORAGE);
    this.counters = buffer(64, U.STORAGE | U.COPY_SRC | U.COPY_DST, new Int32Array([0, 0, 0, 0, 0, 0, 0, this.C, 0, 0, 0, 0, 0, 0, 0, 0]));
    this.args = buffer(48, U.STORAGE | U.INDIRECT);
    this.cand = buffer(2 * this.C * 16, U.STORAGE);
    this.boxes = buffer(MAX_BOXES * 32, U.STORAGE | U.COPY_DST);
    this.cu = buffer(16, U.UNIFORM | U.COPY_DST);
    this.history = buffer(512 * 32, U.STORAGE | U.COPY_SRC | U.COPY_DST);
    const r = b => ({ resource: { buffer: b } });
    const av = this.atlas.createView();
    this.cookBG = device.createBindGroup({
      layout: L.cook,
      entries: [{ binding: 0, ...r(B.frame) }, { binding: 4, ...r(B.ruins) }, { binding: 6, ...r(B.rockbase) }, { binding: 10, ...r(this.page) }, { binding: 11, resource: av },
        { binding: 12, ...r(this.boxes) }, { binding: 13, ...r(this.counters) }, { binding: 14, ...r(this.free) }, { binding: 15, ...r(this.need) },
        { binding: 16, ...r(this.cookl) }, { binding: 17, ...r(this.cu) }, { binding: 26, ...r(this.cand) }],
    });
    this.refineBG = device.createBindGroup({
      layout: L.refine,
      entries: [{ binding: 0, ...r(B.frame) }, { binding: 4, ...r(B.ruins) }, { binding: 6, ...r(B.rockbase) }, { binding: 10, ...r(this.page) }, { binding: 13, ...r(this.counters) },
        { binding: 14, ...r(this.free) }, { binding: 15, ...r(this.need) }, { binding: 16, ...r(this.cookl) }, { binding: 17, ...r(this.cu) }, { binding: 26, ...r(this.cand) }],
    });
    this.argsBG = device.createBindGroup({ layout: L.args, entries: [{ binding: 13, ...r(this.counters) }, { binding: 17, ...r(this.cu) }, { binding: 18, ...r(this.args) }] });
    this.worldBG = device.createBindGroup({
      layout: L.world,
      entries: [{ binding: 0, ...r(B.frame) }, { binding: 1, ...r(this.page) }, { binding: 2, resource: av }, { binding: 3, resource: B.sampler },
        { binding: 4, ...r(B.ruins) }, { binding: 5, ...r(B.stats) }, { binding: 6, ...r(B.rockbase) }],
    });
    this.mem = {
      atlas: (9 * SLOTS_XY) ** 2 * 9 * this.SZ * 2,
      page: N * N * N * this.nlev * 4,
      lists: this.C * (4 + 16 + 16 + 32),
    };
    return this;
  }

  /** GPU safety: WebGPU zero-fills a new texture lazily, on its first use. A trivial dispatch that
   *  binds the atlas puts that fill (hundreds of MB) in a submit of its own. */
  async touch() {
    device.queue.writeBuffer(this.cu, 0, new Uint32Array([0, 1, this.C, 512]));
    const enc = device.createCommandEncoder();
    const p = enc.beginComputePass();
    p.setPipeline(P.classify);
    p.setBindGroup(0, this.cookBG);
    p.dispatchWorkgroups(1);
    p.end();
    const t0 = performance.now();
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
    this.touchMs = r3(performance.now() - t0);
  }

  destroy() {
    for (const k of ['page', 'free', 'need', 'cookl', 'cand', 'counters', 'args', 'boxes', 'cu', 'history']) this[k].destroy();
    this.atlas.destroy();
  }

  memMB() { return r3((this.mem.atlas + this.mem.page + this.mem.lists) / 2 ** 20); }
}

/** The raster comparison's heightfield clipmap: levels of M×M quads, texture period T. */
class HeightClipmap {
  constructor({ N, c0, V }) {
    this.M = 8 * N;
    this.T = this.M + 4;
    this.s0 = c0;
    let l = 0;
    while ((this.M / 2 - 2) * c0 * 2 ** l < V) l++;
    this.nlev = l + 1;
    if (this.nlev > 16) throw new Error('too many raster levels');
    this.org = new Array(this.nlev).fill(null);
  }
  s(l) { return this.s0 * 2 ** l; }
  target(l, cam) { const s = this.s(l); return [cam[0], cam[2]].map(c => 2 * Math.floor(c / (2 * s)) - this.M / 2); }

  plan(cam) {
    const T = this.T, boxes = [];
    let jobs = 0;
    for (let l = 0; l < this.nlev; l++) {
      const to = this.target(l, cam), from = this.org[l];
      const lb = [];
      if (!from || Math.abs(to[0] - from[0]) >= T || Math.abs(to[1] - from[1]) >= T) {
        lb.push({ l, min: [to[0] - 2, to[1] - 2], size: [T, T] });
      } else {
        const cur = from.slice();
        for (let a = 0; a < 2; a++) {
          const d = to[a] - cur[a];
          if (!d) continue;
          const min = [cur[0] - 2, cur[1] - 2], size = [T, T];
          if (d > 0) { min[a] = cur[a] - 2 + T; size[a] = d; } else { min[a] = to[a] - 2; size[a] = -d; }
          lb.push({ l, min, size });
          cur[a] = to[a];
        }
      }
      for (const b of lb) { boxes.push(b); jobs += b.size[0] * b.size[1]; }
      this.org[l] = to;
    }
    return { boxes, jobs };
  }

  gpu() {
    const T = this.T;
    this.hmap = device.createTexture({ size: [T, T, this.nlev], format: 'r32float', usage: TU.STORAGE_BINDING | TU.TEXTURE_BINDING });
    this.nmap = device.createTexture({ size: [T, T, this.nlev], format: 'rgba16float', usage: TU.STORAGE_BINDING | TU.TEXTURE_BINDING });
    this.boxes = buffer(MAX_BOXES * 32, U.STORAGE | U.COPY_DST);
    this.cu = buffer(16, U.UNIFORM | U.COPY_DST);
    this.rf = buffer(RF_BYTES, U.UNIFORM | U.COPY_DST);
    this.maxBlocks = this.nlev * (this.M / 32) ** 2;
    this.blocks = buffer(this.maxBlocks * 16, U.STORAGE | U.COPY_DST);
    const idx = new Uint16Array(32 * 32 * 6);
    let k = 0;
    for (let j = 0; j < 32; j++) for (let i = 0; i < 32; i++) {
      const a = j * 33 + i, b = a + 1, c = a + 34, d = a + 33;
      idx.set([a, b, c, a, c, d], k);
      k += 6;
    }
    this.index = buffer(idx.byteLength, U.INDEX, idx);
    const arr = t => t.createView({ dimension: '2d-array' });
    const r = b => ({ resource: { buffer: b } });
    this.cookBG = device.createBindGroup({
      layout: L.hcook,
      entries: [{ binding: 0, ...r(B.frameH) }, { binding: 12, ...r(this.boxes) }, { binding: 17, ...r(this.cu) }, { binding: 19, resource: arr(this.hmap) }, { binding: 20, resource: arr(this.nmap) }],
    });
    this.drawBG = device.createBindGroup({
      layout: L.raster,
      entries: [{ binding: 21, ...r(this.rf) }, { binding: 22, resource: arr(this.hmap) }, { binding: 23, resource: arr(this.nmap) },
        { binding: 24, resource: device.createSampler({ magFilter: 'linear', minFilter: 'linear', addressModeU: 'repeat', addressModeV: 'repeat' }) },
        { binding: 25, ...r(this.blocks) }],
    });
    this.mem = { textures: T * T * this.nlev * (4 + 8), mesh_index: idx.byteLength };
    return this;
  }

  destroy() { for (const k of ['boxes', 'cu', 'rf', 'blocks', 'index']) this[k].destroy(); this.hmap.destroy(); this.nmap.destroy(); }
}

// ---- views and cameras ------------------------------------------------------------------------------

const W = {};       // the world's placements, from probing the field

/** Probes the terrain: points are [x, y, z, fp]. Returns [h, slope, mask, world] per point. */
async function probe(points) {
  const n = points.length;
  const pin = buffer(n * 16, U.STORAGE, new Float32Array(points.flat()));
  const pout = buffer(n * 16, U.STORAGE | U.COPY_SRC);
  const dummy = device.createTexture({ size: [1, 1], format: 'rgba8unorm', usage: TU.STORAGE_BINDING });
  const bg = device.createBindGroup({ layout: L.tools1, entries: [{ binding: 0, resource: { buffer: pin } }, { binding: 1, resource: { buffer: pout } }, { binding: 2, resource: dummy.createView() }] });
  const enc = device.createCommandEncoder();
  const p = enc.beginComputePass();
  p.setPipeline(P.probe);
  p.setBindGroup(0, B.tools0);
  p.setBindGroup(1, bg);
  p.dispatchWorkgroups(Math.ceil(n / 64));
  p.end();
  device.queue.submit([enc.finish()]);
  const out = new Float32Array(await readback([{ src: pout, size: n * 16 }]));
  pin.destroy();
  pout.destroy();
  dummy.destroy();
  return Array.from({ length: n }, (_, i) => [out[4 * i], out[4 * i + 1], out[4 * i + 2], out[4 * i + 3]]);
}

/** The ruins, authored: [kind, x, z, yaw (deg), a, b, H, wall, broken]. Bases are probed. */
const RUIN_SITES = [
  // kind 0 tower: a = radius;  kind 1 keep: a, b = half-extents;  kind 2 wall: a = half-length
  [0, 750, -1545, 0, 4.5, 0, 20, 1.2, 0],       // the tower whose top is the second view
  [1, 450, -1788, 20, 9, 7, 13, 1.2, 1],        // a broken keep on the slope
  [2, 472, -1768, 110, 14, 0, 7, 1.4, 1],       // its curtain wall
  [0, 150, -2250, 0, 3.6, 0, 16, 1.0, 1],       // a broken tower near the hillside camera
  [0, -325, -1600, 0, 3.8, 0, 18, 1.0, 0.5],    // a damaged tower on the hill to the left
]

/** Cooks the rock scatter once: each cell's base height (an engine would place instances at load). */
async function placeRocks() {
  const bg = device.createBindGroup({ layout: L.place, entries: [{ binding: 0, resource: { buffer: B.frame } }, { binding: 4, resource: { buffer: B.ruins } }, { binding: 27, resource: { buffer: B.rockbase } }] });
  const qs = device.createQuerySet({ type: 'timestamp', count: 2 });
  const enc = device.createCommandEncoder();
  const p = enc.beginComputePass(tsw(qs, 0));
  p.setPipeline(P.place_rocks);
  p.setBindGroup(0, bg);
  p.dispatchWorkgroups(Math.ceil((172 * 172 + 36 * 36) / 64));
  p.end();
  device.queue.submit([enc.finish()]);
  const ts = await resolveTimestamps(qs, 2);
  R.world.rock_scatter_cook_ms = r3(Number(ts[1] - ts[0]) / 1e6);
  R.world.rock_cells = 172 * 172 + 36 * 36;
}

async function placeWorld(sites, views) {
  await placeRocks();
  const pts = [];
  for (const s of sites) {
    const r = s[0] === 0 ? s[4] : s[0] === 1 ? Math.hypot(s[4], s[5]) : s[4];
    pts.push([s[1], 0, s[2], 0.05]);
    for (let k = 0; k < 12; k++) { const a = k / 12 * 2 * Math.PI; pts.push([s[1] + r * Math.cos(a), 0, s[2] + r * Math.sin(a), 0.05]); }
  }
  const res = await probe(pts);
  const data = new Float32Array(4 + sites.length * 12);
  data[0] = sites.length;
  W.ruins = [];
  sites.forEach((s, i) => {
    const hs = res.slice(i * 13, i * 13 + 13).map(r => r[0]);
    const base = Math.min(...hs) - 0.8;
    const [kind, x, z, yaw, a, b, Hh, wall, brk] = s;
    const yr = yaw * Math.PI / 180;
    const bound = kind === 0 ? a + 1.0 : kind === 1 ? Math.hypot(a, b) + 1.5 : a + 1.5;
    data.set([x, base, z, kind, Math.cos(yr), Math.sin(yr), a, b, Hh + (Math.max(...hs) - base), wall, brk, bound], 4 + i * 12);
    W.ruins.push({ kind, x, z, base, H: Hh + (Math.max(...hs) - base), top: base + Hh + (Math.max(...hs) - base) });
  });
  device.queue.writeBuffer(B.ruins, 0, data);
  R.world.ruins = W.ruins;
}

/** A view: eye and target in content space, optionally a path in time. */
function view(name, eye, target, path = null, speed = 0) {
  return { name, eye, target, path, speed, shift: [0, 0, 0] };
}

function pose(sc, time) {
  if (sc.path) { const p = sc.path(time); sc.cur = { eye: p.eye, target: p.target }; }
  else sc.cur = { eye: sc.eye, target: sc.target };
  const f = norm(sub(sc.cur.target, sc.cur.eye));
  const r = norm(cross(f, [0, 1, 0]));
  sc.basis = { f, r, u: cross(r, f) };
  sc.camWorld = add(sc.cur.eye, sc.shift);
}

function writeFrame(sc, cm, [w, h], opts = {}) {
  const ab = new ArrayBuffer(FRAME_BYTES), f = new Float32Array(ab), i32 = new Int32Array(ab), u = new Uint32Array(ab);
  const cam = sc.camWorld;
  const ty = Math.tan(FOVY / 2), tx = ty * w / h;
  f.set([cam[0], cam[1], cam[2], 2 * ty / h], 0);
  f.set([...sc.basis.f, 0], 4);
  f.set([...mul(sc.basis.r, tx), 0], 8);
  f.set([...mul(sc.basis.u, ty), 0], 12);
  f.set([...SUN, 0], 16);
  f.set([sc.shift[0], sc.shift[1], sc.shift[2], cm.V], 20);
  u.set([w, h, cm.nlev, opts.hT ?? cm.N], 24);
  f.set([cm.c0, opts.time ?? 0, SLOTS_XY, SLOTS_XY], 28);
  u.set([opts.view ?? 0, cm.parts, opts.row ?? 0, 0], 32);
  f.set([1 / (9 * SLOTS_XY), 1 / (9 * SLOTS_XY), 1 / (9 * cm.SZ), 0], 36);
  for (let l = 0; l < cm.nlev; l++) {
    const o = 40 + l * 12, b = cm.b(l), org = cm.org[l] || [0, 0, 0];
    f.set([org[0] * b - cam[0], org[1] * b - cam[1], org[2] * b - cam[2], b], o);
    i32.set([org[0], org[1], org[2], 0], o + 4);
    f.set([cm.cell(l), cm.delta[l], cm.A[l], cm.halfdiag(l)], o + 8);
  }
  return ab;
}

function writeBoxes(target, plan, is2d) {
  const d = new Int32Array(MAX_BOXES * 8);
  let prefix = 0;
  plan.boxes.forEach((b, i) => {
    if (is2d) {
      d.set([b.l, b.min[0], 0, b.min[1], b.size[0], 1, b.size[1], prefix], i * 8);
      prefix += b.size[0] * b.size[1];
    } else {
      d.set([b.l, ...b.min, ...b.size, prefix], i * 8);
      prefix += b.size[0] * b.size[1] * b.size[2];
    }
  });
  device.queue.writeBuffer(target.boxes, 0, d);
}

function writeRaster(hm, sc, [w, h]) {
  const ab = new ArrayBuffer(RF_BYTES), f = new Float32Array(ab), u = new Uint32Array(ab), i32 = new Int32Array(ab);
  const { f: fw, r, u: up } = sc.basis;
  const near = 0.05, fy = 1 / Math.tan(FOVY / 2), fx = fy * h / w;
  // view (rows r, up, -f), then reversed-Z infinite perspective; column-major
  const V = [r[0], up[0], -fw[0], 0, r[1], up[1], -fw[1], 0, r[2], up[2], -fw[2], 0, 0, 0, 0, 1];
  const Pm = [fx, 0, 0, 0, 0, fy, 0, 0, 0, 0, 0, -1, 0, 0, near, 0];
  const vp = new Array(16).fill(0);
  for (let c = 0; c < 4; c++) for (let rr = 0; rr < 4; rr++) for (let k = 0; k < 4; k++) vp[c * 4 + rr] += Pm[k * 4 + rr] * V[c * 4 + k];
  f.set(vp, 0);
  u.set([hm.nlev, hm.M, hm.T, 0], 16);
  const cam = sc.camWorld;
  f.set([cam[0] - sc.shift[0], cam[1] - sc.shift[1], cam[2] - sc.shift[2], 2 * Math.tan(FOVY / 2) / h], 20);
  for (let l = 0; l < hm.nlev; l++) {
    const o = 24 + l * 12, s = hm.s(l), g = hm.org[l];
    const ox = g[0] * s - cam[0], oz = g[1] * s - cam[2];
    f.set([ox, 0, oz, s], o);
    f.set([ox, oz, ox + hm.M * s, oz + hm.M * s], o + 4);
    i32.set([g[0], g[1], 0, 0], o + 8);
  }
  device.queue.writeBuffer(hm.rf, 0, ab);
  // Blocks: skip those inside the finer level's square, and those outside the frustum.
  const ty = Math.tan(FOVY / 2), tx = ty * w / h;
  const planes = [sub(mul(fw, tx), r), add(mul(fw, tx), r), sub(mul(fw, ty), up), add(mul(fw, ty), up)];
  const ylo = -100 - (cam[1] - sc.shift[1]), yhi = 1900 - (cam[1] - sc.shift[1]);
  const blocks = [];
  const nb = hm.M / 32;
  for (let l = 0; l < hm.nlev; l++) {
    const s = hm.s(l), g = hm.org[l];
    const fin = l > 0 ? hm.org[l - 1].map(x => x / 2) : null;
    for (let bz = 0; bz < nb; bz++) for (let bx = 0; bx < nb; bx++) {
      const gx = g[0] + 32 * bx, gz = g[1] + 32 * bz;
      if (fin && gx >= fin[0] && gx + 32 <= fin[0] + hm.M / 2 && gz >= fin[1] && gz + 32 <= fin[1] + hm.M / 2) continue;
      const lo = [gx * s - cam[0], ylo, gz * s - cam[2]], hi = [(gx + 32) * s - cam[0], yhi, (gz + 32) * s - cam[2]];
      let inside = true;
      for (const n of planes) {
        const pv = [n[0] >= 0 ? hi[0] : lo[0], n[1] >= 0 ? hi[1] : lo[1], n[2] >= 0 ? hi[2] : lo[2]];
        if (dot(n, pv) < 0) { inside = false; break; }
      }
      if (inside) blocks.push(l, bx, bz, 0);
    }
  }
  device.queue.writeBuffer(hm.blocks, 0, new Uint32Array(blocks));
  return blocks.length / 4;
}

// ---- encoding a frame --------------------------------------------------------------------------------

const tsw = (qs, i) => qs ? { timestampWrites: { querySet: qs, beginningOfPassWriteIndex: i, endOfPassWriteIndex: i + 1 } } : {};

function encodeCook(enc, cm, plan, qs, q0, split) {
  const jobs = plan ? plan.jobs : 0;
  if (jobs) {
    writeBoxes(cm, plan, false);
    device.queue.writeBuffer(cm.cu, 0, new Uint32Array([jobs, plan.boxes.length, cm.C, 512]));
  }
  const t = cm.parts === 7 ? '' : '_t';
  let p = enc.beginComputePass(tsw(qs, q0));
  if (jobs) {
    const wgs = Math.ceil(jobs / 64), x = Math.min(wgs, 1024), y = Math.ceil(wgs / x);
    p.setPipeline(P[`classify${t}`]);
    p.setBindGroup(0, cm.cookBG);
    p.dispatchWorkgroups(x, y);
    p.setPipeline(P.args_refine);
    p.setBindGroup(0, cm.argsBG);
    p.dispatchWorkgroups(1);
    p.setPipeline(P[`refine${t}`]);
    p.setBindGroup(0, cm.refineBG);
    p.dispatchWorkgroupsIndirect(cm.args, 32);
    if (split) { p.end(); p = enc.beginComputePass(tsw(qs, q0 + 2)); }
    p.setPipeline(P.args_alloc);
    p.setBindGroup(0, cm.argsBG);
    p.dispatchWorkgroups(1);
    p.setPipeline(P.allocate);
    p.setBindGroup(0, cm.cookBG);
    p.dispatchWorkgroupsIndirect(cm.args, 0);
    p.setPipeline(P.args_cook);
    p.setBindGroup(0, cm.argsBG);
    p.dispatchWorkgroups(1);
    if (split) { p.end(); p = enc.beginComputePass(tsw(qs, q0 + 4)); }
    p.setPipeline(P[`cook${t}${cm.strict ? '_strict' : ''}`]);
    p.setBindGroup(0, cm.cookBG);
    p.dispatchWorkgroupsIndirect(cm.args, 16);
  }
  p.end();
}

function encodeHcook(enc, hm, plan, qs, q0) {
  const p = enc.beginComputePass(tsw(qs, q0));
  if (hm && plan && plan.jobs) {
    writeBoxes(hm, plan, true);
    const cu = new Uint32Array([plan.jobs, plan.boxes.length, 0, 0]);
    new Float32Array(cu.buffer, 12, 1)[0] = hm.s0;
    device.queue.writeBuffer(hm.cu, 0, cu);
    const wgs = Math.ceil(plan.jobs / 64), x = Math.min(wgs, 1024), y = Math.ceil(wgs / x);
    p.setPipeline(P.hcook);
    p.setBindGroup(0, hm.cookBG);
    p.dispatchWorkgroups(x, y);
  }
  p.end();
}

function encodeGeometry(enc, variant, tg, cm, hm, nblocks, qs, q0, rows) {
  if (variant.startsWith('raster')) {
    const p = enc.beginRenderPass({
      colorAttachments: [
        { view: tg.v.gbuf, clearValue: [0, 0, 0, 0], loadOp: 'clear', storeOp: 'store' },
        { view: tg.v.gdepth, clearValue: [INF, 0, 0, 0], loadOp: 'clear', storeOp: 'store' },
      ],
      depthStencilAttachment: { view: tg.v.zbuf, depthClearValue: 0, depthLoadOp: 'clear', depthStoreOp: 'discard' },
      ...tsw(qs, q0),
    });
    p.setPipeline(P[variant]);
    p.setBindGroup(0, hm.drawBG);
    p.setIndexBuffer(hm.index, 'uint16');
    p.drawIndexed(32 * 32 * 6, nblocks);
    p.end();
    if (variant === 'raster_dn') {
      const c = enc.beginComputePass(tsw(qs, q0 + 6));
      c.setPipeline(P.defnormal);
      c.setBindGroup(0, cm.worldBG);
      c.setBindGroup(1, tg.deferBG);
      c.dispatchWorkgroups(Math.ceil(tg.w / 8), Math.ceil(tg.h / 8));
      c.end();
    }
    return;
  }
  const p = enc.beginComputePass(tsw(qs, q0));
  p.setPipeline(P[`m_${variant}`]);
  p.setBindGroup(0, cm.worldBG);
  p.setBindGroup(1, tg.marchBG);
  p.dispatchWorkgroups(Math.ceil(tg.w / 8), Math.ceil((rows || tg.h) / 8));
  p.end();
}

function encodeShade(enc, tg, cm, qs, q0, lstats) {
  let p = enc.beginComputePass(tsw(qs, q0));
  p.setPipeline(P[W.noShadow ? 'light_nosh' : lstats ? 'light+s' : 'light']);
  p.setBindGroup(0, cm.worldBG);
  p.setBindGroup(1, tg.lightBG);
  p.dispatchWorkgroups(Math.ceil(tg.w / 8), Math.ceil(tg.h / 8));
  p.end();
  p = enc.beginComputePass(tsw(qs, q0 + 2));
  p.setPipeline(P.sky);
  p.setBindGroup(0, B.skyBG0);
  p.setBindGroup(1, tg.skyBG);
  p.dispatchWorkgroups(Math.ceil(tg.w / 8), Math.ceil(tg.h / 8));
  p.end();
}

function present(tg) {
  const enc = device.createCommandEncoder();
  const p = enc.beginRenderPass({ colorAttachments: [{ view: ctx.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
  p.setPipeline(P.blit);
  p.setBindGroup(0, tg.blitBG);
  p.draw(3);
  p.end();
  device.queue.submit([enc.finish()]);
}

async function snapshot(name, tg = TG.full) {
  present(tg);
  const blob = await new Promise(r => $('view').toBlob(r, 'image/png'));
  if (blob) await save(`${name}.png`, blob);
}

// ---- the scene state machine ------------------------------------------------------------------------

const S = { cm: null, hm: null };   // the live clipmaps

function usesCache(variant) { return variant.startsWith('cache') || variant.startsWith('hybrid'); }

async function useClipmap(cfg) {
  const key = `${cfg.N}/${cfg.c0}/${cfg.V}`;
  if (S.cm && S.cmKey === key) {
    if (S.cm.parts !== (cfg.parts ?? 7) || S.cm.strict !== !!cfg.strict) { S.cm.parts = cfg.parts ?? 7; S.cm.strict = !!cfg.strict; S.cm.org.fill(null); }
    return S.cm;
  }
  if (S.cm) S.cm.destroy();
  if (S.hm) { S.hm.destroy(); S.hm = null; }
  await device.queue.onSubmittedWorkDone();
  S.cm = new Clipmap(cfg).gpu();
  await S.cm.touch();
  S.cmKey = key;
  S.cfg = cfg;
  return S.cm;
}

function useHeightClipmap(M) {
  const want = M ?? 8 * S.cfg.N;
  if (S.hm && S.hm.M !== want) { S.hm.destroy(); S.hm = null; }
  if (!S.hm) S.hm = new HeightClipmap({ ...S.cfg, N: want / 8 }).gpu();
  return S.hm;
}

/** GPU safety: a full re-cook of the heightfield clipmap in chunks of rows, one awaited submit each. */
async function rasterTeleport(sc, time, M) {
  pose(sc, time);
  const hm = useHeightClipmap(M);
  hm.org.fill(null);
  const plan = hm.plan(sc.camWorld);
  device.queue.writeBuffer(B.frameH, 0, writeFrame(sc, S.cm, FULL, { hT: hm.T }));
  let gpu = 0;
  for (const b of plan.boxes) {
    const rows = Math.max(1, Math.floor(65536 / b.size[0]));
    for (let z = 0; z < b.size[1]; z += rows) {
      const box = { l: b.l, min: [b.min[0], b.min[1] + z], size: [b.size[0], Math.min(rows, b.size[1] - z)] };
      const qs = device.createQuerySet({ type: 'timestamp', count: 2 });
      const enc = device.createCommandEncoder();
      encodeHcook(enc, hm, { boxes: [box], jobs: box.size[0] * box.size[1] }, qs, 0);
      device.queue.submit([enc.finish()]);
      await device.queue.onSubmittedWorkDone();
      const ts = await resolveTimestamps(qs, 2);
      gpu += Number(ts[1] - ts[0]) / 1e6;
    }
  }
  return { samples: plan.jobs, gpu_ms: r3(gpu), levels: hm.nlev, M: hm.M };
}

/** Teleports the cache to the scene's camera: a full re-cook of every level, timed by stage.
 *  GPU safety: the work is split into sub-boxes (a sixteenth of a level each), one awaited submit
 *  each, so no submission runs long. */
async function teleport(sc, time = 0) {
  const cm = S.cm;
  pose(sc, time);
  cm.org.fill(null);
  const plan = cm.plan(sc.camWorld);
  device.queue.writeBuffer(B.frame, 0, writeFrame(sc, cm, FULL));
  const chunks = [];
  for (const b of plan.boxes) {
    const t = Math.ceil(b.size[2] / 16);
    for (let z = 0; z < b.size[2]; z += t) {
      chunks.push({ boxes: [{ l: b.l, min: [b.min[0], b.min[1], b.min[2] + z], size: [b.size[0], b.size[1], Math.min(t, b.size[2] - z)] }] });
    }
  }
  for (const c of chunks) c.jobs = c.boxes[0].size[0] * c.boxes[0].size[1] * c.boxes[0].size[2];
  const n = chunks.length;
  const qs = device.createQuerySet({ type: 'timestamp', count: 6 * n });
  const rb = buffer(48 * n, U.QUERY_RESOLVE | U.COPY_SRC);
  const hist = buffer(32 * n, U.COPY_DST | U.COPY_SRC);
  const t0 = performance.now();
  let maxWall = 0;
  for (let i = 0; i < n; i++) {
    const enc = device.createCommandEncoder();
    enc.clearBuffer(cm.counters, 0, 28);
    encodeCook(enc, cm, chunks[i], qs, 6 * i, true);
    enc.copyBufferToBuffer(cm.counters, 0, hist, 32 * i, 32);
    const tc = performance.now();
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
    maxWall = Math.max(maxWall, performance.now() - tc);
  }
  const wall = performance.now() - t0;
  const enc = device.createCommandEncoder();
  enc.resolveQuerySet(qs, 0, 6 * n, rb, 0);
  device.queue.submit([enc.finish()]);
  const raw = await readback([{ src: rb, size: 48 * n }, { src: hist, size: 32 * n }]);
  rb.destroy();
  hist.destroy();
  qs.destroy();
  const ts = new BigUint64Array(raw, 0, 6 * n), c = new Int32Array(raw, 48 * n, 8 * n);
  const d = (i, k) => Number(ts[6 * i + 2 * k + 1] - ts[6 * i + 2 * k]) / 1e6;
  const sum = (k) => { let x = 0; for (let i = 0; i < n; i++) x += d(i, k); return x; };
  const csum = (k) => { let x = 0; for (let i = 0; i < n; i++) x += c[8 * i + k]; return x; };
  const cooked = csum(0), free = c[8 * (n - 1) + 7];
  const res = {
    config: cm.label(), levels: cm.nlev, jobs: plan.jobs, submits: n, max_submit_wall_ms: r3(maxWall), atlas_zero_fill_wall_ms: cm.touchMs,
    classify_ms: r3(sum(0)), allocate_ms: r3(sum(1)), cook_ms: r3(sum(2)), total_gpu_ms: r3(sum(0) + sum(1) + sum(2)), wall_ms: r3(wall),
    bricks_cooked: cooked, candidates: csum(4), refined_empty: csum(5), overflow: csum(2), slots_used: cm.C - free, slots_capacity: cm.C,
    bricks_per_ms_cook: r3(cooked / Math.max(sum(2), 1e-3)),
    classified_per_ms: r3(plan.jobs / Math.max(sum(0), 1e-3)),
    memory_mb: cm.memMB(),
    used_mb: r3(((cm.C - free) * 729 * 2 + cm.mem.page + cm.mem.lists) / 2 ** 20),
  };
  if (res.overflow > 0) fail(`cache overflow: ${res.overflow} bricks didn't fit (capacity ${cm.C})`);
  if (S.hm) { S.hm.org.fill(null); }
  return res;
}

/** One frame's worth of updates for the scene at `time`; writes the uniforms. */
function step(sc, variant, res, time, opts = {}) {
  pose(sc, time);
  const cm = S.cm;
  const plan = cm.plan(sc.camWorld, opts.budget);
  let hplan = null, nblocks = 0;
  if (variant.startsWith('raster')) {
    const hm = useHeightClipmap(opts.rasterM);
    hplan = hm.plan(sc.camWorld);
    nblocks = writeRaster(hm, sc, res);
    const fh = writeFrame(sc, cm, res, { hT: hm.T });
    device.queue.writeBuffer(B.frameH, 0, fh);
  }
  device.queue.writeBuffer(B.frame, 0, writeFrame(sc, cm, res, { time, view: opts.view }));
  return { plan, hplan, nblocks };
}

function encodeFull(enc, variant, tg, st, qs, q0, opts = {}) {
  if (opts.history != null) enc.clearBuffer(S.cm.counters, 0, 28);
  else if (st.plan.jobs) enc.clearBuffer(S.cm.counters, 0, 28);
  encodeCook(enc, S.cm, st.plan, qs, q0, false);
  if (opts.history != null) enc.copyBufferToBuffer(S.cm.counters, 0, S.cm.history, opts.history * 32, 32);
  encodeHcook(enc, S.hm, st.hplan, qs, q0 + 2);
  encodeGeometry(enc, variant, tg, S.cm, S.hm, st.nblocks, qs, q0 + 4);   // a deferred normal pass, if any, at q0 + 10
  encodeShade(enc, tg, S.cm, qs, q0 + 6, opts.lstats);
}

const tgFor = res => res === HALF ? TG.half : TG.full;

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

/** GPU safety: a frame whose geometry pass would run longer than this is measured in bands. */
const SAFE_FRAME_MS = 40;

/** One frame with the geometry pass in timed bands of `rows`, each its own awaited submit. Returns
 *  per-pass GPU ms (geometry summed over bands) and the longest band. */
async function tiledFrame(sc, variant, res, time, rows, budget) {
  const tg = tgFor(res);
  const st = step(sc, variant, res, time, { budget });
  const nb = Math.ceil(tg.h / rows);
  const qs = device.createQuerySet({ type: 'timestamp', count: 4 + 2 * nb + 4 });
  let enc = device.createCommandEncoder();
  if (st.plan.jobs) enc.clearBuffer(S.cm.counters, 0, 28);
  encodeCook(enc, S.cm, st.plan, qs, 0, false);
  encodeHcook(enc, S.hm, st.hplan, qs, 2);
  device.queue.submit([enc.finish()]);
  await device.queue.onSubmittedWorkDone();
  for (let i = 0; i < nb; i++) {
    device.queue.writeBuffer(B.frame, 0, writeFrame(sc, S.cm, res, { time, row: i * rows }));
    enc = device.createCommandEncoder();
    encodeGeometry(enc, variant, tg, S.cm, S.hm, st.nblocks, qs, 4 + 2 * i, Math.min(rows, tg.h - i * rows));
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
  }
  device.queue.writeBuffer(B.frame, 0, writeFrame(sc, S.cm, res, { time }));
  enc = device.createCommandEncoder();
  encodeShade(enc, tg, S.cm, qs, 4 + 2 * nb, false);
  device.queue.submit([enc.finish()]);
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, 4 + 2 * nb + 4);
  const d = i => Number(ts[2 * i + 1] - ts[2 * i]) / 1e6;
  const bands = Array.from({ length: nb }, (_, i) => d(2 + i));
  return { cook: d(0), hcook: d(1), geometry: bands.reduce((a, b) => a + b, 0), band_max: Math.max(...bands), light: d(2 + nb), sky: d(3 + nb), jobs: st.plan.jobs };
}

/** Sustained GPU time per pass: teleport, 30 untimed frames, then `frames` back to back.
 *  GPU safety: a probe frame is rendered in bands first; if the geometry pass would exceed
 *  SAFE_FRAME_MS as one submit, the variant is measured tiled instead (a few frames, each in bands)
 *  and marked so. Both are GPU timestamps; the tiled numbers include no back-to-back load. */
async function measure(sc, variant, res = FULL, { frames = 90, warm = 30, budget, rasterM } = {}) {
  if (!variant.startsWith('raster')) {
    await teleport(sc, 0);
    const pr = await tiledFrame(sc, variant, res, 0, 64, budget);
    if (pr.geometry > SAFE_FRAME_MS) {
      const rows = Math.max(8, Math.floor(64 * SAFE_FRAME_MS / Math.max(pr.band_max, 1e-3) / 8) * 8);
      const fr = [];
      await teleport(sc, 0);
      for (let f = 0; f < 3; f++) fr.push(await tiledFrame(sc, variant, res, f / 60, rows, budget));
      const m = k => r3(median(fr.map(x => x[k])));
      const geomCook = usesCache(variant) ? 1 : 0;
      const world = fr.map(x => x.geometry + geomCook * x.cook);
      return {
        variant, res: `${res[0]}x${res[1]}`, speed_mps: sc.speed, tiled: true, tiled_note: `geometry pass > ${SAFE_FRAME_MS} ms: measured in ${Math.ceil(res[1] / rows)} bands of ${rows} rows, 3 frames, not sustained`,
        world_geometry: r3(median(world)), world_geometry_p95: r3(Math.max(...world)), world_geometry_max: r3(Math.max(...world)),
        cook: m('cook'), geometry: m('geometry'), light: m('light'), sky: m('sky'),
        frame_gpu: r3(median(fr.map(x => x.cook + x.hcook + x.geometry + x.light + x.sky))),
      };
    }
  }
  return measureSustained(sc, variant, res, { frames, warm, budget, rasterM });
}

async function measureSustained(sc, variant, res = FULL, { frames = 90, warm = 30, budget, rasterM } = {}) {
  const tg = tgFor(res);
  await teleport(sc, 0);
  const rt = variant.startsWith('raster') ? await rasterTeleport(sc, 0, rasterM) : null;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  const jobs = [], cpu = [], blocks = [];
  for (let f = 0; f < warm + frames; f++) {
    const timed = f >= warm;
    const c0 = performance.now();
    const st = step(sc, variant, res, f / 60, { budget, rasterM });
    if (timed) blocks.push(st.nblocks);
    const enc = device.createCommandEncoder();
    encodeFull(enc, variant, tg, st, timed ? qs : null, (f - warm) * Q, { history: timed ? f - warm : null });
    device.queue.submit([enc.finish()]);
    cpu.push(performance.now() - c0);
    if (timed) jobs.push(st.plan.jobs);
  }
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * Q);
  const hist = new Int32Array(await readback([{ src: S.cm.history, size: frames * 32 }]));
  const d = (f, i) => Number(ts[f * Q + 2 * i + 1] - ts[f * Q + 2 * i]) / 1e6;
  const all = [...Array(frames).keys()];
  const geomCook = usesCache(variant) ? 1 : 0;
  const world = f => d(f, 2) + d(f, 5) + (variant.startsWith('raster') ? d(f, 1) : geomCook * d(f, 0));
  const m = fn => r3(median(all.map(fn)));
  const p95 = fn => r3(pct(all.map(fn), 0.95));
  const mx = fn => r3(Math.max(...all.map(fn)));
  const bricks = all.map(f => hist[f * 8]);
  return {
    variant, res: `${res[0]}x${res[1]}`, speed_mps: sc.speed,
    world_geometry: m(world), world_geometry_p95: p95(world), world_geometry_max: mx(world),
    cook: m(f => d(f, 0)), cook_p95: p95(f => d(f, 0)), cook_max: mx(f => d(f, 0)),
    hcook: m(f => d(f, 1)), hcook_p95: p95(f => d(f, 1)),
    geometry: m(f => d(f, 2)), geometry_p95: p95(f => d(f, 2)),
    light: m(f => d(f, 3)), sky: m(f => d(f, 4)),
    // The sum of passes: Chrome writes no timestamps for an empty compute pass (a static view's cook).
    deferred_normal: m(f => d(f, 5)),
    frame_gpu: m(f => d(f, 0) + d(f, 1) + d(f, 2) + d(f, 3) + d(f, 4) + d(f, 5)),
    frame_gpu_p95: p95(f => d(f, 0) + d(f, 1) + d(f, 2) + d(f, 3) + d(f, 4) + d(f, 5)),
    bricks_cooked_per_frame: r3(median(bricks)), bricks_cooked_p95: pct(bricks, 0.95), bricks_cooked_max: Math.max(...bricks),
    classify_jobs_per_frame: median(jobs), classify_jobs_max: Math.max(...jobs),
    cook_bricks_per_ms: r3(bricks.reduce((a, b) => a + b, 0) / Math.max(all.reduce((a, f) => a + d(f, 0), 0), 1e-3)),
    overflow: Math.max(...all.map(f => hist[f * 8 + 2])),
    cpu_ms_per_frame: r3(median(cpu)),
    lagged_levels: S.cm.lagged,
    ...(rt ? { raster: { teleport: rt, blocks_drawn: median(blocks), triangles_drawn: median(blocks) * 2048, vertices_drawn: median(blocks) * 1089,
      blocks_before_culling: S.hm.nlev * (S.hm.M / 32) ** 2, grid_quads_per_level: S.hm.M ** 2 } } : {}),
  };
}

/** Frames submitted at 60 Hz by busy-wait, as in spikes 01 and 02. */
async function paced(sc, variant, res = FULL, frames = 180) {
  const tg = tgFor(res), period = 1000 / 60;
  await teleport(sc, 0);
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  const t0 = performance.now() + 20;
  for (let f = 0; f < frames; f++) {
    const deadline = t0 + f * period;
    while (performance.now() < deadline) { /* spin */ }
    const st = step(sc, variant, res, f / 60);
    const enc = device.createCommandEncoder();
    encodeFull(enc, variant, tg, st, qs, f * Q);
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * Q);
  const keep = [...Array(frames).keys()].slice(30);
  const d = (f, i) => Number(ts[f * Q + 2 * i + 1] - ts[f * Q + 2 * i]) / 1e6;
  const gpu = keep.map(f => d(f, 0) + d(f, 1) + d(f, 2) + d(f, 3) + d(f, 4) + d(f, 5));    // sum of passes (see measureSustained)
  const world = keep.map(f => d(f, 2) + (usesCache(variant) ? d(f, 0) : 0));
  return {
    pacing: 'busy-wait at 60 Hz', variant, frames: keep.length,
    frame_gpu_median: r3(median(gpu)), frame_gpu_p95: r3(pct(gpu, 0.95)), frame_gpu_max: r3(Math.max(...gpu)),
    frame_gpu_over_16_7: gpu.filter(g => g > 16.7).length,
    world_geometry_median: r3(median(world)), world_geometry_p95: r3(pct(world, 0.95)),
  };
}

/** Renders one frame (no timing). GPU safety: the geometry pass runs in bands of rows, one awaited
 *  submit each, sized adaptively to ~40 ms (the reference starts small). */
async function render(sc, variant, res = FULL, opts = {}) {
  const tg = tgFor(res);
  if (variant.startsWith('raster') && (!S.hm || !S.hm.org[0])) await rasterTeleport(sc, opts.time ?? 0);
  const st = step(sc, variant, res, opts.time ?? 0, { view: opts.view });
  if (opts.stats) { const e = device.createCommandEncoder(); e.clearBuffer(B.stats); device.queue.submit([e.finish()]); }
  let enc = device.createCommandEncoder();
  if (st.plan.jobs) enc.clearBuffer(S.cm.counters, 0, 28);
  encodeCook(enc, S.cm, st.plan, null, 0, false);
  encodeHcook(enc, S.hm, st.hplan, null, 0);
  device.queue.submit([enc.finish()]);
  await device.queue.onSubmittedWorkDone();
  const v = opts.stats ? `${variant}+s` : variant;
  if (variant.startsWith('raster')) {
    enc = device.createCommandEncoder();
    encodeGeometry(enc, v, tg, S.cm, S.hm, st.nblocks, null, 0);
    device.queue.submit([enc.finish()]);
  } else {
    let rows = variant.startsWith('ref') ? 16 : (W.rows?.[variant] ?? 64);
    let worst = 0;
    for (let y = 0; y < tg.h;) {
      const n = Math.min(rows, tg.h - y);
      device.queue.writeBuffer(B.frame, 0, writeFrame(sc, S.cm, res, { time: opts.time ?? 0, view: opts.view, row: y }));
      enc = device.createCommandEncoder();
      encodeGeometry(enc, v, tg, S.cm, S.hm, st.nblocks, null, 0, n);
      const t0 = performance.now();
      device.queue.submit([enc.finish()]);
      await device.queue.onSubmittedWorkDone();
      const ms = performance.now() - t0;
      worst = Math.max(worst, ms);
      y += n;
      rows = Math.max(8, Math.min(tg.h, Math.round(rows * Math.min(40 / Math.max(ms, 0.5), 4) / 8) * 8));
    }
    W.worstBandMs = Math.max(W.worstBandMs || 0, worst);
    device.queue.writeBuffer(B.frame, 0, writeFrame(sc, S.cm, res, { time: opts.time ?? 0, view: opts.view }));
  }
  enc = device.createCommandEncoder();
  encodeShade(enc, tg, S.cm, null, 0, opts.stats);
  device.queue.submit([enc.finish()]);
  await device.queue.onSubmittedWorkDone();
}

async function grab(sc, variant, res = FULL, opts = {}) {
  await render(sc, variant, res, opts);
  const tg = tgFor(res);
  const out = device.createBuffer({ size: tg.w * tg.h * 4, usage: U.COPY_DST | U.MAP_READ });
  const dep = opts.depth ? device.createBuffer({ size: tg.w * tg.h * 4, usage: U.COPY_DST | U.MAP_READ }) : null;
  const enc = device.createCommandEncoder();
  enc.copyTextureToBuffer({ texture: tg.final }, { buffer: out, bytesPerRow: tg.w * 4, rowsPerImage: tg.h }, [tg.w, tg.h]);
  if (dep) enc.copyTextureToBuffer({ texture: tg.gdepth }, { buffer: dep, bytesPerRow: tg.w * 4, rowsPerImage: tg.h }, [tg.w, tg.h]);
  device.queue.submit([enc.finish()]);
  await out.mapAsync(GPUMapMode.READ);
  const px = new Uint8Array(out.getMappedRange().slice(0));
  out.unmap();
  out.destroy();
  let depth = null;
  if (dep) { await dep.mapAsync(GPUMapMode.READ); depth = new Float32Array(dep.getMappedRange().slice(0)); dep.unmap(); dep.destroy(); }
  return { px, depth };
}

function diff(a, b, mask = null) {
  let sum = 0, over8 = 0, n = 0, max = 0;
  for (let i = 0, k = 0; i < a.length; i += 4, k++) {
    if (mask && !mask[k]) continue;
    let m = 0;
    for (let c = 0; c < 3; c++) { const d = Math.abs(a[i + c] - b[i + c]); sum += d; if (d > m) m = d; }
    if (m > 8) over8++;
    if (m > max) max = m;
    n++;
  }
  return { mean_abs_255: r3(sum / Math.max(3 * n, 1)), share_over_8: Math.round(over8 / Math.max(n, 1) * 1e5) / 1e5, max, pixels: n };
}

/** A visual diff: grey image, red where > 8/255, yellow where > 2/255. */
async function diffImage(a, b, name, [w, h] = FULL) {
  const c = document.createElement('canvas');
  c.width = w;
  c.height = h;
  const g = c.getContext('2d'), img = g.createImageData(w, h);
  for (let i = 0; i < a.length; i += 4) {
    let m = 0;
    for (let k = 0; k < 3; k++) m = Math.max(m, Math.abs(a[i + k] - b[i + k]));
    const base = (a[i] + a[i + 1] + a[i + 2]) / 9;
    img.data[i] = m > 2 ? 255 : base;
    img.data[i + 1] = m > 8 ? 0 : (m > 2 ? 210 : base);
    img.data[i + 2] = m > 2 ? 0 : base;
    img.data[i + 3] = 255;
  }
  g.putImageData(img, 0, 0);
  const blob = await new Promise(r => c.toBlob(r, 'image/png'));
  await save(`${name}.png`, blob);
}

async function frameStats(sc, variant, res = FULL) {
  await render(sc, variant, res, { stats: true });
  const s = new Uint32Array(await readback([{ src: B.stats, size: 256 }]));
  const o = {};
  STAT_NAMES.forEach((n, i) => { o[n] = s[i]; });
  const px = res[0] * res[1];
  o.levels_hit = Array.from(s.slice(16, 30));
  o.steps_per_px = r3(o.steps / px);
  o.field_evals_per_px = r3(o.field_evals / px);
  o.empty_steps_per_px = r3(o.empty_steps / px);
  o.cap_share = Math.round(o.caps / px * 1e6) / 1e6;
  o.out_share = r3(o.out / px);
  o.sky_share = r3(o.sky / px);
  o.shadow_steps_per_ray = r3(o.sh_steps / Math.max(o.sh_rays, 1));
  return o;
}

// ---- dev modes ------------------------------------------------------------------------------------

async function setup() {
  await init();
  globalResources();
  await pipelines();
  log('pipelines', JSON.stringify(R.pipelines));
}

async function mapMode() {
  await setup();
  await placeWorld(RUIN_SITES);
  const dummy = buffer(16, U.STORAGE);
  const bg = device.createBindGroup({ layout: L.tools1, entries: [{ binding: 0, resource: { buffer: dummy } }, { binding: 1, resource: { buffer: buffer(16, U.STORAGE) } }, { binding: 2, resource: TG.full.v.final }] });
  const enc = device.createCommandEncoder();
  const p = enc.beginComputePass();
  p.setPipeline(P.map);
  p.setBindGroup(0, B.tools0);
  p.setBindGroup(1, bg);
  p.dispatchWorkgroups(135, 135);
  p.end();
  device.queue.submit([enc.finish()]);
  await snapshot('map');
  // Slope and height statistics over the 8×8 km square, at full detail and at 1 m.
  for (const fp of [0.01, 1.0]) {
    const pts = [];
    let seed = 12345;
    const rnd = () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296; };
    for (let i = 0; i < 200000; i++) pts.push([rnd() * 8192 - 4096, 0, rnd() * 8192 - 4096, fp]);
    const res = await probe(pts);
    const sl = res.map(r => r[1]), hs = res.map(r => r[0]);
    R.world[`slope_fp${fp}`] = {
      max: r3(sl.reduce((a, b) => Math.max(a, b), 0)), p99: r3(pct(sl, 0.99)), p999: r3(pct(sl, 0.999)), median: r3(median(sl)),
      share_over_G: sl.filter(x => x > 3.0).length / sl.length,
      h_min: r3(hs.reduce((a, b) => Math.min(a, b), 1e9)), h_max: r3(hs.reduce((a, b) => Math.max(a, b), -1e9)), h_median: r3(median(hs)),
    };
  }
  log('world', JSON.stringify(R.world));
}

const CFG = { N: 64, c0: 0.125, V: 8000 };
const VARIANTS = ['analytic', 'cache', 'cache_fn', 'hybrid'];   // quick and dev modes
const DEV_VARIANTS = ['cache_norelax', ...VARIANTS, 'ref'];
const VARIANTS_T = ['raster', 'analytic_t', 'cache_t', 'hybrid_t'];

/** The views, placed by probing the field. */
async function makeViews() {
  const v = {};
  const [hs] = await probe([[0, 0, -2500, 0.01]]);
  v.hillside = view('hillside', [0, hs[0] + 1.7, -2500], [450, 330, -300]);
  // Tower top: standing 1.7 m inside the parapet at a crenel (the crenels are centred at 15° + k·30°),
  // looking out and slightly down.
  const t = W.ruins[0], ca = 75 * Math.PI / 180, cd = [Math.cos(ca), 0, Math.sin(ca)];
  const teye = [t.x + 1.6 * cd[0], t.top - 0.05 + 1.7, t.z + 1.6 * cd[2]];
  v.tower = view('tower', teye, add(teye, [cd[0] * 1000, -110, cd[2] * 1000]));
  // Flyover: constant altitude, heading toward the massif; looking ahead and down.
  const f0 = [-700, 760, -2300], fdir = norm([800, 0, 2000]);
  const flyPath = speed => time => {
    const eye = add(f0, mul(fdir, speed * time));
    return { eye, target: add(eye, [fdir[0] * 1000, -310, fdir[2] * 1000]) };
  };
  v.flyover = view('flyover', f0, null, flyPath(60), 60);
  v.flyPath = flyPath;
  // Riding: the hillside camera moving along the ground at 10 m/s, eye height 1.7 m.
  const rdir = norm([450, 0, 2200]);
  const pts = [];
  for (let i = 0; i <= 1200; i++) pts.push([rdir[0] * i * 0.1, 0, -2500 + rdir[2] * i * 0.1, 0.01]);
  const hts = (await probe(pts)).map(r => r[0]);
  const ground = d => { const x = Math.min(Math.max(d / 0.1, 0), 1199.999), i = Math.floor(x), f = x - i; return hts[i] * (1 - f) + hts[i + 1] * f; };
  const ridePath = speed => time => {
    const d = speed * time, eye = [rdir[0] * d, ground(d) + 1.7, -2500 + rdir[2] * d];
    return { eye, target: add(eye, sub([450, 330, -300], [0, hs[0] + 1.7, -2500])) };
  };
  v.riding = view('riding', null, null, ridePath(10), 10);
  v.ridePath = ridePath;
  return v;
}

function viewInfo(sc) {
  pose(sc, 0);
  return { eye: sc.cur.eye.map(r3), target: sc.cur.target.map(r3), speed_mps: sc.speed, path: !!sc.path };
}

/** Depth agreement with a reference: shares of pixels whose depth differs by more than 1% and 0.1%
 *  (or that flip between hit and sky). Added after the first measurements (see README). */
function depthDiff(a, b) {
  let d2 = 0, d3 = 0, flip = 0;
  const n = a.length;
  for (let i = 0; i < n; i++) {
    const s0 = a[i] > 1e29, s1 = b[i] > 1e29;
    if (s0 !== s1) { flip++; d2++; d3++; continue; }
    if (s0) continue;
    const e = Math.abs(a[i] - b[i]) / b[i];
    if (e > 1e-2) d2++;
    if (e > 1e-3) d3++;
  }
  return { share_over_1pct: Math.round(d2 / n * 1e5) / 1e5, share_over_0_1pct: Math.round(d3 / n * 1e5) / 1e5, hit_sky_flips: Math.round(flip / n * 1e5) / 1e5 };
}

/** Correctness for `variants` against the brute-force reference `ref` and the same-tolerance
 *  reference `tol`: lit images (the README's criterion), unshadowed images and depth. */
async function quality(sc, name, variants, refName, tolName) {
  const out = {};
  const ref = await grab(sc, refName, FULL, { depth: true });
  await snapshot(`${name}-ref`);
  const tol = tolName ? await grab(sc, tolName, FULL, { depth: true }) : null;
  for (const v of variants) {
    const g = await grab(sc, v, FULL, { depth: true });
    await snapshot(`${name}-${v}`);
    out[v] = { lit: diff(g.px, ref.px), depth: depthDiff(g.depth, ref.depth) };
    if (tol) out[v].lit_vs_same_tolerance_ref = diff(g.px, tol.px);
    await diffImage(g.px, ref.px, `${name}-${v}-diff`);
  }
  if (tol) out[tolName] = { lit: diff(tol.px, ref.px), depth: depthDiff(tol.depth, ref.depth) };
  return { out, ref };
}

/** The main configuration: three views, the variants, two resolutions, statistics, correctness. */
async function mainConfig(views) {
  for (const name of ['hillside', 'tower', 'flyover']) {
    const sc = views[name];
    R.views[name] = viewInfo(sc);
    await useClipmap(CFG);
    R.teleport[name] = await teleport(sc);
    log('teleport', name, JSON.stringify(R.teleport[name]));
    const heavyHalf = name === 'hillside';
    for (const res of [FULL, HALF]) {
      for (const v of ['analytic', 'cache', 'cache_fn']) {
        if (res === HALF && v === 'analytic' && !heavyHalf) continue;
        const key = `${name}/${v}/${res[1]}p`;
        R.frames[key] = await measure(sc, v, res);
        log(key, JSON.stringify(R.frames[key]));
      }
    }
    if (name === 'hillside') {
      R.frames['hillside/cache_norelax/1080p'] = await measure(sc, 'cache_norelax', FULL);
      await useClipmap({ ...CFG, strict: true });
      R.frames['hillside/cache_strict/1080p'] = await measure(sc, 'cache', FULL);
      log('ablations', JSON.stringify([R.frames['hillside/cache_norelax/1080p'], R.frames['hillside/cache_strict/1080p']]));
      await useClipmap(CFG);
    }
    R.stats[name] = {};
    await teleport(sc);
    for (const v of ['analytic', 'cache', 'cache_fn', 'ref']) R.stats[name][v] = await frameStats(sc, v);
    const q = await quality(sc, name, ['analytic', 'cache', 'cache_fn'], 'ref', name === 'hillside' ? 'reftol' : null);
    R.quality[name] = q.out;
    W.refs[name] = q.ref.px;
    W.refDepth = W.refDepth || {};
    W.refDepth[name] = q.ref.depth;
    for (const [v, mode, file] of [['analytic', 1, 'steps-analytic'], ['cache', 1, 'steps-cache'], ['cache', 2, 'levels']]) {
      await render(sc, v, FULL, { view: mode });
      await snapshot(`${name}-${file}`);
    }
    // The hybrid needs a strict cook (a bound), so it has its own reference with the same lighting.
    await useClipmap({ ...CFG, strict: true });
    await teleport(sc);
    for (const res of heavyHalf ? [FULL, HALF] : [FULL]) {
      R.frames[`${name}/hybrid/${res[1]}p`] = await measure(sc, 'hybrid', res);
      log(`${name}/hybrid/${res[1]}p`, JSON.stringify(R.frames[`${name}/hybrid/${res[1]}p`]));
    }
    await teleport(sc);
    R.stats[name].hybrid = await frameStats(sc, 'hybrid');
    const qh = await quality(sc, `${name}-strict`, ['hybrid'], 'ref', null);
    R.quality[name].hybrid = qh.out.hybrid;
    await render(sc, 'hybrid', FULL, { view: 3 });
    await snapshot(`${name}-fieldevals-hybrid`);
    log('stats', name, JSON.stringify(R.stats[name]));
    log('quality', name, JSON.stringify(R.quality[name]));
  }
  await useClipmap(CFG);
}

async function ridingAndPaced(views) {
  await useClipmap(CFG);
  const sc = views.riding;
  R.views.riding = viewInfo(sc);
  for (const v of ['cache', 'cache_fn']) {
    R.frames[`riding/${v}/1080p`] = await measure(sc, v, FULL);
    log(`riding/${v}`, JSON.stringify(R.frames[`riding/${v}/1080p`]));
  }
  for (const res of [FULL, HALF]) {
    R.paced[`flyover/cache/${res[1]}p`] = await paced(views.flyover, 'cache', res);
    log('paced', res[1], JSON.stringify(R.paced[`flyover/cache/${res[1]}p`]));
  }
}

/** Terrain only: the raster heightfield clipmap against the cache and the analytic field. */
async function terrainPhase(views) {
  await useClipmap({ ...CFG, parts: 1 });
  for (const name of ['hillside', 'flyover']) {
    const sc = views[name];
    for (const res of [FULL, HALF]) {
      for (const v of ['raster', 'raster_nocull', 'raster_fn', 'raster_dn', 'cache_t', 'analytic_t']) {
        if (v === 'analytic_t' && (res === HALF || name !== 'hillside')) continue;
        if (v === 'raster_nocull' && res === HALF) continue;
        const key = `${name}-terrain/${v}/${res[1]}p`;
        R.frames[key] = await measure(sc, v, res);
        log(key, JSON.stringify(R.frames[key]));
      }
    }
    if (name !== 'hillside') continue;
    await teleport(sc);
    const q = await quality(sc, `${name}-terrain`, ['raster', 'raster_dn', 'cache_t', 'analytic_t'], 'ref_t', null);
    R.quality[`${name}-terrain`] = q.out;
    R.stats[`${name}-terrain`] = {};
    for (const v of ['cache_t', 'analytic_t']) R.stats[`${name}-terrain`][v] = await frameStats(sc, v);
    log('quality terrain', name, JSON.stringify(R.quality[`${name}-terrain`]));
  }
  if (S.hm) {
    R.memory_mb.raster_clipmap = r3((S.hm.mem.textures + S.hm.mem.mesh_index) / 2 ** 20);
    R.config.raster = { M: S.hm.M, T: S.hm.T, levels: S.hm.nlev, finest_spacing_m: S.hm.s0 };
  }
  // Raster density: grid quads per level side (the cache's N = 32, 64, 128 have the same cells).
  R.sweeps.raster_M = {};
  for (const M of [256, 512, 1024]) {
    const o = {};
    for (const [name, v] of [['hillside', 'raster'], ['flyover', 'raster']]) {
      o[`${name}/${v}`] = await measure(views[name], v, FULL, { frames: 60, warm: 20, rasterM: M });
    }
    o.memory_mb = r3((S.hm.mem.textures + S.hm.mem.mesh_index) / 2 ** 20);
    R.sweeps.raster_M[M] = o;
    log('sweep raster M', M, JSON.stringify(o));
  }
  useHeightClipmap();
}

/** Sweeps (the cache path): view distance, finest cell, clipmap resolution, camera speed. */
async function sweeps(views) {
  const sc = views.hillside;
  const one = async (label, cfg, withQuality) => {
    await useClipmap(cfg);
    const out = { teleport: await teleport(sc) };
    out.cache = await measure(sc, 'cache', FULL, { frames: 60, warm: 20 });
    if (withQuality) {
      const g = await grab(sc, 'cache', FULL, { depth: true });
      out.depth_vs_ref = depthDiff(g.depth, W.refDepth.hillside);
      out.lit_vs_ref_confounded = diff(g.px, W.refs.hillside);   // lighting uses this config's cache, the reference used the main one
      await snapshot(`sweep-${label}-cache`);
    }
    log('sweep', label, JSON.stringify(out));
    return out;
  };
  R.sweeps.view_distance = {};
  for (const V of [1000, 4000, 8000]) R.sweeps.view_distance[V] = await one(`V${V}`, { ...CFG, V }, false);
  R.sweeps.finest_cell = {};
  for (const c0 of [0.0625, 0.25]) R.sweeps.finest_cell[c0] = await one(`c${c0}`, { ...CFG, c0 }, true);
  R.sweeps.resolution_N = {};
  for (const N of [16, 32]) R.sweeps.resolution_N[N] = await one(`N${N}`, { ...CFG, N }, true);
  await useClipmap(CFG);
  R.sweeps.speed = {};
  for (const speed of [0, 10, 30, 60, 120]) {
    const fsc = view(`fly${speed}`, null, null, views.flyPath(speed), speed);
    R.sweeps.speed[speed] = await measure(fsc, 'cache', FULL, { frames: 60, warm: 20 });
    log('sweep speed', speed, JSON.stringify(R.sweeps.speed[speed]));
  }
}

/** Precision: the same content with the camera 0, 5.8, 11.6 and 46 km from the shader's origin. */
async function precision(views) {
  await useClipmap(CFG);
  const step = 512 * 8;     // a multiple of the coarsest brick, so the cache's grid is the same
  for (const name of ['hillside', 'tower']) {
    const sc = views[name];
    pose(sc, 0);
    const base = sc.cur.eye;
    R.precision[name] = {};
    let ref = {};
    for (const D of [0, 4096, 8192, 32768]) {
      // shift so the camera sits at about (D, y, D) in shader coordinates
      sc.shift = [D - Math.round(base[0] / step) * step, 0, D - Math.round(base[2] / step) * step];
      await teleport(sc);
      const row = { camera_distance_km: r3(Math.hypot(base[0] + sc.shift[0], base[2] + sc.shift[2]) / 1000) };
      for (const v of ['analytic', 'cache']) {
        const g = await grab(sc, v);
        if (D === 0) ref[v] = g.px;
        else {
          row[v] = diff(g.px, ref[v]);
          if (D === 32768) { await snapshot(`${name}-precision-46km-${v}`); await diffImage(g.px, ref[v], `${name}-precision-46km-${v}-diff`); }
        }
      }
      R.precision[name][D] = row;
      log('precision', name, D, JSON.stringify(row));
    }
    sc.shift = [0, 0, 0];
  }
}

/** Horizon shimmer: consecutive frames of a moving camera where the surface is over 2 km away. */
async function shimmer(views) {
  await useClipmap(CFG);
  // The flyover sequence starts 6 frames before the camera crosses a level-7 brick boundary (128 m),
  // so a far level re-centres (and its boundary moves) mid-sequence.
  const cm = new Clipmap(CFG), b7 = cm.b(7), fp0 = views.flyPath(60);
  let tc = 0;
  for (let f = 1; f < 60 * 60; f++) {
    const a = fp0((f - 1) / 60).eye, b = fp0(f / 60).eye;
    if (Math.floor(a[0] / b7) !== Math.floor(b[0] / b7) || Math.floor(a[2] / b7) !== Math.floor(b[2] / b7)) { tc = f; break; }
  }
  R.shimmer.flyover_level7_shift_frame = 6;
  const start = { riding: 0, flyover: Math.max(tc - 6, 0) / 60 };
  const run = async (sc, v, frames) => {
    const t0 = start[sc.name] ?? 0;
    await teleport(sc, t0);
    let prev = null;
    const per = [];
    for (let f = 0; f < frames; f++) {
      const g = await grab(sc, v, FULL, { time: t0 + f / 60, depth: true });
      if (prev) {
        const mask = new Uint8Array(g.depth.length);
        for (let i = 0; i < mask.length; i++) mask[i] = g.depth[i] > 2000 && g.depth[i] < 1e29 && prev.depth[i] > 2000 && prev.depth[i] < 1e29 ? 1 : 0;
        per.push(diff(g.px, prev.px, mask));
      }
      prev = g;
    }
    return {
      frames, start_s: r3(t0), per_frame_share_over_8: per.map(p => p.share_over_8), far_pixels: median(per.map(p => p.pixels)),
      mean_abs_255: r3(median(per.map(p => p.mean_abs_255))), mean_abs_255_max: r3(Math.max(...per.map(p => p.mean_abs_255))),
      share_over_8: r3(median(per.map(p => p.share_over_8)) * 1e4) / 1e4, share_over_8_max: Math.max(...per.map(p => p.share_over_8)),
    };
  };
  for (const name of ['riding', 'flyover']) {
    R.shimmer[name] = {};
    for (const v of ['cache', 'cache_fn']) R.shimmer[name][v] = await run(views[name], v, 12);
    R.shimmer[name].analytic = await run(views[name], 'analytic', 6);
    log('shimmer', name, JSON.stringify(R.shimmer[name]));
  }
}

/** Dev: steps, timings and the reference difference per view and variant (#stats=hillside). */
async function statsMode() {
  await setup();
  await placeWorld(RUIN_SITES);
  const views = await makeViews();
  const only = location.hash.split('=')[1];
  for (const name of ['hillside', 'tower', 'flyover']) {
    if (only && only !== name) continue;
    const sc = views[name];
    await useClipmap(CFG);
    await teleport(sc);
    const t0 = performance.now();
    const ref = await grab(sc, 'ref');
    const rst = await frameStats(sc, 'ref');
    log(name, 'ref wall ms', r3(performance.now() - t0), 'steps/px', rst.steps_per_px, 'caps', rst.cap_share, 'sky', rst.sky_share, 'worst band ms', r3(W.worstBandMs));
    await snapshot(`d-${name}-ref`);
    let refStrict = null;
    for (const v of DEV_VARIANTS) {
      if (v === 'ref') continue;
      await useClipmap({ ...CFG, strict: v === 'hybrid' });
      await teleport(sc);
      if (v === 'hybrid') refStrict = (await grab(sc, 'ref')).px;
      const st = await frameStats(sc, v);
      const g = await grab(sc, v);
      const q = diff(g.px, v === 'hybrid' ? refStrict : ref.px);
      W.noShadow = true;
      const q2 = diff((await grab(sc, v)).px, (await grab(sc, 'ref')).px);
      W.noShadow = false;
      await snapshot(`d-${name}-${v}`);
      await diffImage(g.px, v === 'hybrid' ? refStrict : ref.px, `d-${name}-${v}-diff`);
      const m = await measure(sc, v, FULL, { frames: 30, warm: 10 });
      log(name, v, 'steps/px', st.steps_per_px, 'field/px', st.field_evals_per_px, 'empty/px', st.empty_steps_per_px, 'caps', st.cap_share, 'sky', st.sky_share,
        '| geom', m.geometry, m.tiled ? '(tiled)' : '', 'light', m.light, 'sky', m.sky, '| diff', JSON.stringify(q), '| unshadowed', JSON.stringify(q2));
    }
  }
}

/** Dev: depth differences against the reference (#depth=hillside). */
async function depthMode() {
  await setup();
  await placeWorld(RUIN_SITES);
  const views = await makeViews();
  const name = location.hash.split('=')[1] || 'hillside';
  const sc = views[name];
  await useClipmap(CFG);
  await teleport(sc);
  W.noShadow = true;
  const ref = await grab(sc, 'ref', FULL, { depth: true });
  for (const v of ['analytic', 'cache_fn']) {
    const g = await grab(sc, v, FULL, { depth: true });
    const n = g.depth.length;
    let a3 = 0, a4 = 0, a2 = 0, miss = 0, sum = 0;
    const c = document.createElement('canvas');
    c.width = 1920; c.height = 1080;
    const ctx2 = c.getContext('2d'), img = ctx2.createImageData(1920, 1080);
    for (let i = 0; i < n; i++) {
      const t0 = ref.depth[i], t1 = g.depth[i];
      let e = 0;
      if ((t0 > 1e29) !== (t1 > 1e29)) { miss++; e = 1; }
      else if (t0 < 1e29) { e = Math.abs(t1 - t0) / t0; sum += e; if (e > 1e-2) a2++; if (e > 1e-3) a3++; if (e > 1e-4) a4++; }
      const k = 4 * i;
      img.data[k] = e > 1e-3 ? 255 : 30; img.data[k + 1] = e > 1e-4 && e <= 1e-3 ? 220 : 30; img.data[k + 2] = (t1 > 1e29 && t0 > 1e29) ? 90 : 30; img.data[k + 3] = 255;
      if (e > 0.5) { img.data[k] = 255; img.data[k + 1] = 0; img.data[k + 2] = 255; }
    }
    ctx2.putImageData(img, 0, 0);
    await save(`depth-${name}-${v}.png`, await new Promise(r => c.toBlob(r, 'image/png')));
    log('depth', name, v, 'mean rel', r3(sum / n * 1e4) / 1e4, '>1e-2', r3(a2 / n), '>1e-3', r3(a3 / n), '>1e-4', r3(a4 / n), 'miss/hit flips', r3(miss / n));
  }
}

/** Dev: the longest teleport submit for each sweep configuration (GPU-safety check). */
async function teleportsMode() {
  await setup();
  await placeWorld(RUIN_SITES);
  const views = await makeViews();
  for (const cfg of [CFG, { ...CFG, c0: 0.0625 }, { ...CFG, V: 4000 }, { ...CFG, c0: 0.25 }]) {
    await useClipmap(cfg);
    for (const name of ['hillside', 'tower']) {
      const t = await teleport(views[name]);
      log('teleport', t.config, name, 'submits', t.submits, 'max submit wall ms', t.max_submit_wall_ms, 'zero fill ms', t.atlas_zero_fill_wall_ms, 'gpu', t.total_gpu_ms);
    }
  }
}

async function quick() {
  await setup();
  await placeWorld(RUIN_SITES);
  const views = await makeViews();
  await useClipmap(CFG);
  for (const name of ['hillside', 'tower', 'flyover']) {
    const sc = views[name];
    await useClipmap(CFG);
    log('teleport', name, JSON.stringify(await teleport(sc)));
    for (const v of VARIANTS) {
      if (v === 'hybrid') { await useClipmap({ ...CFG, strict: true }); await teleport(sc); }
      await render(sc, v);
      await snapshot(`q-${name}-${v}`);
    }
    await render(sc, 'hybrid', FULL, { view: 3 });
    await snapshot(`q-${name}-fieldevals-hybrid`);
    await useClipmap(CFG);
    await teleport(sc);
    await render(sc, 'cache', FULL, { view: 2 });
    await snapshot(`q-${name}-levels`);
    await render(sc, 'cache', FULL, { view: 1 });
    await snapshot(`q-${name}-steps-cache`);
  }
  await useClipmap({ ...CFG, parts: 1 });
  await teleport(views.hillside);
  for (const v of ['raster', 'raster_fn', 'raster_dn']) {
    await render(views.hillside, v);
    await snapshot(`q-hillside-${v}`);
  }
  log('quick done');
}

async function runAll() {
  const t0 = performance.now();
  await setup();
  await pipelineTiming();
  log('pipelines', JSON.stringify(R.pipelines));
  await placeWorld(RUIN_SITES);
  W.refs = {};
  const views = await makeViews();
  const cm = new Clipmap(CFG);
  R.config = {
    main: { ...CFG, levels: cm.nlev, brick_cells: 8, finest_brick_m: cm.b(0), coarsest_brick_m: cm.b(cm.nlev - 1),
      capacity_slots: cm.C, delta_m: cm.delta.map(r3), omitted_amp_m: cm.A.map(r3) },
    fovy_deg: FOVY * 180 / Math.PI, sun: SUN.map(r3), resolution: FULL, half: HALF,
    terrain: { assumed_slope_G: 3.0, K: T_K }, timings_contaminated: 'other spikes share this GPU',
  };
  const phase = async (name, fn) => { const t = performance.now(); await fn(views); R.phase_s = R.phase_s || {}; R.phase_s[name] = r3((performance.now() - t) / 1000); log('phase', name, R.phase_s[name], 's'); };
  await phase('main', mainConfig);
  await phase('riding_paced', ridingAndPaced);
  await phase('terrain', terrainPhase);
  await phase('sweeps', sweeps);
  await phase('precision', precision);
  await phase('shimmer', shimmer);
  R.memory_mb.main_cache = (await useClipmap(CFG)).memMB();
  R.total_s = r3((performance.now() - t0) / 1000);
  R.finished = new Date().toISOString();
  await save(`run-${R.started.replace(/[:.]/g, '-')}.json`, JSON.stringify(R, null, 2));
  log('done in', R.total_s, 's');
}

async function main() {
  const h = location.hash;
  try {
    if (h === '#map') await mapMode();
    else if (h.startsWith('#stats')) await statsMode();
    else if (h.startsWith('#depth')) await depthMode();
    else if (h === '#teleports') await teleportsMode();
    else if (h === '#quick') await quick();
    else if (h === '#run') await runAll();
    else return;
  } catch (e) {
    fail('FAILED:', e.stack || e.message);
    R.error = String(e.stack || e.message);
  }
  await save('DONE', R.error ? 'failed' : 'ok');
}

$('run').addEventListener('click', () => { location.hash = '#run'; location.reload(); });
main();
