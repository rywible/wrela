// Spike 12 harness: builds the clearing, the pipelines for every look, times them, checks them against
// brute-force references, measures flicker under camera motion, and saves screenshots.
// This file plays the engine's role plus the measuring. It isn't compiler output.

const FOVY = 45 * Math.PI / 180;
const U = GPUBufferUsage;
const TU = GPUTextureUsage;
const RES = { full: { w: 1920, h: 1080, s: 1 }, half: { w: 960, h: 540, s: 0.5 } };
const PRM = { W: 2, stroke: 12, kuw: 6, crease: 2.5, halo: 18 };   // px at 1080p; halved at 540p (halo: 1.5 × stroke)
const SUN = norm([-0.70, 0.62, -0.35]);   // 38° up, from the wide camera's left

// ---- the looks -----------------------------------------------------------------------------------
// content 0 detailed, 1 simple. shadow: soft (realistic penumbra), hard (cel: thresholded, stops
// early), paint (soft-ish, stops a bit earlier). post: screen-space passes after shading.
const V = {
  real:         { content: 0, lines: 0, shadow: 'soft',  curv: 0, style: 0, post: [] },
  cel:          { content: 0, lines: 1, shadow: 'hard',  curv: 1, style: 1, fieldLines: 1, post: [] },
  cel_ss:       { content: 0, lines: 0, shadow: 'hard',  curv: 0, style: 1, fieldLines: 0, post: ['sobel'] },
  paint:        { content: 0, lines: 1, shadow: 'paint', curv: 0, style: 2, halo: 1, post: [] },
  paint_kuw:    { content: 0, lines: 0, shadow: 'soft',  curv: 0, style: 0, post: ['kuw'] },
  real_simple:  { content: 1, lines: 0, shadow: 'soft',  curv: 0, style: 0, post: [] },
  cel_simple:   { content: 1, lines: 1, shadow: 'hard',  curv: 1, style: 1, fieldLines: 1, post: [] },
  paint_simple: { content: 1, lines: 1, shadow: 'paint', curv: 0, style: 2, halo: 1, post: [] },
  // sweep-only
  cel_disk:     { content: 0, lines: 0, shadow: 'hard',  curv: 0, style: 1, fieldLines: 0, post: ['disk'] },
  paint_nohalo: { content: 0, lines: 0, shadow: 'paint', curv: 0, style: 2, halo: 0, post: [] },
};
const MAIN = ['real', 'cel', 'cel_ss', 'paint', 'paint_kuw', 'real_simple', 'cel_simple', 'paint_simple'];
const CUT = { soft: 0.002, hard: 0.25, paint: 0.06 };
const REF_T = { STEP_SCALE: 0.5, EPS_SCALE: 0.005, MAX_STEPS: 4000, LIP_SCALE: 1, EXACT_WOLF: 1 };
const REF_L = { SH_STEP: 0.5, SH_MAX: 2000, LIP_SCALE: 1, EXACT_WOLF: 1 };
const STAT_NAMES = ['px_sky', 'px_ground', 'px_trunk', 'px_leaf', 'px_stone', 'px_rock', 'px_wolf', '_7', 'steps', 'caps',
  'line_px', 'covered', 'leaf_steps', 'wolf_steps', '_14', '_15', 'sh_marches', 'sh_steps', 'sh_caps'];

const $ = id => document.getElementById(id);
const log = (...a) => { const s = a.join(' '); $('log').textContent += s + '\n'; console.log(s); };
const R = { started: new Date().toISOString(), device: {}, setup: {}, pipelines: {}, frames: {}, stats: {}, quality: {}, lines: {}, sweeps: {}, flicker: {}, paced: {} };
window.__results = R;

let device, ctx, canvasFormat, src = {}, mods = {}, L = {}, B = {}, PL = new Map();
const res = {};

const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[s.length >> 1] : NaN; };
const pct = (a, q) => { const s = [...a].sort((x, y) => x - y); return s[Math.min(s.length - 1, Math.floor(s.length * q))]; };
const r3 = x => Math.round(x * 1000) / 1000;
function norm(a) { const l = Math.hypot(...a); return a.map(x => x / l); }
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sub = (a, b) => a.map((x, i) => x - b[i]);
const add = (a, b) => a.map((x, i) => x + b[i]);
const scale = (a, s) => a.map(x => x * s);
const smoothstep = (a, b, x) => { const t = Math.min(Math.max((x - a) / (b - a), 0), 1); return t * t * (3 - 2 * t); };
function mulberry32(a) { return () => { a |= 0; a = a + 0x6D2B79F5 | 0; let t = Math.imul(a ^ a >>> 15, 1 | a); t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t; return ((t ^ t >>> 14) >>> 0) / 4294967296; }; }

async function save(name, body) {
  try { const r = await fetch(`results/${name}`, { method: 'PUT', body }); if (!r.ok) console.error('save failed', name, r.status); }
  catch (e) { console.error('save failed', name, e.message); }
}

// ---- the scene ---------------------------------------------------------------------------------------

/** Same as scene.wgsl's terrain_h. */
function terrainH(x, z) {
  const r = Math.hypot(x, z);
  let h = (0.18 * Math.sin(0.21 * x + 0.5) * Math.cos(0.17 * z) + 0.10 * Math.sin(0.43 * x + 1.0) * Math.sin(0.37 * z + 1.0))
    * (0.15 + 0.85 * smoothstep(8, 20, r));
  h += 4.0 * smoothstep(16, 85, r);
  h += 22.0 * smoothstep(110, 300, r) * (0.55 + 0.45 * Math.sin(0.012 * x + 0.7) * Math.cos(0.010 * z - 0.4));
  h += 4.0 * Math.sin(0.035 * x + 2.0) * Math.sin(0.03 * z) * smoothstep(60, 150, r);
  return h;
}

const PLACE = (() => {
  const tower = [-7.5, 0, 20.5];
  tower[1] = terrainH(tower[0], tower[2]);
  const wolf = [0.4, 0, 0.8];
  wolf[1] = terrainH(wolf[0], wolf[2]);
  const face = norm([-1.0, 0, -0.75]);                 // the wolf looks toward the camera's left
  const rocks = [
    { c: [-3.6, 0, 16.5], r: [1.3, 0.85, 1.0], yaw: 0.4 },
    { c: [-10.8, 0, 15.2], r: [0.8, 0.55, 0.7], yaw: 1.1 },
    { c: [3.4, 0, 4.0], r: [0.55, 0.38, 0.62], yaw: 2.0 },
    { c: [6.5, 0, 12.5], r: [0.95, 0.6, 0.8], yaw: 0.2 },
  ];
  for (const k of rocks) k.c[1] = terrainH(k.c[0], k.c[2]) - 0.25 * k.r[1];
  return { tower, wolf, wolfYaw: Math.atan2(face[0], face[2]), rocks, doorA: Math.atan2(-6 - tower[2], 2.5 - tower[0]) };
})();

const CELL = 8, GRID_N = 22, GRID_O = -88, TREE_REC = 10, GRID_A = 0.52;
const GC = Math.cos(GRID_A), GS = Math.sin(GRID_A);
const toGrid = (x, z) => [GC * x - GS * z, GS * x + GC * z];
const fromGrid = (gx, gz) => [GC * gx + GS * gz, -GS * gx + GC * gz];

/** One tree per 8 m cell outside the clearing, kept inside its cell so a march only needs its own,
 *  plus each cell's clearance from every other cell's tree (so empty cells cost one step). */
function makeTrees() {
  const rnd = mulberry32(1234);
  const data = new Float32Array(GRID_N * GRID_N * TREE_REC * 4);
  const circles = [];                                   // per tree: grid-space centre, radius, cell
  let count = 0, worst = Infinity, shrunk = 0;
  const pad = 0.7 / 4 + 0.30 + 0.05;                    // smooth-union bulge + displacement + margin
  for (let iz = 0; iz < GRID_N; iz++) {
    for (let ix = 0; ix < GRID_N; ix++) {
      const shift = (iz & 1) * 0.5 * CELL;             // odd rows shifted by half a cell
      const gx = GRID_O + shift + (ix + 0.5) * CELL + (rnd() - 0.5) * 2.8, gz = GRID_O + (iz + 0.5) * CELL + (rnd() - 0.5) * 2.8;
      const [bx, bz] = fromGrid(gx, gz);
      const r = Math.hypot(bx, bz);
      const edge = 15.5 + 2.5 * Math.sin(Math.atan2(bz, bx) * 3 + 1) + 2 * (rnd() - 0.5);
      const keep = rnd() < 0.92;
      const dt = Math.hypot(bx - PLACE.tower[0], bz - PLACE.tower[2]);
      const u = [rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd(), rnd()];
      if (!(r > edge && r < 82 && keep && dt > 6.5)) continue;
      let sc = Math.min(Math.max(0.62 + (r - edge) * 0.05, 0.62), 1.0) * (0.85 + 0.3 * u[0]);
      const lo = [GRID_O + shift + ix * CELL, GRID_O + iz * CELL];
      let tree;
      for (let tries = 0; tries < 12; tries++, sc *= 0.92) {
        const H = (6.5 + 3 * u[1]) * sc;
        const by = terrainH(bx, bz) - 0.3;
        const lx = (u[2] - 0.5) * 0.4 * sc, lz = (u[3] - 0.5) * 0.4 * sc;
        const top = [bx + lx, by + 0.72 * H, bz + lz];
        const clumps = [[top[0], top[1] + 0.12 * H, top[2], (1.6 + 0.4 * u[4]) * sc]];
        const a0 = u[5] * Math.PI * 2;
        for (let i = 0; i < 4; i++) {
          const a = a0 + i * Math.PI / 2 + (u[6 + i] - 0.5) * 0.8;
          const off = (0.9 + 0.35 * u[10 + i]) * sc;
          clumps.push([bx + lx * 0.6 + Math.cos(a) * off, by + (0.5 + 0.18 * u[14 + (i & 1)]) * H, bz + lz * 0.6 + Math.sin(a) * off, (1.0 + 0.3 * u[16 + (i % 3)]) * sc]);
        }
        let m = Infinity, ext = 0;
        for (const c of clumps) {
          const [cx, cz] = toGrid(c[0], c[2]);
          const e = c[3] + pad;
          m = Math.min(m, cx - e - lo[0], lo[0] + CELL - cx - e, cz - e - lo[1], lo[1] + CELL - cz - e);
          ext = Math.max(ext, Math.hypot(cx - gx, cz - gz) + e);
        }
        tree = { H, by, top, clumps, m, ext, sc };
        if (m > 0.15) break;
        shrunk++;
      }
      const { H, by, top, clumps } = tree;
      worst = Math.min(worst, tree.m);
      const bc = [0, 1, 2].map(k => clumps.reduce((acc, c) => acc + c[k], 0) / clumps.length);
      const bR = Math.max(...clumps.map(c => Math.hypot(c[0] - bc[0], c[1] - bc[1], c[2] - bc[2]) + c[3])) + pad;
      const o = (iz * GRID_N + ix) * TREE_REC * 4;
      data.set([bx, by, bz, H], o);
      data.set([...top, 0.12 * tree.sc], o + 4);
      data.set([...bc, bR], o + 8);
      clumps.forEach((c, i) => data.set(c, o + 12 + 4 * i));
      data.set([u[17], u[18], 0.30 * tree.sc, 0.45 + 0.1 * u[2]], o + 32);
      circles.push({ gx, gz, rad: Math.max(tree.ext, 0.4 * tree.sc + 0.3), cell: iz * GRID_N + ix });
      count++;
    }
  }
  // clearance: from anywhere in a cell to the nearest other tree's footprint circle (grid space)
  let maxClear = 0;
  for (let iz = 0; iz < GRID_N; iz++) {
    for (let ix = 0; ix < GRID_N; ix++) {
      const lo = [GRID_O + (iz & 1) * 0.5 * CELL + ix * CELL, GRID_O + iz * CELL], cell = iz * GRID_N + ix;
      let c = 64;
      for (const t of circles) {
        if (t.cell === cell) continue;
        const dx = Math.max(lo[0] - t.gx, 0, t.gx - lo[0] - CELL), dz = Math.max(lo[1] - t.gz, 0, t.gz - lo[1] - CELL);
        c = Math.min(c, Math.max(Math.hypot(dx, dz) - t.rad, 0));
      }
      data[(cell * TREE_REC + 9) * 4] = c;
      maxClear = Math.max(maxClear, c);
    }
  }
  return { data, count, cellMargin: r3(worst), shrunk, maxClear: r3(maxClear) };
}

/** The terrain's steepest slope inside each zone (circles about the origin), plus 10%. */
const ZONES = [24, 100];
function zoneSlopes() {
  const out = [0, 0, 0];
  const e = 0.05;
  for (let x = -720; x <= 720; x += 0.5) {
    for (let z = -720; z <= 720; z += 0.5) {
      const r = Math.hypot(x, z);
      if (r > 720) continue;
      const g = Math.hypot((terrainH(x + e, z) - terrainH(x - e, z)) / (2 * e), (terrainH(x, z + e) - terrainH(x, z - e)) / (2 * e));
      if (r <= ZONES[0] + 1) out[0] = Math.max(out[0], g);
      if (r <= ZONES[1] + 1) out[1] = Math.max(out[1], g);
      out[2] = Math.max(out[2], g);
    }
  }
  return out.map(x => x * 1.1 + 0.002);
}

function placementWGSL(zs) {
  const v = a => a.map(x => x.toFixed(5)).join(', ');
  const P = PLACE;
  return `// generated by main.js
const TOWER_P = vec3f(${v(P.tower)});
const DOOR_A: f32 = ${P.doorA.toFixed(5)};
const WOLF_P = vec3f(${v(P.wolf)});
const WOLF_YAW: f32 = ${P.wolfYaw.toFixed(5)};
const N_ROCKS: u32 = ${P.rocks.length}u;
var<private> ROCKS: array<vec4f, ${P.rocks.length}> = array<vec4f, ${P.rocks.length}>(${P.rocks.map(k => `vec4f(${v(k.c)}, ${(Math.max(...k.r) + 0.1).toFixed(3)})`).join(', ')});
var<private> ROCK_R: array<vec4f, ${P.rocks.length}> = array<vec4f, ${P.rocks.length}>(${P.rocks.map(k => `vec4f(${v(k.r)}, ${k.yaw.toFixed(3)})`).join(', ')});
const T_SLOPE: f32 = ${zs[2].toFixed(4)};
const T_COS: f32 = ${(1 / Math.hypot(1, zs[2])).toFixed(5)};
const ZONE_R1: f32 = ${ZONES[0].toFixed(1)};
const ZONE_S1: f32 = ${zs[0].toFixed(4)};
const ZONE_R2: f32 = ${ZONES[1].toFixed(1)};
const ZONE_S2: f32 = ${zs[1].toFixed(4)};
const GRID_C: f32 = ${GC.toFixed(6)};
const GRID_S: f32 = ${GS.toFixed(6)};
`;
}

// ---- cameras -----------------------------------------------------------------------------------------

function wolfPoint(local) {
  const c = Math.cos(PLACE.wolfYaw), s = Math.sin(PLACE.wolfYaw);
  return [PLACE.wolf[0] + c * local[0] + s * local[2], PLACE.wolf[1] + local[1], PLACE.wolf[2] - s * local[0] + c * local[2]];
}

const CAMS = {
  wide: { eye: [3.0, 1.65, -9.0], target: [-1.6, 2.4, 8.0] },
  close: { eye: wolfPoint([0.95, 0.80, 1.55]), target: wolfPoint([0.02, 0.60, 0.05]) },
  canopy: { eye: [-3.5, 1.6, -1.5], target: [-15.0, 5.5, -8.5] },   // toward the sun: backlit leaves
};
for (const c of Object.values(CAMS)) {
  c.subject = Math.hypot(...sub(PLACE.wolf, c.eye));     // flicker speeds are px/frame at the wolf's distance
}

function camBasis(eye, target, r) {
  const f = norm(sub(target, eye));
  const right = norm(cross(f, [0, 1, 0]));
  const up = cross(right, f);
  const ty = Math.tan(FOVY / 2), tx = ty * r.w / r.h;
  return { eye, f, cr: scale(right, tx), cu: scale(up, ty), right, pa: 2 * ty / r.h };
}

function writeFrame(cam, r, prm = PRM, camA = null) {
  const b = camBasis(cam.eye, cam.target, r);
  const a = camA ? camBasis(camA.eye, camA.target, r) : b;
  const buf = new ArrayBuffer(208), f = new Float32Array(buf), u = new Uint32Array(buf);
  f.set([...b.eye, b.pa], 0);
  f.set([...b.f, 0], 4);
  f.set([...b.cr, 0], 8);
  f.set([...b.cu, 0], 12);
  f.set([...SUN, 0], 16);
  f.set([prm.W * r.s, prm.stroke * r.s, prm.kuw * r.s, prm.crease * r.s], 20);
  f.set([prm.halo * r.s, r.s, 0, 0], 24);
  f.set([...PLACE.wolf, PLACE.wolfYaw], 28);
  u.set([r.w, r.h, 0, 0], 32);
  f.set([...a.eye, a.pa], 36);
  f.set([...a.f, 0], 40);
  f.set([...a.cr, 0], 44);
  f.set([...a.cu, 0], 48);
  device.queue.writeBuffer(B.frame, 0, buf);
}

// ---- setup ---------------------------------------------------------------------------------------------

async function init() {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
  const want = ['timestamp-query'].filter(f => adapter.features.has(f));
  device = await adapter.requestDevice({ requiredFeatures: want });
  device.lost.then(i => console.error('DEVICE LOST:', i.message));
  device.addEventListener('uncapturederror', e => console.error('GPU ERROR:', e.error.message));
  const info = adapter.info || {};
  R.device = { vendor: info.vendor, architecture: info.architecture, description: info.description, userAgent: navigator.userAgent,
    timestampQuery: want.includes('timestamp-query'), pageVisibility: document.visibilityState };
  if (!R.device.timestampQuery) throw new Error('timestamp-query is required');

  for (const f of ['common', 'wolf', 'scene', 'trace', 'light', 'shade', 'post']) src[f] = await (await fetch(`${f}.wgsl`)).text();
  let t = performance.now();
  const zs = zoneSlopes();
  R.setup.slope_bounds = { zones_m: ZONES, bounds: zs.map(r3) };
  const trees = makeTrees();
  R.setup.trees = { count: trees.count, cell_margin_m: trees.cellMargin, shrunk_to_fit: trees.shrunk, max_clearance_m: trees.maxClear };
  R.setup.placement = PLACE;
  R.setup.cpu_setup_ms = r3(performance.now() - t);
  log('setup', JSON.stringify(R.setup.slope_bounds), JSON.stringify(R.setup.trees));
  if (trees.cellMargin <= 0) console.error('a tree crosses its cell wall: the march could miss it');
  src.place = placementWGSL(zs);

  const canvas = $('view');
  canvas.width = 1920;
  canvas.height = 1080;
  ctx = canvas.getContext('webgpu');
  canvasFormat = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format: canvasFormat, alphaMode: 'opaque' });

  const C = GPUShaderStage.COMPUTE;
  const buf = (binding, type) => ({ binding, visibility: C, buffer: { type } });
  const tex = (binding, sampleType) => ({ binding, visibility: C, texture: { sampleType } });
  const sto = (binding, format) => ({ binding, visibility: C, storageTexture: { access: 'write-only', format } });
  L.trace = device.createBindGroupLayout({ entries: [buf(0, 'uniform'), buf(1, 'read-only-storage'), sto(2, 'rgba32uint'), buf(3, 'storage')] });
  L.light = device.createBindGroupLayout({ entries: [buf(0, 'uniform'), buf(1, 'read-only-storage'), tex(2, 'uint'), sto(3, 'rgba16float'), buf(4, 'storage')] });
  L.shade = device.createBindGroupLayout({ entries: [buf(0, 'uniform'), buf(1, 'read-only-storage'), tex(2, 'uint'), tex(3, 'unfilterable-float'), sto(4, 'rgba16float')] });
  L.edges = device.createBindGroupLayout({ entries: [buf(0, 'uniform'), tex(2, 'uint'), tex(3, 'unfilterable-float'), sto(4, 'rgba16float')] });
  L.st = device.createBindGroupLayout({ entries: [buf(0, 'uniform'), tex(3, 'unfilterable-float'), sto(4, 'rgba16float')] });
  L.kuw = device.createBindGroupLayout({ entries: [buf(0, 'uniform'), tex(3, 'unfilterable-float'), sto(4, 'rgba16float'), tex(5, 'unfilterable-float')] });
  L.flick = device.createBindGroupLayout({ entries: [buf(0, 'uniform'), tex(2, 'uint'), tex(3, 'unfilterable-float'), tex(6, 'uint'), tex(7, 'unfilterable-float'), buf(8, 'storage')] });
  L.blit = device.createBindGroupLayout({ entries: [
    { binding: 9, visibility: GPUShaderStage.FRAGMENT, texture: {} }, { binding: 10, visibility: GPUShaderStage.FRAGMENT, sampler: {} }] });

  B.frame = device.createBuffer({ size: 208, usage: U.UNIFORM | U.COPY_DST });
  B.trees = device.createBuffer({ size: trees.data.byteLength, usage: U.STORAGE | U.COPY_DST });
  device.queue.writeBuffer(B.trees, 0, trees.data);
  B.stats = device.createBuffer({ size: 128, usage: U.STORAGE | U.COPY_SRC | U.COPY_DST });
  B.fl = device.createBuffer({ size: 32, usage: U.STORAGE | U.COPY_SRC | U.COPY_DST });
  B.sampler = device.createSampler({ magFilter: 'linear', minFilter: 'linear' });

  // Modules: the scene passes share common + placement + wolf + scene; post has no scene.
  const parts = name => name === 'post' ? [['common', src.common], ['post', src.post]]
    : [['common', src.common], ['placement', src.place], ['wolf', src.wolf], ['scene', src.scene], [name, src[name]]];
  for (const name of ['trace', 'light', 'shade', 'post']) {
    const ps = parts(name);
    const code = ps.map(p => p[1]).join('\n');
    const m = device.createShaderModule({ code, label: name });
    const info = await m.getCompilationInfo();
    for (const msg of info.messages) {
      // map the concatenated line back to its file
      let line = msg.lineNum, file = '?';
      for (const [f, s] of ps) { const n = s.split('\n').length; if (line <= n) { file = f; break; } line -= n; }
      const say = msg.type === 'error' ? console.error : console.log;
      say(`WGSL ${msg.type} in ${name} module, ${file}.wgsl:${line}:${msg.linePos}: ${msg.message}`);
    }
    if (info.messages.some(x => x.type === 'error')) throw new Error(`${name}: WGSL errors`);
    mods[name] = m;
  }
  for (const k of Object.keys(RES)) makeRes(k);
  const bm = mods.post;
  B.blitPipe = device.createRenderPipeline({
    layout: device.createPipelineLayout({ bindGroupLayouts: [L.blit] }),
    vertex: { module: bm, entryPoint: 'blit_vs' },
    fragment: { module: bm, entryPoint: 'blit_fs', targets: [{ format: canvasFormat }] },
  });
}

function makeRes(k) {
  const r = RES[k];
  const S = TU.STORAGE_BINDING | TU.TEXTURE_BINDING | TU.COPY_SRC | TU.COPY_DST;
  const t = fmt => device.createTexture({ size: [r.w, r.h], format: fmt, usage: S });
  const T = { gb: t('rgba32uint'), light: t('rgba16float'), color: t('rgba16float'), color2: t('rgba16float'),
    st: t('rgba16float'), st2: t('rgba16float'), gbA: t('rgba32uint'), colA: t('rgba16float') };
  const v = Object.fromEntries(Object.entries(T).map(([n, x]) => [n, x.createView()]));
  const ub = { binding: 0, resource: { buffer: B.frame } };
  const bg = (layout, entries) => device.createBindGroup({ layout, entries: [ub, ...entries.map(([binding, resource]) => ({ binding, resource }))] });
  const tb = { buffer: B.trees }, sb = { buffer: B.stats };
  r.T = T;
  r.bg = {
    trace: bg(L.trace, [[1, tb], [2, v.gb], [3, sb]]),
    light: bg(L.light, [[1, tb], [2, v.gb], [3, v.light], [4, sb]]),
    shade: bg(L.shade, [[1, tb], [2, v.gb], [3, v.light], [4, v.color]]),
    edges: bg(L.edges, [[2, v.gb], [3, v.color], [4, v.color2]]),
    structure: bg(L.st, [[3, v.color], [4, v.st]]),
    blur0: bg(L.st, [[3, v.st], [4, v.st2]]),
    blur1: bg(L.st, [[3, v.st2], [4, v.st]]),
    kuwahara: bg(L.kuw, [[3, v.color], [4, v.color2], [5, v.st]]),
    flicker_color: bg(L.flick, [[2, v.gbA], [3, v.colA], [6, v.gb], [7, v.color], [8, { buffer: B.fl }]]),
    flicker_color2: bg(L.flick, [[2, v.gbA], [3, v.colA], [6, v.gb], [7, v.color2], [8, { buffer: B.fl }]]),
  };
  r.blit = {
    color: device.createBindGroup({ layout: L.blit, entries: [{ binding: 9, resource: v.color }, { binding: 10, resource: B.sampler }] }),
    color2: device.createBindGroup({ layout: L.blit, entries: [{ binding: 9, resource: v.color2 }, { binding: 10, resource: B.sampler }] }),
  };
  res[k] = r;
}

// ---- pipelines and passes -----------------------------------------------------------------------------------

const KIND = {
  trace: { mod: 'trace', entry: 'trace', layout: 'trace' },
  light: { mod: 'light', entry: 'light', layout: 'light' },
  shade: { mod: 'shade', entry: 'shade', layout: 'shade' },
  edges: { mod: 'post', entry: 'edges', layout: 'edges' },
  structure: { mod: 'post', entry: 'structure', layout: 'st' },
  blur: { mod: 'post', entry: 'blur', layout: 'st' },
  kuwahara: { mod: 'post', entry: 'kuwahara', layout: 'kuw' },
  flicker: { mod: 'post', entry: 'flicker', layout: 'flick' },
};

function pipeline(kind, consts) {
  const key = kind + JSON.stringify(consts);
  if (!PL.has(key)) {
    const k = KIND[kind];
    const t0 = performance.now();
    const p = device.createComputePipelineAsync({
      layout: device.createPipelineLayout({ bindGroupLayouts: [L[k.layout]] }),
      compute: { module: mods[k.mod], entryPoint: k.entry, constants: consts },
    }).then(x => { x.__ms = performance.now() - t0; return x; });
    p.catch(e => console.error('pipeline failed', key, e.message));
    PL.set(key, p);
  }
  return PL.get(key);
}

/** The passes of one look. opts: ref (brute-force reference), stats, view (1 lines only, 2 heat). */
function passesFor(name, opts = {}) {
  const v = V[name];
  const C = v.content;
  const out = [];
  out.push({ name: 'trace', kind: 'trace', bg: 'trace',
    consts: { CONTENT: C, LINES: v.lines || opts.view === 1 ? 1 : 0, HALO_REACH: v.halo ? 1 : 0, ...(opts.off ? { DBG_OFF: opts.off } : {}), ...(opts.ref ? REF_T : {}), ...(opts.stats ? { STATS: 1 } : {}) } });
  if (!opts.view || opts.view >= 3) {
    out.push({ name: 'light', kind: 'light', bg: 'light',
      consts: { CONTENT: C, SHADOW_CUT: CUT[v.shadow], CURV: v.curv, ...(opts.off ? { DBG_OFF: opts.off } : {}), ...(opts.ref ? REF_L : {}), ...(opts.stats ? { STATS: 1 } : {}) } });
  }
  out.push({ name: 'shade', kind: 'shade', bg: 'shade',
    consts: { CONTENT: C, STYLE: v.style, FIELD_LINES: (v.fieldLines || 0), HALO: v.halo || 0, VIEW: opts.view || 0 } });
  for (const p of v.post) {
    if (p === 'sobel' || p === 'disk') out.push({ name: 'edges', kind: 'edges', bg: 'edges', consts: { EDGE_MODE: p === 'disk' ? 1 : 0, EDGE_VIEW: opts.view === 1 ? 1 : 0 } });
    if (p === 'kuw') {
      out.push({ name: 'structure', kind: 'structure', bg: 'structure', consts: {} });
      out.push({ name: 'blur_h', kind: 'blur', bg: 'blur0', consts: { DIR: 0 } });
      out.push({ name: 'blur_v', kind: 'blur', bg: 'blur1', consts: { DIR: 1 } });
      out.push({ name: 'kuwahara', kind: 'kuwahara', bg: 'kuwahara', consts: {} });
    }
  }
  // The heavy passes run as top and bottom halves, one submission each, so even a throttled
  // close-up stays far below ~100 ms per submission. A pass's time is the sum of its halves.
  const passes = out.flatMap(p => (p.kind === 'trace' || p.kind === 'light' || p.kind === 'kuwahara') ? [{ ...p, half: 0 }, { ...p, half: 1 }] : [p]);
  const final = v.post.length ? 'color2' : 'color';
  return { passes, final };
}

async function ready(pl) {
  for (const p of pl.passes) p.pipe = await pipeline(p.kind, p.consts);
  return pl;
}

/** Rows where the second half starts: a multiple of 8, so the halves don't overlap. */
const halfRow = r => Math.ceil(r.h / 32) * 16;   // a multiple of 16 (the Kuwahara's workgroup)

function encodePass(enc, p, i, r, qs, q0) {
  const desc = qs ? { timestampWrites: { querySet: qs, beginningOfPassWriteIndex: q0 + 2 * i, endOfPassWriteIndex: q0 + 2 * i + 1 } } : {};
  const c = enc.beginComputePass(desc);
  c.setPipeline(p.pipe);
  c.setBindGroup(0, r.bg[p.bg]);
  const rows = p.half === undefined ? r.h : p.half === 0 ? halfRow(r) : r.h - halfRow(r);
  const g = p.kind === 'kuwahara' ? 16 : 8;
  c.dispatchWorkgroups(Math.ceil(r.w / g), Math.ceil(rows / g));
  c.end();
}

/** One frame, one submission per pass: the trace alone can take ~50 ms in a close-up at 1080p, and
 *  no single submission may run past ~100 ms (spikes/README.md, GPU safety rules). Frames still run
 *  back to back. `before` and `after` add commands to the first and last submission. */
function submitFrame(pl, r, qs = null, q0 = 0, before = null, after = null) {
  pl.passes.forEach((p, i) => {
    // the half's row origin goes in dims.w (queue writes are ordered with submissions)
    if (p.half !== undefined) device.queue.writeBuffer(B.frame, 136, new Uint32Array([0, p.half ? halfRow(r) : 0]));
    const enc = device.createCommandEncoder();
    if (i === 0 && before) before(enc);
    encodePass(enc, p, i, r, qs, q0);
    device.queue.submit([enc.finish()]);
    if (p.half === 1) device.queue.writeBuffer(B.frame, 136, new Uint32Array([0, 0]));
  });
  // after the origin is back to 0: the flicker pass reads it too
  if (after) { const enc = device.createCommandEncoder(); after(enc); device.queue.submit([enc.finish()]); }
}

/** A brute-force frame, one tile of one pass per submission, waiting in between, so no single
 *  submission runs long (spikes/README.md, GPU safety rules). */
const TILE = [240, 128];
async function renderTiled(pl, r) {
  for (const p of pl.passes.filter(x => x.half !== 1)) {
    for (let ty = 0; ty < r.h; ty += TILE[1]) {
      for (let tx = 0; tx < r.w; tx += TILE[0]) {
        device.queue.writeBuffer(B.frame, 128, new Uint32Array([r.w, r.h, tx, ty]));
        const enc = device.createCommandEncoder();
        const c = enc.beginComputePass();
        c.setPipeline(p.pipe);
        c.setBindGroup(0, r.bg[p.bg]);
        const g = p.kind === 'kuwahara' ? 16 : 8;
        c.dispatchWorkgroups(TILE[0] / g, TILE[1] / g);
        c.end();
        device.queue.submit([enc.finish()]);
        await device.queue.onSubmittedWorkDone();
      }
    }
  }
  device.queue.writeBuffer(B.frame, 128, new Uint32Array([r.w, r.h, 0, 0]));
}

function present(r, final) {
  const enc = device.createCommandEncoder();
  const p = enc.beginRenderPass({ colorAttachments: [{ view: ctx.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
  p.setPipeline(B.blitPipe);
  p.setBindGroup(0, r.blit[final]);
  p.draw(3);
  p.end();
  device.queue.submit([enc.finish()]);
}

async function readBuffer(srcBuf, size) {
  const rb = device.createBuffer({ size, usage: U.MAP_READ | U.COPY_DST });
  const enc = device.createCommandEncoder();
  enc.copyBufferToBuffer(srcBuf, 0, rb, 0, size);
  device.queue.submit([enc.finish()]);
  await rb.mapAsync(GPUMapMode.READ);
  const out = rb.getMappedRange().slice(0);
  rb.unmap();
  rb.destroy();
  return out;
}

async function resolveTimestamps(qs, count) {
  const buf = device.createBuffer({ size: count * 8, usage: U.QUERY_RESOLVE | U.COPY_SRC });
  const enc = device.createCommandEncoder();
  enc.resolveQuerySet(qs, 0, count, buf, 0);
  device.queue.submit([enc.finish()]);
  const raw = await readBuffer(buf, count * 8);
  buf.destroy();
  qs.destroy();
  return new BigUint64Array(raw);
}

// ---- measuring ---------------------------------------------------------------------------------------------

/** GPU time per pass under sustained load: 30 untimed frames, then `frames` back to back. */
async function measure(camName, name, rk = 'full', prm = PRM, frames = 90, warm = 30) {
  const r = res[rk];
  const pl = await ready(passesFor(name));
  const n = pl.passes.length, Q = 2 * n;
  writeFrame(CAMS[camName], r, prm);
  for (let f = 0; f < warm; f++) submitFrame(pl, r);
  await device.queue.onSubmittedWorkDone();
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  const t1 = performance.now();
  for (let f = 0; f < frames; f++) submitFrame(pl, r, qs, f * Q);
  await device.queue.onSubmittedWorkDone();
  const throughput = (performance.now() - t1) / frames;
  const ts = await resolveTimestamps(qs, frames * Q);
  const d = (f, i) => Number(ts[f * Q + 2 * i + 1] - ts[f * Q + 2 * i]) / 1e6;
  const all = [...Array(frames).keys()];
  const frame = all.map(f => Number(ts[f * Q + Q - 1] - ts[f * Q]) / 1e6);
  const out = { frame: r3(median(frame)), frame_p95: r3(pct(frame, 0.95)), throughput_ms: r3(throughput), passes: {}, pass_p95: {} };
  for (const name of [...new Set(pl.passes.map(p => p.name))]) {
    const idx = pl.passes.map((p, i) => p.name === name ? i : -1).filter(i => i >= 0);
    const xs = all.map(f => idx.reduce((acc, i) => acc + d(f, i), 0));   // halves summed per frame
    out.passes[name] = r3(median(xs));
    out.pass_p95[name] = r3(pct(xs, 0.95));
  }
  out.max_submission_ms = r3(Math.max(...pl.passes.map((p, i) => median(all.map(f => d(f, i))))));
  return out;
}

/** Times one pass alone, its inputs filled by one full frame first: 30 untimed, then `frames`. For
 *  sweeps that change only that pass (line width in the edges pass, stroke density in the shading). */
async function measurePass(camName, name, passName, rk = 'full', prm = PRM, frames = 90, warm = 30) {
  const r = res[rk];
  const pl = await ready(passesFor(name));
  writeFrame(CAMS[camName], r, prm);
  submitFrame(pl, r);
  const one = { passes: pl.passes.filter(p => p.name === passName), final: pl.final };
  for (let f = 0; f < warm; f++) submitFrame(one, r);
  await device.queue.onSubmittedWorkDone();
  const Q = 2 * one.passes.length;
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  for (let f = 0; f < frames; f++) submitFrame(one, r, qs, f * Q);
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * Q);
  const xs = [...Array(frames).keys()].map(f => one.passes.reduce((acc, _, i) => acc + Number(ts[f * Q + 2 * i + 1] - ts[f * Q + 2 * i]) / 1e6, 0));
  return { median: r3(median(xs)), p95: r3(pct(xs, 0.95)) };
}

/** 60 Hz pacing by busy-wait, as spikes 01 and 02: GPU frame time when the OS picks the clocks. */
async function paced(camName, name, frames = 150, rk = 'full') {
  const r = res[rk];
  const pl = await ready(passesFor(name));
  const n = pl.passes.length, Q = 2 * n, period = 1000 / 60;
  writeFrame(CAMS[camName], r);
  const qs = device.createQuerySet({ type: 'timestamp', count: frames * Q });
  const t0 = performance.now() + 20;
  for (let f = 0; f < frames; f++) {
    const deadline = t0 + f * period;
    while (performance.now() < deadline) { /* spin */ }
    submitFrame(pl, r, qs, f * Q);
  }
  await device.queue.onSubmittedWorkDone();
  const ts = await resolveTimestamps(qs, frames * Q);
  const gpu = [...Array(frames).keys()].slice(30).map(f => Number(ts[f * Q + Q - 1] - ts[f * Q]) / 1e6);
  return { pacing: 'busy-wait at 60 Hz', resolution: `${r.w}x${r.h}`, frames: gpu.length, frame_median: r3(median(gpu)), frame_p95: r3(pct(gpu, 0.95)),
    frame_max: r3(Math.max(...gpu)), over_16_7: gpu.filter(g => g > 16.7).length };
}

async function stats(camName, name, opts = {}) {
  const r = res.full;
  const pl = await ready(passesFor(name, { ...opts, stats: true }));
  writeFrame(CAMS[camName], r);
  const enc = device.createCommandEncoder();
  enc.clearBuffer(B.stats);
  device.queue.submit([enc.finish()]);
  if (opts.ref) await renderTiled(pl, r);
  else submitFrame(pl, r);
  const s = new Uint32Array(await readBuffer(B.stats, 128));
  const o = {};
  STAT_NAMES.forEach((k, i) => { if (!k.startsWith('_')) o[k] = s[i]; });
  const px = r.w * r.h;
  o.covered_share = r3(o.covered / px);
  o.leaf_share = r3(o.px_leaf / px);
  o.steps_per_px = r3(o.steps / px);
  o.steps_per_leaf_px = r3(o.leaf_steps / Math.max(o.px_leaf, 1));
  o.steps_per_wolf_px = r3(o.wolf_steps / Math.max(o.px_wolf, 1));
  o.cap_share_of_covered = o.caps / Math.max(o.covered, 1);
  o.shadow_steps_per_march = r3(o.sh_steps / Math.max(o.sh_marches, 1));
  o.shadow_cap_share = o.sh_caps / Math.max(o.sh_marches, 1);
  return o;
}

/** Renders a look and reads the final image back as 8-bit display values (RGB). */
async function grab(camName, name, opts = {}, rk = 'full', prm = PRM, camOverride = null) {
  const r = res[rk];
  const pl = await ready(passesFor(name, opts));
  writeFrame(camOverride || CAMS[camName], r, prm);
  const out = device.createBuffer({ size: r.w * r.h * 8, usage: U.COPY_DST | U.MAP_READ });
  if (opts.ref) await renderTiled(pl, r);
  else submitFrame(pl, r);
  const enc = device.createCommandEncoder();
  enc.copyTextureToBuffer({ texture: r.T[pl.final] }, { buffer: out, bytesPerRow: r.w * 8, rowsPerImage: r.h }, [r.w, r.h]);
  device.queue.submit([enc.finish()]);
  await out.mapAsync(GPUMapMode.READ);
  const half = new Uint16Array(out.getMappedRange().slice(0));
  out.unmap();
  out.destroy();
  const px = new Uint8Array(r.w * r.h * 3);
  for (let i = 0, j = 0; i < half.length; i += 4) for (let c = 0; c < 3; c++) px[j++] = BYTE[half[i + c]];
  return { px, w: r.w, h: r.h, final: pl.final, rk };
}

const BYTE = new Uint8Array(65536);
for (let h = 0; h < 65536; h++) {
  const s = h >> 15, e = (h >> 10) & 31, m = h & 1023;
  let v = e === 0 ? m * 2 ** -24 : e === 31 ? 1 : (1 + m / 1024) * 2 ** (e - 15);
  if (s) v = 0;
  BYTE[h] = Math.round(255 * Math.pow(Math.min(Math.max(v, 0), 1), 1 / 2.2));
}

function diff(a, b) {
  let sum = 0, over8 = 0, max = 0;
  const A = a.px, Bp = b.px;
  for (let i = 0; i < A.length; i += 3) {
    let m = 0;
    for (let c = 0; c < 3; c++) { const d = Math.abs(A[i + c] - Bp[i + c]); sum += d; if (d > m) m = d; }
    if (m > 8) over8++;
    if (m > max) max = m;
  }
  const n = A.length / 3;
  return { mean_abs_255: r3(sum / A.length), share_over_8: Number((over8 / n).toFixed(5)), max };
}

/** Where two images differ: red over 8/255, yellow over 2/255, on a dimmed copy of the first. */
function diffImage(a, b) {
  const px = new Uint8Array(a.px.length);
  for (let i = 0; i < a.px.length; i += 3) {
    let m = 0;
    for (let c = 0; c < 3; c++) m = Math.max(m, Math.abs(a.px[i + c] - b.px[i + c]));
    const base = (a.px[i] + a.px[i + 1] + a.px[i + 2]) / 9;
    px[i] = m > 2 ? 255 : base; px[i + 1] = m > 8 ? 0 : (m > 2 ? 220 : base); px[i + 2] = m > 2 ? 0 : base;
  }
  return { ...a, px };
}

/** Line-only images (white, black lines): coverage error inside the union of either image's lines. */
function lineDiff(test, ref) {
  let uni = 0, sum = 0, inter = 0, union5 = 0, falsePx = 0, missed = 0;
  for (let i = 1; i < test.px.length; i += 3) {
    const a = 1 - test.px[i] / 255, b = 1 - ref.px[i] / 255;
    if (a > 0.02 || b > 0.02) { uni++; sum += Math.abs(a - b); }
    const A = a > 0.5, Bm = b > 0.5;
    if (A || Bm) union5++;
    if (A && Bm) inter++;
    if (A && !Bm) falsePx++;
    if (!A && Bm) missed++;
  }
  return { union_px: uni, mean_coverage_error: r3(sum / Math.max(uni, 1)), iou: r3(inter / Math.max(union5, 1)),
    false_share: r3(falsePx / Math.max(union5, 1)), missed_share: r3(missed / Math.max(union5, 1)) };
}

async function savePNG(name, img, crop = null) {
  const [x0, y0, w, h] = crop || [0, 0, img.w, img.h];
  const c = document.createElement('canvas');
  c.width = w;
  c.height = h;
  const g = c.getContext('2d');
  const id = g.createImageData(w, h);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const s = ((y + y0) * img.w + (x + x0)) * 3, d = (y * w + x) * 4;
      id.data[d] = img.px[s]; id.data[d + 1] = img.px[s + 1]; id.data[d + 2] = img.px[s + 2]; id.data[d + 3] = 255;
    }
  }
  g.putImageData(id, 0, 0);
  const blob = await new Promise(r => c.toBlob(r, 'image/png'));
  if (blob) await save(`${name}.png`, blob);
}

/** Flicker under a sideways strafe at `speed` px/frame (at the wolf's distance): frame B warped into A. */
async function flicker(camName, name, speed, prm = PRM, pairs = 6) {
  const r = res.full;
  const cam = CAMS[camName];
  const pl = await ready(passesFor(name));
  const fp = await pipeline('flicker', {});
  const b = camBasis(cam.eye, cam.target, r);
  const stepM = speed * cam.subject * b.pa;
  const at = f => ({ eye: add(cam.eye, scale(b.right, stepM * f)), target: add(cam.target, scale(b.right, stepM * f)) });
  let valid = 0, sum = 0, over8 = 0;
  const copyA = enc => {
    enc.copyTextureToTexture({ texture: r.T.gb }, { texture: r.T.gbA }, [r.w, r.h]);
    enc.copyTextureToTexture({ texture: r.T[pl.final] }, { texture: r.T.colA }, [r.w, r.h]);
  };
  writeFrame(at(0), r, prm);
  submitFrame(pl, r, null, 0, null, copyA);
  for (let f = 1; f <= pairs; f++) {
    writeFrame(at(f), r, prm, at(f - 1));
    submitFrame(pl, r, null, 0, enc => enc.clearBuffer(B.fl), enc => {
      const c = enc.beginComputePass();
      c.setPipeline(fp);
      c.setBindGroup(0, r.bg[`flicker_${pl.final}`]);
      c.dispatchWorkgroups(Math.ceil(r.w / 8), Math.ceil(r.h / 8));
      c.end();
      copyA(enc);
    });
    const s = new Uint32Array(await readBuffer(B.fl, 32));
    valid += s[0]; sum += s[1] / 16; over8 += s[2];
  }
  return { mean_warp_error_255: r3(sum / Math.max(valid, 1)), share_over_8: Number((over8 / Math.max(valid, 1)).toFixed(5)),
    valid_share: r3(valid / (pairs * r.w * r.h)) };
}

// ---- runs ------------------------------------------------------------------------------------------------------

/** Creates the pipelines two at a time: each heavy kernel spawns a Metal compiler process, and many
 *  at once starve the rest of the machine (spikes/README.md, GPU safety rules). */
async function compileAll(list) {
  const t0 = performance.now();
  const want = [];
  const seen = new Set();
  for (const [name, opts] of list) {
    for (const p of passesFor(name, opts).passes) {
      const k = p.kind + JSON.stringify(p.consts);
      if (!seen.has(k)) { seen.add(k); want.push([p.kind, p.consts]); }
    }
  }
  want.push(['flicker', {}]);
  for (let i = 0; i < want.length; i += 2) await Promise.all(want.slice(i, i + 2).map(([k, c]) => pipeline(k, c)));
  return r3(performance.now() - t0);
}

// What the run measures. Frames here cost 37–130 ms at 1080p (the baseline isn't in budget), so the
// matrix is trimmed to keep a run near ~4 minutes: the screen-space and content variants go where
// they answer something (simple content where foliage or fur dominate).
const PLAN = {
  full: { wide: ['real', 'cel', 'cel_ss', 'paint', 'paint_kuw'],
    close: ['real', 'cel', 'cel_ss', 'paint', 'paint_kuw', 'real_simple', 'cel_simple'],
    canopy: MAIN },
  half: { wide: ['real', 'cel', 'paint'], close: ['real', 'cel', 'paint'], canopy: ['real', 'cel', 'paint', 'paint_simple'] },
  flickerLooks: ['real', 'cel', 'cel_ss', 'paint', 'paint_kuw', 'paint_simple'],
};

function allPipelineList() {
  const list = [];
  for (const v of MAIN) list.push([v, {}], [v, { stats: true }], [v, { stats: true, ref: true }]);
  list.push(['cel_disk', {}], ['paint_nohalo', {}]);
  for (const v of ['cel', 'cel_ss', 'cel_disk']) list.push([v, { view: 1 }]);
  list.push(['cel', { view: 1, ref: true }], ['real', { view: 2 }], ['real_simple', { view: 2 }]);
  return list;
}

/** A look with its stats: one render gives the image and the counters (stats variants draw the same image). */
async function grabStats(camName, name, opts = {}) {
  const enc = device.createCommandEncoder();
  enc.clearBuffer(B.stats);
  device.queue.submit([enc.finish()]);
  const img = await grab(camName, name, { ...opts, stats: true });
  const s = new Uint32Array(await readBuffer(B.stats, 128));
  const o = {};
  STAT_NAMES.forEach((k, i) => { if (!k.startsWith('_')) o[k] = s[i]; });
  const px = img.w * img.h;
  o.covered_share = r3(o.covered / px);
  o.leaf_share = r3(o.px_leaf / px);
  o.wolf_share = r3(o.px_wolf / px);
  o.steps_per_px = r3(o.steps / px);
  o.steps_per_leaf_px = r3(o.leaf_steps / Math.max(o.px_leaf, 1));
  o.steps_per_wolf_px = r3(o.wolf_steps / Math.max(o.px_wolf, 1));
  o.cap_share_of_covered = Number((o.caps / Math.max(o.covered, 1)).toFixed(6));
  o.shadow_steps_per_march = r3(o.sh_steps / Math.max(o.sh_marches, 1));
  o.shadow_cap_share = Number((o.sh_caps / Math.max(o.sh_marches, 1)).toFixed(6));
  return { img, stats: o };
}

async function runAll() {
  $('run').disabled = true;
  const T0 = performance.now();
  const lap = what => log(`[${((performance.now() - T0) / 1000).toFixed(1)} s] ${what}`);
  const crop = [560, 300, 800, 450];
  try {
    await init();
    log('device', JSON.stringify(R.device));
    R.pipelines.compile_two_at_a_time_ms = await compileAll(allPipelineList());
    R.pipelines.count = PL.size;
    lap(`compiled ${PL.size} pipelines in ${R.pipelines.compile_two_at_a_time_ms} ms`);

    // 1. GPU time per pass: 30 warm-up frames, then 90 back to back, per look, camera and resolution.
    // The first configuration after compiling read high in a trial run, so one is run and discarded.
    await measure('wide', 'real', 'full');
    for (const cam of Object.keys(CAMS)) {
      for (const rk of ['full', 'half']) {
        for (const v of PLAN[rk][cam]) R.frames[`${cam}/${v}/${rk}`] = await measure(cam, v, rk);
        const base = R.frames[`${cam}/real/${rk}`].frame;
        log(`${cam} ${rk}: ` + PLAN[rk][cam].map(v => { const f = R.frames[`${cam}/${v}/${rk}`].frame; return `${v} ${f} (${f - base >= 0 ? '+' : ''}${r3(f - base)})`; }).join(', '));
      }
      lap(`timed ${cam}`);
    }

    // 2. Correctness against the brute-force references (rendered in tiles), stats, screenshots.
    for (const cam of Object.keys(CAMS)) {
      R.stats[cam] = {};
      R.quality[cam] = {};
      for (const v of PLAN.full[cam]) {
        const fast = await grabStats(cam, v);
        const ref = await grabStats(cam, v, { ref: true });
        R.stats[cam][v] = fast.stats;
        R.stats[cam][`${v}/ref`] = { caps: ref.stats.caps, cap_share_of_covered: ref.stats.cap_share_of_covered, sh_caps: ref.stats.sh_caps, shadow_cap_share: ref.stats.shadow_cap_share };
        R.quality[cam][v] = diff(fast.img, ref.img);
        await savePNG(`${cam}-${v}`, fast.img);
        if (v === 'real' || v === 'cel' || v === 'paint') {
          await savePNG(`${cam}-${v}-ref`, ref.img);
          await savePNG(`${cam}-${v}-diff`, diffImage(fast.img, ref.img));
        }
      }
      await savePNG(`${cam}-heat`, await grab(cam, 'real', { view: 2 }));
      await savePNG(`${cam}-heat-simple`, await grab(cam, 'real_simple', { view: 2 }));
      log(`${cam} quality ` + JSON.stringify(R.quality[cam]));
      lap(`checked ${cam}`);
    }

    // 3. Outline width on `wide`: what each method costs, and its error against exact lines.
    R.sweeps.outline = { trace_without_lines: await measurePass('wide', 'cel_ss', 'trace') };
    for (const W of [1, 2, 4, 6]) {
      const prm = { ...PRM, W };
      const row = { edges_sobel: await measurePass('wide', 'cel_ss', 'edges', 'full', prm), edges_disk: await measurePass('wide', 'cel_disk', 'edges', 'full', prm) };
      if (W === 6) row.trace_with_lines = await measurePass('wide', 'cel', 'trace', 'full', prm);
      const exact = await grab('wide', 'cel', { view: 1, ref: true }, 'full', prm);
      const q = {};
      for (const v of ['cel', 'cel_ss', 'cel_disk']) {
        const img = await grab('wide', v, { view: 1 }, 'full', prm);
        q[v] = lineDiff(img, exact);
        if (W === 2) await savePNG(`lines-${v}`, img, crop);
      }
      if (W === 2) await savePNG('lines-exact', exact, crop);
      R.sweeps.outline[W] = { cost: row, quality: q };
      log(`outline W=${W} ` + JSON.stringify(R.sweeps.outline[W]));
    }
    for (const W of [1, 4]) await savePNG(`wide-cel-W${W}`, await grab('wide', 'cel', {}, 'full', { ...PRM, W }), crop);
    lap('outline sweep');

    // 4. Stroke size (density) on `wide`: shading cost and flicker; and what the overshoot costs.
    R.sweeps.strokes = {};
    for (const stroke of [32, 16, 12, 8, 4]) {
      const prm = { ...PRM, stroke, halo: 1.5 * stroke };
      R.sweeps.strokes[stroke] = { shade: await measurePass('wide', 'paint', 'shade', 'full', prm), flicker_2px: await flicker('wide', 'paint', 2, prm) };
      if (stroke !== 12) await savePNG(`wide-paint-s${stroke}`, await grab('wide', 'paint', {}, 'full', prm), crop);
      log(`strokes ${stroke}px ` + JSON.stringify(R.sweeps.strokes[stroke]));
    }
    R.sweeps.halo = {};
    for (const cam of Object.keys(CAMS)) {
      R.sweeps.halo[cam] = { paint_shade: await measurePass(cam, 'paint', 'shade'), no_overshoot_shade: await measurePass(cam, 'paint_nohalo', 'shade') };
    }
    log('overshoot ' + JSON.stringify(R.sweeps.halo));
    lap('stroke sweep');

    // 5. Flicker under camera motion.
    for (const cam of ['wide', 'canopy']) {
      for (const speed of [0.5, 2, 8]) {
        for (const v of PLAN.flickerLooks) R.flicker[`${cam}/${v}/${speed}`] = await flicker(cam, v, speed);
        log(`flicker ${cam} ${speed}px: ` + PLAN.flickerLooks.map(v => { const f = R.flicker[`${cam}/${v}/${speed}`]; return `${v} ${f.mean_warp_error_255}/${f.share_over_8}`; }).join(', '));
      }
    }
    lap('flicker');

    // 6. Paced at 60 Hz (busy-wait), both resolutions.
    for (const rk of ['full', 'half']) for (const v of ['real', 'cel', 'paint']) R.paced[`wide/${v}/${rk}`] = await paced('wide', v, 90, rk);
    log('paced ' + JSON.stringify(R.paced));
    lap('paced');

    // 7. 960×540 screenshots, stretched by the blit only (no upscaler).
    for (const v of ['cel', 'paint']) await savePNG(`wide-${v}-540`, await grab('wide', v, {}, 'half'));

    R.finished = new Date().toISOString();
    R.run_seconds = r3((performance.now() - T0) / 1000);
    await save(`run-${R.started.replace(/[:.]/g, '-')}.json`, JSON.stringify(R, null, 2));
    lap('saved');
  } catch (e) {
    console.error('FAILED:', e.stack || e.message);
    R.error = String(e.stack || e.message);
    await save(`run-${R.started.replace(/[:.]/g, '-')}-failed.json`, JSON.stringify(R, null, 2));
  }
  await save('DONE', 'done');
  $('run').disabled = false;
}

/** Every look on every camera once, screenshots and a short timing each. */
async function quick() {
  const T0 = performance.now();
  try {
    await init();
    log('device', JSON.stringify(R.device));
    // '#quick' or '#quick=looks:real,cel;cams:wide'
    const opt = Object.fromEntries((location.hash.split('=')[1] || '').split(';').filter(Boolean).map(kv => kv.split(':')));
    const looks = opt.looks ? opt.looks.split(',') : ['real', 'cel', 'paint'];
    const cams = opt.cams ? opt.cams.split(',') : Object.keys(CAMS);
    const frames = +(opt.frames || 5);
    const ms = await compileAll(looks.flatMap(v => [[v, {}], ...(opt.stats ? [[v, { stats: true }]] : []), ...(opt.heat ? [[v, { view: 2 }]] : []), ...(opt.ref ? [[v, { ref: true }]] : []), ...(opt.dbg ? opt.dbg.split(',').map(k => [v, { view: +k }]) : [])]));
    log(`compiled ${PL.size} pipelines in ${ms} ms`);
    for (const cam of cams) {
      for (const v of looks) {
        const img = await grab(cam, v);
        await savePNG(`q-${cam}-${v}`, img);
        const m = await measure(cam, v, 'full', PRM, frames, +(opt.warm || Math.min(frames, 5)));
        log(`${cam}/${v}: frame ${m.frame} ms (p95 ${m.frame_p95}) ` + JSON.stringify(m.passes));
        if (opt.stats) log(`${cam}/${v} stats ` + JSON.stringify(await stats(cam, v)));
        if (opt.ref) {
          const t = performance.now();
          const ref = await grab(cam, v, { ref: true });
          log(`${cam}/${v} vs reference ` + JSON.stringify(diff(img, ref)) + ` (tiled reference took ${r3(performance.now() - t)} ms)`);
          await savePNG(`q-${cam}-${v}-ref`, ref);
          await savePNG(`q-${cam}-${v}-diff`, diffImage(img, ref));
        }
        if (opt.heat) await savePNG(`q-${cam}-${v}-heat`, await grab(cam, v, { view: 2 }));
        if (opt.dbg) for (const k of opt.dbg.split(',')) await savePNG(`q-${cam}-${v}-view${k}`, await grab(cam, v, { view: +k }));
        if (opt.shade) log(`${cam}/${v} shade pass alone: ` + JSON.stringify(await measurePass(cam, v, 'shade', 'full', PRM, 30, 10)));
      }
    }
    const pl = await ready(passesFor('real'));
    writeFrame(CAMS.wide, res.full);
    submitFrame(pl, res.full);
    present(res.full, pl.final);
    if (opt.profile) {
      // cost attribution: the same look with scene components left out
      for (const cam of cams) {
        for (const off of [1, 2, 4 | 8, 15]) {
          const pl = await ready(passesFor('real', { off }));
          const r = res.full;
          writeFrame(CAMS[cam], r);
          const qs = device.createQuerySet({ type: 'timestamp', count: 5 * 2 * pl.passes.length });
          for (let f = 0; f < 5; f++) submitFrame(pl, r, f ? qs : null, (f - 1) * 2 * pl.passes.length);
          await device.queue.onSubmittedWorkDone();
          const ts = await resolveTimestamps(qs, 5 * 2 * pl.passes.length);
          const Q = 2 * pl.passes.length;
          const per = pl.passes.map((p, i) => `${p.name} ${r3(median([1, 2, 3].map(f => Number(ts[f * Q + 2 * i + 1] - ts[f * Q + 2 * i]) / 1e6)))}`);
          log(`${cam}/real without ${({ 1: 'trees', 2: 'wolf', 12: 'tower+rocks', 15: 'all objects' })[off]}: ${per.join(', ')}`);
        }
      }
    }
    log(`quick done in ${((performance.now() - T0) / 1000).toFixed(1)} s`);
  } catch (e) {
    console.error('FAILED:', e.stack || e.message);
  }
  await save('DONE', 'done');
}

window.__grab = grab; window.__measure = measure; window.__stats = stats; window.__flicker = flicker; window.__savePNG = savePNG;
$('run').addEventListener('click', runAll);
if (location.hash === '#run') runAll();
if (location.hash.startsWith('#quick')) quick();
