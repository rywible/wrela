// Spike 03 harness: builds the world tables and textures, the pipelines, the scenes; times, checks
// and photographs all of it. This file plays the engine's role plus the measuring.

import * as Wd from './world.js';

const FULL = { w: 1920, h: 1080, name: '1080p' };
const HALF = { w: 960, h: 540, name: '540p' };
const FOVY = 50 * Math.PI / 180;
const U = GPUBufferUsage, TU = GPUTextureUsage;
const SUN = (() => { const v = [-0.55, 0.62, 0.56]; const l = Math.hypot(...v); return v.map(x => x / l); })();
const SUN_RADIUS = 0.00465;    // rad
const T_SNAP = 1.3;            // frozen time for correctness images (s)
const REF_HALF = 48;           // reference samples per half (two halves: 96)
const CROP = [480, 270, 1440, 810];   // the reference covers the frame's central quarter (cost), at 96 samples
const CONV = 48;               // frames for converged fast-path images (three Halton cycles)

const DEFAULTS = {
  occupancy: 0.85, grassN: 2, fern: 1.0,
  k0: 0.25, k1: 0.35, minLevel: 0, forced: -1,
  kg: 0.55, kf: 0.25, volSteps: 10, shadowSteps: 6, leafBudget: 32,
  view: 2000, wind: 1.0, taaAlpha: 0.1, iso: null, noFloor: 0,
};

const $ = id => document.getElementById(id);
const log = (...a) => { $('log').textContent += a.join(' ') + '\n'; console.log(...a); };
const err = (...a) => { $('log').textContent += 'ERROR ' + a.join(' ') + '\n'; console.error(...a); };
const R = { started: new Date().toISOString(), device: {}, setup: {}, pipelines: {}, scenes: {}, frames: {}, stats: {}, quality: {}, shimmer: {}, transitions: {}, sweeps: {}, paced: {} };
window.__results = R;

let device, ctx, canvasFormat, world, QUICK = false;
const src = {}, L = {}, P = {}, RES = {};
let frameBuf, leafBuf, texB, texC, sampRepeat, sampClamp, frameCounter = 0;

const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[s.length >> 1] : NaN; };
const pct = (a, p) => { const s = [...a].sort((x, y) => x - y); return s[Math.min(s.length - 1, Math.floor(s.length * p))]; };
const r3 = x => Math.round(x * 1000) / 1000;
const halton = (i, b) => { let f = 1, r = 0; while (i > 0) { f /= b; r += f * (i % b); i = Math.floor(i / b); } return r; };

function buffer(size, usage, data) {
  if (data) size = Math.max(size, data.byteLength);
  const b = device.createBuffer({ size: Math.max(16, Math.ceil(size / 16) * 16), usage, mappedAtCreation: !!data });
  if (data) { new data.constructor(b.getMappedRange()).set(data); b.unmap(); }
  return b;
}

async function readback(src, size) {
  const rb = device.createBuffer({ size, usage: U.MAP_READ | U.COPY_DST });
  const enc = device.createCommandEncoder();
  enc.copyBufferToBuffer(src, 0, rb, 0, size);
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

// ---- setup -----------------------------------------------------------------------------------------------

async function init() {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
  const want = ['timestamp-query'].filter(f => adapter.features.has(f));
  device = await adapter.requestDevice({ requiredFeatures: want, requiredLimits: { maxStorageBufferBindingSize: adapter.limits.maxStorageBufferBindingSize, maxBufferSize: adapter.limits.maxBufferSize } });
  device.lost.then(i => err('DEVICE LOST:', i.message));
  device.addEventListener('uncapturederror', e => err('GPU ERROR:', e.error.message));
  const info = adapter.info || {};
  R.device = { vendor: info.vendor, architecture: info.architecture, description: info.description, userAgent: navigator.userAgent,
    timestampQuery: want.includes('timestamp-query'), pageVisibility: document.visibilityState };
  log('device', JSON.stringify(R.device));
  if (!R.device.timestampQuery) throw new Error('timestamp-query is required');
  for (const f of ['common', 'foliage', 'trace', 'light', 'post', 'blit', 'map']) src[f] = await (await fetch(`${f}.wgsl`)).text();

  const canvas = $('view');
  canvas.width = FULL.w;
  canvas.height = FULL.h;
  ctx = canvas.getContext('webgpu');
  canvasFormat = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format: canvasFormat, alphaMode: 'opaque' });

  // World: leaf tiles, the volumetric textures, the calibration.
  const t0 = performance.now();
  const tiles = Wd.SPECIES.map((sp, k) => Wd.quantizeTile(Wd.buildLeafTile(sp, k)));
  const mips = Wd.SPECIES.map((sp, k) => Wd.bakeTile(sp, tiles[k]));
  const t1 = performance.now();
  const cal = Wd.SPECIES.map((sp, k) => Wd.calibrateTile(sp, tiles[k], mips[k], { keep: 0.7, rays: QUICK ? 800 : 3000 }));
  const calCheck = QUICK ? cal : Wd.SPECIES.map((sp, k) => Wd.calibrateTile(sp, tiles[k], mips[k], { keep: 0.4, rays: 1500, seed: 9 }));
  const t2 = performance.now();
  const FAR_SCALE = [0.95, 1.0];
  const fpT = Math.SQRT2 * DEFAULTS.k1;   // the footprint where the volume → far blend is half-way
  const far = Wd.SPECIES.map((sp, k) => Wd.calibrateFar(k, mips[k], cal[k].c, FAR_SCALE[k], { fpT, volSteps: DEFAULTS.volSteps, ...(QUICK ? { trees: 6, rays: 400 } : {}) }));
  const t3 = performance.now();
  world = { tiles, cal, far, farScale: FAR_SCALE };
  R.setup.bake_ms = r3(t1 - t0);
  R.setup.calibrate_ms = r3(t2 - t1);
  R.setup.calibrate_far_ms = r3(t3 - t2);
  R.setup.calibration = Wd.SPECIES.map((sp, k) => ({
    species: sp.name, explicit_transmittance_by_band: cal[k].target.map(r3),
    c_by_mip_band: cal[k].c.map(row => row.map(r3)),
    check_at_density_0_4: { explicit_transmittance_by_band: calCheck[k].target.map(r3), c_by_mip_band: calCheck[k].c.map(row => row.map(r3)) },
    mean_area_density_per_cell: r3(mips[k][Wd.MIPS - 1][0]),
    far_sigma_per_m_horizontal: r3(far[k].sigma), far_sigma_per_m_34deg_down: r3(far[k].sigmaDown), far_mean_opacity: r3(far[k].meanOpacity), far_trees: far[k].trees, far_shape_scale: FAR_SCALE[k],
  }));
  log('setup', JSON.stringify(R.setup));

  leafBuf = buffer(0, U.STORAGE, Wd.leafTable(tiles));
  const mk3d = m => {
    const tex = device.createTexture({ size: [Wd.TEX_N, Wd.TEX_N, Wd.TEX_N], dimension: '3d', format: 'rgba16float', mipLevelCount: Wd.MIPS, usage: TU.TEXTURE_BINDING | TU.COPY_DST });
    for (let l = 0; l < Wd.MIPS; l++) {
      const n = Wd.TEX_N >> l;
      device.queue.writeTexture({ texture: tex, mipLevel: l }, Wd.toHalf(m[l]), { bytesPerRow: n * 8, rowsPerImage: n }, [n, n, n]);
    }
    return tex;
  };
  texB = mk3d(mips[0]);
  texC = mk3d(mips[1]);
  sampRepeat = device.createSampler({ addressModeU: 'repeat', addressModeV: 'repeat', addressModeW: 'repeat', magFilter: 'linear', minFilter: 'linear', mipmapFilter: 'linear' });
  sampClamp = device.createSampler({ magFilter: 'linear', minFilter: 'linear' });
  frameBuf = buffer(FRAME_BYTES, U.UNIFORM | U.COPY_DST);
  R.memory_mb = { leaf_table: r3(Wd.leafTable(tiles).byteLength / 2 ** 20), textures_3d: r3(2 * Wd.TEX_N ** 3 * 8 * 8 / 7 / 2 ** 20) };

  const C = GPUShaderStage.COMPUTE;
  const ub = b => ({ binding: b, visibility: C, buffer: { type: 'uniform' } });
  const sb = (b, ro) => ({ binding: b, visibility: C, buffer: { type: ro ? 'read-only-storage' : 'storage' } });
  const t3d = b => ({ binding: b, visibility: C, texture: { sampleType: 'float', viewDimension: '3d' } });
  const tf = b => ({ binding: b, visibility: C, texture: { sampleType: 'float' } });
  const tu = b => ({ binding: b, visibility: C, texture: { sampleType: 'uint' } });
  const sm = b => ({ binding: b, visibility: C, sampler: { type: 'filtering' } });
  const st = (b, format) => ({ binding: b, visibility: C, storageTexture: { access: 'write-only', format } });
  L.trace = device.createBindGroupLayout({ entries: [ub(0), sb(1, true), t3d(2), t3d(3), sm(4), st(5, 'rgba32uint'), st(6, 'rg32uint'), sb(7), sb(9, true)] });
  L.light = device.createBindGroupLayout({ entries: [ub(0), sb(1, true), t3d(2), t3d(3), sm(4), tu(5), tu(6), sb(7), st(8, 'rgba16float'), sb(9, true)] });
  L.map = device.createBindGroupLayout({ entries: [ub(0), sb(10)] });
  L.taa = device.createBindGroupLayout({ entries: [ub(0), tf(1), tf(2), sm(3), tu(4), st(5, 'rgba16float')] });
  L.accum = device.createBindGroupLayout({ entries: [tf(10), sb(11), ub(12)] });
  L.resolve = device.createBindGroupLayout({ entries: [sb(11), ub(12), st(13, 'rgba16float')] });
  L.diff = device.createBindGroupLayout({ entries: [ub(12), tf(14), tf(15), sb(16)] });
  L.diffimg = device.createBindGroupLayout({ entries: [ub(12), tf(14), tf(15), st(18, 'rgba16float')] });
  L.cov = device.createBindGroupLayout({ entries: [ub(12), sb(16), tu(17)] });
  L.blit = device.createBindGroupLayout({ entries: [
    { binding: 0, visibility: GPUShaderStage.FRAGMENT, texture: {} }, { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: {} },
    { binding: 2, visibility: GPUShaderStage.FRAGMENT, buffer: { type: 'uniform' } }] });
  statsBuf = buffer(64 * 4, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  mapBuf = buffer(1536 * 1536 * 32, U.STORAGE);
  R.memory_mb.tree_table = r3(1536 * 1536 * 32 / 2 ** 20);
}
let mapBuf, mapOcc = -1;
let statsBuf;

const TRACE_VARIANTS = {
  full: {}, terrain: { TREES: 0, FLOOR: 0 }, stats: { STATS: 1 }, heat: { VIEW: 1 }, ref: { REF: 1 }, nomap: { MAP: 0 },
  trees_only: { FLOOR: 0 }, floor_only: { TREES: 0 },
};
const LIGHT_VARIANTS = {
  full: {}, noshadow: { SHADOWS: 0 }, stats: { STATS: 1 }, heat: { VIEW: 2 }, ref: { REF: 1 }, nomap: { MAP: 0 },
};

async function checkModule(module, label) {
  const info = await module.getCompilationInfo();
  for (const m of info.messages) (m.type === 'error' ? err : log)(`${label} ${m.type} line ${m.lineNum}:${m.linePos} ${m.message}`);
  return !info.messages.some(m => m.type === 'error');
}

/** Builds the pipelines named in `want` (all variants when null), two at a time: every override
 *  variant is a separate Metal compile of a large shader, and compiling twenty at once (as an earlier
 *  version did) floods the machine with shader-compiler processes. */
async function pipelines(want) {
  const tm = device.createShaderModule({ code: src.common + src.foliage + src.trace, label: 'trace' });
  const lm = device.createShaderModule({ code: src.common + src.foliage + src.light, label: 'light' });
  const pm = device.createShaderModule({ code: src.common + src.post, label: 'post' });
  const mm = device.createShaderModule({ code: src.common + src.foliage + src.map, label: 'map' });
  const ok = (await checkModule(tm, 'trace.wgsl (common+foliage+trace)')) & (await checkModule(lm, 'light.wgsl (common+foliage+light)'))
    & (await checkModule(pm, 'post.wgsl (common+post)')) & (await checkModule(mm, 'map.wgsl (common+foliage+map)'));
  if (!ok) throw new Error('WGSL errors (see console)');
  const lay = bgl => device.createPipelineLayout({ bindGroupLayouts: [bgl] });
  const all = [];
  for (const [n, c] of Object.entries(TRACE_VARIANTS)) all.push([`trace_${n}`, () => device.createComputePipelineAsync({ layout: lay(L.trace), compute: { module: tm, entryPoint: 'trace', constants: c } })]);
  for (const [n, c] of Object.entries(LIGHT_VARIANTS)) all.push([`light_${n}`, () => device.createComputePipelineAsync({ layout: lay(L.light), compute: { module: lm, entryPoint: 'light', constants: c } })]);
  for (const [n, l] of [['taa', L.taa], ['accum', L.accum], ['resolve', L.resolve], ['diff', L.diff], ['coverage', L.cov], ['diffimg', L.diffimg]]) {
    all.push([n, () => device.createComputePipelineAsync({ layout: lay(l), compute: { module: pm, entryPoint: n } })]);
  }
  all.push(['build_map', () => device.createComputePipelineAsync({ layout: lay(L.map), compute: { module: mm, entryPoint: 'build_map', constants: { MAP: 0 } } })]);
  const list = all.filter(([n]) => !want || want.includes(n));
  const out = {}, ms = {};
  const t0 = performance.now();
  for (let i = 0; i < list.length; i += 2) {
    await Promise.all(list.slice(i, i + 2).map(async ([n, f]) => { const t = performance.now(); out[n] = await f(); ms[n] = r3(performance.now() - t); }));
  }
  ms.total = r3(performance.now() - t0);
  return { out, ms };
}

async function blitPipeline() {
  const bm = device.createShaderModule({ code: src.blit, label: 'blit' });
  await checkModule(bm, 'blit.wgsl');
  P.blit = await device.createRenderPipelineAsync({
    layout: device.createPipelineLayout({ bindGroupLayouts: [L.blit] }),
    vertex: { module: bm, entryPoint: 'blit_vs' },
    fragment: { module: bm, entryPoint: 'blit_fs', targets: [{ format: canvasFormat }] },
  });
}

function makeRes(r, extras) {
  const tex = (format, usage) => device.createTexture({ size: [r.w, r.h], format, usage });
  const S = TU.STORAGE_BINDING | TU.TEXTURE_BINDING;
  const o = { ...r };
  o.gA = tex('rgba32uint', S);
  o.gB = tex('rg32uint', S);
  o.lit = tex('rgba16float', S | TU.COPY_SRC);
  o.hist = [tex('rgba16float', S | TU.COPY_SRC), tex('rgba16float', S | TU.COPY_SRC)];
  o.ping = 0;
  if (extras) {
    o.acc = buffer(r.w * r.h * 16, U.STORAGE);
    o.img = {};
    for (const n of ['ref', 'refA', 'refB', 'conv', 'taa', 'raw', 'lvA', 'lvB', 'prev']) o.img[n] = tex('rgba16float', S | TU.COPY_DST | TU.COPY_SRC);
  }
  o.ap = buffer(32, U.UNIFORM | U.COPY_DST);
  device.queue.writeBuffer(o.ap, 0, new Uint32Array([r.w, r.h, 0, 1, 0, 0, r.w, r.h]));
  o.dres = buffer(32, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  const res = b => ({ resource: { buffer: b } });
  const v = t => t.createView();
  const common = [{ binding: 0, ...res(frameBuf) }, { binding: 1, ...res(leafBuf) }, { binding: 2, resource: texB.createView() },
    { binding: 3, resource: texC.createView() }, { binding: 4, resource: sampRepeat }];
  o.bgTrace = device.createBindGroup({ layout: L.trace, entries: [...common, { binding: 5, resource: v(o.gA) }, { binding: 6, resource: v(o.gB) }, { binding: 7, ...res(statsBuf) }, { binding: 9, ...res(mapBuf) }] });
  o.bgLight = device.createBindGroup({ layout: L.light, entries: [...common, { binding: 5, resource: v(o.gA) }, { binding: 6, resource: v(o.gB) },
    { binding: 7, ...res(statsBuf) }, { binding: 8, resource: v(o.lit) }, { binding: 9, ...res(mapBuf) }] });
  o.bgTaa = [0, 1].map(i => device.createBindGroup({ layout: L.taa, entries: [{ binding: 0, ...res(frameBuf) }, { binding: 1, resource: v(o.lit) },
    { binding: 2, resource: v(o.hist[i]) }, { binding: 3, resource: sampClamp }, { binding: 4, resource: v(o.gA) }, { binding: 5, resource: v(o.hist[1 - i]) }] }));
  o.bgBlit = new Map();
  return o;
}

let blitBuf;
function blitGroup(res, texture) {
  blitBuf ||= buffer(16, U.UNIFORM | U.COPY_DST);
  if (!res.bgBlit.has(texture)) res.bgBlit.set(texture, device.createBindGroup({ layout: L.blit, entries: [{ binding: 0, resource: texture.createView() }, { binding: 1, resource: sampClamp }, { binding: 2, resource: { buffer: blitBuf } }] }));
  return res.bgBlit.get(texture);
}

// ---- scenes -----------------------------------------------------------------------------------------------------------

const norm = a => { const l = Math.hypot(...a); return a.map(x => x / l); };
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sub = (a, b) => a.map((x, i) => x - b[i]);
const add = (a, b) => a.map((x, i) => x + b[i]);
const scl = (a, s) => a.map(x => x * s);

function camera(eye, target) {
  const f = norm(sub(target, eye));
  const r = norm(cross(f, [0, 1, 0]));
  const u = cross(r, f);
  return { eye, target, f, r, u };
}

/** A camera on the forest floor whose view of the sun passes through a broadleaf crown: a canopy
 *  tree just inside the forest on the sun's side, the camera on the anti-sun side, under other trees,
 *  and no trunk within 1.5 m. */
function findUpCamera() {
  const az = norm([SUN[0], SUN[2]]);
  const tanE = SUN[1] / Math.hypot(SUN[0], SUN[2]);
  let best = null;
  for (let i = -14; i <= 13; i++) for (let j = -14; j <= 13; j++) {
    const t = Wd.treeAt(i, j, DEFAULTS.occupancy);
    if (!t.exists || t.kind !== 0) continue;
    const r = Math.hypot(t.px, t.pz);
    if (r < 40 || r > 75) continue;
    let ex = t.px, ez = t.pz, ey = 0;
    for (let it = 0; it < 3; it++) {
      ey = Wd.terrainH(ex, ez) + 1.7;
      const dh = t.base + t.cy - 0.25 * t.rv - ey;
      ex = t.px - az[0] * dh / tanE; ez = t.pz - az[1] * dh / tanE;
    }
    let clear = true, cover = 0;
    for (let a = -2; a <= 2; a++) for (let b = -2; b <= 2; b++) for (const slot of [0, 1]) {
      const n = Wd.treeAt(Math.floor(ex / Wd.TREE_C) + a, Math.floor(ez / Wd.TREE_C) + b, DEFAULTS.occupancy, slot);
      if (!n.exists) continue;
      const dd = Math.hypot(n.px - ex, n.pz - ez);
      if (dd < 1.5 + (slot ? n.rh : 0)) clear = false;
      if (slot === 0 && dd < n.rh) cover++;
    }
    if (!clear || Wd.forestMask(ex, ez) < 0.9) continue;
    const s = cover + 0.5 * t.size - 0.02 * r;
    if (!best || s > best.s) best = { s, t, eye: [ex, ey, ez] };
  }
  return best;
}

/** Trees for the transition test: the first `n` of a species along a row of the forest. */
function findIsoTrees(kind, n) {
  const out = [];
  for (let i = 4; i < 80 && out.length < n; i++) {
    const t = Wd.treeAt(i, -9, DEFAULTS.occupancy);
    if (t.exists && t.kind === kind) out.push(t);
  }
  return out;
}

function makeScenes() {
  const h0 = Wd.terrainH(-3, -8);
  const floorEye = [-3, h0 + 1.7, -8];
  const floor = { name: 'floor', cam: camera(floorEye, add(floorEye, [0.28, -0.03, 1.0])) };
  const uc = findUpCamera();
  const ut = uc.t;
  const look = norm(add(SUN, [0, 0.25, 0]));
  const up = { name: 'up', cam: camera(uc.eye, add(uc.eye, look)), tree: { cell: [ut.ci, ut.cj], pos: [ut.px, ut.pz], height: ut.height } };
  const ah = Wd.terrainH(5, 5);
  const aboveEye = [5, ah + 60, 5];
  const above = { name: 'above', cam: camera(aboveEye, add(aboveEye, [0.85, -0.2, 0.53])) };
  return [floor, up, above];
}

// ---- frame uniform --------------------------------------------------------------------------------------------------

const FRAME_BYTES = 496;
const FR = new ArrayBuffer(FRAME_BYTES);
const FF = new Float32Array(FR), FI = new Int32Array(FR), FU = new Uint32Array(FR);

/** Writes the frame uniform. `tile` = [x, y, w, h] limits trace and light to a tile (long renders). */
function writeFrame(res, cam, prm, st, tile = null) {
  const ty = Math.tan(FOVY / 2), tx = ty * res.w / res.h;
  FF.set([...cam.eye, 2 * ty / res.h], 0);
  FF.set([...cam.f, st.time], 4);
  FF.set([...scl(cam.r, tx), prm.wind], 8);
  FF.set([...scl(cam.u, ty), st.frame], 12);
  FF.set([...SUN, SUN_RADIUS], 16);
  FF.set([st.jx, st.jy, prm.view, prm.taaAlpha], 20);
  FF.set([...cam.eye, 0], 24);
  FF.set([...cam.f, 0], 28);
  FF.set([...scl(cam.r, tx), 0], 32);
  FF.set([...scl(cam.u, ty), 0], 36);
  FF.set([prm.occupancy, prm.grassN, prm.fern, st.taaFrames], 40);
  FF.set([prm.k0, prm.k1, prm.minLevel, prm.forced], 44);
  FF.set([prm.kg, prm.kf, prm.volSteps, prm.shadowSteps], 48);
  FI.set([prm.iso ? prm.iso[0] : 0, prm.iso ? prm.iso[1] : 0, prm.iso ? 1 : 0, prm.noFloor], 52);
  FU.set([res.w, res.h, st.frame, st.sample], 56);
  const cal = world.cal;
  for (let k = 0; k < 2; k++) for (let m = 0; m < 8; m++) for (let b = 0; b < 3; b++) FF[60 + k * 24 + m * 3 + b] = cal[k].c[m][b];
  FF.set([world.far[0].sigma, world.far[1].sigma, world.farScale[0], world.farScale[1]], 108);
  FU.set(tile || [0, 0, res.w, res.h], 112);
  FU.set([prm.dbg || 0, 0, 0, 0], 116);
  FF.set([prm.leafBudget, world.far[0].sigmaDown, world.far[1].sigmaDown, 0], 120);
  device.queue.writeBuffer(frameBuf, 0, FR);
  if (prm.occupancy !== mapOcc) buildMap(prm.occupancy);
}

/** Cooks the tree table for this occupancy (uses the frame uniform just written). */
function buildMap(occ) {
  mapOcc = occ;
  const bg = device.createBindGroup({ layout: L.map, entries: [{ binding: 0, resource: { buffer: frameBuf } }, { binding: 10, resource: { buffer: mapBuf } }] });
  const enc = device.createCommandEncoder();
  const p = enc.beginComputePass();
  p.setPipeline(P.build_map); p.setBindGroup(0, bg); p.dispatchWorkgroups(192, 192); p.end();
  device.queue.submit([enc.finish()]);
}

/** Per-frame state: jitter (Halton 2,3 over 16), frame counter, TAA frames since reset. */
function frameState(i, { jitter = true, time = null, taaFrames = i, sample = 0 } = {}) {
  frameCounter++;
  const h = (i % 16) + 1;
  return { jx: jitter ? halton(h, 2) - 0.5 : 0, jy: jitter ? halton(h, 3) - 0.5 : 0, frame: frameCounter, taaFrames,
    time: time ?? i / 60, sample };
}

// ---- encoding -------------------------------------------------------------------------------------------------------------

/** Submits one frame (or one tile of it): trace and light, each as two half-frame submissions (so no
 *  single submission holds the GPU for long; their times add), then optionally TAA. Timestamps go to
 *  query slots q0…q0+9: trace halves, light halves, TAA. */
const QPF = 10;
function submitFrame(res, tv, lv, qs, q0, taa = true, tile = null) {
  const ts = i => qs ? { timestampWrites: { querySet: qs, beginningOfPassWriteIndex: q0 + 2 * i, endOfPassWriteIndex: q0 + 2 * i + 1 } } : {};
  const pass = (i, pipe, bg, w, h) => {
    const enc = device.createCommandEncoder();
    const p = enc.beginComputePass(ts(i));
    p.setPipeline(pipe);
    p.setBindGroup(0, bg);
    p.dispatchWorkgroups(Math.ceil(w / 8), Math.ceil(h / 8));
    p.end();
    device.queue.submit([enc.finish()]);
  };
  const parts = tile ? [tile] : [[0, 0, res.w, res.h >> 1], [0, res.h >> 1, res.w, res.h - (res.h >> 1)]];
  for (const [k, pipe, bg] of [[0, P[`trace_${tv}`], res.bgTrace], [2, P[`light_${lv}`], res.bgLight]]) {
    parts.forEach((pt, j) => {
      if (!tile) device.queue.writeBuffer(frameBuf, 448, new Uint32Array(pt));
      pass(k + j, pipe, bg, pt[2], pt[3]);
    });
  }
  if (taa) {
    pass(4, P.taa, res.bgTaa[res.ping], res.w, res.h);
    res.ping = 1 - res.ping;
  }
}
const taaOut = res => res.hist[res.ping];   // after submitFrame flips ping, the last output is hist[ping]

function render(res, cam, prm, st, tv = 'full', lv = 'full', taa = true) {
  writeFrame(res, cam, prm, st);
  submitFrame(res, tv, lv, null, 0, taa);
}

async function resolveTimestamps(qs, count) {
  const buf = buffer(count * 8, U.QUERY_RESOLVE | U.COPY_SRC);
  const enc = device.createCommandEncoder();
  enc.resolveQuerySet(qs, 0, count, buf, 0);
  device.queue.submit([enc.finish()]);
  const raw = await readback(buf, count * 8);
  buf.destroy();
  qs.destroy();
  return new BigUint64Array(raw, 0, count);
}

/** GPU time per pass under sustained load: 30 untimed frames, then `frames` back to back. */
async function measure(res, cam, prm, tv = 'full', lv = 'full', frames = 90, warm = 30) {
  const Q = QPF;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  for (let f = 0; f < warm; f++) {
    writeFrame(res, cam, prm, frameState(f));
    submitFrame(res, tv, lv, null, 0);
    if (f % 10 === 9) await device.queue.onSubmittedWorkDone();
  }
  await device.queue.onSubmittedWorkDone();
  const t1 = performance.now();
  for (let f = 0; f < frames; f++) {
    writeFrame(res, cam, prm, frameState(30 + f));
    submitFrame(res, tv, lv, qs, f * Q);
  }
  await device.queue.onSubmittedWorkDone();
  const wall = (performance.now() - t1) / frames;
  const ts = await resolveTimestamps(qs, frames * Q);
  const d = (f, i) => Number(ts[f * Q + 2 * i + 1] - ts[f * Q + 2 * i]) / 1e6;
  const all = [...Array(frames).keys()];
  const out = {};
  for (const [n, fn] of [['trace', f => d(f, 0) + d(f, 1)], ['light', f => d(f, 2) + d(f, 3)], ['taa', f => d(f, 4)]]) {
    const v = all.map(fn);
    out[n] = r3(median(v));
    out[`${n}_p95`] = r3(pct(v, 0.95));
  }
  out.max_submission_ms = r3(Math.max(...all.flatMap(f => [0, 1, 2, 3, 4].map(i => d(f, i)))));
  const fr = all.map(f => Number(ts[f * Q + 9] - ts[f * Q]) / 1e6);
  out.frame = r3(median(fr));
  out.frame_p95 = r3(pct(fr, 0.95));
  out.wall_ms_per_frame = r3(wall);
  return out;
}

/** 60 Hz pacing by busy-wait, as spikes 01 and 02. */
async function paced(res, cam, prm, frames = 180) {
  const Q = QPF, period = 1000 / 60;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  const t0 = performance.now() + 20;
  for (let f = 0; f < frames; f++) {
    const deadline = t0 + f * period;
    while (performance.now() < deadline) { /* spin */ }
    writeFrame(res, cam, prm, frameState(f));
    submitFrame(res, 'full', 'full', qs, f * Q);
  }
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * Q);
  const keep = [...Array(frames).keys()].slice(30);
  const gpu = keep.map(f => Number(ts[f * Q + 9] - ts[f * Q]) / 1e6);
  return { pacing: 'busy-wait at 60 Hz', frames: keep.length, frame_gpu_median: r3(median(gpu)), frame_gpu_p95: r3(pct(gpu, 0.95)),
    frame_gpu_max: r3(Math.max(...gpu)), frame_gpu_over_16_7: gpu.filter(g => g > 16.7).length };
}

const STAT_NAMES = ['px_sky', 'px_ground', 'px_bark', 'px_leaf', 'px_needle', 'px_grass', 'px_fern', 'crowns_explicit_then_volume', 'px_foliage_volume',
  'terrain_steps', 'tree_slabs', 'tree_cells', 'trees_tested', 'crowns_explicit', 'crowns_texture', 'crowns_far', 'leaf_cells', 'leaf_tests',
  'volume_samples', 'grass_slabs', 'grass_cells', 'blade_tests', 'fern_cells', 'frond_tests', 'trunk_tests', 'branch_tests',
  'cap_terrain', 'cap_slabs', 'cap_leaf_cells', 'cap_volume', 'cap_floor', 'px_any_cap'];
const LSTAT_NAMES = ['shadow_rays', 'shadow_slabs', 'shadow_cells', 'shadow_trees', 'shadow_volume_samples', 'shadow_leaf_cells', 'shadow_leaf_tests',
  'shadow_caps', 'ao_tree_lookups', 'px_two_layers', 'shadow_bark_tests', 'px_shadow_any_cap'];

async function stats(res, cam, prm) {
  writeFrame(res, cam, prm, frameState(0, { time: T_SNAP }));
  const enc = device.createCommandEncoder();
  enc.clearBuffer(statsBuf);
  device.queue.submit([enc.finish()]);
  submitFrame(res, 'stats', 'stats', null, 0, false);
  const s = new Uint32Array(await readback(statsBuf, 256));
  const o = {};
  STAT_NAMES.forEach((n, i) => { o[n] = s[i]; });
  LSTAT_NAMES.forEach((n, i) => { o[n] = s[32 + i]; });
  const px = res.w * res.h;
  const per = n => r3(o[n] / px);
  o.per_pixel = {
    terrain_steps: per('terrain_steps'), tree_slabs: per('tree_slabs'), tree_cells: per('tree_cells'), trees_tested: per('trees_tested'),
    leaf_cells: per('leaf_cells'), leaf_tests: per('leaf_tests'), volume_samples: per('volume_samples'),
    grass_cells: per('grass_cells'), blade_tests: per('blade_tests'), fern_cells: per('fern_cells'), frond_tests: per('frond_tests'),
    shadow_rays: per('shadow_rays'), shadow_cells: per('shadow_cells'), shadow_volume_samples: per('shadow_volume_samples'),
    ao_tree_lookups: per('ao_tree_lookups'),
  };
  o.cap_share_of_pixels = r3(o.px_any_cap / px * 1000) / 1000;
  o.shadow_cap_share_of_pixels = r3(o.px_shadow_any_cap / px * 1000) / 1000;
  return o;
}

// ---- images ----------------------------------------------------------------------------------------------------------

function accumulate(res, srcTex, mode) {
  const bg = device.createBindGroup({ layout: L.accum, entries: [{ binding: 10, resource: srcTex.createView() }, { binding: 11, resource: { buffer: res.acc } }, { binding: 12, resource: { buffer: res.ap } }] });
  device.queue.writeBuffer(res.ap, 0, new Uint32Array([res.w, res.h, mode, 1, 0, 0, res.w, res.h]));
  const enc = device.createCommandEncoder();
  const p = enc.beginComputePass();
  p.setPipeline(P.accum); p.setBindGroup(0, bg); p.dispatchWorkgroups(Math.ceil(res.w / 8), Math.ceil(res.h / 8)); p.end();
  device.queue.submit([enc.finish()]);
}
function resolveAcc(res, dst, n) {
  const bg = device.createBindGroup({ layout: L.resolve, entries: [{ binding: 11, resource: { buffer: res.acc } }, { binding: 12, resource: { buffer: res.ap } }, { binding: 13, resource: dst.createView() }] });
  device.queue.writeBuffer(res.ap, 0, new Uint32Array([res.w, res.h, 0, n, 0, 0, res.w, res.h]));
  const enc = device.createCommandEncoder();
  const p = enc.beginComputePass();
  p.setPipeline(P.resolve); p.setBindGroup(0, bg); p.dispatchWorkgroups(Math.ceil(res.w / 8), Math.ceil(res.h / 8)); p.end();
  device.queue.submit([enc.finish()]);
}
function copyTex(res, from, to) {
  const enc = device.createCommandEncoder();
  enc.copyTextureToTexture({ texture: from }, { texture: to }, [res.w, res.h]);
  device.queue.submit([enc.finish()]);
}

/** Differences in 8-bit display values over `rect` (default the whole frame), as spike 02 reports them. */
async function diff(res, a, b, rect = null) {
  const rc = rect || [0, 0, res.w, res.h];
  const bg = device.createBindGroup({ layout: L.diff, entries: [{ binding: 12, resource: { buffer: res.ap } }, { binding: 14, resource: a.createView() },
    { binding: 15, resource: b.createView() }, { binding: 16, resource: { buffer: res.dres } }] });
  device.queue.writeBuffer(res.ap, 0, new Uint32Array([res.w, res.h, 0, 1, ...rc]));
  const enc = device.createCommandEncoder();
  enc.clearBuffer(res.dres);
  const p = enc.beginComputePass();
  p.setPipeline(P.diff); p.setBindGroup(0, bg); p.dispatchWorkgroups(Math.ceil(res.w / 8), Math.ceil(res.h / 8)); p.end();
  device.queue.submit([enc.finish()]);
  const s = new Uint32Array(await readback(res.dres, 32));
  const n = (rc[2] - rc[0]) * (rc[3] - rc[1]);
  return { mean_abs_255: r3(s[0] / (3 * n)), share_over_8: Math.round(s[1] / n * 1e5) / 1e5, share_any: r3(s[2] / n) };
}

/** Writes the error image of a against b into res.img.prev and saves the crop. */
async function errorImage(res, a, b, name) {
  const bg = device.createBindGroup({ layout: L.diffimg, entries: [{ binding: 12, resource: { buffer: res.ap } }, { binding: 14, resource: a.createView() },
    { binding: 15, resource: b.createView() }, { binding: 18, resource: res.img.prev.createView() }] });
  device.queue.writeBuffer(res.ap, 0, new Uint32Array([res.w, res.h, 0, 1, 0, 0, res.w, res.h]));
  const enc = device.createCommandEncoder();
  const p = enc.beginComputePass();
  p.setPipeline(P.diffimg); p.setBindGroup(0, bg); p.dispatchWorkgroups(Math.ceil(res.w / 8), Math.ceil(res.h / 8)); p.end();
  device.queue.submit([enc.finish()]);
  await snapshot(res, res.img.prev, name, false, CROP);
}

async function coverage(res) {
  const bg = device.createBindGroup({ layout: L.cov, entries: [{ binding: 12, resource: { buffer: res.ap } }, { binding: 16, resource: { buffer: res.dres } }, { binding: 17, resource: res.gA.createView() }] });
  device.queue.writeBuffer(res.ap, 0, new Uint32Array([res.w, res.h, 0, 1, 0, 0, res.w, res.h]));
  const enc = device.createCommandEncoder();
  enc.clearBuffer(res.dres);
  const p = enc.beginComputePass();
  p.setPipeline(P.coverage); p.setBindGroup(0, bg); p.dispatchWorkgroups(Math.ceil(res.w / 8), Math.ceil(res.h / 8)); p.end();
  device.queue.submit([enc.finish()]);
  const s = new Uint32Array(await readback(res.dres, 32));
  return { sum: s[0] / 1024, px: s[1] };
}

async function snapshot(res, texture, name, jpeg = false, crop = null) {
  const rc = crop ? [crop[0] / res.w, crop[1] / res.h, (crop[2] - crop[0]) / res.w, (crop[3] - crop[1]) / res.h] : [0, 0, 1, 1];
  const bg = blitGroup(res, texture);
  device.queue.writeBuffer(blitBuf, 0, new Float32Array(rc));
  const enc = device.createCommandEncoder();
  const p = enc.beginRenderPass({ colorAttachments: [{ view: ctx.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
  p.setPipeline(P.blit);
  p.setBindGroup(0, bg);
  p.draw(3);
  p.end();
  device.queue.submit([enc.finish()]);
  await device.queue.onSubmittedWorkDone();
  const blob = await new Promise(r => $('view').toBlob(r, jpeg ? 'image/jpeg' : 'image/png', 0.85));
  if (blob) await save(`${name}.${jpeg ? 'jpg' : 'png'}`, blob);
  else err('snapshot failed', name);
}

/** The reference: REF pipelines, per-pixel random jitter and sun-disc samples, accumulated. Rendered
 *  in tiles, one submission each, awaited, so no submission holds the GPU for long; tiles shrink if
 *  one takes more than 40 ms. */
let refTile = [240, 135];
async function reference(res, cam, prm, dst, n, seed, crop = CROP) {
  let worst = 0;
  const t0 = performance.now();
  for (let i = 0; i < n; i++) {
    const st = frameState(0, { jitter: false, time: T_SNAP, sample: seed * 1000 + i });
    const [tw, th] = refTile;
    let slow = 0;
    for (let ty = crop[1]; ty < crop[3]; ty += th) for (let tx = crop[0]; tx < crop[2]; tx += tw) {
      const tile = [tx, ty, Math.min(tw, crop[2] - tx), Math.min(th, crop[3] - ty)];
      const t1 = performance.now();
      writeFrame(res, cam, prm, st, tile);
      submitFrame(res, 'ref', 'ref', null, 0, false, tile);
      await device.queue.onSubmittedWorkDone();
      const ms = performance.now() - t1;
      worst = Math.max(worst, ms);
      slow = Math.max(slow, ms);
    }
    if (slow > 40 && refTile[0] > 120) { refTile = [refTile[0] / 2, refTile[1] / 2]; log(`reference tiles -> ${refTile} (a tile took ${r3(slow)} ms)`); }
    accumulate(res, res.lit, i === 0 ? 0 : 1);
  }
  resolveAcc(res, dst, n);
  await device.queue.onSubmittedWorkDone();
  return { samples: n, ms: r3(performance.now() - t0), worst_tile_ms: r3(worst), tile: refTile };
}

/** The fast path averaged over `n` jittered frames (no TAA): what its stochastic choices converge to. */
async function converged(res, cam, prm, dst, n) {
  for (let i = 0; i < n; i++) {
    render(res, cam, prm, frameState(i, { time: T_SNAP }), 'full', 'full', false);
    accumulate(res, res.lit, i === 0 ? 0 : 1);
    if (i % 2 === 1) await device.queue.onSubmittedWorkDone();
  }
  resolveAcc(res, dst, n);
  await device.queue.onSubmittedWorkDone();
}

/** TAA from a reset, `n` frames, static camera, frozen time. */
async function taaRun(res, cam, prm, n, time = T_SNAP) {
  for (let i = 0; i < n; i++) {
    render(res, cam, prm, frameState(i, { time, taaFrames: i }), 'full', 'full', true);
    if (i % 2 === 1) await device.queue.onSubmittedWorkDone();
  }
  await device.queue.onSubmittedWorkDone();
  return taaOut(res);
}

/** Mean frame-to-frame difference of the displayed image: static camera, still wind, jittered frames. */
async function shimmer(res, cam, prm, n = 17) {
  const still = { ...prm, wind: 0 };
  const out = {};
  for (const mode of ['raw', 'taa']) {
    const vals = [];
    // Settle TAA first so the metric measures the steady state, not convergence.
    for (let i = 0; i < 16; i++) {
      render(res, cam, still, frameState(i, { time: 0, taaFrames: i }), 'full', 'full', true);
      if (i % 2 === 1) await device.queue.onSubmittedWorkDone();
    }
    for (let i = 0; i < n; i++) {
      render(res, cam, still, frameState(16 + i, { time: 0, taaFrames: 16 + i }), 'full', 'full', true);
      const cur = mode === 'raw' ? res.lit : taaOut(res);
      if (i > 0) vals.push(await diff(res, cur, res.img.prev));
      copyTex(res, cur, res.img.prev);
    }
    out[mode] = { mean_abs_255: r3(median(vals.map(v => v.mean_abs_255))), share_over_8: r3(median(vals.map(v => v.share_over_8)) * 1000) / 1000 };
  }
  out.reduction = r3(out.raw.mean_abs_255 / Math.max(out.taa.mean_abs_255, 1e-6));
  return out;
}

// ---- run --------------------------------------------------------------------------------------------------------------

async function setupAll(want = null) {
  await init();
  const t0 = performance.now();
  const all = await pipelines(want);
  Object.assign(P, all.out);
  R.pipelines.creation_ms_two_at_a_time = all.ms;   // includes any browser cache hits; cold compile isn't measured
  R.pipelines.per_scene = 'trace, light, taa (+ blit) = 4';
  await blitPipeline();
  log('pipelines', JSON.stringify(R.pipelines), `${r3(performance.now() - t0)} ms`);
  RES.full = makeRes(FULL, !QUICK);
  if (!QUICK) RES.half = makeRes(HALF, false);
}

async function runScene(sc, prm) {
  const res = RES.full;
  const name = sc.name;
  const cam = sc.cam;
  R.scenes[name] = { eye: cam.eye.map(r3), target: cam.target.map(r3), trees_within_300m: Wd.countTrees(cam.eye[0], cam.eye[2], 300, prm.occupancy),
    understory_within_300m: Wd.countTrees(cam.eye[0], cam.eye[2], 300, prm.occupancy, 1) };
  if (sc.tree) R.scenes[name].tree = sc.tree;
  R.stats[name] = await stats(res, cam, prm);
  log(`${name} stats`, JSON.stringify(R.stats[name]));

  for (const [rname, r] of [['1080p', RES.full], ['540p', RES.half]]) {
    const hd = rname === '1080p';
    const full = await measure(r, cam, prm, 'full', 'full', hd ? 90 : 40, hd ? 30 : 10);
    const terr = await measure(r, cam, prm, 'terrain', 'full', hd ? 90 : 40, hd ? 30 : 10);
    R.frames[`${name}/${rname}/full`] = full;
    R.frames[`${name}/${rname}/terrain`] = terr;
    if (rname === '1080p') R.frames[`${name}/${rname}/noshadow`] = await measure(r, cam, prm, 'full', 'noshadow', 30, 10);
    if (rname === '540p' && name === 'floor') {
      // What cooking the per-cell tree attributes buys: the same frame with them evaluated inline.
      // (At 540p, so the slower variant's submissions stay short.)
      R.frames[`${name}/${rname}/inline-tree-attributes`] = await measure(r, cam, prm, 'nomap', 'nomap', 30, 10);
    }
    R.scenes[name][rname] = {
      vegetation_ms: r3(full.trace - terr.trace), lighting_ms: full.light, taa_ms: full.taa, frame_ms: full.frame,
      trace_ms: full.trace, terrain_trace_ms: terr.trace,
    };
    if (rname === '1080p') R.scenes[name][rname].shadow_ms = r3(full.light - R.frames[`${name}/${rname}/noshadow`].light);
    log(`${name} ${rname}`, JSON.stringify(R.scenes[name][rname]), 'full', JSON.stringify(full));
  }

  // Correctness: reference (two independent halves), converged fast path, TAA, single frame.
  const cprm = { ...prm };
  R.scenes[name].reference = [await reference(res, cam, cprm, res.img.refA, REF_HALF, 1), await reference(res, cam, cprm, res.img.refB, REF_HALF, 2)];
  log(`${name} reference`, JSON.stringify(R.scenes[name].reference));
  accumulate(res, res.img.refA, 0);
  accumulate(res, res.img.refB, 1);
  resolveAcc(res, res.img.ref, 2);
  await converged(res, cam, cprm, res.img.conv, CONV);
  const taaTex = await taaRun(res, cam, cprm, CONV);
  copyTex(res, taaTex, res.img.taa);
  render(res, cam, cprm, frameState(0, { jitter: false, time: T_SNAP }), 'full', 'full', false);
  copyTex(res, res.lit, res.img.raw);
  R.quality[name] = {
    region: CROP,
    reference_noise_floor: await diff(res, res.img.refA, res.img.refB, CROP),
    converged_vs_reference: await diff(res, res.img.conv, res.img.ref, CROP),
    taa_vs_reference: await diff(res, res.img.taa, res.img.ref, CROP),
    single_frame_vs_reference: await diff(res, res.img.raw, res.img.ref, CROP),
  };
  R.stats[name].cap_share_of_pixels_note = 'whole frame, fast path';
  log(`${name} quality`, JSON.stringify(R.quality[name]));
  // The compared region, magnified 2×.
  await snapshot(res, res.img.ref, `${name}-crop-ref`, false, CROP);
  await snapshot(res, res.img.conv, `${name}-crop-converged`, false, CROP);
  await snapshot(res, res.img.taa, `${name}-crop-taa`, false, CROP);
  await snapshot(res, res.img.raw, `${name}-crop-single`, false, CROP);
  await errorImage(res, res.img.conv, res.img.ref, `${name}-crop-error`);
  await snapshot(res, res.img.taa, `${name}-taa`);
  await snapshot(res, res.img.raw, `${name}-single`);

  // Heat maps.
  render(res, cam, cprm, frameState(0, { jitter: false, time: T_SNAP }), 'heat', 'full', false);
  await snapshot(res, res.lit, `${name}-heat`);
  render(res, cam, cprm, frameState(0, { jitter: false, time: T_SNAP }), 'full', 'heat', false);
  await snapshot(res, res.lit, `${name}-heat-shadow`);

  R.shimmer[name] = await shimmer(res, cam, prm);
  log(`${name} shimmer`, JSON.stringify(R.shimmer[name]));
}

/** The sweeps. Each point is 10 warm-up and 30 measured frames (shorter than the scenes' 30 + 90, to
 *  keep the run near three minutes); vegetation uses the scene's own terrain-only time, except the
 *  view-distance sweep, which changes the terrain too. */
async function sweeps(scenes, prm) {
  const by = Object.fromEntries(scenes.map(s => [s.name, s]));
  const res = RES.full;
  const point = async (key, scn, p, terrainMs = null) => {
    const m = await measure(res, by[scn].cam, p, 'full', 'full', 30, 10);
    const tt = terrainMs ?? (await measure(res, by[scn].cam, p, 'terrain', 'full', 30, 10)).trace;
    const s = await stats(res, by[scn].cam, p);
    R.sweeps[key] = { vegetation_ms: r3(m.trace - tt), lighting_ms: m.light, frame_ms: m.frame, trace_ms: m.trace,
      leaf_cells_per_px: s.per_pixel.leaf_cells, volume_samples_per_px: s.per_pixel.volume_samples, tree_cells_per_px: s.per_pixel.tree_cells,
      blade_tests_per_px: s.per_pixel.blade_tests, shadow_cells_per_px: s.per_pixel.shadow_cells, cap_share: s.cap_share_of_pixels };
    log('sweep', key, JSON.stringify(R.sweeps[key]));
  };
  const terr = n => R.frames[`${n}/1080p/terrain`].trace;
  for (const occ of [0.35, 0.6, 1.0]) {
    for (const n of ['floor', 'above']) {
      await point(`density/${n}/occupancy=${occ}`, n, { ...prm, occupancy: occ }, terr(n));
      R.sweeps[`density/${n}/occupancy=${occ}`].trees_within_300m = Wd.countTrees(by[n].cam.eye[0], by[n].cam.eye[2], 300, occ);
    }
  }
  for (const g of [1, 3]) await point(`grass/floor/blades=${g}`, 'floor', { ...prm, grassN: g }, terr('floor'));
  for (const v of [250, 500, 1000, 4000]) await point(`view/above/${v}m`, 'above', { ...prm, view: v });
  for (const b of [12, 1024]) for (const n of ['floor', 'up']) await point(`explicit-budget/${n}/${b}-cells`, n, { ...prm, leafBudget: b }, terr(n));
  for (const n of ['floor', 'above']) await point(`volume-steps/${n}/16`, n, { ...prm, volSteps: 16 }, terr(n));
  for (const [rep, lv] of [['no-explicit', 1], ['clumps', 2]]) {
    for (const n of ['floor', 'up']) {
      await point(`representation/${n}/${rep}`, n, { ...prm, minLevel: lv }, terr(n));
      await taaRun(res, by[n].cam, { ...prm, minLevel: lv }, 24);
      await snapshot(res, taaOut(res), `${n}-${rep}`);
    }
  }
}

/** Criterion 4: isolated trees at each boundary's midpoint (where the random choice is 50/50), the two
 *  adjacent levels forced, converged over 16 jittered frames. Four trees per species, each seen from a
 *  different azimuth; the criterion uses the change in their summed coverage. */
async function transitions(prm) {
  const res = RES.full;
  const out = {};
  const pf = 2 * Math.tan(FOVY / 2) / res.h;
  for (const kind of [0, 1]) {
    const trees = findIsoTrees(kind, 4);
    const cell = Wd.SPECIES[kind].cell;
    for (const [a, b, fp] of [[0, 1, Math.SQRT2 * prm.k0 * cell], [1, 2, Math.SQRT2 * prm.k1]]) {
      const dist = fp / pf;
      const key = `${Wd.SPECIES[kind].name}/${a}-${b}`;
      const per = [];
      let sumA = 0, sumB = 0, diffSum = 0, foot = 0;
      for (const [ti, t] of trees.entries()) {
        const cc = [t.px, t.base + t.cy + (kind === 1 ? t.rv * 0.35 : 0), t.pz];
        const az = 0.6 + ti * 1.7;
        const eye = [cc[0] - dist * Math.cos(az), cc[1] + dist * 0.05, cc[2] - dist * Math.sin(az)];
        const cam = camera(eye, cc);
        const covs = [];
        for (const [lvl, img] of [[a, res.img.lvA], [b, res.img.lvB]]) {
          const p = { ...prm, iso: [t.ci, t.cj], noFloor: 1, forced: lvl, wind: 0, leafBudget: 1024 };
          let sum = 0, px = 0;
          for (let i = 0; i < 16; i++) {
            render(res, cam, p, frameState(i, { time: 0 }), 'full', 'full', false);
            const c = await coverage(res);
            sum += c.sum; px = Math.max(px, c.px);
            accumulate(res, res.lit, i === 0 ? 0 : 1);
          }
          resolveAcc(res, img, 16);
          covs.push({ coverage_px: sum / 16, footprint_px: px });
        }
        const d = await diff(res, res.img.lvA, res.img.lvB);
        sumA += covs[0].coverage_px; sumB += covs[1].coverage_px;
        diffSum += d.mean_abs_255 * res.w * res.h; foot += Math.max(covs[0].footprint_px, covs[1].footprint_px);
        per.push(r3((covs[1].coverage_px - covs[0].coverage_px) / covs[0].coverage_px));
        if (ti === 0) {
          await snapshot(res, res.img.lvA, `transition-${key.replace('/', '-')}-a`);
          await snapshot(res, res.img.lvB, `transition-${key.replace('/', '-')}-b`);
        }
      }
      out[key] = { distance_m: r3(dist), trees: trees.length, coverage_px: [r3(sumA), r3(sumB)],
        coverage_change: r3((sumB - sumA) / sumA), per_tree_change: per, mean_abs_within_trees_255: r3(diffSum / Math.max(foot, 1)) };
      log('transition', key, JSON.stringify(out[key]));
    }
  }
  return out;
}

async function runAll() {
  $('run').disabled = true;
  const tStart = performance.now();
  try {
    await setupAll();
    const prm = { ...DEFAULTS };
    const scenes = makeScenes();
    for (const sc of scenes) { await runScene(sc, prm); log(`elapsed ${r3((performance.now() - tStart) / 1000)} s`); }
    R.paced.floor = await paced(RES.full, scenes[0].cam, prm);
    log('paced', JSON.stringify(R.paced));
    await sweeps(scenes, prm);
    log(`elapsed ${r3((performance.now() - tStart) / 1000)} s`);
    R.transitions = await transitions(prm);
    R.params = prm;
  } catch (e) {
    err('FAILED:', e.stack || e.message);
    R.error = String(e.stack || e.message);
  }
  R.finished = new Date().toISOString();
  R.elapsed_s = r3((performance.now() - tStart) / 1000);
  await save(`run-${R.started.replace(/[:.]/g, '-')}.json`, JSON.stringify(R, null, 2));
  log('done', R.elapsed_s, 's');
  await save('DONE', R.error ? 'failed' : 'ok');
  $('run').disabled = false;
}

/** Development smoke test (~10 s): each scene with TAA settled, a single frame and a heat map, plus
 *  the work counters and a short, indicative timing. Then DONE. */
async function quick() {
  const tStart = performance.now();
  try {
    QUICK = true;
    await setupAll(['trace_full', 'light_full', 'trace_stats', 'light_stats', 'trace_heat', 'taa', 'build_map']);
    const prm = { ...DEFAULTS };
    const scenes = makeScenes();
    for (const sc of scenes) {
      const res = RES.full;
      const st = await stats(res, sc.cam, prm);
      log(`${sc.name} stats`, JSON.stringify(st));
      const m = await measure(res, sc.cam, prm, 'full', 'full', 10, 4);
      log(`${sc.name} quick timing (10 frames, indicative)`, JSON.stringify(m));
      const tex = await taaRun(res, sc.cam, prm, 16);
      await snapshot(res, tex, `quick-${sc.name}`);
      render(res, sc.cam, prm, frameState(0, { jitter: false, time: T_SNAP }), 'heat', 'full', false);
      await snapshot(res, res.lit, `quick-${sc.name}-heat`);
    }
  } catch (e) {
    err('FAILED:', e.stack || e.message);
  }
  log('quick done', r3((performance.now() - tStart) / 1000), 's');
  await save('DONE', 'quick');
}

/** Cost breakdown for development (~25 s): each scene timed with one feature switched off at a time.
 *  Indicative only (16 frames). */
async function probe() {
  const tStart = performance.now();
  try {
    QUICK = true;
    await setupAll(['trace_full', 'light_full', 'trace_terrain', 'trace_stats', 'light_stats', 'taa', 'build_map', 'trace_trees_only', 'trace_floor_only']);
    const scenes = makeScenes();
    const base = { ...DEFAULTS };
    const cfgs = [['base', {}], ['no-grass-ferns', { dbg: 3 }], ['no-understory', { dbg: 4 }],
      ['no-shadow-trees', { dbg: 32 }], ['clumps-only', { minLevel: 2 }], ['volume-from-0', { minLevel: 1 }], ['budget-12', { leafBudget: 12 }],
      ['budget-48', { leafBudget: 48 }], ['budget-1024', { leafBudget: 1024 }], ['base-again', {}]];
    const want = (process => process)(location.hash.split('=')[1]);
    for (const sc of scenes) {
      if (want && !want.split(',').includes(sc.name)) continue;
      const res = RES.full;
      const st = await stats(res, sc.cam, base);
      log(`${sc.name} stats`, JSON.stringify(st.per_pixel));
      const t = await measure(res, sc.cam, base, 'terrain', 'full', 12, 4);
      log(`${sc.name} terrain-only trace ${t.trace}`);
      for (const v of ['trees_only', 'floor_only', 'full']) {
        const m = await measure(res, sc.cam, base, v, 'full', 12, 4);
        log(`${sc.name} kernel ${v}: trace ${m.trace}`);
      }
      for (const [n, c] of cfgs) {
        if (sc.name !== 'floor' && n === 'no-grass-ferns') continue;
        const m = await measure(res, sc.cam, { ...base, ...c }, 'full', 'full', 12, 4);
        log(`${sc.name} ${n}: trace ${m.trace} light ${m.light} frame ${m.frame}`);
      }
    }
  } catch (e) {
    err('FAILED:', e.stack || e.message);
  }
  log('probe done', r3((performance.now() - tStart) / 1000), 's');
  await save('DONE', 'probe');
}

$('run').addEventListener('click', runAll);
if (location.hash === '#run') runAll();
if (location.hash === '#quick') quick();
if (location.hash.startsWith('#probe')) probe();
if (location.hash === '#trans') (async () => {
  try {
    await setupAll(['trace_full', 'light_full', 'taa', 'build_map', 'accum', 'resolve', 'diff', 'coverage']);
    makeScenes();
    R.transitions = await transitions({ ...DEFAULTS });
  } catch (e) { err('FAILED:', e.stack || e.message); }
  await save('DONE', 'trans');
})();
