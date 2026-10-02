// Spike 10 harness: builds each scene's module and pipelines, renders, times, sweeps, compares with
// the brute-force references and saves results. It plays the engine's role plus the measuring; it
// isn't compiler output.

const W = 1920, H = 1080;
const RES = { full: [1920, 1080], half: [960, 540] };
const FOVY = 40 * Math.PI / 180;
const U = GPUBufferUsage;
// GPU safety (spikes/README): no submission may run longer than ~100 ms. Heavy variants render in
// tiles of rows, one submission each, awaited; the tile height adapts to keep each near TILE_MS.
const TILE_MS = 35, TILE_MIN = 1, TILE_MAX = 16, TILE_START = 2;
const COMPILE_BATCH = 4;               // pipelines compiled at once (each spawns a Metal compiler)
const WARM = 30, FRAMES = 90;

const B = { SKIN: 1, LEAF: 2, EYE: 4, WET: 8, LAYER: 16, SPECAA: 32 };
const ID = { SKY: 0, GROUND: 1, HIDE: 2, LEAF: 3, EYE: 4, STONE: 5, MORTAR: 6, WOOD: 7, GLOSS: 8, NOSE: 9, SLAB: 10 };
const ID_NAMES = ['sky', 'ground', 'hide', 'leaf', 'eye', 'stone', 'mortar', 'wood', 'gloss', 'nose', 'slab'];
const STAT = { steps: 12, caps: 13, th_march: 14, th_steps: 15, th_unresolved: 16, sh_rays: 17, sh_steps: 18, sh_caps: 19,
  ref_caps: 20, tr_shadow: 21, extra_evals: 22, ground_caps: 23, puddle_px: 24, moss_px: 25, backlit_px: 26 };

const $ = id => document.getElementById(id);
const log = (...a) => { $('log').textContent += a.join(' ') + '\n'; console.log(...a); };
const hashArgs = new URLSearchParams(location.hash.slice(1).split('&').slice(1).join('&'));
const R = { started: new Date().toISOString(), device: {}, pipelines: {}, scenes: {}, timing: {}, stats: {}, quality: {},
  coverage: {}, thickness: {}, footprint: {}, shimmer: {}, paced: {}, verdict: {}, phases_s: {} };
window.__results = R;

let device, ctx, canvasFormat;
const src = {}, L = {}, P = {}, T = {}, Bf = {};

const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[s.length >> 1] : NaN; };
const pct = (a, q) => { const s = [...a].sort((x, y) => x - y); return s[Math.min(s.length - 1, Math.floor(s.length * q))]; };
const r3 = x => Math.round(x * 1000) / 1000;
const r4 = x => Math.round(x * 10000) / 10000;
const norm = a => { const l = Math.hypot(...a); return a.map(x => x / l); };
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sub = (a, b) => a.map((x, i) => x - b[i]);
const add = (a, b) => a.map((x, i) => x + b[i]);
const scale = (a, s) => a.map(x => x * s);

function buffer(size, usage, data) {
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
  try { await fetch(`results/${name}`, { method: 'PUT', body }); } catch (e) { console.error('save failed', name, e.message); }
}

// ---- the shrub's twigs and leaves (deterministic) ---------------------------------------------------------

function mulberry32(a) {
  return () => {
    a |= 0; a = a + 0x6D2B79F5 | 0;
    let t = Math.imul(a ^ a >>> 15, 1 | a);
    t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
    return ((t ^ t >>> 14) >>> 0) / 4294967296;
  };
}

function shrubData() {
  const r = mulberry32(1234);
  const base = [0.62, 0.0, 1.0];
  const NT = 8, NPL = 6;
  const twigs = [], leaves = [];
  for (let i = 0; i < NT; i++) {
    const az = i * 2 * Math.PI / NT + (r() - 0.5) * 0.6;
    const el1 = (58 + r() * 20) * Math.PI / 180, el2 = (20 + r() * 26) * Math.PI / 180;
    const l1 = 0.18 + r() * 0.14, l2 = 0.22 + r() * 0.12;
    const a = [base[0] + Math.cos(az) * 0.03, -0.02, base[2] + Math.sin(az) * 0.03];
    const d1 = [Math.cos(az) * Math.cos(el1), Math.sin(el1), Math.sin(az) * Math.cos(el1)];
    const b = add(a, scale(d1, l1));
    const az2 = az + (r() - 0.5) * 0.5;
    const d2 = [Math.cos(az2) * Math.cos(el2), Math.sin(el2), Math.sin(az2) * Math.cos(el2)];
    const c = add(b, scale(d2, l2));
    twigs.push([...a, 0.008], [...b, 0.005], [...c, 0.0022]);
    for (let k = 0; k < NPL; k++) {
      const s = 0.3 + 0.7 * (k + 0.5 + (r() - 0.5) * 0.4) / NPL;
      const tot = l1 + l2, sl = s * tot;
      const [Pt, Tg] = sl < l1 ? [add(a, scale(d1, sl)), d1] : [add(b, scale(d2, sl - l1)), d2];
      const side = norm(cross(Tg, [0, 1, 0]));
      const sgn = k % 2 ? 1 : -1;
      const dir = norm(add(add(scale(Tg, 0.5), scale(side, 0.85 * sgn)), [0, 0.25 + r() * 0.3, 0]));
      const o = add(Pt, scale(dir, 0.025));
      const Ln = 0.12 + r() * 0.05, Wd = Ln * (0.40 + r() * 0.08), curl = 0.06 + r() * 0.1;
      const Y = dir;
      let X = norm(cross(Y, [0, 1, 0]));
      let Z = cross(X, Y);
      if (Z[1] < 0) { X = scale(X, -1); Z = cross(X, Y); }
      const roll = (r() - 0.5) * 1.6;
      const Xr = add(scale(X, Math.cos(roll)), scale(Z, Math.sin(roll)));
      leaves.push([...o, Ln], [...Xr, Wd], [...Y, curl], [...Pt, 0]);
    }
  }
  // A static uniform grid over the shrub (the engine's acceleration structure, here built once on the
  // CPU): each cell lists the parts whose bounds come within MG of it, so for a point in a cell, every
  // unlisted part is at least MG away and the shrub's field is min(MG, the listed parts).
  const CS = 0.08, MG = 0.05;
  const parts = [];
  for (let i = 0; i < leaves.length / 4; i++) {
    const [o, X, Y] = [leaves[4 * i], leaves[4 * i + 1], leaves[4 * i + 2]];
    const Ln = o[3], c = add(o.slice(0, 3), scale(Y.slice(0, 3), Ln / 2)), rad = Ln / 2 + 0.035;
    parts.push({ id: i, lo: c.map(x => x - rad), hi: c.map(x => x + rad), sphere: [c, rad] });
  }
  for (let j = 0; j < NT; j++) {
    for (let s = 0; s < 2; s++) {
      const a = twigs[3 * j + s], b = twigs[3 * j + s + 1], rr = Math.max(a[3], b[3]);
      parts.push({ id: 1000 + j, lo: [0, 1, 2].map(k => Math.min(a[k], b[k]) - rr), hi: [0, 1, 2].map(k => Math.max(a[k], b[k]) + rr) });
    }
  }
  const lo = [0, 1, 2].map(k => Math.min(...parts.map(p => p.lo[k])) - MG);
  const hiAll = [0, 1, 2].map(k => Math.max(...parts.map(p => p.hi[k])) + MG);
  const G = [0, 1, 2].map(k => Math.ceil((hiAll[k] - lo[k]) / CS));
  const cells = [], idx = [];
  for (let z = 0; z < G[2]; z++) for (let y = 0; y < G[1]; y++) for (let x = 0; x < G[0]; x++) {
    const cmin = [x, y, z].map((v, k) => lo[k] + v * CS - MG), cmax = [x, y, z].map((v, k) => lo[k] + (v + 1) * CS + MG);
    const list = new Set();
    for (const p of parts) {
      if (p.sphere) {
        const [c, rad] = p.sphere;
        const dd = Math.hypot(...[0, 1, 2].map(k => Math.max(cmin[k] - c[k], 0, c[k] - cmax[k])));
        if (dd <= rad) list.add(p.id);
      } else if ([0, 1, 2].every(k => p.lo[k] <= cmax[k] && p.hi[k] >= cmin[k])) list.add(p.id);
    }
    cells.push([idx.length, list.size, 0, 0]);
    idx.push(...list);
  }
  while (idx.length % 4) idx.push(0);
  const idxRows = [];
  for (let i = 0; i < idx.length; i += 4) idxRows.push(idx.slice(i, i + 4));
  const rows = [[NT, NT * NPL, 0, 0], ...twigs, ...leaves, [...lo, CS], [...G, MG], ...cells, ...idxRows];
  const grid = { cells: cells.length, dims: G, mean_list: r3(cells.reduce((s, c) => s + c[1], 0) / cells.length), max_list: Math.max(...cells.map(c => c[1])) };
  return { NT, NL: NT * NPL, data: new Float32Array(rows.flat()), grid };
}

// ---- scenes, views and variants ---------------------------------------------------------------------------

const SHRUB_C = [0.62, 0.40, 1.0], EYE_C0 = [0.0, 0.40, 0.25];
const HEAD_T = [0.0, 0.93, 0.63];
const WALL_P = [3.2 * Math.cos(-0.5236), 2.2, 3.2 * Math.sin(-0.5236)];

function along(target, dir, dist) { return { eye: add(target, scale(norm(dir), dist)), target }; }

const SCENES = {
  gallery: {
    file: 'scene_gallery.wgsl', sun: norm([-0.45, 0.27, 0.85]), sunE: [5.6, 3.5, 1.9], exposure: 1.0, wet: 1.0, treeline: 1,
    mats: { skin: B.SKIN, leaf: B.LEAF, eye: B.EYE, wet: B.WET, layer: B.LAYER, specaa: B.SPECAA },
    refs: ['skin', 'leaf', 'eye', 'wet', 'layer'],
    thick: { mat: 'leaf', n: [1, 2, 8, 16] },
    ss: true,
    views: {
      main: { eye: [0, 1.55, -4.2], target: [0.0, 0.32, 0.4] },
      slab: along([-0.75, 0.08, -0.25], [0.25, 0.35, -0.9], 1.4),
      ...Object.fromEntries([2.6, 1.4, 0.8, 0.48].map((d, i) => [`leaf${i}`, along(SHRUB_C, sub([0, 1.25, -4.4], SHRUB_C), d)])),
      ...Object.fromEntries([2.2, 1.0, 0.5, 0.25].map((d, i) => [`eye${i}`, along(EYE_C0, [0.35, 0.22, -1], d)])),
    },
  },
  head: {
    file: 'scene_head.wgsl', wolf: true, sun: norm([-0.75, 0.34, -0.56]), sunE: [5.6, 3.5, 1.9], exposure: 1.0, wet: 1.0, treeline: 1,
    mats: { skin: B.SKIN, eye: B.EYE, specaa: B.SPECAA },
    refs: ['skin', 'eye'],
    thick: { mat: 'skin', n: [1, 2, 8, 16] },
    views: {
      main: along(HEAD_T, [0.78, 0.10, 0.62], 0.62),
      ...Object.fromEntries([2.4, 1.2, 0.7, 0.42].map((d, i) => [`skin${i}`, along(HEAD_T, [0.78, 0.10, 0.62], d)])),
    },
  },
  tower: {
    file: 'scene_tower.wgsl', sun: norm([0.77, 0.38, 0.50]), sunE: [5.6, 3.5, 1.9], exposure: 1.0, wet: 1.0, treeline: 0.7,
    mats: { wet: B.WET, layer: B.LAYER, specaa: B.SPECAA },
    refs: ['wet', 'layer'],
    ss: true,
    views: {
      main: along(WALL_P, [0.996, -0.27, -0.087], 5.0),
      ...Object.fromEntries([16, 8, 4.5, 2.2].map((d, i) => [`wall${i}`, along(WALL_P, [0.996, -0.27, -0.087], d)])),
    },
  },
};

const allBits = sc => Object.values(sc.mats).reduce((a, b) => a | b, 0);

function variantsFor(sc) {
  const all = allBits(sc);
  const v = { plain: {} };
  for (const [k, bit] of Object.entries(sc.mats)) v[k] = { MATS: bit };
  for (const k of ['skin', 'leaf']) if (sc.mats[k]) v[`${k}_novis`] = { MATS: sc.mats[k], TR_VIS: 0 };
  v.all = { MATS: all };
  v.filt = { MATS: all & ~B.SPECAA };
  v.noaa = { MATS: all & ~B.SPECAA, FILTER: 0 };
  v.stats = { MATS: all, STATS: 1 };
  v.stats_plain = { STATS: 1 };
  for (const [k, bit] of Object.entries(sc.mats)) v[`stats_${k}`] = { MATS: bit, STATS: 1 };
  if (all & B.WET) v.dry = { MATS: all & ~B.WET };
  for (const k of sc.refs) v[`ref_${k}`] = { MATS: sc.mats[k], REF: sc.mats[k], STATS: 1 };
  v.ref_all = { MATS: all, REF: all & ~B.SPECAA, REF_VIS: 1, STATS: 1 };
  if (sc.thick) for (const n of sc.thick.n) v[`${sc.thick.mat}_n${n}`] = { MATS: sc.mats[sc.thick.mat], THICK_N: n };
  if (sc.ss) v.ss = { MATS: all & ~B.SPECAA, FILTER: 0, entry: 'shade_ss' };
  for (const x of (hashArgs.get('extra') || '').split(',').filter(Boolean)) {
    const [scn, vn, kv] = x.split(':');
    if (SCENES[scn] !== sc) continue;
    v[vn] = Object.fromEntries(kv.split(';').map(p => { const [k, val] = p.split('='); return [k, k === 'entry' ? val : Number(val)]; }));
  }
  return v;
}

// ---- setup ------------------------------------------------------------------------------------------------------

async function init() {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
  const want = ['timestamp-query'].filter(f => adapter.features.has(f));
  device = await adapter.requestDevice({ requiredFeatures: want, requiredLimits: { maxStorageBufferBindingSize: adapter.limits.maxStorageBufferBindingSize } });
  device.lost.then(i => console.error('DEVICE LOST:', i.message));
  device.addEventListener('uncapturederror', e => console.error('GPU ERROR:', e.error.message));
  const info = adapter.info || {};
  R.device = { vendor: info.vendor, architecture: info.architecture, description: info.description, userAgent: navigator.userAgent,
    timestampQuery: want.includes('timestamp-query'), pageVisibility: document.visibilityState };
  if (!R.device.timestampQuery) throw new Error('timestamp-query is required');
  for (const f of ['common', 'wolf', 'render', 'blit', 'scene_gallery', 'scene_head', 'scene_tower']) src[f] = await (await fetch(`${f}.wgsl`)).text();

  const canvas = $('view');
  canvas.width = W;
  canvas.height = H;
  ctx = canvas.getContext('webgpu');
  canvasFormat = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format: canvasFormat, alphaMode: 'opaque' });

  const C = GPUShaderStage.COMPUTE;
  const e = (binding, type) => ({ binding, visibility: C, buffer: { type } });
  L.main = device.createBindGroupLayout({
    entries: [e(0, 'uniform'), e(1, 'storage'), { binding: 2, visibility: C, storageTexture: { access: 'write-only', format: 'rgba16float' } },
      e(3, 'storage'), e(4, 'read-only-storage')],
  });
  L.blit = device.createBindGroupLayout({
    entries: [{ binding: 0, visibility: GPUShaderStage.FRAGMENT, texture: {} }, { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: {} },
      { binding: 2, visibility: GPUShaderStage.FRAGMENT, buffer: { type: 'uniform' } }],
  });

  T.out = device.createTexture({ size: [W, H], format: 'rgba16float', usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_SRC });
  Bf.frame = buffer(128, U.UNIFORM | U.COPY_DST);
  Bf.vbuf = buffer(W * H * 16, U.STORAGE | U.COPY_SRC);
  Bf.stats = buffer(64 * 4, U.STORAGE | U.COPY_SRC | U.COPY_DST);
  Bf.blitScale = buffer(16, U.UNIFORM | U.COPY_DST);
  const shrub = shrubData();
  SCENES.gallery.shrub = shrub;
  Bf.sd = { gallery: buffer(shrub.data.byteLength, U.STORAGE, shrub.data), none: buffer(16, U.STORAGE) };
  const res = b => ({ resource: { buffer: b } });
  Bf.bg = {};
  for (const name of Object.keys(SCENES)) {
    Bf.bg[name] = device.createBindGroup({
      layout: L.main,
      entries: [{ binding: 0, ...res(Bf.frame) }, { binding: 1, ...res(Bf.vbuf) }, { binding: 2, resource: T.out.createView() },
        { binding: 3, ...res(Bf.stats) }, { binding: 4, ...res(name === 'gallery' ? Bf.sd.gallery : Bf.sd.none) }],
    });
  }
  Bf.blitBG = device.createBindGroup({
    layout: L.blit,
    entries: [{ binding: 0, resource: T.out.createView() }, { binding: 1, resource: device.createSampler({ magFilter: 'linear', minFilter: 'linear' }) },
      { binding: 2, ...res(Bf.blitScale) }],
  });
  const bm = device.createShaderModule({ code: src.blit, label: 'blit' });
  P.blit = await device.createRenderPipelineAsync({
    layout: device.createPipelineLayout({ bindGroupLayouts: [L.blit] }),
    vertex: { module: bm, entryPoint: 'blit_vs' },
    fragment: { module: bm, entryPoint: 'blit_fs', targets: [{ format: canvasFormat }] },
  });
  R.memory_mb = { vbuf: r3(W * H * 16 / 2 ** 20), frame_rgba16f: r3(W * H * 8 / 2 ** 20), shrub_data: r3(shrub.data.byteLength / 2 ** 20) };
  R.shrub_grid = shrub.grid;
  log('shrub grid', JSON.stringify(shrub.grid));
}

function moduleCode(name) {
  const sc = SCENES[name];
  let pre = '';
  if (name === 'gallery') pre = `const NTWIG: u32 = ${sc.shrub.NT}u;\nconst NLEAF: u32 = ${sc.shrub.NL}u;\n`;
  return src.common + '\n' + pre + (sc.wolf ? src.wolf + '\n' : '') + src[sc.file.replace('.wgsl', '')] + '\n' + src.render;
}

async function checkModule(module, label, code) {
  const info = await module.getCompilationInfo();
  const lines = code.split('\n');
  for (const m of info.messages) {
    const msg = `${label} ${m.type} line ${m.lineNum}:${m.linePos} ${m.message} | ${(lines[m.lineNum - 1] || '').trim()}`;
    if (m.type === 'error') console.error(msg); else console.log(msg);
  }
  return !info.messages.some(m => m.type === 'error');
}

/** Every pipeline a scene needs, created in parallel; returns the wall time. */
async function buildPipelines(name, only) {
  const sc = SCENES[name];
  const code = moduleCode(name);
  const t0 = performance.now();
  const module = device.createShaderModule({ code, label: name });
  if (!(await checkModule(module, name, code))) throw new Error(`${name}: WGSL errors`);
  const layout = device.createPipelineLayout({ bindGroupLayouts: [L.main] });
  sc.variants = variantsFor(sc);
  const jobs = [
    ['vis', () => device.createComputePipelineAsync({ layout, compute: { module, entryPoint: 'visibility', constants: { REF_VIS: 0 } } })],
  ];
  if (!only || only.includes('ref_all')) {
    jobs.push(['vis_ref', () => device.createComputePipelineAsync({ layout, compute: { module, entryPoint: 'visibility', constants: { REF_VIS: 1 } } })]);
  }
  for (const [v, c] of Object.entries(sc.variants)) {
    if (only && !only.includes(v)) continue;
    const { entry = 'shade', ...constants } = c;
    jobs.push([v, () => device.createComputePipelineAsync({ layout, compute: { module, entryPoint: entry, constants } })]);
  }
  sc.P = {};
  for (let i = 0; i < jobs.length; i += COMPILE_BATCH) {
    const batch = jobs.slice(i, i + COMPILE_BATCH);
    const ps = await Promise.all(batch.map(([n, f]) => f().catch(e => { console.error(`pipeline ${name}/${n}:`, e.message); throw e; })));
    batch.forEach(([n], k) => { sc.P[n] = ps[k]; });
  }
  const ms = r3(performance.now() - t0);
  log(`${name}: ${jobs.length} pipelines in ${ms} ms (cold, ${COMPILE_BATCH} at a time)`);
  return { count: jobs.length, ms };
}

// ---- frames -----------------------------------------------------------------------------------------------------

function frameData(sc, view, res, o = {}) {
  const [w, h] = RES[res];
  const off = o.offset || [0, 0, 0];
  const eye = add(view.eye, off), target = add(view.target, off);
  const f = norm(sub(target, eye));
  const r = norm(cross(f, [0, 1, 0]));
  const u = cross(r, f);
  const ty = Math.tan(FOVY / 2), tx = ty * w / h;
  const ab = new ArrayBuffer(128), fl = new Float32Array(ab), fu = new Uint32Array(ab);
  fl.set([...eye, 2 * ty / h], 0);
  fl.set([...f, 0], 4);
  fl.set([...r.map(x => x * tx), 0], 8);
  fl.set([...u.map(x => x * ty), 0], 12);
  fl.set([...sc.sun, 0], 16);
  fl.set([...sc.sunE, sc.exposure], 20);
  fu.set([w, h, o.row0 || 0, o.rowEnd ?? h], 24);
  fl.set([o.fp || 1, o.seed || 0, sc.wet, sc.treeline || 0], 28);
  return { ab, right: r, pxa: 2 * ty / h };
}

function setFrame(sc, view, res, o) { device.queue.writeBuffer(Bf.frame, 0, frameData(sc, view, res, o).ab); }

function encodeFrame(enc, sc, name, shade, vis, res, qs, q0, rows) {
  const [w, h] = RES[res];
  const ts = i => qs ? { timestampWrites: { querySet: qs, beginningOfPassWriteIndex: q0 + 2 * i, endOfPassWriteIndex: q0 + 2 * i + 1 } } : {};
  const gx = Math.ceil(w / 8), gy = Math.ceil((rows ?? h) / 8);
  if (vis) {
    const p = enc.beginComputePass(ts(0));
    p.setPipeline(sc.P[vis]);
    p.setBindGroup(0, Bf.bg[name]);
    p.dispatchWorkgroups(gx, gy);
    p.end();
  }
  const p = enc.beginComputePass(ts(1));
  p.setPipeline(sc.P[shade]);
  p.setBindGroup(0, Bf.bg[name]);
  p.dispatchWorkgroups(gx, gy);
  p.end();
}

const visFor = v => v === 'ss' ? null : v.startsWith('ref_all') ? 'vis_ref' : 'vis';
const heavy = v => v.startsWith('ref_') || v === 'ss';

/** Renders one frame. Heavy variants go in tiles of rows, one submission each, so no dispatch runs long. */
async function render(name, view, variant, res, o = {}) {
  const sc = SCENES[name];
  const [, h] = RES[res];
  if (sc.variants[variant]?.STATS) device.queue.writeBuffer(Bf.stats, 0, new Uint32Array(64));
  if (!heavy(variant)) {
    setFrame(sc, view, res, o);
    const t = performance.now();
    submitFrame(sc, name, variant, res, null, 0);
    await device.queue.onSubmittedWorkDone();
    const ms = performance.now() - t;
    R.max_frame_ms = Math.max(R.max_frame_ms || 0, r3(ms));
    if (ms > 200) console.error(`frame (two submissions) over 200 ms: ${name}/${variant}: ${r3(ms)} ms`);
    return;
  }
  let tile = TILE_START;
  for (let r0 = 0; r0 < h;) {
    const rows = Math.min(tile, h - r0);
    setFrame(sc, view, res, { ...o, row0: r0, rowEnd: r0 + rows });
    const enc = device.createCommandEncoder();
    encodeFrame(enc, sc, name, variant, visFor(variant), res, null, 0, rows);
    const t = performance.now();
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
    const ms = performance.now() - t;
    R.max_tile_ms = Math.max(R.max_tile_ms || 0, r3(ms));
    (R.tile_ms ||= {})[`${name}/${variant}`] = Math.max(R.tile_ms[`${name}/${variant}`] || 0, r3(ms));
    if (ms > 100) console.error(`tile over 100 ms: ${name}/${variant} rows ${r0}+${rows}: ${r3(ms)} ms`);
    r0 += rows;
    if (ms < TILE_MS * 0.5) tile = Math.min(tile * 2, TILE_MAX);
    else if (ms > TILE_MS * 1.3) tile = Math.max(Math.floor(tile / 2), TILE_MIN);
  }
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

// GPU safety: each pass goes in row bands, one submission each, sized so no submission runs long even
// in the heaviest views (calibrateBands); timed pass times are the sum of their bands' timestamps.
const BAND_MS = 35, CAL_BANDS = 8;

/** One pass over the frame in `nb` row bands, one submission each; timestamps at q0 + 2·band. */
function submitPass(name, pipe, res, qs, q0, nb) {
  const sc = SCENES[name];
  const [w, h] = RES[res];
  const rows = Math.ceil(h / nb / 8) * 8;
  for (let b = 0; b < nb; b++) {
    const r0 = b * rows, r1 = Math.min(h, r0 + rows);
    if (r0 >= h) break;
    device.queue.writeBuffer(Bf.frame, 104, new Uint32Array([r0, r1]));
    const enc = device.createCommandEncoder();
    const p = enc.beginComputePass(qs ? { timestampWrites: { querySet: qs, beginningOfPassWriteIndex: q0 + 2 * b, endOfPassWriteIndex: q0 + 2 * b + 1 } } : {});
    p.setPipeline(sc.P[pipe]);
    p.setBindGroup(0, Bf.bg[name]);
    p.dispatchWorkgroups(Math.ceil(w / 8), Math.ceil((r1 - r0) / 8));
    p.end();
    device.queue.submit([enc.finish()]);
  }
}

/** How many bands a pass needs: run it once in CAL_BANDS bands, one at a time, and size bands so the
 *  heaviest would take about BAND_MS. (Assumes the frame uniform is already set for the view.) */
async function calibrateBands(name, pipes, res) {
  let worst = 0;
  const [, h] = RES[res];
  for (const pipe of pipes) {
    const rows = Math.ceil(h / CAL_BANDS / 8) * 8;
    for (let b = 0; b < CAL_BANDS; b++) {
      const r0 = b * rows;
      if (r0 >= h) break;
      device.queue.writeBuffer(Bf.frame, 104, new Uint32Array([r0, Math.min(h, r0 + rows)]));
      const enc = device.createCommandEncoder();
      const p = enc.beginComputePass();
      p.setPipeline(SCENES[name].P[pipe]);
      p.setBindGroup(0, Bf.bg[name]);
      p.dispatchWorkgroups(Math.ceil(RES[res][0] / 8), Math.ceil(rows / 8));
      p.end();
      const t = performance.now();
      device.queue.submit([enc.finish()]);
      await device.queue.onSubmittedWorkDone();
      worst = Math.max(worst, performance.now() - t);
    }
  }
  const nb = Math.min(CAL_BANDS * 2, Math.max(1, Math.ceil(worst * CAL_BANDS / BAND_MS)));
  R.bands = R.bands || {};
  R.bands[`${name}/${pipes.join('+')}/${res}`] = { worst_of_8_ms: r3(worst), bands: nb };
  return nb;
}

/** One frame: visibility, then shading, each in `nb` bands. Timestamps: vis at q0.., shade at q0 + 2·nb.. */
function submitFrame(sc, name, variant, res, qs, q0, nb = CAL_BANDS) {
  submitPass(name, 'vis', res, qs, q0, nb);
  submitPass(name, variant, res, qs, q0 + 2 * nb, nb);
}

function submitShade(sc, name, variant, res, qs, q0, nb) { submitPass(name, variant, res, qs, q0, nb); }

/** Sum of a pass's band durations (ms) for frame f, from timestamps laid out q per frame. */
function passMs(ts, f, q, first, nb) {
  let s = 0;
  for (let b = 0; b < nb; b++) {
    const d = Number(ts[f * q + first + 2 * b + 1] - ts[f * q + first + 2 * b]) / 1e6;
    if (d > (R.max_submission_ms || 0)) R.max_submission_ms = r3(d);   // longest single timed submission (GPU safety)
    s += d;
  }
  return s;
}

/** Shading-pass time for several configurations of one view, interleaved so slow drift (clocks,
 *  heat: the reference device is fanless) cancels in their differences: WARM untimed frames across
 *  them, then ROUNDS rounds of FRAMES/ROUNDS back-to-back frames each. `cfgs`: [{ key, variant, o }].
 *  Returns { key: { shade, shade_p95 } } over FRAMES frames per configuration. */
const ROUNDS = 3;
async function measureShadeSet(name, view, cfgs, res) {
  const sc = SCENES[name];
  for (const c of cfgs) if (heavy(c.variant)) throw new Error(`measureShadeSet(${c.variant}): references aren't timed`);
  setFrame(sc, view, res, cfgs[0].o || {});
  submitFrame(sc, name, cfgs[0].variant, res, null, 0);
  await device.queue.onSubmittedWorkDone();
  const nb = await calibrateBands(name, [...new Set(cfgs.map(c => c.variant))], res);
  const per = Math.ceil(WARM / cfgs.length);
  for (const c of cfgs) {
    setFrame(sc, view, res, c.o || {});
    for (let f = 0; f < per; f++) submitShade(sc, name, c.variant, res, null, 0, nb);
  }
  await device.queue.onSubmittedWorkDone();
  const n = FRAMES / ROUNDS, q = 2 * nb;
  const samples = Object.fromEntries(cfgs.map(c => [c.key, []]));
  for (let r = 0; r < ROUNDS; r++) {
    for (const c of cfgs) {
      setFrame(sc, view, res, c.o || {});
      const qs = device.createQuerySet({ type: 'timestamp', count: n * q });
      for (let f = 0; f < n; f++) submitShade(sc, name, c.variant, res, qs, q * f, nb);
      await device.queue.onSubmittedWorkDone();
      const ts = await resolveTimestamps(qs, n * q);
      for (let f = 0; f < n; f++) samples[c.key].push(passMs(ts, f, q, 0, nb));
    }
  }
  return Object.fromEntries(cfgs.map(c => [c.key, { shade: r3(median(samples[c.key])), shade_p95: r3(pct(samples[c.key], 0.95)) }]));
}

/** GPU time per pass under sustained load: WARM untimed frames, then FRAMES back to back. */
async function measure(name, view, variant, res, o = {}) {
  if (heavy(variant)) throw new Error(`measure(${variant}): references aren't timed (and would break the 100 ms rule)`);
  const sc = SCENES[name];
  setFrame(sc, view, res, o);
  const nb = await calibrateBands(name, ['vis', variant], res), Q = 4 * nb;
  const qs = device.createQuerySet({ type: 'timestamp', count: FRAMES * Q });
  for (let f = 0; f < WARM; f++) submitFrame(sc, name, variant, res, null, 0, nb);
  await device.queue.onSubmittedWorkDone();
  const t1 = performance.now();
  for (let f = 0; f < FRAMES; f++) submitFrame(sc, name, variant, res, qs, f * Q, nb);
  await device.queue.onSubmittedWorkDone();
  const throughput = (performance.now() - t1) / FRAMES;
  const ts = await resolveTimestamps(qs, FRAMES * Q);
  const all = [...Array(FRAMES).keys()];
  const vis = all.map(f => passMs(ts, f, Q, 0, nb)), shade = all.map(f => passMs(ts, f, Q, 2 * nb, nb));
  const frame = all.map(f => Number(ts[f * Q + Q - 1] - ts[f * Q]) / 1e6);
  return { vis: r3(median(vis)), shade: r3(median(shade)), frame: r3(median(frame)), shade_p95: r3(pct(shade, 0.95)),
    frame_p95: r3(pct(frame, 0.95)), throughput_ms: r3(throughput) };
}

/** The 60 fps check: frames submitted at 60 Hz by busy-wait (rAF and timers are throttled headless). */
async function paced(name, view, variant, frames = 120) {
  const sc = SCENES[name];
  setFrame(sc, view, 'full');
  const nb = await calibrateBands(name, ['vis', variant], 'full'), Q = 4 * nb, period = 1000 / 60;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  const t0 = performance.now() + 20;
  for (let f = 0; f < frames; f++) {
    const deadline = t0 + f * period;
    while (performance.now() < deadline) { /* spin */ }
    submitFrame(sc, name, variant, 'full', qs, f * Q, nb);
  }
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * Q);
  const keep = [...Array(frames).keys()].slice(30);
  const gpu = keep.map(f => Number(ts[f * Q + Q - 1] - ts[f * Q]) / 1e6);
  return { pacing: 'busy-wait at 60 Hz', frames: keep.length, frame_gpu_median: r3(median(gpu)), frame_gpu_p95: r3(pct(gpu, 0.95)),
    frame_gpu_max: r3(Math.max(...gpu)), frame_gpu_over_16_7: gpu.filter(g => g > 16.7).length };
}

// ---- readback, comparison, screenshots -----------------------------------------------------------------------

const BYTE = new Uint8Array(65536);
for (let h = 0; h < 65536; h++) {
  const s = h >> 15, e = (h >> 10) & 31, m = h & 1023;
  let v = e === 0 ? m * 2 ** -24 : e === 31 ? 1 : (1 + m / 1024) * 2 ** (e - 15);
  if (s) v = 0;
  BYTE[h] = Math.round(255 * Math.pow(Math.min(Math.max(v, 0), 1), 1 / 2.2));
}

/** The current frame as 8-bit display values (RGB). */
async function grab(res) {
  const [w, h] = RES[res];
  const bpr = Math.ceil(w * 8 / 256) * 256;
  const out = device.createBuffer({ size: bpr * h, usage: U.COPY_DST | U.MAP_READ });
  const enc = device.createCommandEncoder();
  enc.copyTextureToBuffer({ texture: T.out }, { buffer: out, bytesPerRow: bpr, rowsPerImage: h }, [w, h]);
  device.queue.submit([enc.finish()]);
  await out.mapAsync(GPUMapMode.READ);
  const half = new Uint16Array(out.getMappedRange().slice(0));
  out.unmap();
  out.destroy();
  const px = new Uint8Array(w * h * 3);
  const rowh = bpr / 2;
  for (let y = 0, j = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = y * rowh + x * 4;
      px[j++] = BYTE[half[i]]; px[j++] = BYTE[half[i + 1]]; px[j++] = BYTE[half[i + 2]];
    }
  }
  return px;
}

/** Material ids and primary step-cap flags from the visibility buffer. */
async function readVis(res) {
  const [w, h] = RES[res];
  const f = new Float32Array(await readback(Bf.vbuf, w * h * 16));
  const ids = new Uint8Array(w * h), caps = new Uint8Array(w * h);
  for (let i = 0; i < w * h; i++) { ids[i] = f[i * 4 + 1]; caps[i] = f[i * 4 + 3] > 0 ? 1 : 0; }
  return { ids, caps };
}

async function readStats(res) {
  const s = new Uint32Array(await readback(Bf.stats, 256));
  const [w, h] = RES[res];
  const o = { px: {} };
  ID_NAMES.forEach((n, i) => { o.px[n] = s[i]; });
  for (const [k, i] of Object.entries(STAT)) o[k] = s[i];
  o.share = Object.fromEntries(ID_NAMES.map((n, i) => [n, r4(s[i] / (w * h))]));
  return o;
}

function diff(a, b, ids, idSet) {
  let sum = 0, over8 = 0, n = 0, max = 0, signed = 0;
  for (let p = 0; p < a.length / 3; p++) {
    if (idSet && !idSet.has(ids[p])) continue;
    let m = 0;
    for (let c = 0; c < 3; c++) { const d = Math.abs(a[3 * p + c] - b[3 * p + c]); signed += a[3 * p + c] - b[3 * p + c]; sum += d; if (d > m) m = d; }
    if (m > 8) over8++;
    if (m > max) max = m;
    n++;
  }
  return { mean_abs_255: r3(sum / Math.max(3 * n, 1)), mean_signed_255: r3(signed / Math.max(3 * n, 1)), share_over_8: r4(over8 / Math.max(n, 1)), pixels: n, max };
}

function capShare(caps, ids, idSet) {
  let n = 0, c = 0;
  for (let i = 0; i < ids.length; i++) if (!idSet || idSet.has(ids[i])) { n++; c += caps[i]; }
  return { caps: c, share: r4(c / Math.max(n, 1)) };
}

async function present(res) {
  const [w, h] = RES[res];
  device.queue.writeBuffer(Bf.blitScale, 0, new Float32Array([w / W, h / H, 0, 0]));
  const enc = device.createCommandEncoder();
  const p = enc.beginRenderPass({ colorAttachments: [{ view: ctx.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
  p.setPipeline(P.blit);
  p.setBindGroup(0, Bf.blitBG);
  p.draw(3);
  p.end();
  device.queue.submit([enc.finish()]);
  await device.queue.onSubmittedWorkDone();
}

async function snapshot(file, res = 'full') {
  await present(res);
  const blob = await new Promise(r => $('view').toBlob(r, 'image/png'));
  if (blob) await save(`${file}.png`, blob);
}

/** Saves an image of where a and b differ: red > 8/255, green > 2/255, over a dimmed copy of a. */
async function diffImage(file, a, b, res) {
  const [w, h] = RES[res];
  const c = document.createElement('canvas');
  c.width = w; c.height = h;
  const g = c.getContext('2d'), img = g.createImageData(w, h);
  for (let i = 0, j = 0; i < a.length; i += 3, j += 4) {
    let m = 0;
    for (let k = 0; k < 3; k++) m = Math.max(m, Math.abs(a[i + k] - b[i + k]));
    const base = (a[i] + a[i + 1] + a[i + 2]) / 9;
    img.data[j] = m > 8 ? 255 : base; img.data[j + 1] = m > 8 ? 0 : (m > 2 ? 200 : base); img.data[j + 2] = base; img.data[j + 3] = 255;
  }
  g.putImageData(img, 0, 0);
  const blob = await new Promise(r => c.toBlob(r, 'image/png'));
  await save(`${file}.png`, blob);
}

// ---- the experiments ----------------------------------------------------------------------------------------------

const MAT_IDS = {
  skin: new Set([ID.HIDE]), leaf: new Set([ID.LEAF]), eye: new Set([ID.EYE]),
  wet: { gallery: new Set([ID.SLAB]), tower: new Set([ID.GROUND, ID.STONE, ID.MORTAR]) },
  layer: new Set([ID.STONE, ID.MORTAR]),
};
const idsOf = (mat, scene) => MAT_IDS[mat] instanceof Set ? MAT_IDS[mat] : MAT_IDS[mat][scene];
const NONSKY = new Set([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

// Timed at each scene's main view (each material's verdict comes from its own coverage sweep).
const TIMED = { gallery: ['plain', 'all'], head: ['plain', 'skin', 'skin_novis', 'eye', 'all'], tower: ['plain', 'wet', 'layer', 'specaa', 'all'] };
const TIMED_HALF = ['plain', 'all'];
const MAT_PIXELS = { skin: [ID.HIDE], leaf: [ID.LEAF], eye: [ID.EYE], wet: { gallery: [ID.SLAB], head: [ID.NOSE], tower: [ID.GROUND, ID.STONE, ID.MORTAR] },
  layer: [ID.STONE, ID.MORTAR], specaa: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] };

async function sceneLevel(name) {
  const sc = SCENES[name], view = sc.views.main;
  R.scenes[name] = { eye: view.eye, target: view.target, sun: sc.sun };
  for (const res of ['full', 'half']) {
    await render(name, view, 'stats', res);
    R.stats[`${name}/main/${res}`] = await readStats(res);
    const timed = res === 'full' ? TIMED[name] : TIMED_HALF;
    const set = await measureShadeSet(name, view, timed.map(v => ({ key: v, variant: v })), res);
    for (const v of timed) R.timing[`${name}/main/${v}/${res}`] = set[v];
    // a whole frame (visibility + shading) with every material on
    R.timing[`${name}/main/frame_all/${res}`] = await measure(name, view, 'all', res);
    const t = v => R.timing[`${name}/main/${v}/${res}`];
    R.scenes[name][`extra_ms_${res}`] = Object.fromEntries(timed.filter(v => v !== 'plain').map(v => [v, r3(t(v).shade - t('plain').shade)]));
    if (res === 'full') {
      // Extra field evaluations per material pixel, and what one costs (ns per evaluation, in this pass).
      await render(name, view, 'stats_plain', res);
      const base = await readStats(res);
      const ev = R.scenes[name].evals = {};
      for (const k of Object.keys(sc.mats)) {
        await render(name, view, `stats_${k}`, res);
        const st = await readStats(res);
        const ids = Array.isArray(MAT_PIXELS[k]) ? MAT_PIXELS[k] : MAT_PIXELS[k][name];
        const px = ids.reduce((s, id) => s + st.px[ID_NAMES[id]], 0);
        const evals = st.extra_evals + st.th_steps + (st.sh_steps - base.sh_steps);
        const ms = R.scenes[name].extra_ms_full[k] ?? null;
        ev[k] = { material_px: px, coverage: r4(px / (1920 * 1080)), backlit_px: st.backlit_px, extra_evals: evals,
          per_material_px: r3(evals / Math.max(px, 1)), curvature_occlusion: st.extra_evals, inward_steps: st.th_steps,
          extra_shadow_steps: st.sh_steps - base.sh_steps, transmission_shadow_rays: st.tr_shadow, inward_unresolved: st.th_unresolved,
          extra_ms: ms, ns_per_extra_eval: evals > 0 && ms != null ? r3(ms * 1e6 / evals) : null };
      }
      log(`${name} extra evaluations:`, JSON.stringify(ev));
    }
    log(`${name} ${res}: vis ${t('frame_all').vis} ms, shade plain ${t('plain').shade}, all ${t('all').shade}; extra`, JSON.stringify(R.scenes[name][`extra_ms_${res}`]));
  }
  // Screenshots and correctness at 1080p.
  const Q = R.quality[name] = {};
  if (sc.variants.dry) { await render(name, view, 'dry', 'full'); await snapshot(`${name}-dry`); }
  await render(name, view, 'plain', 'full');
  await snapshot(`${name}-plain`);
  const { ids, caps } = await readVis('full');
  Q.primary_caps = capShare(caps, ids, NONSKY);
  for (const m of sc.refs) {
    const set = idsOf(m, name);
    await render(name, view, m, 'full');
    const fast = await grab('full');
    if (m === sc.refs[0]) await snapshot(`${name}-${m}`);
    await render(name, view, `ref_${m}`, 'full');
    const ref = await grab('full');
    const st = await readStats('full');
    Q[m] = { own: diff(fast, ref, ids, set), frame: diff(fast, ref), caps: capShare(caps, ids, set), ref_caps: st.ref_caps };
    if (name === 'head' && m === 'skin') { await snapshot('head-skin-ref'); await diffImage('head-skin-diff', fast, ref, 'full'); }
    log(`${name} ${m} vs reference:`, JSON.stringify(Q[m]));
  }
  for (const res of ['full', 'half']) {
    await render(name, view, 'all', res);
    const fast = await grab(res);
    await snapshot(`${name}-all${res === 'half' ? '-540' : ''}`, res);
    await render(name, view, 'ref_all', res);
    const ref = await grab(res);
    const st = await readStats(res);
    if (res === 'full') await snapshot(`${name}-ref`);
    Q[`all_${res}`] = { frame: diff(fast, ref), ref_stats: { caps: st.caps, ref_caps: st.ref_caps, sh_caps: st.sh_caps, ground_caps: st.ground_caps } };
    log(`${name} all vs full reference (${res}):`, JSON.stringify(Q[`all_${res}`]));
  }
}

/** Interpolates y at x from points sorted by x; extrapolates from the last two if needed. */
function interp(pts, x) {
  const s = [...pts].sort((a, b) => a[0] - b[0]);
  let i = s.findIndex(p => p[0] >= x);
  if (i === -1) i = s.length - 1;
  if (i === 0) i = 1;
  const [x0, y0] = s[i - 1], [x1, y1] = s[i];
  return { value: r3(y0 + (y1 - y0) * (x - x0) / Math.max(x1 - x0, 1e-9)), extrapolated: x > s[s.length - 1][0] || x < s[0][0] };
}

const COVER = {
  skin: { scene: 'head', views: ['skin0', 'skin1', 'skin2', 'skin3'], variants: ['skin'], ids: new Set([ID.HIDE]) },
  leaf: { scene: 'gallery', views: ['leaf0', 'leaf1', 'leaf2', 'leaf3'], variants: ['leaf'], ids: new Set([ID.LEAF]) },
  eye: { scene: 'gallery', views: ['eye1', 'eye2', 'eye3'], variants: ['eye'], ids: new Set([ID.EYE]) },
  world: { scene: 'tower', views: ['wall0', 'wall1', 'wall2', 'wall3'], variants: ['wet', 'layer', 'specaa'], ids: NONSKY,
    idsFor: { wet: new Set([ID.GROUND, ID.STONE, ID.MORTAR]), layer: new Set([ID.STONE, ID.MORTAR]), specaa: NONSKY } },
};

async function coverageSweeps() {
  for (const [key, cv] of Object.entries(COVER)) {
    const sc = SCENES[cv.scene];
    const rows = [];
    for (const vn of cv.views) {
      const view = sc.views[vn];
      const row = { view: vn, eye: view.eye.map(r3) };
      for (const res of ['full']) {   // 960×540 is measured per scene, not per coverage point (run time)
        await render(cv.scene, view, 'stats', res);
        const st = await readStats(res);
        const [w, h] = RES[res];
        const cov = set => { let px = 0; for (const id of set) px += st.px[ID_NAMES[id]]; return r4(px / (w * h)); };
        row[`coverage_${res}`] = cov(cv.ids);
        for (const v of cv.variants) row[`coverage_${v}_${res}`] = cov(cv.idsFor?.[v] || cv.ids);
        row[`backlit_${res}`] = r4(st.backlit_px / (w * h));
        row[`stats_${res}`] = { th_march: st.th_march, th_steps: st.th_steps, th_unresolved: st.th_unresolved, tr_shadow: st.tr_shadow, extra_evals: st.extra_evals };
        const set = await measureShadeSet(cv.scene, view, ['plain', ...cv.variants].map(v => ({ key: v, variant: v })), res);
        row[`plain_${res}`] = set.plain;
        for (const v of cv.variants) {
          row[`${v}_${res}`] = set[v];
          row[`extra_${v}_${res}`] = r3(set[v].shade - set.plain.shade);
        }
      }
      rows.push(row);
      log(`coverage ${key} ${vn}: ${row.coverage_full} of the screen; extra`, cv.variants.map(v => `${v} ${row[`extra_${v}_full`]} ms`).join(', '));
      if (vn === cv.views[2] || vn === cv.views[3]) {
        await render(cv.scene, view, 'all', 'full');
        await snapshot(`cover-${vn}-all`);
        if (key !== 'world') { await render(cv.scene, view, 'plain', 'full'); await snapshot(`cover-${vn}-plain`); }
      }
    }
    R.coverage[key] = rows;
  }
}

async function eyeCloseupQuality() {
  const view = SCENES.gallery.views.eye3;
  await render('gallery', view, 'eye', 'full');
  const { ids } = await readVis('full');
  const fast = await grab('full');
  await render('gallery', view, 'ref_eye', 'full');
  const ref = await grab('full');
  await snapshot('cover-eye3-ref');
  R.quality.gallery.eye_closeup = { own: diff(fast, ref, ids, new Set([ID.EYE])), frame: diff(fast, ref) };
  log('eye close-up vs reference:', JSON.stringify(R.quality.gallery.eye_closeup));
}

async function thicknessSweeps() {
  const jobs = [['head', 'main', 'skin', new Set([ID.HIDE])], ['gallery', 'leaf2', 'leaf', new Set([ID.LEAF])]];
  for (const [name, vn, mat, set] of jobs) {
    const sc = SCENES[name], view = sc.views[vn];
    await render(name, view, `ref_${mat}`, 'full');
    const ref = await grab('full');
    const { ids } = await readVis('full');
    if (name === 'gallery') { await snapshot('cover-leaf2-ref'); }
    const ns = name === 'head' ? [1, 2, 4, 8, 16] : [1, 2, 4, 8];   // leaves: N=16 is checked for correctness only (run time)
    const vn_ = n => n === 4 ? mat : `${mat}_n${n}`;
    const tset = await measureShadeSet(name, view, [{ key: 'plain', variant: 'plain' }, { key: 'novis', variant: `${mat}_novis` },
      ...ns.map(n => ({ key: `n${n}`, variant: vn_(n) }))], 'full');
    const plain = tset.plain, novis = tset.novis;
    const rows = [];
    for (const n of ns) {
      const v = vn_(n);
      const m = tset[`n${n}`];
      await render(name, view, v, 'full');
      const img = await grab('full');
      if (name === 'gallery' && n === 4) await diffImage('cover-leaf2-diff', img, ref, 'full');
      rows.push({ n, shade: m.shade, extra_ms: r3(m.shade - plain.shade), own: diff(img, ref, ids, set) });
      log(`thickness ${mat} N=${n}: extra ${r3(m.shade - plain.shade)} ms, vs reference`, JSON.stringify(rows.at(-1).own));
    }
    if (!ns.includes(16)) {
      await render(name, view, `${mat}_n16`, 'full');
      rows.push({ n: 16, shade: null, extra_ms: null, own: diff(await grab('full'), ref, ids, set) });
    }
    R.thickness[`${name}/${vn}/${mat}`] = { plain_shade: plain.shade, rows, n4_without_transmission_shadow_extra_ms: r3(novis.shade - plain.shade) };
  }
}

/** Shimmer: slow sideways camera motion (~¼ pixel per frame at the target), over non-sky pixels.
 *  - mean frame-to-frame change (the README's first metric; motion alone changes the image too);
 *  - flicker: mean |I_t − (I_{t−1} + I_{t+1})/2|, which cancels change that's linear in time, as
 *    slow motion of a stable image is, and keeps sparkle that pops on and off;
 *  - accuracy at frame 0 against the supersampled reference, next to that reference's own noise
 *    (two renders with different jitter). */
async function shimmerTest(name, frames = 6, refFrames = 3) {
  const sc = SCENES[name], view = sc.views.main;
  const fd = frameData(sc, view, 'full');
  const dist = Math.hypot(...sub(view.target, view.eye));
  const step = scale(fd.right, 0.25 * dist * fd.pxa);
  await render(name, view, 'all', 'full');
  const { ids } = await readVis('full');
  const set = NONSKY;
  const flick = (a, b, c) => {
    let s = 0, n = 0;
    for (let p = 0; p < ids.length; p++) {
      if (!set.has(ids[p])) continue;
      for (let k = 0; k < 3; k++) { const i = 3 * p + k; s += Math.abs(b[i] - 0.5 * (a[i] + c[i])); }
      n += 3;
    }
    return s / Math.max(n, 1);
  };
  const seq = async (variant, fp, n, file) => {
    const imgs = [];
    let acc = 0, fl = 0;
    for (let f = 0; f < n; f++) {
      await render(name, view, variant, 'full', { offset: scale(step, f), fp });
      imgs.push(await grab('full'));
      if (f === 0 && file) await snapshot(file);
      if (f > 0) acc += diff(imgs[f], imgs[f - 1], ids, set).mean_abs_255;
      if (f > 1) { fl += flick(imgs[f - 2], imgs[f - 1], imgs[f]); imgs[f - 2] = null; }
    }
    return { mean_frame_diff_255: r4(acc / (n - 1)), flicker_255: r4(fl / (n - 2)), first: imgs[0] || null, firstImg: null };
  };
  // frame 0 of the reference is kept separately (seq drops old frames to save memory)
  await render(name, view, 'ss', 'full', { seed: 1 });
  const ref0b = await grab('full');
  const ref = await seq('ss', 1, refFrames, `${name}-aa-ref`);
  await render(name, view, 'ss', 'full', { seed: 0 });
  const ref0 = await grab('full');
  const out = { step_m: r4(Math.hypot(...step)), frames, ref_frames: refFrames, ref_frame_diff_255: ref.mean_frame_diff_255,
    ref_flicker_255: ref.flicker_255, ref_noise_floor: diff(ref0, ref0b, ids, set), variants: {} };
  const list = [['noaa', 'noaa', 1], ['filt', 'filt', 1], ['aa', 'all', 1], ['aa_fp0.5', 'all', 0.5], ['aa_fp2', 'all', 2], ['aa_fp4', 'all', 4]];
  for (const [label, variant, fp] of list) {
    await render(name, view, variant, 'full', { fp });
    const f0 = await grab('full');
    const s = await seq(variant, fp, frames, label === 'noaa' || label === 'aa' || label === 'filt' ? `${name}-aa-${label}` : null);
    out.variants[label] = { variant, fp, mean_frame_diff_255: s.mean_frame_diff_255, flicker_255: s.flicker_255,
      excess_over_ref: r4(s.mean_frame_diff_255 - ref.mean_frame_diff_255), vs_ref: diff(f0, ref0, ids, set) };
    log(`shimmer ${name} ${label}:`, JSON.stringify(out.variants[label]));
  }
  const fl = k => out.variants[k].flicker_255 - out.ref_flicker_255;
  out.flicker_excess_reduction_vs_filtered = r3(1 - fl('aa') / Math.max(fl('filt'), 1e-6));
  out.flicker_reduction_vs_filtered = r3(1 - out.variants.aa.flicker_255 / Math.max(out.variants.filt.flicker_255, 1e-6));
  out.flicker_reduction_vs_unfiltered = r3(1 - out.variants.aa.flicker_255 / Math.max(out.variants.noaa.flicker_255, 1e-6));
  R.shimmer[name] = out;
}

async function footprintSweep(name) {
  const view = SCENES[name].views.main;
  const rows = [];
  const fps = [0.5, 1, 2, 4];
  const set = await measureShadeSet(name, view, [{ key: 'plain', variant: 'plain' }, ...fps.map(fp => ({ key: `fp${fp}`, variant: 'all', o: { fp } }))], 'full');
  for (const fp of fps) {
    const m = set[`fp${fp}`];
    rows.push({ fp, all_shade: m.shade, all_extra_vs_plain: r3(m.shade - set.plain.shade) });
  }
  R.footprint[name] = rows;
  log(`footprint ${name}:`, JSON.stringify(rows));
}

function verdicts() {
  const slices = { skin: 2.5, eye: 2.5, leaf: 4.0, wet: 3.0, layer: 3.0, specaa: 3.0 };
  const reps = { skin: 0.25, eye: 0.02, leaf: 0.5, wet: 0.5, layer: 0.5, specaa: 0.5 };
  const source = { skin: ['skin', 'skin'], eye: ['eye', 'eye'], leaf: ['leaf', 'leaf'], wet: ['world', 'wet'], layer: ['world', 'layer'], specaa: ['world', 'specaa'] };
  for (const [mat, [cov, v]] of Object.entries(source)) {
    const rows = R.coverage[cov];
    if (!rows) continue;
    const out = {};
    for (const res of ['full', 'half']) {
      const pts = rows.filter(r => r[`coverage_${v}_${res}`] != null).map(r => [r[`coverage_${v}_${res}`], r[`extra_${v}_${res}`]]);
      if (pts.length < 2) continue;
      const at = interp(pts, reps[mat]);
      out[res] = { extra_ms_at_rep: at.value, extrapolated: at.extrapolated, points: pts };
      if (mat === 'eye') out[res].extra_ms_at_25pct = interp(pts, 0.25);
    }
    const x = out.full.extra_ms_at_rep, s = slices[mat];
    out.slice_ms = s;
    out.representative_coverage = reps[mat];
    out.share_of_slice = r3(x / s);
    out.cost_verdict = x <= 0.25 * s ? 'pass' : x <= 0.5 * s ? 'inconclusive' : 'fail';
    R.verdict[mat] = out;
    log(`verdict ${mat}: +${x} ms at ${reps[mat] * 100}% coverage = ${r3(100 * x / s)}% of ${s} ms -> ${out.cost_verdict}`);
  }
}

// ---- run -------------------------------------------------------------------------------------------------------------

async function phase(label, fn) {
  const t = performance.now();
  await fn();
  R.phases_s[label] = r3((performance.now() - t) / 1000);
  log(`phase ${label}: ${R.phases_s[label]} s`);
}

async function runAll() {
  try {
    await init();
    log('device', JSON.stringify(R.device));
    await phase('pipelines', async () => { for (const n of Object.keys(SCENES)) R.pipelines[n] = await buildPipelines(n); });
    const skip = (hashArgs.get('skip') || '').split(',');
    for (const n of Object.keys(SCENES)) await phase(`scene_${n}`, () => sceneLevel(n));
    if (!skip.includes('eye')) await phase('eye_closeup', eyeCloseupQuality);
    if (!skip.includes('thickness')) await phase('thickness', thicknessSweeps);
    if (!skip.includes('shimmer')) for (const n of ['tower', 'gallery']) await phase(`shimmer_${n}`, () => shimmerTest(n));
    if (!skip.includes('footprint')) await phase('footprint_tower', () => footprintSweep('tower'));
    if (!skip.includes('coverage')) await phase('coverage', coverageSweeps);
    await phase('paced', async () => {
      R.paced.head = await paced('head', SCENES.head.views.main, 'all');
      R.paced.tower = await paced('tower', SCENES.tower.views.main, 'all');
      log('paced', JSON.stringify(R.paced));
    });
    verdicts();
    R.finished = new Date().toISOString();
    log(`done; longest tile submission ${R.max_tile_ms} ms, longest timed submission ${R.max_submission_ms} ms`);
  } catch (e) {
    console.error('FAILED:', e.stack || e.message);
    R.error = String(e.stack || e.message);
  }
  await save(`run-${R.started.replace(/[:.]/g, '-')}.json`, JSON.stringify(R, null, 2));
  await save('DONE', 'done');
}

/** Renders each scene once (plus close-ups) and saves screenshots. `#quick&shots=scene:view:variant,...` picks. */
async function quick() {
  try {
    await init();
    const shots = (hashArgs.get('shots') || 'gallery:main:all,head:main:all,tower:main:dry')
      .split(',').map(s => s.split(':'));
    for (const n of Object.keys(SCENES)) {
      const need = shots.filter(s => s[0] === n).map(s => s[2]);
      if (need.length) await buildPipelines(n, need);
    }
    for (const [n, vn, v, res = 'full'] of shots) {
      const t = performance.now();
      await render(n, SCENES[n].views[vn], v, res);
      const ms = r3(performance.now() - t);
      if (SCENES[n].variants[v]?.STATS) log(`stats ${n}/${vn}/${v}`, JSON.stringify(await readStats(res)));
      await snapshot(`quick-${n}-${vn}-${v}${res === 'half' ? '-540' : ''}`, res);
      log(`quick ${n}/${vn}/${v} (${res}) rendered in ${ms} ms wall`);
    }
    // cmp=scene:view:fast:ref,... prints fast-vs-reference numbers on the material's own pixels
    for (const c of (hashArgs.get('cmp') || '').split(',').filter(Boolean)) {
      const [n, vn, fast, ref, res = 'full'] = c.split(':');
      const sc = SCENES[n];
      if (!sc.P || !sc.P[fast] || !sc.P[ref]) await buildPipelines(n, [fast, ref, ...Object.keys(sc.P || {})]);
      await render(n, sc.views[vn], fast, res);
      const { ids } = await readVis(res);
      const a = await grab(res);
      await snapshot(`cmp-${n}-${vn}-${fast}`, res);
      await render(n, sc.views[vn], ref, res);
      const b = await grab(res);
      await snapshot(`cmp-${n}-${vn}-${ref}`, res);
      await diffImage(`cmp-${n}-${vn}-${fast}-diff`, a, b, res);
      const mat = fast.replace(/_.*$/, '');
      const set = MAT_IDS[mat] ? idsOf(mat, n) : NONSKY;
      log(`cmp ${n}/${vn} ${fast} vs ${ref}: own`, JSON.stringify(diff(a, b, ids, set)), 'frame', JSON.stringify(diff(a, b)));
      if (hashArgs.get('raw')) {
        // mean of decoded (linear) R and G over the material's pixels where both are nonzero
        let s = [0, 0, 0, 0, 0, 0], nn = 0;
        const dec = x => Math.pow(x / 255, 2.2);
        for (let p = 0; p < ids.length; p++) {
          if (!set.has(ids[p]) || a[3 * p] === 0 || b[3 * p] === 0) continue;
          s[0] += dec(a[3 * p]); s[1] += dec(a[3 * p + 1]); s[2] += dec(b[3 * p]); s[3] += dec(b[3 * p + 1]);
          s[4] += dec(a[3 * p + 2]); s[5] += dec(b[3 * p + 2]); nn++;
        }
        log(`raw ${fast}/${ref}: chord_mm fast ${r4(2 * s[0] / nn)} ref ${r4(2 * s[2] / nn)}, cos fast ${r4(s[1] / nn)} ref ${r4(s[3] / nn)}, entry_mm fast ${r4(s[4] / nn)} ref ${r4(s[5] / nn)} (${nn} px)`);
      }
    }
    if (hashArgs.get('time')) {
      for (const [n, vn, v, res = 'full'] of shots) if (!heavy(v)) log(`time ${n}/${vn}/${v}`, JSON.stringify(await measure(n, SCENES[n].views[vn], v, res)));
    }
    log(`quick done; longest tile submission ${R.max_tile_ms || 0} ms, longest timed submission ${R.max_submission_ms || 0} ms`, JSON.stringify(R.tile_ms || {}));
  } catch (e) {
    console.error('FAILED:', e.stack || e.message);
  }
  await save('DONE', 'done');
}

$('run').addEventListener('click', runAll);
$('quick').addEventListener('click', quick);
if (location.hash.startsWith('#run')) runAll();
if (location.hash.startsWith('#quick')) quick();
