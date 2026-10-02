// Spike 11 harness: builds the pipelines and the content, simulates, draws every technique, times all
// of it, compares against brute-force references, sweeps, and saves results and screenshots.
// This file plays the engine's role plus the measuring. It isn't compiler output.

import { makeCast, buildSystem, writeChar, writePins, bodyFrame, VIEWS, CHAR_STRIDE, MAX_CHARS, MAX_SHEETS, HAIR } from './scene.js';

const U = GPUBufferUsage, TU = GPUTextureUsage;
const GRID = 256;                       // light-grid cells per side
const CAP = 12 << 20;                   // bin list capacity (entries)
const PATCH = 1;                        // quads per patch side (default; the patch sweep also tries 2)
// Development options ride in the hash after '|' (headless.sh appends the hash to the page path):
// '#prof=courtyard:field|sub=2&it=3'.
const HASH = location.hash.split('|')[0];
const QP = new URLSearchParams(location.hash.split('|')[1] || '');
let SUBSTEPS = +(QP.get('sub') || 8), ITERS = +(QP.get('it') || 1);   // per 60 Hz frame ("small steps"); ITERS odd
const SIM_WG = +(QP.get('wg') || 256);
const WIND = [3.2, 0.0, -4.6], GUST = 0.8;
const SUN = (() => { const v = [0.66, 0.46, -0.30], l = Math.hypot(...v); return v.map(x => x / l); })();
const FRAME_BYTES = 352;
const TILE_W = 320, TILE_H = 64;        // reference renders: one submit per tile (GPU safety rules)
const PASSES = ['sim_cloth', 'sim_hair', 'build_cloth', 'build_hair', 'bin', 'smap', 'prepass', 'trace', 'raster'];
const QPF = PASSES.length * 2;
const GRIDS = { fine: [64, 96, 64], coarse: [32, 48, 32] };   // hair density grids
const gridOf = T => (T.hair === 'volume' ? 'fine' : 'coarse');
const LIGHT_DIV = { fine: 2, coarse: 1 };                       // both light grids 32×48×32
const gridConst = name => ({ HDX: GRIDS[name][0], HDY: GRIDS[name][1], HDZ: GRIDS[name][2], HLIGHT_DIV: LIGHT_DIV[name] });
const STAT_NAMES = ['cloth_px', 'char_px', 'ground_px', 'tower_px', 'sky_px', 'hair_px',
  'cloth_marches', 'cloth_steps', 'cloth_hits', 'cloth_misses', 'cloth_caps', 'cloth_candidates', 'cloth_entered', 'cloth_tris',
  'step_lt_1mm', 'step_1_4mm', 'step_4_16mm', 'step_16_64mm', 'step_ge_64mm', 'min_step_bits',
  'body_marches', 'body_steps', 'body_caps', 'strand_candidates', 'strand_layers', 'strand_px', 'volume_samples', 'volume_px',
  'shadow_rays', 'shadow_steps', 'shadow_caps', 'shadow_hair_samples', 'cloth_px_steps', 'max_cloth_steps_px'];

// Trace kernels (trace.wgsl overrides). r_* are the brute-force references.
const REFC = { REF: 1, STEP_SCALE: 0.5, EPS_SCALE: 0.025, MAX_STEPS: 4000, SHADOW_STEPS: 2000, LAYERS: 16, VOL_STEP: 0.25 };
const TRACE = {
  base: {},
  c1h2: { CLOTH: 1, HAIR: 2, HAIRSH: 1 }, c1h1: { CLOTH: 1, HAIR: 1, HAIRSH: 1 }, c2h1: { CLOTH: 2, HAIR: 1, HAIRSH: 1 },
  ras: { SMAP: 1, HAIRSH: 1, DEPTH_OUT: 1, RASTER_BOUND: 1 },
  c1h0: { CLOTH: 1 }, c2h0: { CLOTH: 2 }, rasc: { SMAP: 1, DEPTH_OUT: 1, RASTER_BOUND: 1 },
  c1h0n: { CLOTH: 1, CLOTHSH: 0 }, c2h0n: { CLOTH: 2, CLOTHSH: 0 }, c1h0s: { CLOTH: 1, FIELD_SOFT: 1 },
  c0h2: { HAIR: 2, HAIRSH: 1 }, c0h1: { HAIR: 1, HAIRSH: 1 }, rash: { HAIRSH: 1, DEPTH_OUT: 1, RASTER_BOUND: 1 },
  r_base: { ...REFC },
  r_c1h2: { CLOTH: 1, HAIR: 2, HAIRSH: 1, ...REFC }, r_c1h1: { CLOTH: 1, HAIR: 1, HAIRSH: 1, ...REFC },
  r_c2h1: { CLOTH: 2, HAIR: 1, HAIRSH: 1, ...REFC }, r_c1h0: { CLOTH: 1, ...REFC }, r_c2h0: { CLOTH: 2, ...REFC },
  r_c0h2: { HAIR: 2, HAIRSH: 1, ...REFC }, r_c0h1: { HAIR: 1, HAIRSH: 1, ...REFC },
  r_c0h2_light: { HAIR: 2, HAIRSH: 1, ...REFC, REF_LIGHT: 1 }, r_c0h1_light: { HAIR: 1, HAIRSH: 1, ...REFC, REF_LIGHT: 1 },
  s_c1h2: { CLOTH: 1, HAIR: 2, HAIRSH: 1, STATS: 1 }, s_c1h1: { CLOTH: 1, HAIR: 1, HAIRSH: 1, STATS: 1 },
  s_c2h1: { CLOTH: 2, HAIR: 1, HAIRSH: 1, STATS: 1 },
  v_c1h2: { CLOTH: 1, HAIR: 2, HAIRSH: 1, VIEW: 1 }, v_c2h1: { CLOTH: 2, HAIR: 1, HAIRSH: 1, VIEW: 2 },
};

// Techniques: what's simulated and drawn how, which trace kernel, which reference.
const TECHS = {
  base: { trace: 'base' },
  field: { cloth: 'field', hair: 'volume', trace: 'c1h2', ref: 'r_field', stats: 's_c1h2', heat: 'v_c1h2' },
  field_strands: { cloth: 'field', hair: 'strands', trace: 'c1h1', ref: 'r_field_strands', stats: 's_c1h1' },
  tri_strands: { cloth: 'tri', hair: 'strands', trace: 'c2h1', ref: 'r_tri_strands', stats: 's_c2h1', heat: 'v_c2h1' },
  raster: { cloth: 'raster', hair: 'raster', trace: 'ras', ref: 'r_tri_strands' },
  cloth_field: { cloth: 'field', trace: 'c1h0', ref: 'r_cloth_field' },
  cloth_tri: { cloth: 'tri', trace: 'c2h0', ref: 'r_cloth_tri' },
  cloth_raster: { cloth: 'raster', trace: 'rasc', ref: 'r_cloth_tri' },
  cloth_field_nosh: { cloth: 'field', trace: 'c1h0n' },
  cloth_tri_nosh: { cloth: 'tri', trace: 'c2h0n' },
  cloth_field_softsh: { cloth: 'field', trace: 'c1h0s' },
  hair_volume: { hair: 'volume', trace: 'c0h2', ref: 'r_hair_volume' },
  hair_strands: { hair: 'strands', trace: 'c0h1', ref: 'r_hair_strands' },
  hair_raster: { hair: 'raster', trace: 'rash', ref: 'r_hair_strands' },
  r_base: { trace: 'r_base', isRef: true },
  r_field: { cloth: 'field', hair: 'volume', trace: 'r_c1h2', isRef: true },
  r_field_strands: { cloth: 'field', hair: 'strands', trace: 'r_c1h1', isRef: true },
  r_tri_strands: { cloth: 'tri', hair: 'strands', trace: 'r_c2h1', isRef: true },
  r_cloth_field: { cloth: 'field', trace: 'r_c1h0', isRef: true },
  r_cloth_tri: { cloth: 'tri', trace: 'r_c2h0', isRef: true },
  r_hair_volume: { hair: 'volume', trace: 'r_c0h2', isRef: true },
  r_hair_strands: { hair: 'strands', trace: 'r_c0h1', isRef: true },
  r_hair_volume_light: { hair: 'volume', trace: 'r_c0h2_light', isRef: true },
  r_hair_strands_light: { hair: 'strands', trace: 'r_c0h1_light', isRef: true },
  hair_volume_lightref: { hair: 'volume', trace: 'r_c0h2', ref: 'r_hair_volume_light', isRef: true },
  hair_strands_lightref: { hair: 'strands', trace: 'r_c0h1', ref: 'r_hair_strands_light', isRef: true },
};

const $ = id => document.getElementById(id);
const T0 = performance.now();
const stamp = () => ((performance.now() - T0) / 1000).toFixed(1).padStart(6) + 's';
const log = (...a) => { const s = a.join(' '); $('log').textContent += s + '\n'; console.log(`[${stamp()}] ${s}`); };
const logErr = (...a) => { const s = a.join(' '); $('log').textContent += 'ERROR ' + s + '\n'; console.error(`[${stamp()}] ERROR ${s}`); };
const R = { started: new Date().toISOString(), device: {}, config: {}, pipelines: {}, content: {}, frames: {}, stats: {}, quality: {}, sweeps: {}, paced: {}, budget: {} };
window.__results = R;

let device, ctx, canvasFormat;
const SRC = {}, L = {}, P = { trace: {} }, TG = {};
let world = null;

const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[s.length >> 1] : NaN; };
const mean = a => a.reduce((s, x) => s + x, 0) / Math.max(a.length, 1);
const pct = (a, q) => { const s = [...a].sort((x, y) => x - y); return s[Math.min(s.length - 1, Math.floor(s.length * q))]; };
const r3 = x => Math.round(x * 1000) / 1000;
const norm = a => { const l = Math.hypot(...a); return a.map(x => x / l); };
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sub = (a, b) => a.map((x, i) => x - b[i]);
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

function buffer(size, usage, data) {
  const b = device.createBuffer({ size: Math.max(16, Math.ceil(size / 16) * 16), usage, mappedAtCreation: !!data });
  if (data) { new data.constructor(b.getMappedRange()).set(data); b.unmap(); }
  return b;
}

async function readback(copies) {
  const total = copies.reduce((s, c) => s + c.size, 0);
  const rb = device.createBuffer({ size: Math.max(16, total), usage: U.MAP_READ | U.COPY_DST });
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
  try { await fetch(`results/${name}`, { method: 'PUT', body }); } catch (e) { logErr('save failed', name, e.message); }
}

// ---- setup -------------------------------------------------------------------------------------------

async function init() {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
  if (!adapter) throw new Error('no WebGPU adapter');
  const want = ['timestamp-query'].filter(f => adapter.features.has(f));
  const lim = {};
  for (const k of ['maxStorageBuffersPerShaderStage', 'maxStorageBufferBindingSize', 'maxBufferSize', 'maxComputeWorkgroupStorageSize',
    'maxStorageBuffersInFragmentStage', 'maxStorageBuffersInVertexStage', 'maxStorageTexturesPerShaderStage',
    'maxComputeInvocationsPerWorkgroup', 'maxComputeWorkgroupSizeX']) {
    if (k in adapter.limits) lim[k] = adapter.limits[k];
  }
  device = await adapter.requestDevice({ requiredFeatures: want, requiredLimits: lim });
  device.lost.then(i => logErr('DEVICE LOST:', i.message));
  device.addEventListener('uncapturederror', e => logErr('GPU ERROR:', e.error.message));
  const info = adapter.info || {};
  R.device = { vendor: info.vendor, architecture: info.architecture, description: info.description, userAgent: navigator.userAgent,
    timestampQuery: want.includes('timestamp-query'), pageVisibility: document.visibilityState, limits: lim };
  if (!R.device.timestampQuery) throw new Error('timestamp-query is required');
  for (const f of ['common', 'world', 'trace', 'raster', 'sim', 'build', 'hair', 'bin', 'blit']) {
    SRC[f] = await (await fetch(`${f}.wgsl`)).text();
  }
  const canvas = $('view');
  canvas.width = 1920;
  canvas.height = 1080;
  ctx = canvas.getContext('webgpu');
  canvasFormat = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format: canvasFormat, alphaMode: 'opaque' });
  makeLayouts();
}

function makeLayouts() {
  const C = GPUShaderStage.COMPUTE, V = GPUShaderStage.VERTEX, Fr = GPUShaderStage.FRAGMENT, CVF = C | V | Fr;
  const bgl = entries => device.createBindGroupLayout({ entries });
  const buf = (binding, visibility, type, extra = {}) => ({ binding, visibility, buffer: { type, ...extra } });
  const tex = (binding, visibility, sampleType, viewDimension = '2d') => ({ binding, visibility, texture: { sampleType, viewDimension } });
  const sto = (binding, visibility, format, viewDimension = '2d') => ({ binding, visibility, storageTexture: { access: 'write-only', format, viewDimension } });
  L.sim = bgl([buf(0, C, 'uniform', { hasDynamicOffset: true }), buf(1, C, 'storage'), buf(2, C, 'storage'),
    ...[3, 4, 5, 6, 7].map(b => buf(b, C, 'read-only-storage'))]);
  L.build = bgl([buf(0, C, 'uniform'), buf(1, C, 'read-only-storage'), buf(2, C, 'read-only-storage'),
    buf(3, C, 'storage'), buf(4, C, 'storage'), buf(5, C, 'storage')]);
  L.hair0 = bgl([buf(0, C, 'uniform'), buf(1, C, 'read-only-storage'), buf(2, C, 'read-only-storage'), buf(3, C, 'storage'),
    buf(4, C, 'storage'), buf(5, C, 'read-only-storage'), buf(6, C, 'storage'), buf(7, C, 'storage')]);
  L.hres = bgl([sto(0, C, 'rgba16float', '3d')]);
  L.hskip = bgl([sto(1, C, 'r32float', '3d')]);
  L.hlight = bgl([tex(2, C, 'float', '3d'), { binding: 3, visibility: C, sampler: { type: 'filtering' } },
    tex(4, C, 'unfilterable-float', '3d'), sto(5, C, 'rgba16float', '3d')]);
  L.bin = bgl([buf(0, C, 'uniform'), buf(1, C, 'read-only-storage'), buf(2, C, 'read-only-storage'), buf(3, C, 'read-only-storage'),
    buf(4, C, 'storage'), buf(5, C, 'storage'), buf(6, C, 'storage'), buf(7, C, 'storage'), buf(8, C, 'storage')]);
  L.g0 = bgl([buf(0, CVF, 'uniform'), buf(1, CVF, 'uniform'), ...[2, 3, 4, 5, 6, 7].map(b => buf(b, CVF, 'read-only-storage')),
    tex(8, CVF, 'float', '3d'), tex(9, CVF, 'float', '3d'), tex(10, CVF, 'unfilterable-float', '3d'),
    { binding: 11, visibility: CVF, sampler: { type: 'filtering' } }, tex(12, CVF, 'depth'),
    { binding: 13, visibility: CVF, sampler: { type: 'comparison' } }]);
  L.trace1 = bgl([sto(0, C, 'rgba16float'), sto(1, C, 'r32float'), buf(2, C, 'storage'), buf(3, C, 'read-only-storage'), buf(4, C, 'read-only-storage'),
    tex(5, C, 'depth')]);
  L.raster1 = bgl([buf(3, V | Fr, 'read-only-storage'), buf(4, V | Fr, 'read-only-storage')]);
  L.depth2 = bgl([tex(0, Fr, 'unfilterable-float')]);
  L.smap0 = bgl([buf(0, V, 'uniform'), buf(4, V, 'read-only-storage')]);
  L.blit = bgl([tex(0, Fr, 'float'), { binding: 1, visibility: Fr, sampler: { type: 'filtering' } }]);
}

/** A module from concatenated files, with compile messages mapped back to file:line. */
async function moduleOf(label, files, salt) {
  let code = '';
  const starts = [];
  for (const f of files) {
    let s = SRC[f];
    if (f === 'common' && salt != null) s = s.replace('const SALT: f32 = 0.0;', `const SALT: f32 = ${salt.toFixed(4)};`);
    starts.push([f, code === '' ? 1 : code.split('\n').length + 1]);
    code += (code === '' ? '' : '\n') + s;
  }
  const module = device.createShaderModule({ code, label });
  const info = await module.getCompilationInfo();
  let errors = 0;
  for (const m of info.messages) {
    let file = label, line = m.lineNum;
    for (const [f, s] of starts) if (m.lineNum >= s) { file = f; line = m.lineNum - s + 1; }
    const text = `${label}: ${m.type} ${file}.wgsl:${line}:${m.linePos} ${m.message}`;
    if (m.type === 'error') { errors++; logErr(text); } else log(text);
  }
  if (errors) throw new Error(`${label}: ${errors} WGSL error(s)`);
  return module;
}

async function makePipelines() {
  const t0 = performance.now();
  const [simM, buildM, hairM, binM, traceM, rasterM, blitM] = await Promise.all([
    moduleOf('sim', ['common', 'sim']), moduleOf('build', ['common', 'build']), moduleOf('hair', ['common', 'hair']),
    moduleOf('bin', ['common', 'bin']), moduleOf('trace', ['common', 'world', 'trace']),
    moduleOf('raster', ['common', 'world', 'raster']), moduleOf('blit', ['blit'])]);
  R.pipelines.modules_ms = r3(performance.now() - t0);
  const pl = (...g) => device.createPipelineLayout({ bindGroupLayouts: g });
  const comp = (module, entryPoint, layout, constants) => () => device.createComputePipelineAsync({ layout, compute: { module, entryPoint, constants } });
  const jobs = {
    sim: comp(simM, 'simulate', pl(L.sim), { WG: SIM_WG }),
    cloth_normals: comp(buildM, 'cloth_normals', pl(L.build)),
    patch_bounds: comp(buildM, 'patch_bounds', pl(L.build), { P: PATCH }),
    sheet_bounds: comp(buildM, 'sheet_bounds', pl(L.build), { P: PATCH }),
    patch_bounds_p2: comp(buildM, 'patch_bounds', pl(L.build), { P: 2 }),
    sheet_bounds_p2: comp(buildM, 'sheet_bounds', pl(L.build), { P: 2 }),
    hair_bounds_fine: comp(hairM, 'hair_bounds', pl(L.hair0), gridConst('fine')),
    hair_children_fine: comp(hairM, 'hair_children', pl(L.hair0), gridConst('fine')),
    hair_resolve_fine: comp(hairM, 'hair_resolve', pl(L.hair0, L.hres), gridConst('fine')),
    hair_skip_fine: comp(hairM, 'hair_skip', pl(L.hair0, L.hskip), gridConst('fine')),
    hair_light_fine: comp(hairM, 'hair_light', pl(L.hair0, L.hlight), gridConst('fine')),
    strand_bounds_fine: comp(hairM, 'strand_bounds', pl(L.hair0), gridConst('fine')),
    hair_bounds_coarse: comp(hairM, 'hair_bounds', pl(L.hair0), gridConst('coarse')),
    hair_children_coarse: comp(hairM, 'hair_children', pl(L.hair0), gridConst('coarse')),
    hair_resolve_coarse: comp(hairM, 'hair_resolve', pl(L.hair0, L.hres), gridConst('coarse')),
    hair_skip_coarse: comp(hairM, 'hair_skip', pl(L.hair0, L.hskip), gridConst('coarse')),
    hair_light_coarse: comp(hairM, 'hair_light', pl(L.hair0, L.hlight), gridConst('coarse')),
    strand_bounds_coarse: comp(hairM, 'strand_bounds', pl(L.hair0), gridConst('coarse')),
    bin_count: comp(binM, 'bin_items', pl(L.bin), { FILL: 0 }),
    bin_fill: comp(binM, 'bin_items', pl(L.bin), { FILL: 1 }),
    bin_big_count: comp(binM, 'bin_big', pl(L.bin), { FILL: 0 }),
    bin_big_fill: comp(binM, 'bin_big', pl(L.bin), { FILL: 1 }),
    scan_reduce: comp(binM, 'scan_reduce', pl(L.bin)),
    scan_blocks: comp(binM, 'scan_blocks', pl(L.bin)),
    scan_down: comp(binM, 'scan_down', pl(L.bin)),
    smap: () => device.createRenderPipelineAsync({
      layout: pl(L.smap0), vertex: { module: rasterM, entryPoint: 'smap_vs', constants: { P: PATCH, ...gridConst('coarse') } },
      primitive: { topology: 'triangle-list', cullMode: 'none' },
      depthStencil: { format: 'depth32float', depthCompare: 'less', depthWriteEnabled: true, depthBias: 2, depthBiasSlopeScale: 1.5 },
    }),
    cloth_prepass: () => device.createRenderPipelineAsync({
      layout: pl(L.g0, L.raster1), vertex: { module: rasterM, entryPoint: 'cloth_vs', constants: { P: PATCH, ...gridConst('coarse') } },
      primitive: { topology: 'triangle-list', cullMode: 'none' },
      depthStencil: { format: 'depth32float', depthCompare: 'less', depthWriteEnabled: true },
    }),
    hair_prepass: () => device.createRenderPipelineAsync({
      layout: pl(L.g0, L.raster1), vertex: { module: rasterM, entryPoint: 'hair_vs', constants: { P: PATCH, ...gridConst('coarse') } },
      primitive: { topology: 'triangle-list', cullMode: 'none' },
      depthStencil: { format: 'depth32float', depthCompare: 'less', depthWriteEnabled: true },
    }),
    world_depth: () => device.createRenderPipelineAsync({
      layout: pl(L.g0, L.raster1, L.depth2), vertex: { module: rasterM, entryPoint: 'full_vs', constants: { P: PATCH, ...gridConst('coarse') } },
      fragment: { module: rasterM, entryPoint: 'world_depth_fs', constants: { P: PATCH, ...gridConst('coarse') }, targets: [{ format: 'rgba16float', writeMask: 0 }] },
      depthStencil: { format: 'depth32float', depthCompare: 'less', depthWriteEnabled: true },
    }),
    cloth_raster: () => device.createRenderPipelineAsync({
      layout: pl(L.g0, L.raster1, L.depth2), vertex: { module: rasterM, entryPoint: 'cloth_vs', constants: { P: PATCH, ...gridConst('coarse') } },
      fragment: { module: rasterM, entryPoint: 'cloth_fs', constants: { P: PATCH, ...gridConst('coarse') }, targets: [{ format: 'rgba16float' }] },
      primitive: { topology: 'triangle-list', cullMode: 'none' },
      depthStencil: { format: 'depth32float', depthCompare: 'equal', depthWriteEnabled: false },
    }),
    hair_raster: () => device.createRenderPipelineAsync({
      layout: pl(L.g0, L.raster1, L.depth2), vertex: { module: rasterM, entryPoint: 'hair_vs', constants: { P: PATCH, ...gridConst('coarse') } },
      fragment: { module: rasterM, entryPoint: 'hair_fs', constants: { P: PATCH, ...gridConst('coarse') }, targets: [{ format: 'rgba16float' }] },
      primitive: { topology: 'triangle-list', cullMode: 'none' },
      depthStencil: { format: 'depth32float', depthCompare: 'equal', depthWriteEnabled: false },
    }),
    blit: () => device.createRenderPipelineAsync({
      layout: pl(L.blit), vertex: { module: blitM, entryPoint: 'blit_vs' },
      fragment: { module: blitM, entryPoint: 'blit_fs', targets: [{ format: canvasFormat }] },
    }),
  };
  // At most 3 compiles at once (GPU safety rules: dozens of concurrent Metal compilers starved the display).
  const names = Object.keys(jobs);
  for (let i = 0; i < names.length; i += 3) {
    const chunk = names.slice(i, i + 3);
    const made = await Promise.all(chunk.map(async n => {
      try { return await jobs[n](); } catch (e) { logErr(`pipeline ${n}:`, e.message); throw e; }
    }));
    chunk.forEach((n, k) => { P[n] = made[k]; });
  }
  P.traceModule = traceM;
  P.traceLayout = pl(L.g0, L.trace1);
  R.pipelines.base_ms = r3(performance.now() - t0);
  R.pipelines.base_count = names.length;
  R.pipelines.trace_ms = {};
  log('pipelines', JSON.stringify(R.pipelines));
}

/** Trace kernels are compiled when first needed, at most 3 at a time. */
async function ensureTrace(keys) {
  keys = keys.map(k => (k && world && world.P !== PATCH ? `${k}@P${world.P}` : k));
  const missing = [...new Set(keys)].filter(k => k && !P.trace[k]);
  for (let i = 0; i < missing.length; i += 3) {
    await Promise.all(missing.slice(i, i + 3).map(async k => {
      const t0 = performance.now();
      const [base, pq] = k.split('@P');
      const grid = TRACE[base].HAIR === 2 ? 'fine' : 'coarse';
      P.trace[k] = await device.createComputePipelineAsync({ layout: P.traceLayout, compute: { module: P.traceModule, entryPoint: 'trace',
        constants: { P: pq ? +pq : PATCH, ...gridConst(grid), ...TRACE[base] } } });
      R.pipelines.trace_ms[k] = r3(performance.now() - t0);
    }));
  }
}

// ---- render targets (one per internal resolution) -----------------------------------------------------------

function makeTarget(name, w, h) {
  const tx = Math.ceil(w / 8), ty = Math.ceil(h / 8);
  const cells = (tx * ty + GRID * GRID) * 3;
  const t = { name, w, h, tx, ty, cells, blocks: Math.ceil(cells / 1024) };
  t.out = device.createTexture({ size: [w, h], format: 'rgba16float', usage: TU.STORAGE_BINDING | TU.TEXTURE_BINDING | TU.RENDER_ATTACHMENT | TU.COPY_SRC });
  t.tdepth = device.createTexture({ size: [w, h], format: 'r32float', usage: TU.STORAGE_BINDING | TU.TEXTURE_BINDING });
  t.z = device.createTexture({ size: [w, h], format: 'depth32float', usage: TU.RENDER_ATTACHMENT | TU.TEXTURE_BINDING });
  t.outView = t.out.createView();
  t.zView = t.z.createView();
  t.bcount = buffer(cells * 4, U.STORAGE | U.COPY_DST);
  t.boff = buffer((cells + 1) * 4, U.STORAGE | U.COPY_SRC);
  t.blist = buffer(CAP * 4, U.STORAGE);
  t.bsum = buffer(4096 * 4, U.STORAGE);
  t.big = buffer(65536 * 4, U.STORAGE | U.COPY_DST);
  t.stats = buffer(64 * 4, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  const res = b => ({ buffer: b });
  t.bgTrace = device.createBindGroup({ layout: L.trace1, entries: [
    { binding: 0, resource: t.outView }, { binding: 1, resource: t.tdepth.createView() }, { binding: 2, resource: res(t.stats) },
    { binding: 3, resource: res(t.boff) }, { binding: 4, resource: res(t.blist) }, { binding: 5, resource: t.zView }] });
  t.bgRaster = device.createBindGroup({ layout: L.raster1, entries: [{ binding: 3, resource: res(t.boff) }, { binding: 4, resource: res(t.blist) }] });
  t.bgDepth = device.createBindGroup({ layout: L.depth2, entries: [{ binding: 0, resource: t.tdepth.createView() }] });
  t.bgBlit = device.createBindGroup({ layout: L.blit, entries: [{ binding: 0, resource: t.outView },
    { binding: 1, resource: device.createSampler({ magFilter: 'linear', minFilter: 'linear' }) }] });
  return t;
}

// ---- content ------------------------------------------------------------------------------------------------

let worldId = 0;

function makeWorld(opts) {
  if (world) world.destroy();
  const t0 = performance.now();
  const cast = makeCast(opts.npcs);
  const pq = opts.P || PATCH;
  const sys = buildSystem(cast, opts.cloth, opts.strands, pq);
  const W = { id: worldId++, opts, cast, sys, t: 0, N: sys.N, P: pq, nsegs: opts.strands * HAIR.segsPerStrand, bufs: [] };
  const B = (size, usage, data) => { const b = buffer(size, usage, data); W.bufs.push(b); return b; };
  const N = sys.N;
  const pb = new Float32Array(N * 8);
  pb.set(sys.pos0, 0);
  pb.set(sys.pos0, N * 4);
  W.pbuf = B(N * 32, U.STORAGE | U.COPY_DST | U.COPY_SRC, pb);
  W.prev = B(N * 16, U.STORAGE | U.COPY_DST, sys.pos0);
  W.pinfo = B(N * 32, U.STORAGE, sys.pinfo);
  W.cons = B(sys.nCons * 16, U.STORAGE, sys.cons);
  W.npins = sys.pins.length;
  W.pins = B(W.npins * 32, U.STORAGE | U.COPY_DST);
  W.pinData = new Float32Array(W.npins * 8);
  W.chars = B(MAX_CHARS * CHAR_STRIDE * 16, U.STORAGE | U.COPY_DST);
  W.charData = new Float32Array(MAX_CHARS * CHAR_STRIDE * 4);
  const groups = new Uint32Array(sys.groups.length * 4);
  sys.groups.forEach((g, i) => groups.set(g, i * 4));
  W.groups = B(groups.byteLength, U.STORAGE, groups);
  W.attr = B(N * 32, U.STORAGE | U.COPY_DST, sys.attr);
  const prStrand = 2 + 3 * sys.nPatches, prInfo = prStrand + 2 * opts.strands;
  const prims = new Float32Array((prInfo + sys.nPatches) * 4);
  new Uint32Array(prims.buffer).set(sys.patchInfo, prInfo * 4);
  W.prims = B(prims.byteLength, U.STORAGE | U.COPY_DST, prims);
  W.segs = B(W.nsegs * 32, U.STORAGE);
  W.children = B(sys.children.byteLength, U.STORAGE, sys.children);
  W.sheetb = B(MAX_SHEETS * 24, U.STORAGE | U.COPY_DST);
  const mats = sys.mats.slice();
  sys.sheets.forEach((sh, si) => { mats[si * 16 + 12] = sh.patch0; mats[si * 16 + 13] = sh.patchCount; mats[si * 16 + 14] = sh.nu; mats[si * 16 + 15] = sh.wrap ? 1 : 0; });
  W.mats = B(mats.byteLength, U.UNIFORM, mats);
  W.tris = B(sys.tris.byteLength, U.INDEX, sys.tris);
  W.frame = B(FRAME_BYTES, U.UNIFORM | U.COPY_DST);
  W.simU = B(512, U.UNIFORM | U.COPY_DST);
  const tex3 = (size, format, usage) => { const t = device.createTexture({ size, dimension: '3d', format, usage }); W.bufs.push(t); return t; };
  W.smap = device.createTexture({ size: [2048, 2048], format: 'depth32float', usage: TU.RENDER_ATTACHMENT | TU.TEXTURE_BINDING });
  W.bufs.push(W.smap);
  W.smapView = W.smap.createView();
  const lin = device.createSampler({ magFilter: 'linear', minFilter: 'linear', addressModeU: 'clamp-to-edge', addressModeV: 'clamp-to-edge', addressModeW: 'clamp-to-edge' });
  const cmp = device.createSampler({ compare: 'less', magFilter: 'linear', minFilter: 'linear' });
  const res = (b, offset = 0, size) => ({ buffer: b, offset, size });
  const ents = list => list.map(([binding, resource]) => ({ binding, resource }));
  const xs = res(W.pbuf, 0, N * 16);
  W.bg = {
    sim: device.createBindGroup({ layout: L.sim, entries: ents([[0, res(W.simU, 0, 48)], [1, res(W.pbuf)], [2, res(W.prev)],
      [3, res(W.pinfo)], [4, res(W.cons)], [5, res(W.pins)], [6, res(W.chars)], [7, res(W.groups)]]) }),
    build: device.createBindGroup({ layout: L.build, entries: ents([[0, res(W.frame)], [1, xs], [2, res(W.pinfo)],
      [3, res(W.attr)], [4, res(W.prims)], [5, res(W.sheetb)]]) }),
    smap0: device.createBindGroup({ layout: L.smap0, entries: ents([[0, res(W.frame)], [4, xs]]) }),
    bin: {},
  };
  // Two hair density grids: fine for volume hair, coarse where hair only casts shadows.
  W.grids = {};
  for (const [name, D] of Object.entries(GRIDS)) {
    const G = { dims: D };
    G.splat = B(D[0] * D[1] * D[2] * 16, U.STORAGE);
    G.occ = B((D[0] / 4) * (D[1] / 4) * (D[2] / 4) * 4, U.STORAGE | U.COPY_DST);
    G.hvol = tex3(D, 'rgba16float', TU.STORAGE_BINDING | TU.TEXTURE_BINDING);
    G.hlight = tex3(D.map(x => x / LIGHT_DIV[name]), 'rgba16float', TU.STORAGE_BINDING | TU.TEXTURE_BINDING);
    G.hskip = tex3(D.map(x => x / 4), 'r32float', TU.STORAGE_BINDING | TU.TEXTURE_BINDING);
    G.hair0 = device.createBindGroup({ layout: L.hair0, entries: ents([[0, res(W.frame)], [1, xs], [2, res(W.chars)], [3, res(W.prims)],
      [4, res(W.segs)], [5, res(W.children)], [6, res(G.splat)], [7, res(G.occ)]]) });
    G.hres = device.createBindGroup({ layout: L.hres, entries: ents([[0, G.hvol.createView()]]) });
    G.hskip_bg = device.createBindGroup({ layout: L.hskip, entries: ents([[1, G.hskip.createView()]]) });
    G.hlight_bg = device.createBindGroup({ layout: L.hlight, entries: ents([[2, G.hvol.createView()], [3, lin], [4, G.hskip.createView()], [5, G.hlight.createView()]]) });
    G.g0 = device.createBindGroup({ layout: L.g0, entries: ents([[0, res(W.frame)], [1, res(W.mats)], [2, res(W.prims)], [3, res(W.chars)],
      [4, xs], [5, res(W.attr)], [6, res(W.segs)], [7, res(W.sheetb)], [8, G.hvol.createView()], [9, G.hlight.createView()],
      [10, G.hskip.createView()], [11, lin], [12, W.smapView], [13, cmp]]) });
    W.grids[name] = G;
  }
  for (const t of Object.values(TG)) {
    W.bg.bin[t.name] = device.createBindGroup({ layout: L.bin, entries: ents([[0, res(W.frame)], [1, res(W.prims)], [2, res(W.chars)],
      [3, res(W.segs)], [4, res(t.bcount)], [5, res(t.boff)], [6, res(t.blist)], [7, res(t.bsum)], [8, res(t.big)]]) });
  }
  W.destroy = () => W.bufs.forEach(b => b.destroy());
  W.sheetInit = new Uint32Array(MAX_SHEETS * 6);
  for (let s = 0; s < MAX_SHEETS; s++) W.sheetInit.fill(0xffffffff, s * 6, s * 6 + 3);
  const sheetParticles = sys.sheets.map(s => s.nu * s.nv);
  R.content[contentKey(opts)] = {
    ...opts, characters: cast.length, sheets: sys.sheets.length, cloth_particles: sys.clothCount, hair_guides: HAIR.guides,
    hair_particles: sys.hairCount, constraints: sys.nCons, patches: sys.nPatches, cloth_triangles: sys.tris.length / 3,
    render_strands: opts.strands, strand_segments: W.nsegs, strand_radius_m: sys.strandRadius,
    cape_particles: `${sys.sheets[0].nu}×${sys.sheets[0].nv}`, particles_per_sheet_median: median(sheetParticles),
    sim_groups: sys.groups.length, build_ms: r3(performance.now() - t0),
  };
  world = W;
  return W;
}

const contentKey = o => `npcs${o.npcs}_cloth${o.cloth}_strands${o.strands}${o.P ? `_P${o.P}` : ''}`;

// ---- per frame -----------------------------------------------------------------------------------------------

function poseChars(W, t) {
  const frames = W.cast.map(c => bodyFrame(c, t));
  W.cast.forEach((c, i) => writeChar(c, frames[i], W.charData, i * CHAR_STRIDE * 4));
  device.queue.writeBuffer(W.chars, 0, W.charData, 0, W.cast.length * CHAR_STRIDE * 4);
  return frames;
}

/** Advances the simulation's inputs from W.t to W.t + 1/60: bodies, pins, sim uniform. */
function advance(W) {
  const t0 = W.t, t1 = t0 + 1 / 60;
  const f0 = W.cast.map(c => bodyFrame(c, t0));
  const f1 = poseChars(W, t1);
  writePins(W.sys, f0, W.pinData, 0);
  writePins(W.sys, f1, W.pinData, W.npins * 4);
  device.queue.writeBuffer(W.pins, 0, W.pinData);
  const su = new ArrayBuffer(512), f = new Float32Array(su), u = new Uint32Array(su);
  for (const [slot, g0] of [[0, 0], [64, W.sys.clothGroups]]) {
    f[slot] = t0; f[slot + 1] = 1 / 60; u[slot + 2] = SUBSTEPS; u[slot + 3] = ITERS;
    f.set([...WIND, GUST], slot + 4);
    u[slot + 8] = W.N; u[slot + 9] = g0; u[slot + 10] = W.npins;
  }
  device.queue.writeBuffer(W.simU, 0, su);
  W.t = t1;
}

function lookAt(eye, target) {
  const f = norm(sub(target, eye));
  const s = norm(cross(f, [0, 1, 0]));
  const u = cross(s, f);
  return { f, s, u, m: [s[0], u[0], -f[0], 0, s[1], u[1], -f[1], 0, s[2], u[2], -f[2], 0, -dot(s, eye), -dot(u, eye), dot(f, eye), 1] };
}

function mul4(a, b) {
  const o = new Array(16).fill(0);
  for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) for (let k = 0; k < 4; k++) o[c * 4 + r] += a[k * 4 + r] * b[c * 4 + k];
  return o;
}

/** Writes the frame uniform for a view, target and technique at time t (band: first row of a dispatch band). */
function writeFrame(W, viewName, tg, techName, band = 0, col = 0) {
  const v = VIEWS[viewName], T = TECHS[techName];
  const { w, h } = tg;
  const cam = lookAt(v.eye, v.target);
  const ty = Math.tan(v.fovy * Math.PI / 360), tx = ty * w / h;
  const near = 0.05, far = 1000, fy = 1 / ty;
  const proj = [fy * h / w, 0, 0, 0, 0, fy, 0, 0, 0, 0, far / (near - far), -1, 0, 0, near * far / (near - far), 0];
  const viewproj = mul4(proj, cam.m);
  const lz = SUN, lx = norm(cross([0, 1, 0], lz)), ly = cross(lz, lx);
  const e = v.extent, D = 40, c = v.centre;
  const lightvp = [lx[0] / e, ly[0] / e, -lz[0] / (2 * D), 0, lx[1] / e, ly[1] / e, -lz[1] / (2 * D), 0,
    lx[2] / e, ly[2] / e, -lz[2] / (2 * D), 0, -dot(lx, c) / e, -dot(ly, c) / e, (D + dot(lz, c)) / (2 * D), 1];
  const buf = new ArrayBuffer(FRAME_BYTES), f = new Float32Array(buf), u = new Uint32Array(buf);
  f.set([...v.eye, 2 * ty / h], 0);
  f.set([...cam.f, 0], 4);
  f.set([...cam.s.map(x => x * tx), 0], 8);
  f.set([...cam.u.map(x => x * ty), 0], 12);
  f.set([...SUN, W.t], 16);
  f.set([...c, e], 20);
  f.set([...lx, 0], 24);
  f.set([...ly, 0], 28);
  u.set([w, h, tg.tx, tg.ty], 32);
  u.set([GRID, W.cast.length, W.sys.nPatches, W.nsegs], 36);
  u.set([W.opts.strands, HAIR.segsPerStrand, W.sys.sheets.length, band], 40);
  f.set([...WIND, W.sys.strandRadius], 44);
  f.set(viewproj, 48);
  f.set(lightvp, 64);
  const traced = T.cloth === 'field' || T.cloth === 'tri';
  const flags = (traced ? 3 : 0) | (T.hair === 'strands' ? 4 : 0);
  u.set([tg.tx * tg.ty, CAP, flags, W.sys.hairBase], 80);
  u.set([HAIR.guides, HAIR.perGuide, W.sys.clothCount, col], 84);
  device.queue.writeBuffer(W.frame, 0, buf);
}

/**
 * Records one frame: simulation (optional), preparation, binning, the trace and the raster fallback.
 * With `prof`, every dispatch gets its own timed pass (development profiling of single kernels).
 */
function encodeFrame(enc, W, tg, techName, { sim = true, qs = null, q0 = 0, traceKey = null, skipTrace = false, prof = null } = {}) {
  const T = TECHS[techName];
  const tsw = i => (qs ? { timestampWrites: { querySet: qs, beginningOfPassWriteIndex: q0 + 2 * i, endOfPassWriteIndex: q0 + 2 * i + 1 } } : {});
  const ptw = name => {
    const k = prof.names.length;
    prof.names.push(name);
    return { timestampWrites: { querySet: prof.qs, beginningOfPassWriteIndex: prof.q0 + 2 * k, endOfPassWriteIndex: prof.q0 + 2 * k + 1 } };
  };
  const compute = (i, fn) => {
    if (prof) {
      fn((name, cb) => { const p = enc.beginComputePass(ptw(name)); cb(p); p.end(); });
      return;
    }
    const p = enc.beginComputePass(tsw(i));
    fn((name, cb) => cb(p));
    p.end();
  };
  const render = (i, name, desc) => enc.beginRenderPass({ ...desc, ...(prof ? ptw(name) : tsw(i)) });
  const S = W.sys;
  const gname = gridOf(T), G = W.grids[gname], HDg = GRIDS[gname];
  if (sim) {
    compute(0, D => D('sim_cloth', p => { p.setPipeline(P.sim); p.setBindGroup(0, W.bg.sim, [0]); p.dispatchWorkgroups(S.clothGroups); }));
    compute(1, D => D('sim_hair', p => { p.setPipeline(P.sim); p.setBindGroup(0, W.bg.sim, [256]); p.dispatchWorkgroups(S.groups.length - S.clothGroups); }));
  }
  const traced = T.cloth === 'field' || T.cloth === 'tri';
  if (T.cloth) {
    compute(2, D => {
      D('cloth_normals', p => { p.setBindGroup(0, W.bg.build); p.setPipeline(P.cloth_normals); p.dispatchWorkgroups(Math.ceil(S.clothCount / 256)); });
      const sfx = W.P === PATCH ? '' : `_p${W.P}`;
      if (traced) D('patch_bounds', p => { p.setBindGroup(0, W.bg.build); p.setPipeline(P[`patch_bounds${sfx}`]); p.dispatchWorkgroups(Math.ceil(S.nPatches / 64)); });
      if (traced && T.isRef) D('sheet_bounds', p => { p.setBindGroup(0, W.bg.build); p.setPipeline(P[`sheet_bounds${sfx}`]); p.dispatchWorkgroups(Math.ceil(S.nPatches / 64)); });
    });
  }
  if (T.hair) {
    enc.clearBuffer(G.occ);
    compute(3, D => {
      const h = (name, pipe, bg1, x, y = 1, z = 1) => D(name, p => {
        p.setBindGroup(0, G.hair0);
        if (bg1) p.setBindGroup(1, bg1);
        p.setPipeline(pipe);
        p.dispatchWorkgroups(x, y, z);
      });
      h('hair_bounds', P[`hair_bounds_${gname}`], null, 1);
      h('hair_children', P[`hair_children_${gname}`], null, W.opts.strands);
      h('hair_resolve', P[`hair_resolve_${gname}`], G.hres, HDg[0] / 4, HDg[1] / 4, HDg[2] / 4);
      h('hair_skip', P[`hair_skip_${gname}`], G.hskip_bg, HDg[0] / 16, HDg[1] / 16, HDg[2] / 16);
      h('hair_light', P[`hair_light_${gname}`], G.hlight_bg, HDg[0] / (4 * LIGHT_DIV[gname]), HDg[1] / (4 * LIGHT_DIV[gname]), HDg[2] / (4 * LIGHT_DIV[gname]));
      if (T.isRef && T.hair === 'strands') h('strand_bounds', P[`strand_bounds_${gname}`], null, Math.ceil(W.opts.strands / 64));
    });
  }
  enc.clearBuffer(tg.big, 0, 16);
  compute(4, D => {
    const strands = T.hair === 'strands';
    const items = W.cast.length + (traced || strands ? S.nPatches : 0) + (strands ? W.nsegs : 0);
    const b = (name, pipe, x) => D(name, p => { p.setBindGroup(0, W.bg.bin[tg.name]); p.setPipeline(pipe); p.dispatchWorkgroups(x); });
    b('bin_count', P.bin_count, Math.ceil(items / 64));
    b('bin_big_count', P.bin_big_count, 128);
    b('scan_reduce', P.scan_reduce, tg.blocks);
    b('scan_blocks', P.scan_blocks, 1);
    b('scan_down', P.scan_down, tg.blocks);
    b('bin_fill', P.bin_fill, Math.ceil(items / 64));
    b('bin_big_fill', P.bin_big_fill, 128);
  });
  const rasterCloth = T.cloth === 'raster', rasterHair = T.hair === 'raster';
  if (rasterCloth) {
    const p = render(5, 'smap', { colorAttachments: [], depthStencilAttachment: { view: W.smapView, depthClearValue: 1, depthLoadOp: 'clear', depthStoreOp: 'store' } });
    p.setPipeline(P.smap);
    p.setBindGroup(0, W.bg.smap0);
    p.setIndexBuffer(W.tris, 'uint32');
    p.drawIndexed(S.tris.length);
    p.end();
  }
  if (rasterCloth || rasterHair) {
    // Raster first: depth of the meshes, so the trace can stop at them and nothing is shaded twice.
    const p = render(6, 'prepass', { colorAttachments: [], depthStencilAttachment: { view: tg.zView, depthClearValue: 1, depthLoadOp: 'clear', depthStoreOp: 'store' } });
    p.setBindGroup(0, G.g0);
    p.setBindGroup(1, tg.bgRaster);
    if (rasterCloth) { p.setPipeline(P.cloth_prepass); p.setIndexBuffer(W.tris, 'uint32'); p.drawIndexed(S.tris.length); }
    if (rasterHair) { p.setPipeline(P.hair_prepass); p.draw(6, W.nsegs); }
    p.end();
  }
  if (skipTrace) return;
  compute(7, D => D('trace', p => {
    p.setPipeline(P.trace[traceName(W, traceKey || T.trace)]);
    p.setBindGroup(0, G.g0);
    p.setBindGroup(1, tg.bgTrace);
    p.dispatchWorkgroups(Math.ceil(tg.w / 8), Math.ceil(tg.h / 8));
  }));
  if (rasterCloth || rasterHair) {
    const p = render(8, 'raster', {
      colorAttachments: [{ view: tg.outView, loadOp: 'load', storeOp: 'store' }],
      depthStencilAttachment: { view: tg.zView, depthLoadOp: 'load', depthStoreOp: 'store' },
    });
    p.setBindGroup(0, G.g0);
    p.setBindGroup(1, tg.bgRaster);
    p.setBindGroup(2, tg.bgDepth);
    p.setPipeline(P.world_depth);
    p.draw(3);
    if (rasterCloth) { p.setPipeline(P.cloth_raster); p.setIndexBuffer(W.tris, 'uint32'); p.drawIndexed(S.tris.length); }
    if (rasterHair) { p.setPipeline(P.hair_raster); p.draw(6, W.nsegs); }
    p.end();
  }
}

const traceName = (W, k) => (W.P === PATCH ? k : `${k}@P${W.P}`);

/** Passes a technique runs (indices into PASSES). */
function passesOf(techName) {
  const T = TECHS[techName];
  const out = [];
  if (techName !== 'base') out.push(0, 1);
  if (T.cloth) out.push(2);
  if (T.hair) out.push(3);
  out.push(4);
  if (T.cloth === 'raster') out.push(5);
  if (T.cloth === 'raster' || T.hair === 'raster') out.push(6);
  out.push(7);
  if (T.cloth === 'raster' || T.hair === 'raster') out.push(8);
  return out;
}

/** Runs the simulation alone for `frames` frames (settling, or catching up). */
async function settle(W, frames) {
  for (let f = 0; f < frames; f++) {
    advance(W);
    const enc = device.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(P.sim);
    pass.setBindGroup(0, W.bg.sim, [0]);
    pass.dispatchWorkgroups(W.sys.clothGroups);
    pass.setBindGroup(0, W.bg.sim, [256]);
    pass.dispatchWorkgroups(W.sys.groups.length - W.sys.clothGroups);
    pass.end();
    device.queue.submit([enc.finish()]);
    if (f % 30 === 29) await device.queue.onSubmittedWorkDone();
  }
  await device.queue.onSubmittedWorkDone();
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
async function measure(W, viewName, tg, techName, frames = 90) {
  await ensureTrace([TECHS[techName].trace]);
  const sim = techName !== 'base';
  const used = passesOf(techName);
  const frame = (qs, q0) => {
    if (sim) advance(W); else poseChars(W, W.t);
    writeFrame(W, viewName, tg, techName);
    const enc = device.createCommandEncoder();
    encodeFrame(enc, W, tg, techName, { sim, qs, q0 });
    device.queue.submit([enc.finish()]);
  };
  for (let f = 0; f < 30; f++) frame(null, 0);
  await device.queue.onSubmittedWorkDone();
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * QPF });
  const t1 = performance.now();
  for (let f = 0; f < frames; f++) frame(qs, f * QPF);
  await device.queue.onSubmittedWorkDone();
  const wall = (performance.now() - t1) / frames;
  const ts = await resolveTimestamps(qs, frames * QPF);
  return timeline(ts, frames, used, sim, wall);
}

/**
 * Per-pass GPU time from timestamps. On this GPU a render pass's begin timestamp can be taken while
 * the compute pass before it is still running, so raw pass durations overlap and don't add up.
 * Each pass is charged its serialized share: end − max(begin, previous pass's end). Those shares sum
 * to the frame's span (first begin to last end), which is what the verdict uses.
 */
function timeline(ts, frames, used, sim, wall) {
  const t = (f, i, e) => Number(ts[f * QPF + 2 * i + e]);
  const all = [...Array(frames).keys()];
  const out = { passes: {}, wall_ms_per_frame: r3(wall) };
  const eff = all.map(f => {
    let prev = -Infinity;
    const o = {};
    for (const i of used) {
      const b = t(f, i, 0), e = t(f, i, 1);
      o[i] = Math.max(0, e - Math.max(b, prev)) / 1e6;
      prev = Math.max(prev, e);
    }
    return o;
  });
  for (const i of used) {
    const v = all.map(f => eff[f][i]);
    const raw = all.map(f => (t(f, i, 1) - t(f, i, 0)) / 1e6);
    out.passes[PASSES[i]] = { median: r3(median(v)), mean: r3(mean(v)), p95: r3(pct(v, 0.95)), raw_median: r3(median(raw)) };
  }
  const first = used[0], last = used[used.length - 1];
  const framev = all.map(f => (t(f, last, 1) - t(f, first, 0)) / 1e6);
  const simv = all.map(f => (sim ? eff[f][0] + eff[f][1] : 0));
  const drawv = framev.map((x, f) => x - simv[f]);
  out.sim = { median: r3(median(simv)), mean: r3(mean(simv)), p95: r3(pct(simv, 0.95)) };
  out.draw_passes = { median: r3(median(drawv)), mean: r3(mean(drawv)), p95: r3(pct(drawv, 0.95)) };
  out.frame_gpu = { median: r3(median(framev)), mean: r3(mean(framev)), p95: r3(pct(framev, 0.95)) };
  return out;
}

/** 60 Hz pacing by busy-wait (spike 01/02): the GPU's own frame time at the clocks the OS picks. */
async function paced(W, viewName, tg, techName, frames = 180) {
  await ensureTrace([TECHS[techName].trace]);
  const used = passesOf(techName), period = 1000 / 60;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * QPF });
  const t0 = performance.now() + 20;
  for (let f = 0; f < frames; f++) {
    const deadline = t0 + f * period;
    while (performance.now() < deadline) { /* spin */ }
    advance(W);
    writeFrame(W, viewName, tg, techName);
    const enc = device.createCommandEncoder();
    encodeFrame(enc, W, tg, techName, { sim: true, qs, q0: f * QPF });
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * QPF);
  const first = used[0], last = used[used.length - 1];
  const keep = [...Array(frames).keys()].slice(30);
  const gpu = keep.map(f => Number(ts[f * QPF + 2 * last + 1] - ts[f * QPF + 2 * first]) / 1e6);
  const spacing = keep.slice(1).map(f => Number(ts[f * QPF + 2 * first] - ts[(f - 1) * QPF + 2 * first]) / 1e6);
  return {
    pacing: 'busy-wait at 60 Hz', frames: keep.length, frame_gpu_median: r3(median(gpu)), frame_gpu_p95: r3(pct(gpu, 0.95)),
    frame_gpu_max: r3(Math.max(...gpu)), frames_over_16_7: gpu.filter(g => g > 16.7).length,
    start_spacing_median: r3(median(spacing)), start_spacing_max: r3(Math.max(...spacing)),
  };
}

/** Renders one frame of a technique from the current simulation state, without stepping it. */
async function draw(W, viewName, tg, techName, traceKey = null) {
  const T = TECHS[techName];
  await ensureTrace([traceKey || T.trace]);
  poseChars(W, W.t);
  if (T.isRef) {
    device.queue.writeBuffer(W.sheetb, 0, W.sheetInit);
    writeFrame(W, viewName, tg, techName);
    let enc = device.createCommandEncoder();
    encodeFrame(enc, W, tg, techName, { sim: false, skipTrace: true });
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
    // Tiles, one submit each, so no submission runs long and the display gets the GPU in between.
    for (let y = 0; y < tg.h; y += TILE_H) {
      for (let x = 0; x < tg.w; x += TILE_W) {
        const G = W.grids[gridOf(T)];
        writeFrame(W, viewName, tg, techName, y, x);
        enc = device.createCommandEncoder();
        const p = enc.beginComputePass();
        p.setPipeline(P.trace[traceName(W, T.trace)]);
        p.setBindGroup(0, G.g0);
        p.setBindGroup(1, tg.bgTrace);
        p.dispatchWorkgroups(Math.ceil(Math.min(TILE_W, tg.w - x) / 8), Math.ceil(Math.min(TILE_H, tg.h - y) / 8));
        p.end();
        device.queue.submit([enc.finish()]);
        await device.queue.onSubmittedWorkDone();
      }
    }
    return;
  }
  writeFrame(W, viewName, tg, techName);
  const enc = device.createCommandEncoder();
  encodeFrame(enc, W, tg, techName, { sim: false, traceKey });
  device.queue.submit([enc.finish()]);
  await device.queue.onSubmittedWorkDone();
}

const BYTE = new Uint8Array(65536);
for (let h = 0; h < 65536; h++) {
  const s = h >> 15, e = (h >> 10) & 31, m = h & 1023;
  let v = e === 0 ? m * 2 ** -24 : e === 31 ? 1 : (1 + m / 1024) * 2 ** (e - 15);
  if (s) v = 0;
  BYTE[h] = Math.round(255 * Math.pow(Math.min(Math.max(v, 0), 1), 1 / 2.2));
}

/** The current target as 8-bit display values (what the canvas shows). */
async function grabPixels(tg) {
  const bpr = Math.ceil(tg.w * 8 / 256) * 256;
  const out = device.createBuffer({ size: bpr * tg.h, usage: U.COPY_DST | U.MAP_READ });
  const enc = device.createCommandEncoder();
  enc.copyTextureToBuffer({ texture: tg.out }, { buffer: out, bytesPerRow: bpr, rowsPerImage: tg.h }, [tg.w, tg.h]);
  device.queue.submit([enc.finish()]);
  await out.mapAsync(GPUMapMode.READ);
  const half = new Uint16Array(out.getMappedRange().slice(0));
  out.unmap();
  out.destroy();
  const px = new Uint8Array(tg.w * tg.h * 3);
  for (let y = 0, j = 0; y < tg.h; y++) {
    const row = y * bpr / 2;
    for (let x = 0; x < tg.w; x++) for (let c = 0; c < 3; c++) px[j++] = BYTE[half[row + x * 4 + c]];
  }
  return px;
}

function diff(a, b, mask) {
  let sum = 0, over8 = 0, n = 0, max = 0, mOver = 0, mN = 0;
  for (let i = 0, p = 0; i < a.length; i += 3, p++) {
    let m = 0;
    for (let c = 0; c < 3; c++) { const d = Math.abs(a[i + c] - b[i + c]); sum += d; m = Math.max(m, d); }
    if (m > 8) over8++;
    if (mask && mask[p]) { mN++; if (m > 8) mOver++; }
    max = Math.max(max, m);
    n++;
  }
  const out = { mean_abs_255: r3(sum / a.length), share_over_8: r3(over8 / n * 1e4) / 1e4, max };
  if (mask) out.share_over_8_of_masked = r3(mOver / Math.max(mN, 1) * 1e4) / 1e4;
  return out;
}

/** A diff image: grey base, red where off by more than 8/255, yellow where by more than 2. */
async function diffImage(name, tg, a, b) {
  const c = document.createElement('canvas');
  c.width = tg.w; c.height = tg.h;
  const g = c.getContext('2d'), img = g.createImageData(tg.w, tg.h);
  for (let i = 0, j = 0; i < a.length; i += 3, j += 4) {
    let m = 0;
    for (let k = 0; k < 3; k++) m = Math.max(m, Math.abs(a[i + k] - b[i + k]));
    const base = (b[i] + b[i + 1] + b[i + 2]) / 9;
    img.data[j] = m > 8 ? 255 : (m > 2 ? 220 : base);
    img.data[j + 1] = m > 8 ? 0 : (m > 2 ? 190 : base);
    img.data[j + 2] = m > 2 ? 0 : base;
    img.data[j + 3] = 255;
  }
  g.putImageData(img, 0, 0);
  const blob = await new Promise(r => c.toBlob(r, 'image/png'));
  if (blob) await save(`${name}.png`, blob);
}

function present(tg) {
  const enc = device.createCommandEncoder();
  const p = enc.beginRenderPass({ colorAttachments: [{ view: ctx.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
  p.setPipeline(P.blit);
  p.setBindGroup(0, tg.bgBlit);
  p.draw(3);
  p.end();
  device.queue.submit([enc.finish()]);
}

async function snapshot(name, tg) {
  present(tg);
  await device.queue.onSubmittedWorkDone();
  const blob = await new Promise(r => $('view').toBlob(r, 'image/png'));
  if (blob) await save(`${name}.png`, blob); else logErr('snapshot failed', name);
}

async function stats(W, viewName, tg, techName) {
  const T = TECHS[techName];
  const init = new Uint32Array(64);
  init[19] = 0x7f800000;
  device.queue.writeBuffer(tg.stats, 0, init);
  await draw(W, viewName, tg, techName, T.stats);
  const raw = await readback([{ src: tg.stats, size: 256 }, { src: tg.boff, offset: tg.cells * 4, size: 4 }]);
  const s = new Uint32Array(raw, 0, 64);
  const o = {};
  STAT_NAMES.forEach((n, i) => { o[n] = s[i]; });
  o.min_step_mm = o.min_step_bits === 0x7f800000 ? null : r3(new Float32Array(new Uint32Array([o.min_step_bits]).buffer)[0] * 1000);
  delete o.min_step_bits;
  o.bin_entries = new Uint32Array(raw, 256, 1)[0];
  o.bin_capacity = CAP;
  const px = tg.w * tg.h;
  o.cloth_share = r3(o.cloth_px / px);
  o.hair_share = r3(o.hair_px / px);
  o.steps_per_cloth_march = r3(o.cloth_steps / Math.max(o.cloth_marches, 1));
  o.cloth_steps_per_cloth_px = r3(o.cloth_px_steps / Math.max(o.cloth_px, 1));
  o.cloth_marches_per_cloth_px = r3(o.cloth_marches / Math.max(o.cloth_px, 1));
  o.cloth_miss_share = r3(o.cloth_misses / Math.max(o.cloth_marches, 1));
  o.cap_share_of_cloth_px = o.cloth_px ? r3(o.cloth_caps / o.cloth_px * 1e4) / 1e4 : null;
  o.strand_candidates_per_hair_px = r3(o.strand_candidates / Math.max(o.hair_px, 1));
  o.strand_layers_per_hair_px = r3(o.strand_layers / Math.max(o.strand_px, 1));
  o.volume_samples_per_hair_px = r3(o.volume_samples / Math.max(o.volume_px, 1));
  const hs = o.step_lt_1mm + o.step_1_4mm + o.step_4_16mm + o.step_16_64mm + o.step_ge_64mm;
  o.step_histogram_share = [o.step_lt_1mm, o.step_1_4mm, o.step_4_16mm, o.step_16_64mm, o.step_ge_64mm].map(x => r3(x / Math.max(hs, 1)));
  return o;
}

// ---- the run -------------------------------------------------------------------------------------------------

const MAIN = ['base', 'field', 'field_strands', 'tri_strands', 'raster'];
const COMPONENTS = ['cloth_field', 'cloth_tri', 'cloth_raster', 'hair_volume', 'hair_strands', 'hair_raster'];
const COMPONENTS_TIMED = [...COMPONENTS, 'cloth_field_nosh', 'cloth_tri_nosh', 'cloth_field_softsh'];

function summarize(key, m, base) {
  const draw = r3(m.draw_passes.median - (base ? base.draw_passes.median : 0));
  const drawMean = r3(m.draw_passes.mean - (base ? base.draw_passes.mean : 0));
  // (base: the mean of the bracketing base measurements; see measureSet)
  return { ...m, draw_marginal_ms: draw, draw_marginal_mean_ms: drawMean, sim_ms: m.sim.median, sim_mean_ms: m.sim.mean };
}

function verdict(draw, sim) {
  if (draw <= 1.5 && sim <= 0.5) return 'pass';
  if (draw <= 3.0 && sim <= 1.0) return 'inconclusive';
  return 'fail';
}

/**
 * Times a set of techniques against the base (no cloth or hair). GPU clocks drift during a run (the
 * base was seen to move by 30% within one set), so each technique is bracketed by base measurements
 * and its marginal cost is taken against the mean of the two bases around it.
 */
async function measureSet(W, viewName, tg, techs, prefix = '') {
  const key0 = `${prefix}${viewName}/${tg.name}`;
  let before = await measure(W, viewName, tg, 'base');
  const bases = [before.frame_gpu.median];
  R.frames[`${key0}/base`] = summarize('base', before, null);
  for (const t of techs) {
    if (t === 'base') continue;
    const m = await measure(W, viewName, tg, t);
    const after = await measure(W, viewName, tg, 'base');
    bases.push(after.frame_gpu.median);
    const mid = { draw_passes: { median: (before.draw_passes.median + after.draw_passes.median) / 2, mean: (before.draw_passes.mean + after.draw_passes.mean) / 2 } };
    const s = summarize(t, m, mid);
    s.base_frame_bracket = [r3(before.frame_gpu.median), r3(after.frame_gpu.median)];
    R.frames[`${key0}/${t}`] = s;
    log(`${key0}/${t}`, `sim ${s.sim_ms} draw +${s.draw_marginal_ms} (mean +${s.draw_marginal_mean_ms}) frame ${m.frame_gpu.median} base ${s.base_frame_bracket}`,
      JSON.stringify(Object.fromEntries(Object.entries(m.passes).map(([k, v]) => [k, v.median]))));
    before = after;
  }
  R.frames[`${key0}/base`].base_frames_over_set = bases.map(r3);
}

/**
 * Correctness: each technique against its brute-force reference, from the same simulation state.
 * Also reported, for diagnosis: the floor (base against the base reference: errors of the world
 * itself, the same in every technique), and the error over only the pixels cloth or hair change
 * (pixels where the technique's reference differs from the base reference).
 */
async function quality(W, viewName, tg, techs, shots) {
  const refs = {};
  const key = `${viewName}/${tg.name}`;
  R.quality[key] ||= {};
  await draw(W, viewName, tg, 'base');
  const basePx = await grabPixels(tg);
  await draw(W, viewName, tg, 'r_base');
  const baseRef = await grabPixels(tg);
  R.quality[key].base_floor = diff(basePx, baseRef);
  for (const t of techs) {
    const T = TECHS[t];
    if (!refs[T.ref]) {
      const t0 = performance.now();
      await draw(W, viewName, tg, T.ref);
      const px = await grabPixels(tg);
      const mask = new Uint8Array(tg.w * tg.h);
      for (let i = 0, p = 0; i < px.length; i += 3, p++) {
        mask[p] = Math.max(Math.abs(px[i] - baseRef[i]), Math.abs(px[i + 1] - baseRef[i + 1]), Math.abs(px[i + 2] - baseRef[i + 2])) > 2 ? 1 : 0;
      }
      refs[T.ref] = { px, mask, share: r3(mask.reduce((a, b) => a + b, 0) / mask.length) };
      if (shots) await snapshot(`${viewName}-${T.ref}`, tg);
      log(`ref ${T.ref} ${key} in ${r3(performance.now() - t0)} ms`);
    }
    await draw(W, viewName, tg, t);
    const px = await grabPixels(tg);
    const q = diff(px, refs[T.ref].px, refs[T.ref].mask);
    q.cloth_hair_share = refs[T.ref].share;
    R.quality[key][t] = { ref: T.ref, ...q };
    log(`quality ${key}/${t}`, JSON.stringify(q), 'floor', JSON.stringify(R.quality[key].base_floor));
    if (shots) {
      await snapshot(`${viewName}-${t}`, tg);
      if (shots === 'diff') await diffImage(`${viewName}-${t}-diff`, tg, px, refs[T.ref].px);
    }
  }
}

async function runAll() {
  $('run').disabled = true;
  const tStart = performance.now();
  try {
    await init();
    log('device', JSON.stringify(R.device));
    await makePipelines();
    TG.full = makeTarget('full', 1920, 1080);
    TG.half = makeTarget('half', 960, 540);
    R.config = { patch_quads: PATCH, substeps: SUBSTEPS, iterations: ITERS, wind: WIND, gust: GUST, sun: SUN, light_grid: GRID,
      hair_grids: GRIDS, cloth_half_thickness_m: 0.004, views: VIEWS };
    const def = { npcs: 20, cloth: 1, strands: 2048 };
    let W = makeWorld(def);
    log('content', JSON.stringify(R.content[contentKey(def)]));
    await settle(W, 240);

    // Correctness and looks first, from one settled simulation state.
    for (const v of ['courtyard', 'hero', 'banners']) {
      await quality(W, v, TG.full, ['field', 'field_strands', 'tri_strands', 'raster'], v === 'courtyard' ? 'diff' : true);
      for (const t of ['field', 'field_strands', 'tri_strands']) {
        R.stats[`${v}/full/${t}`] = await stats(W, v, TG.full, t);
        log(`stats ${v}/${t}`, JSON.stringify(R.stats[`${v}/full/${t}`]));
      }
      await draw(W, v, TG.full, 'base');
      await snapshot(`${v}-base`, TG.full);
      await draw(W, v, TG.full, 'field', 'v_c1h2');
      await snapshot(`${v}-heat-cloth`, TG.full);
      await draw(W, v, TG.full, 'tri_strands', 'v_c2h1');
      await snapshot(`${v}-heat-strands`, TG.full);
    }
    await quality(W, 'courtyard', TG.full, COMPONENTS, false);
    await quality(W, 'courtyard', TG.half, ['field', 'tri_strands', 'raster'], false);
    // The hair light grid's own error: references lit by the grid against references lit exactly.
    await quality(W, 'hero', TG.full, ['hair_strands_lightref', 'hair_volume_lightref'], false);
    log('quality done in', r3((performance.now() - tStart) / 1000), 's');

    // Timing: every view, both resolutions, the main techniques; components at the judged view.
    for (const v of ['courtyard', 'hero', 'banners']) {
      for (const tg of [TG.full, TG.half]) {
        await measureSet(W, v, tg, MAIN);
      }
    }
    await measureSet(W, 'courtyard', TG.full, COMPONENTS_TIMED, 'components/');
    R.sim_bench = await simBench(W, [[8, 1], [4, 5], [4, 3], [4, 1], [2, 3]]);
    log('sim bench (substeps x iterations)', JSON.stringify(R.sim_bench));
    R.paced['courtyard/full/field'] = await paced(W, 'courtyard', TG.full, 'field');
    R.paced['courtyard/full/raster'] = await paced(W, 'courtyard', TG.full, 'raster');
    log('paced', JSON.stringify(R.paced));
    log('main timing done in', r3((performance.now() - tStart) / 1000), 's');

    // Sweeps at the judged view, native resolution.
    for (const cloth of [0.5, 2]) {
      W = makeWorld({ ...def, cloth });
      await settle(W, 120);
      const k = `cloth${cloth}`;
      await measureSet(W, 'courtyard', TG.full, ['cloth_field', 'cloth_tri', 'cloth_raster'], `sweep/${k}/`);
      R.stats[`sweep/${k}/courtyard/cloth_field`] = await stats(W, 'courtyard', TG.full, 'field');
      await draw(W, 'hero', TG.full, 'field');
      await snapshot(`sweep-${k}-hero-field`, TG.full);
    }
    {
      W = makeWorld({ ...def, P: 2 });
      await settle(W, 120);
      await measureSet(W, 'courtyard', TG.full, ['cloth_field', 'cloth_field_softsh', 'cloth_tri'], 'sweep/patch2/');
      await measureSet(W, 'hero', TG.full, ['cloth_field', 'cloth_field_softsh', 'cloth_tri'], 'sweep/patch2/');
      R.stats['sweep/patch2/courtyard/field'] = await stats(W, 'courtyard', TG.full, 'field');
    }
    for (const strands of [512, 8192]) {
      W = makeWorld({ ...def, strands });
      await settle(W, 120);
      const k = `strands${strands}`;
      await measureSet(W, 'courtyard', TG.full, ['hair_volume', 'hair_strands', 'hair_raster'], `sweep/${k}/`);
      await measureSet(W, 'hero', TG.full, ['hair_volume', 'hair_strands', 'hair_raster'], `sweep/${k}/`);
      R.stats[`sweep/${k}/hero/tri_strands`] = await stats(W, 'hero', TG.full, 'tri_strands');
      for (const t of ['field', 'tri_strands', 'raster']) { await draw(W, 'hero', TG.full, t); await snapshot(`sweep-${k}-hero-${t}`, TG.full); }
    }
    for (const npcs of [0, 10, 40]) {
      W = makeWorld({ ...def, npcs });
      await settle(W, 120);
      await measureSet(W, 'courtyard', TG.full, ['field', 'tri_strands', 'raster'], `sweep/npcs${npcs}/`);
      if (npcs === 40) { await draw(W, 'courtyard', TG.full, 'field'); await snapshot('sweep-npcs40-courtyard-field', TG.full); }
    }
    log('sweeps done in', r3((performance.now() - tStart) / 1000), 's');

    R.budget = budgetTable();
    log('budget', JSON.stringify(R.budget));
    R.finished = new Date().toISOString();
    R.run_seconds = r3((performance.now() - tStart) / 1000);
    await save(`run-${R.started.replace(/[:.]/g, '-')}.json`, JSON.stringify(R, null, 2));
    log('done in', R.run_seconds, 's');
  } catch (e) {
    logErr('FAILED:', e.stack || e.message);
    R.error = String(e.stack || e.message);
    await save(`run-${R.started.replace(/[:.]/g, '-')}-failed.json`, JSON.stringify(R, null, 2));
  }
  await save('DONE', R.error ? 'failed' : 'ok');
  $('run').disabled = false;
}

function budgetTable() {
  const out = {};
  for (const v of ['courtyard', 'hero', 'banners']) {
    for (const res of ['full', 'half']) {
      for (const t of MAIN.slice(1)) {
        const m = R.frames[`${v}/${res}/${t}`];
        if (!m) continue;
        out[`${v}/${res}/${t}`] = { draw_ms: m.draw_marginal_ms, sim_ms: m.sim_ms, verdict: verdict(m.draw_marginal_ms, m.sim_ms) };
      }
    }
  }
  return out;
}

/** Renders each view and technique once, saves screenshots, and exposes helpers. */
async function quick() {
  try {
    await init();
    await makePipelines();
    TG.full = makeTarget('full', 1920, 1080);
    TG.half = makeTarget('half', 960, 540);
    const W = makeWorld({ npcs: 20, cloth: 1, strands: 2048 });
    log('content', JSON.stringify(R.content[contentKey(W.opts)]));
    await settle(W, 240);
    for (const v of ['courtyard', 'hero', 'banners']) {
      for (const t of ['field', 'tri_strands', 'raster']) {
        await draw(W, v, TG.full, t);
        await snapshot(`quick-${v}-${t}`, TG.full);
      }
    }
    window.__draw = async (v, t, key) => { await draw(W, v, TG.full, t, key); present(TG.full); };
    window.__snap = name => snapshot(name, TG.full);
    window.__settle = n => settle(W, n);
    log('quick done');
  } catch (e) {
    logErr('FAILED:', e.stack || e.message);
  }
  await save('DONE', 'ok');
}

/** Development: #debug=view:tech[:traceKey],... renders those and saves debug-*.png. */
async function debug(spec) {
  try {
    await init();
    await makePipelines();
    TG.full = makeTarget('full', 1920, 1080);
    TG.half = makeTarget('half', 960, 540);
    const W = makeWorld({ npcs: 20, cloth: 1, strands: 2048 });
    await settle(W, 240);
    for (const item of spec.split(',')) {
      const [v, t, key] = item.split(':');
      const t0 = performance.now();
      await draw(W, v, TG.full, t, key || null);
      log(`debug ${item} ${r3(performance.now() - t0)} ms`);
      await snapshot(`debug-${v}-${t}${key ? '-' + key : ''}`, TG.full);
      if (TECHS[t].stats && !TECHS[t].isRef) log(`stats ${item}`, JSON.stringify(await stats(W, v, TG.full, t)));
    }
  } catch (e) {
    logErr('FAILED:', e.stack || e.message);
  }
  await save('DONE', 'ok');
}

/** The simulation alone, at several substep × iteration settings: GPU ms per 60 Hz frame. */
async function simBench(W, configs, frames = 60) {
  const out = {};
  const keep = [SUBSTEPS, ITERS];
  for (const [sub, it] of configs) {
    SUBSTEPS = sub; ITERS = it;
    const qs = device.createQuerySet({ type: 'timestamp', count: frames * 4 });
    for (let f = 0; f < frames + 10; f++) {
      advance(W);
      const enc = device.createCommandEncoder();
      const q = f >= 10 ? (k => ({ timestampWrites: { querySet: qs, beginningOfPassWriteIndex: (f - 10) * 4 + 2 * k, endOfPassWriteIndex: (f - 10) * 4 + 2 * k + 1 } })) : (() => ({}));
      let p = enc.beginComputePass(q(0));
      p.setPipeline(P.sim); p.setBindGroup(0, W.bg.sim, [0]); p.dispatchWorkgroups(W.sys.clothGroups); p.end();
      p = enc.beginComputePass(q(1));
      p.setPipeline(P.sim); p.setBindGroup(0, W.bg.sim, [256]); p.dispatchWorkgroups(W.sys.groups.length - W.sys.clothGroups); p.end();
      device.queue.submit([enc.finish()]);
    }
    await device.queue.onSubmittedWorkDone();
    const ts = await resolveTimestamps(qs, frames * 4);
    const d = k => [...Array(frames).keys()].map(f => Number(ts[f * 4 + 2 * k + 1] - ts[f * 4 + 2 * k]) / 1e6);
    out[`${sub}x${it}`] = { cloth: r3(median(d(0))), cloth_mean: r3(mean(d(0))), hair: r3(median(d(1))), hair_mean: r3(mean(d(1))) };
  }
  [SUBSTEPS, ITERS] = keep;
  return out;
}

/** Per-kernel GPU time: each dispatch in its own timed pass (development; adds pass overhead). */
async function profileKernels(W, viewName, tg, techName, frames = 30) {
  await ensureTrace([TECHS[techName].trace]);
  const sim = techName !== 'base';
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * 64 });
  const per = {};
  for (let f = 0; f < frames + 10; f++) {
    if (sim) advance(W); else poseChars(W, W.t);
    writeFrame(W, viewName, tg, techName);
    const enc = device.createCommandEncoder();
    const prof = f >= 10 ? { qs, q0: (f - 10) * 64, names: [] } : null;
    encodeFrame(enc, W, tg, techName, { sim, prof });
    device.queue.submit([enc.finish()]);
    if (prof) per[f - 10] = prof.names;
  }
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * 64);
  const acc = {};
  for (let f = 0; f < frames; f++) {
    per[f].forEach((n, k) => { (acc[n] ||= []).push(Number(ts[f * 64 + 2 * k + 1] - ts[f * 64 + 2 * k]) / 1e6); });
  }
  return Object.fromEntries(Object.entries(acc).map(([n, v]) => [n, { median: r3(median(v)), mean: r3(mean(v)) }]));
}

/** Development: #prof=view:tech,... profiles every kernel. */
async function profOnly(spec) {
  try {
    await init();
    await makePipelines();
    TG.full = makeTarget('full', 1920, 1080);
    TG.half = makeTarget('half', 960, 540);
    const W = makeWorld({ npcs: 20, cloth: 1, strands: 2048 });
    await settle(W, 120);
    for (const item of spec.split(',')) {
      const [v, t, res] = item.split(':');
      const k = await profileKernels(W, v, TG[res || 'full'], t);
      log(`prof ${item}:`, Object.entries(k).map(([n, x]) => `${n} ${x.median}/${x.mean}`).join(', '));
    }
  } catch (e) {
    logErr('FAILED:', e.stack || e.message);
  }
  await save('DONE', 'ok');
}

/** Development: #time=view:tech,... measures those at 1080p (indicative; the GPU is shared). */
async function timeOnly(spec) {
  try {
    await init();
    await makePipelines();
    TG.full = makeTarget('full', 1920, 1080);
    TG.half = makeTarget('half', 960, 540);
    const W = makeWorld({ npcs: +(QP.get('npcs') || 20), cloth: +(QP.get('cloth') || 1), strands: +(QP.get('strands') || 2048), P: +(QP.get('P') || PATCH) });
    await settle(W, 120);
    let base = null;
    if (QP.has('simbench')) log('simbench', JSON.stringify(await simBench(W, [[4, 5], [1, 1], [2, 3], [4, 1], [4, 3], [8, 1]])));
    for (const item of spec.split(',')) {
      const [v, t, res] = item.split(':');
      const m = await measure(W, v, TG[res || 'full'], t);
      if (t === 'base') base = m;
      const s = summarize(t, m, base);
      log(`time ${item}: sim ${s.sim_ms} draw +${s.draw_marginal_ms} (mean +${s.draw_marginal_mean_ms}) frame ${m.frame_gpu.median}`,
        JSON.stringify(Object.fromEntries(Object.entries(m.passes).map(([k, x]) => [k, x.median]))));
    }
  } catch (e) {
    logErr('FAILED:', e.stack || e.message);
  }
  await save('DONE', 'ok');
}

/** Development: #qual=view:tech,... compares against references and saves diff images. */
async function qualOnly(spec) {
  try {
    await init();
    await makePipelines();
    TG.full = makeTarget('full', 1920, 1080);
    TG.half = makeTarget('half', 960, 540);
    const W = makeWorld({ npcs: 20, cloth: 1, strands: 2048 });
    await settle(W, 240);
    const byView = {};
    for (const item of spec.split(',')) { const [v, t] = item.split(':'); (byView[v] ||= []).push(t); }
    for (const [v, ts] of Object.entries(byView)) await quality(W, v, TG.full, ts, 'diff');
    for (const [v, ts] of Object.entries(byView)) {
      for (const t of ts) {
        if (!TECHS[t].stats) continue;
        const st = await stats(W, v, TG.full, t);
        log(`stats ${v}/${t}`, JSON.stringify({ cap_share_of_cloth_px: st.cap_share_of_cloth_px, cloth_caps: st.cloth_caps, cloth_px: st.cloth_px,
          steps_per_cloth_march: st.steps_per_cloth_march, cloth_steps_per_cloth_px: st.cloth_steps_per_cloth_px, min_step_mm: st.min_step_mm,
          hist: st.step_histogram_share, strand_cand: st.strand_candidates_per_hair_px, vol_samples: st.volume_samples_per_hair_px, bins: st.bin_entries }));
      }
    }
  } catch (e) {
    logErr('FAILED:', e.stack || e.message);
  }
  await save('DONE', 'ok');
}

/** Development: the same scene simulated at several substep × iteration settings, for looks. */
async function simLook(spec) {
  try {
    await init();
    await makePipelines();
    TG.full = makeTarget('full', 1920, 1080);
    TG.half = makeTarget('half', 960, 540);
    for (const cfg of spec.split(',')) {
      [SUBSTEPS, ITERS] = cfg.split('x').map(Number);
      const W = makeWorld({ npcs: 20, cloth: 1, strands: 2048 });
      await settle(W, 240);
      for (const v of ['hero', 'courtyard']) { await draw(W, v, TG.full, 'tri_strands'); await snapshot(`simlook-${cfg}-${v}`, TG.full); }
      log('simlook', cfg);
    }
  } catch (e) {
    logErr('FAILED:', e.stack || e.message);
  }
  await save('DONE', 'ok');
}

$('run').addEventListener('click', runAll);
if (HASH.startsWith('#simlook=')) simLook(HASH.slice(9));
if (HASH.startsWith('#qual=')) qualOnly(HASH.slice(6));
if (HASH.startsWith('#prof=')) profOnly(decodeURIComponent(HASH.slice(6)));
if (HASH.startsWith('#time=')) timeOnly(decodeURIComponent(HASH.slice(6)));
if (HASH === '#run') runAll();
if (HASH === '#quick') quick();
if (HASH.startsWith('#debug=')) debug(decodeURIComponent(HASH.slice(7)));
