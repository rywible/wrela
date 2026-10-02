// The lens engine: one lifted field on the GPU, and every operation of the spike as a plain async
// call (render, query, probe, ray, move, fit, set, writeBack). The page UI, the #run tests and the
// agent API all call these. Throwaway spike code; see README.md.

import { lift, preamble, writeBack, diffLines, verifyWriteBack, formatLiteral, tokenize } from './lift.js';

export const PSTRIDE = 32;
export const DEFAULT_FRAME = { center: [0, 0.52, 0.04], radius: 0.86 };
export const MODES = { shaded: 0, parts: 1, silhouette: 2, influence: 3, tint: 4, isolate: 0 };
const FD_H = 2e-4;

// ---- small vector helpers ----
export const v3 = {
  add: (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]],
  sub: (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]],
  mul: (a, s) => [a[0] * s, a[1] * s, a[2] * s],
  dot: (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2],
  cross: (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]],
  len: a => Math.hypot(a[0], a[1], a[2]),
  norm: a => { const l = Math.hypot(a[0], a[1], a[2]) || 1; return [a[0] / l, a[1] / l, a[2] / l]; },
  dist: (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]),
};
const smoothstep = (a, b, x) => { const t = Math.min(1, Math.max(0, (x - a) / (b - a))); return t * t * (3 - 2 * t); };
const r4 = x => Math.round(x * 1e4) / 1e4;
const r6 = x => Math.round(x * 1e6) / 1e6;

// ---- views ----
function orthoView(name, c, f, upHint, half) {
  const r = v3.norm(v3.cross(f, upHint));
  const u = v3.cross(r, f);
  return { name, ortho: true, o: c, f, r, u, half };
}
function perspView(name, eye, at, tanHalf) {
  const f = v3.norm(v3.sub(at, eye));
  const r = v3.norm(v3.cross(f, [0, 1, 0]));
  const u = v3.cross(r, f);
  return { name, ortho: false, o: eye, f, r, u, tanHalf };
}

/** The standard views: side (from +x, nose left), front (from +z), three-quarter (perspective), top. */
export function makeView(name, frame = DEFAULT_FRAME) {
  if (typeof name === 'object') return name.f ? name : customView(name, frame);
  const c = frame.center, R = frame.radius;
  switch (name) {
    case 'side': return orthoView('side', c, [-1, 0, 0], [0, 1, 0], R);
    case 'front': return orthoView('front', c, [0, 0, -1], [0, 1, 0], R);
    case 'top': return orthoView('top', c, [0, -1, 0], [0, 0, -1], R);
    case 'threeq': { const d = 2.8 * R; return perspView('threeq', [c[0] + 0.62 * d, c[1] + 0.45 * d, c[2] + 0.62 * d], c, 1 / 2.8); }
    default: throw new Error(`unknown view '${name}' (side, front, top, threeq, sheet, or {dir, center, half})`);
  }
}
function customView(spec, frame) {
  const c = spec.center || frame.center;
  if (spec.eye) return perspView('custom', spec.eye, c, spec.tanHalf || 1 / 2.8);
  const f = v3.norm(spec.dir || [-1, 0, 0]);
  const up = Math.abs(f[1]) > 0.9 ? [0, 0, -1] : [0, 1, 0];
  return orthoView('custom', c, f, spec.up || up, spec.half || frame.radius);
}

/** Lay out a view (or the 2x2 sheet) in an image of `size` pixels. */
export function makeLayout(view = 'side', size = 512, frame = DEFAULT_FRAME) {
  if (view === 'sheet') {
    const t = Math.floor(size / 2);
    const vs = [['side', 0, 0], ['front', t, 0], ['threeq', 0, t], ['top', t, t]];
    return { name: 'sheet', tile: t, width: 2 * t, height: 2 * t, views: vs.map(([n, x0, y0]) => ({ ...makeView(n, frame), x0, y0 })) };
  }
  const v = makeView(view, frame);
  return { name: v.name, tile: size, width: size, height: size, views: [{ ...v, x0: 0, y0: 0 }] };
}

export function pixelRay(lay, px, py) {
  const v = lay.views.find(v => px >= v.x0 && px < v.x0 + lay.tile && py >= v.y0 && py < v.y0 + lay.tile) || lay.views[0];
  const uv = [(px - v.x0) / lay.tile * 2 - 1, (py - v.y0) / lay.tile * 2 - 1];
  return viewRay(v, uv);
}
export function viewRay(v, uv) {
  if (v.ortho) {
    const o = v3.sub(v3.add(v3.add(v.o, v3.mul(v.r, uv[0] * v.half)), v3.mul(v.u, -uv[1] * v.half)), v3.mul(v.f, 6));
    return { o, d: v.f, tmax: 12, view: v };
  }
  const d = v3.norm(v3.add(v3.add(v3.mul(v.f, 1 / v.tanHalf), v3.mul(v.r, uv[0])), v3.mul(v.u, -uv[1])));
  return { o: v.o, d, tmax: 40, view: v };
}
/** World point -> pixel in a layout (the tile of view `name`, or the first). */
export function project(lay, p, name = null) {
  const v = name ? lay.views.find(x => x.name === name) : lay.views[0];
  let u, w;
  if (v.ortho) {
    const q = v3.sub(p, v.o);
    u = v3.dot(q, v.r) / v.half; w = -v3.dot(q, v.u) / v.half;
  } else {
    const q = v3.sub(p, v.o), z = v3.dot(q, v.f);
    u = v3.dot(q, v.r) / z / v.tanHalf; w = -v3.dot(q, v.u) / z / v.tanHalf;
  }
  return [v.x0 + (u + 1) / 2 * lay.tile, v.y0 + (w + 1) / 2 * lay.tile];
}
export function metresPerPixel(lay) { const v = lay.views[0]; return v.ortho ? 2 * v.half / lay.tile : null; }

// ---- linear algebra (small dense) ----
/** Solve A x = b for symmetric positive (semi)definite A (n x n, row-major), by Gaussian elimination with partial pivoting. */
export function solve(A, b, n) {
  const M = new Float64Array(n * (n + 1));
  for (let i = 0; i < n; i++) { for (let j = 0; j < n; j++) M[i * (n + 1) + j] = A[i * n + j]; M[i * (n + 1) + n] = b[i]; }
  for (let c = 0; c < n; c++) {
    let p = c;
    for (let r = c + 1; r < n; r++) if (Math.abs(M[r * (n + 1) + c]) > Math.abs(M[p * (n + 1) + c])) p = r;
    if (p !== c) for (let j = 0; j <= n; j++) { const t = M[c * (n + 1) + j]; M[c * (n + 1) + j] = M[p * (n + 1) + j]; M[p * (n + 1) + j] = t; }
    const d = M[c * (n + 1) + c];
    if (Math.abs(d) < 1e-300) continue;
    for (let r = c + 1; r < n; r++) {
      const f = M[r * (n + 1) + c] / d;
      if (f === 0) continue;
      for (let j = c; j <= n; j++) M[r * (n + 1) + j] -= f * M[c * (n + 1) + j];
    }
  }
  const x = new Float64Array(n);
  for (let i = n - 1; i >= 0; i--) {
    let s = M[i * (n + 1) + n];
    for (let j = i + 1; j < n; j++) s -= M[i * (n + 1) + j] * x[j];
    const d = M[i * (n + 1) + i];
    x[i] = Math.abs(d) < 1e-300 ? 0 : s / d;
  }
  return x;
}

// ---- GPU context shared by every lens on the page ----
export class LensGPU {
  static async create() {
    if (!navigator.gpu) throw new Error('WebGPU is not available');
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
    const feats = ['timestamp-query'].filter(f => adapter.features.has(f));
    const device = await adapter.requestDevice({
      requiredFeatures: feats,
      requiredLimits: { maxStorageBufferBindingSize: Math.min(adapter.limits.maxStorageBufferBindingSize, 256 << 20), maxBufferSize: Math.min(adapter.limits.maxBufferSize, 256 << 20) },
    });
    const g = new LensGPU();
    g.device = device;
    g.adapterInfo = adapter.info || {};
    g.timestamp = feats.includes('timestamp-query');
    device.lost.then(i => console.error('DEVICE LOST:', i.message));
    device.addEventListener('uncapturederror', e => console.error('GPU ERROR:', e.error.message));
    g.lib = await (await fetch('lib.wgsl')).text();
    g.kernels = await (await fetch('lens.wgsl')).text();
    const C = GPUShaderStage.COMPUTE;
    g.bgl = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: C, buffer: { type: 'uniform' } },
        { binding: 1, visibility: C, buffer: { type: 'uniform' } },
        { binding: 2, visibility: C, buffer: { type: 'read-only-storage' } },
        { binding: 3, visibility: C, buffer: { type: 'storage' } },
        { binding: 4, visibility: C, storageTexture: { access: 'write-only', format: 'rgba8unorm' } },
        { binding: 5, visibility: C, buffer: { type: 'storage' } },
        { binding: 6, visibility: C, buffer: { type: 'read-only-storage' } },
      ],
    });
    g.layout = device.createPipelineLayout({ bindGroupLayouts: [g.bgl] });
    // The library's own float literals are not lifted (shared code); count them for the report.
    g.libFloatLiterals = tokenize(g.lib).filter(t => t.t === 'num' && /[.eE]/.test(t.text) && !/^0[xX]/.test(t.text)).length;
    return g;
  }
}

// ---- one lens: a lifted field file on the GPU ----
export class Lens {
  constructor(gpu) {
    this.gpu = gpu;
    this.device = gpu.device;
    this.frame = DEFAULT_FRAME;
    this.texCache = new Map();
    this.version = 0;
    this.history = [];
  }

  /** Lift `src`, compile every kernel, upload the literal values. `lifted: false` compiles the source as written (a reference). */
  async load(src, { name = 'subject', entry = 'field', lifted = true, salt: salt_ = false, ...opts } = {}) {
    const dev = this.device;
    const t0 = performance.now();
    this.L = lift(src, { entry, name });
    const tLift = performance.now() - t0;
    this.name = name;
    this.lifted = lifted;
    this.src = src;
    // Two modules from the same lifted text. 'fast': each literal is a static read of the parameter
    // uniform. 'fd': the same plus a per-invocation perturbation of one literal (finite differences).
    // Literals are read from a read-only storage buffer: the same reads from a uniform buffer made the
    // wolf's render ~12x slower on the M4 in Chrome, storage ~1.5x (spike 09 README, #perf).
    const subject = lifted ? this.L.shader : src;
    const read = opts.uniform ? (id => `lens_lits.v[${id >> 2}].${'xyzw'[id & 3]}`) : (id => `lens_litbuf[${id}]`);
    const salt = `\nconst LENS_SALT: f32 = ${salt_ ? Math.random().toFixed(8) : '0.0'};\n`;
    let pre = preamble(this.L);
    if (opts.noTapStore) pre = pre.replace('{ lens_tap_v[k] = v; return v; }', '{ return v; }');
    let subj0 = subject;
    if (opts.unrollLits) subj0 = subject.replace(/lens_lit\((\d+)\)/g, (_, id) => { const v = this.L.literals[Number(id)].value; return `f32(${v.toPrecision(9)})`; });
    const tail = '\n' + pre + '\n' + this.gpu.lib + '\n' + this.gpu.kernels + salt;
    const codeFast = subj0.replace(/lens_lit\((\d+)\)/g, (_, id) => read(Number(id))) + tail;
    const codeFd = subject.replace(/lens_lit\((\d+)\)/g, (_, id) => `(${read(Number(id))} + select(0.0, lens_dh, lens_di == ${id}))`) + tail;
    this.code = codeFast;
    const t1 = performance.now();
    const mFast = dev.createShaderModule({ code: codeFast, label: name + ' fast' });
    const mFd = dev.createShaderModule({ code: codeFd, label: name + ' fd' });
    const list = [['render', mFast, 'lens_render'], ['silmin', mFast, 'lens_silmin'], ['probe', mFast, 'lens_probe'],
      ['grad', mFd, 'lens_grad'], ['renderFd', mFd, 'lens_render'], ['grid', mFast, 'lens_gridk']];
    let pipes = [];
    try {
      // One at a time: parallel compiles spawn a Metal compiler process each (GPU safety rules).
      for (const [, module, ep] of list) pipes.push(await dev.createComputePipelineAsync({ layout: this.gpu.layout, compute: { module, entryPoint: ep }, label: ep }));
    } catch (err) {
      const msgs = [];
      for (const m of [mFast, mFd]) {
        const info = await m.getCompilationInfo();
        const subjLines = this.L.src.split('\n').length;
        for (const x of info.messages) msgs.push(`${x.type} ${x.lineNum <= subjLines ? `${name}:${x.lineNum}` : `(generated):${x.lineNum}`}:${x.linePos} ${x.message}`);
      }
      msgs.forEach(m => console.error(m));
      throw new Error(`${name}: WGSL errors\n${msgs.join('\n')}`);
    }
    this.compileMs = performance.now() - t1;
    this.pipe = Object.fromEntries(list.map(([n], i) => [n, pipes[i]]));
    const U = GPUBufferUsage;
    const nv = Math.max(1, Math.ceil(this.L.literals.length / 4));
    this.uni = dev.createBuffer({ size: 384, usage: U.UNIFORM | U.COPY_DST });
    this.lits = dev.createBuffer({ size: nv * 16, usage: U.UNIFORM | U.COPY_DST });
    this.litbuf = dev.createBuffer({ size: nv * 16, usage: U.STORAGE | U.COPY_DST });
    this.inb = dev.createBuffer({ size: 1 << 20, usage: U.STORAGE | U.COPY_DST });
    this.outb = dev.createBuffer({ size: 16 << 20, usage: U.STORAGE | U.COPY_SRC | U.COPY_DST });
    this.pts = dev.createBuffer({ size: 16 << 20, usage: U.STORAGE | U.COPY_SRC | U.COPY_DST });
    this.base = Float64Array.from(this.L.literals.map(l => l.value));
    this.values = Float64Array.from(this.base);
    this.fieldIds = this.L.literals.filter(l => l.inField).map(l => l.id);
    this.upload();
    this.texFor(64);
    await this.computeBounds();
    return { liftMs: tLift, compileMs: this.compileMs, totalMs: performance.now() - t0 };
  }

  /** Padded axis-aligned bounds of the creature, from coarse side and front silhouettes: rays outside skip the march. */
  async computeBounds(pad = 0.1, res = 96) {
    this.bounds = null;
    const ext = async view => {
      const s = await this.silhouette(view, res);
      const v = makeView(view, this.frame);
      let x0 = res, x1 = -1, y0 = res, y1 = -1;
      for (let y = 0; y < res; y++) for (let x = 0; x < res; x++) if (s.m[y * res + x] < 0.02) { x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y); }
      if (x1 < 0) return null;
      const px = 2 * v.half / res;
      const at = (x, y) => v3.add(v3.add(v.o, v3.mul(v.r, (x / res * 2 - 1) * v.half)), v3.mul(v.u, -(y / res * 2 - 1) * v.half));
      return { a: at(x0, y1 + 1), b: at(x1 + 1, y0), px };
    };
    const side = await ext('side'), front = await ext('front');
    if (!side || !front) return;
    const zs = [side.a[2], side.b[2]], ys = [side.a[1], side.b[1], front.a[1], front.b[1]], xs = [front.a[0], front.b[0]];
    this.bounds = { min: [Math.min(...xs) - pad, Math.min(...ys) - pad, Math.min(...zs) - pad], max: [Math.max(...xs) + pad, Math.max(...ys) + pad, Math.max(...zs) + pad] };
  }

  /**
   * Separate pieces: inside cells (d < 0) of a grid over the bounds, 6-connected, like fieldview's
   * diagnostic. Cell 8 mm by default; pieces thinner than a cell can be missed.
   */
  async pieces(cell = 0.008) {
    if (this._pieces && this._pieces.version === this.version && this._pieces.cell === cell) return this._pieces;
    const t0 = performance.now();
    const b = this.bounds || { min: [-0.4, -0.05, -1], max: [0.4, 1.3, 1] };
    const lo = [b.min[0], Math.max(-0.02, b.min[1]), b.min[2]];
    const n = [0, 1, 2].map(a => Math.ceil((b.max[a] - lo[a]) / cell));
    const total = n[0] * n[1] * n[2];
    if (total * 4 > 16 << 20) throw new Error('pieces: grid too large');
    const v = { o: lo, f: n, r: [1, 0, 0], u: [0, 1, 0], ortho: false, tanHalf: 1 / cell, x0: 0, y0: 0 };
    const CH = 1 << 20;   // GPU safety: ~1M field evaluations per submission
    for (let c0 = 0; c0 < total; c0 += CH) {
      const m = Math.min(CH, total - c0);
      this.writeUniform({ views: [v], rect: [c0, 0, m, 0] });
      const enc = this.device.createCommandEncoder();
      const pass = enc.beginComputePass();
      pass.setPipeline(this.pipe.grid);
      pass.setBindGroup(0, this.texFor(64).bg);
      this.dispatch1D(pass, m);
      pass.end();
      this.device.queue.submit([enc.finish()]);
      await this.device.queue.onSubmittedWorkDone();
    }
    const d = await this.readBuffers(this.device.createCommandEncoder(), [{ buf: this.outb, offset: 0, size: total * 4 }]);
    const label = new Int32Array(total);
    const sizes = [];
    const stack = new Int32Array(total);
    const nx = n[0], nxy = n[0] * n[1];
    for (let i = 0; i < total; i++) {
      if (d[i] >= 0 || label[i]) continue;
      const id = sizes.length + 1;
      let sp = 0, count = 0;
      stack[sp++] = i; label[i] = id;
      while (sp) {
        const j = stack[--sp];
        count++;
        const x = j % nx, y = Math.floor(j / nx) % n[1], z = Math.floor(j / nxy);
        const nb = [x > 0 ? j - 1 : -1, x < nx - 1 ? j + 1 : -1, y > 0 ? j - nx : -1, y < n[1] - 1 ? j + nx : -1, z > 0 ? j - nxy : -1, z < n[2] - 1 ? j + nxy : -1];
        for (const k of nb) if (k >= 0 && !label[k] && d[k] < 0) { label[k] = id; stack[sp++] = k; }
      }
      sizes.push(count);
    }
    sizes.sort((a, b2) => b2 - a);
    this._pieces = { version: this.version, cell, count: sizes.length, cellsPerPiece: sizes.slice(0, 8), volumeM3: r6(sizes.reduce((s2, x) => s2 + x, 0) * cell ** 3), ms: r4(performance.now() - t0) };
    return this._pieces;
  }

  destroy() {
    for (const b of [this.uni, this.lits, this.inb, this.outb, this.pts]) b && b.destroy();
    for (const t of this.texCache.values()) t.tex.destroy();
  }

  // ---- literal state ----
  upload() {
    const f = new Float32Array(Math.ceil(this.values.length / 4) * 4 || 4);
    f.set(this.values);
    this.device.queue.writeBuffer(this.lits, 0, f);
    this.device.queue.writeBuffer(this.litbuf, 0, f);
    this.version++;
  }
  setValues(vals) { this.values = Float64Array.from(vals); this.upload(); }
  reset() { this.values = Float64Array.from(this.base); this.upload(); }
  changed() {
    const out = [];
    for (let i = 0; i < this.values.length; i++) if (this.values[i] !== this.base[i]) out.push(i);
    return out;
  }
  literalInfo(id, extra = {}) {
    const l = this.L.literals[id];
    return { id, name: l.name, value: r6(this.values[id]), source_value: l.value, text: l.text, line: l.line, col: l.col, fn: l.fn, const: l.constName, sites: l.sites, comment: l.comment || undefined, ...extra };
  }

  /**
   * Resolve literal selectors: an id; a name ('J_HEAD.y', 'EAR_H', 'sd_neck:105:42'); 'const:J_HEAD';
   * 'fn:sd_neck'; 'part:sd_head'; '@105' (every literal on line 105); '@105:0.122' (by line and text).
   */
  resolve(sel, { fieldOnly = true } = {}) {
    const L = this.L, out = new Set();
    const add = l => { if (!fieldOnly || l.inField) out.add(l.id); };
    for (const s of Array.isArray(sel) ? sel : [sel]) {
      if (typeof s === 'number') { add(L.literals[s]); continue; }
      let m;
      if ((m = /^const:(\w+)$/.exec(s))) L.literals.filter(l => l.constName === m[1]).forEach(add);
      else if ((m = /^fn:(\w+)$/.exec(s))) L.literals.filter(l => l.fn === m[1]).forEach(add);
      else if ((m = /^part:([\w#]+)$/.exec(s))) {
        const p = L.parts.find(p => p.name === m[1] || p.callee === m[1]);
        if (!p) throw new Error(`no part ${m[1]}`);
        L.literals.filter(l => (l.fn && p.fns.includes(l.fn)) || (l.constName && p.consts.includes(l.constName))).forEach(add);
      } else if ((m = /^@(\d+)(?::(.+))?$/.exec(s))) {
        const ls = L.literals.filter(l => l.line === Number(m[1]) && (!m[2] || l.text === m[2] || l.text === '-' + m[2]));
        if (!ls.length) throw new Error(`no literal matches ${s}`);
        (m[2] ? [ls[0]] : ls).forEach(add);
      } else {
        const l = L.literals.find(l => l.name === s);
        if (!l) throw new Error(`no literal named ${s}`);
        add(l);
      }
    }
    return [...out];
  }

  // ---- GPU plumbing ----
  texFor(w, h = w) {
    const key = `${w}x${h}`;
    if (!this.texCache.has(key)) {
      const tex = this.device.createTexture({ size: [w, h], format: 'rgba8unorm', usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.COPY_SRC | GPUTextureUsage.TEXTURE_BINDING });
      const bg = this.device.createBindGroup({
        layout: this.gpu.bgl,
        entries: [this.uni, this.lits, this.inb, this.outb].map((b, i) => ({ binding: i, resource: { buffer: b } }))
          .concat([{ binding: 4, resource: tex.createView() }, { binding: 5, resource: { buffer: this.pts } }, { binding: 6, resource: { buffer: this.litbuf } }]),
      });
      this.texCache.set(key, { tex, bg, w, h });
    }
    return this.texCache.get(key);
  }
  writeUniform({ views = [], tile = 0, mode = 0, flags = 0, iso = -1, highlight = -1, literal = -1, normalized = 0, scale = 1, span = 1, d = [0, 0, 0, 0], e = [0, 0, 0, 0], bounds = true, rect = [0, 0, 1 << 30, 1 << 30] }) {
    const buf = new ArrayBuffer(384), f = new Float32Array(buf), u = new Uint32Array(buf), i = new Int32Array(buf);
    u.set(rect, 92);
    if (bounds && this.bounds && iso < 0) f.set([...this.bounds.min, 1, ...this.bounds.max, 0], 84);
    views.slice(0, 4).forEach((v, k) => {
      const b = k * 16;
      f.set([...v.o, v.ortho ? v.half : 0, ...v.f, v.ortho ? 0 : 1 / v.tanHalf, ...v.r, v.x0 || 0, ...v.u, v.y0 || 0], b);
    });
    u.set([views.length, tile, mode, flags], 64);
    i.set([iso, highlight, literal, normalized], 68);
    f.set([0, scale, span, 0], 72);
    u.set(d, 76);
    u.set(e, 80);
    this.device.queue.writeBuffer(this.uni, 0, buf);
  }
  async readBuffers(enc, reads) {
    const dev = this.device;
    const total = reads.reduce((s, r) => s + r.size, 0);
    const rb = dev.createBuffer({ size: Math.max(16, total), usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });
    let off = 0;
    for (const r of reads) { enc.copyBufferToBuffer(r.buf, r.offset || 0, rb, off, r.size); off += r.size; }
    dev.queue.submit([enc.finish()]);
    await rb.mapAsync(GPUMapMode.READ);
    const out = new Float32Array(rb.getMappedRange().slice(0));
    rb.unmap();
    rb.destroy();
    return out;
  }
  dispatch1D(pass, n, wg = 64) {
    const groups = Math.ceil(n / wg);
    if (groups <= 65535) pass.dispatchWorkgroups(groups);
    else pass.dispatchWorkgroups(65535, Math.ceil(groups / 65535));
  }

  // ---- render ----
  /** Render a view or the sheet into a texture. Returns the layout and the texture entry. */
  async render({ view = 'sheet', size = view === 'sheet' ? 1024 : 512, mode = 'shaded', part = null, literal = null, shadows = true, grid = true, scale = 1, wait = true, bounds = true, tileSize = null } = {}) {
    const lay = makeLayout(view, size, this.frame);
    const t = this.texFor(lay.width, lay.height);
    const partIdx = part == null ? -1 : this.partIndex(part);
    const litIdx = literal == null ? -1 : this.resolve(literal, { fieldOnly: false })[0];
    const iso = mode === 'isolate' ? partIdx : -1;
    const flags = (shadows ? 1 : 0) | (grid ? 2 : 0) | (mode === 'silhouette' ? 0 : 4);
    const t0 = performance.now();
    // GPU safety: one submission per tile, each well under ~100 ms, with the queue drained in between.
    const ts = tileSize || (mode === 'influence' ? 128 : 256);
    let tiles = 0;
    for (let y0 = 0; y0 < lay.height; y0 += ts) {
      for (let x0 = 0; x0 < lay.width; x0 += ts) {
        const rect = [x0, y0, Math.min(ts, lay.width - x0), Math.min(ts, lay.height - y0)];
        this.writeUniform({ views: lay.views, tile: lay.tile, mode: MODES[mode] ?? 0, flags, iso, highlight: mode === 'tint' || mode === 'parts' ? partIdx : -1, literal: litIdx, scale, bounds, rect });
        const enc = this.device.createCommandEncoder();
        const pass = enc.beginComputePass();
        pass.setPipeline(mode === 'influence' ? this.pipe.renderFd : this.pipe.render);
        pass.setBindGroup(0, t.bg);
        pass.dispatchWorkgroups(Math.ceil(rect[2] / 8), Math.ceil(rect[3] / 8));
        pass.end();
        this.device.queue.submit([enc.finish()]);
        await this.device.queue.onSubmittedWorkDone();
        tiles++;
      }
    }
    return { layout: lay, tex: t, ms: performance.now() - t0, tiles, mode, iso, highlight: partIdx, literal: litIdx };
  }
  partIndex(p) {
    if (typeof p === 'number') return p;
    const k = this.L.parts.findIndex(x => x.name === p || x.callee === p);
    if (k < 0) throw new Error(`no part ${p}; parts: ${this.L.parts.map(x => x.name).join(', ')}`);
    return k;
  }
  /** Read the rendered texture back as RGBA bytes. */
  async readImage(r) {
    const { w, h, tex } = r.tex;
    const bpr = Math.ceil(w * 4 / 256) * 256;
    const buf = this.device.createBuffer({ size: bpr * h, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });
    const enc = this.device.createCommandEncoder();
    enc.copyTextureToBuffer({ texture: tex }, { buffer: buf, bytesPerRow: bpr }, [w, h]);
    this.device.queue.submit([enc.finish()]);
    await buf.mapAsync(GPUMapMode.READ);
    const src = new Uint8Array(buf.getMappedRange());
    const out = new Uint8ClampedArray(w * h * 4);
    for (let y = 0; y < h; y++) out.set(src.subarray(y * bpr, y * bpr + w * 4), y * w * 4);
    buf.unmap();
    buf.destroy();
    return { data: out, width: w, height: h };
  }

  // ---- probe, gradients ----
  /** Trace rays (or snap points onto the surface). Returns hit, point, normal, value, part and part values. */
  async probe(rays, { iso = -1 } = {}) {
    const out = [];
    const CH = 2048;   // GPU safety: a ray marches up to ~400 steps; 2048 rays per submission
    for (let c0 = 0; c0 < rays.length; c0 += CH) {
      const chunk = rays.slice(c0, c0 + CH), n = chunk.length;
      const inb = new Float32Array(n * 8);
      chunk.forEach((r, i) => inb.set([...r.o, r.tmax ?? 12, ...(r.d || [0, 0, 0]), r.snap ? 1 : 0], i * 8));
      this.device.queue.writeBuffer(this.inb, 0, inb);
      this.writeUniform({ iso, d: [0, n, 0, 0], e: [0, 0, 0, 0] });
      const enc = this.device.createCommandEncoder();
      const pass = enc.beginComputePass();
      pass.setPipeline(this.pipe.probe);
      pass.setBindGroup(0, this.texFor(64).bg);
      this.dispatch1D(pass, n);
      pass.end();
      const res = await this.readBuffers(enc, [{ buf: this.outb, offset: 0, size: n * PSTRIDE * 4 }]);
      for (let i = 0; i < n; i++) out.push(this.parseProbe(res, i * PSTRIDE));
    }
    return out;
  }
  parseProbe(out, o) {
    const np = out[o + 11];
    return {
      hit: out[o] === 1 ? 'surface' : out[o] === 2 ? 'ground' : 'miss',
      t: out[o + 1], point: [out[o + 2], out[o + 3], out[o + 4]], normal: [out[o + 5], out[o + 6], out[o + 7]],
      d: out[o + 8], gradLen: out[o + 9], part: out[o + 10],
      partValues: Array.from(out.subarray(o + 12, o + 12 + Math.min(np, 20))),
    };
  }
  /** d field / d literal at points, by GPU central differences. Returns values, gradient lengths and the M x K Jacobian. */
  async grads(points, ids, { iso = -1, ptsOffset = 0, upload = true, normalized = false } = {}) {
    const M = points ? points.length : this._ptsCount, K = ids.length;
    if (upload && points) {
      const pts = new Float32Array(M * 4);
      points.forEach((p, i) => pts.set([p[0], p[1], p[2], 0], i * 4));
      this.device.queue.writeBuffer(this.pts, ptsOffset * 16, pts);
    }
    const lits = new Float32Array(Math.max(1, K) * 4);
    ids.forEach((id, j) => lits.set([id, FD_H * Math.max(1, Math.abs(this.values[id]))], j * 4));
    this.device.queue.writeBuffer(this.inb, 0, lits);
    // GPU safety: at most ~1M field evaluations per submission (2 per literal, 14 when normalized).
    const evalsPerInv = normalized ? 14 : 2;
    const ptsPerChunk = Math.max(1, Math.floor(1e6 / ((K + 1) * evalsPerInv)));
    for (let p0 = 0; p0 < M; p0 += ptsPerChunk) {
      const m = Math.min(ptsPerChunk, M - p0);
      this.writeUniform({ iso, normalized: normalized ? 1 : 0, d: [K, m, ptsOffset + p0, 0], e: [0, 0, p0 * (K + 2), 0] });
      const enc = this.device.createCommandEncoder();
      const pass = enc.beginComputePass();
      pass.setPipeline(this.pipe.grad);
      pass.setBindGroup(0, this.texFor(64).bg);
      this.dispatch1D(pass, m * (K + 1));
      pass.end();
      this.device.queue.submit([enc.finish()]);
      if (p0 + m < M) await this.device.queue.onSubmittedWorkDone();
    }
    const out = await this.readBuffers(this.device.createCommandEncoder(), [{ buf: this.outb, offset: 0, size: M * (K + 2) * 4 }]);
    const d = new Float64Array(M), gl = new Float64Array(M), J = new Float64Array(M * K);
    for (let i = 0; i < M; i++) {
      d[i] = out[i * (K + 2)];
      gl[i] = out[i * (K + 2) + 1];
      for (let j = 0; j < K; j++) J[i * K + j] = out[i * (K + 2) + 2 + j];
    }
    return { d, gl, J, M, K };
  }

  // ---- click to source ----
  /**
   * What made this pixel: the winning part and the literals ranked by |d field / d literal| at the hit,
   * grouped by source line. `at`: {view, pixel, size} | {point} (snapped to the surface) | {ray: {o, d}}.
   * One GPU submission: the probe writes the hit point, the gradient kernel reads it.
   */
  async query(at, { top = 12, lines = 6, iso = -1 } = {}) {
    const t0 = performance.now();
    const ray = this.rayFor(at);
    const ids = this.fieldIds, K = ids.length;
    const inb = new Float32Array(8 + K * 4);
    inb.set([...ray.o, ray.tmax ?? 12, ...(ray.d || [0, 0, 0]), ray.snap ? 1 : 0], 0);
    ids.forEach((id, j) => inb.set([id, FD_H * Math.max(1, Math.abs(this.values[id]))], 8 + j * 4));
    this.device.queue.writeBuffer(this.inb, 0, inb);
    this.writeUniform({ iso, d: [K, 1, 0, 2], e: [0, 0, PSTRIDE, 0] });
    const enc = this.device.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setBindGroup(0, this.texFor(64).bg);
    pass.setPipeline(this.pipe.probe);
    pass.dispatchWorkgroups(1);
    pass.setPipeline(this.pipe.grad);
    this.dispatch1D(pass, K + 1);
    pass.end();
    const out = await this.readBuffers(enc, [{ buf: this.outb, offset: 0, size: (PSTRIDE + K + 2) * 4 }]);
    const tGpu = performance.now() - t0;
    const pr = this.parseProbe(out, 0);
    const res = { at: { ...at, ray: { o: ray.o.map(r4), d: ray.d.map(r4) } }, hit: pr.hit, point: pr.point.map(r6), normal: pr.normal.map(r4) };
    if (pr.hit !== 'surface') return { ...res, ms: { total: performance.now() - t0, gpu: tGpu } };
    const g = new Float64Array(K);
    for (let j = 0; j < K; j++) g[j] = out[PSTRIDE + 2 + j];
    const gl = out[PSTRIDE + 1] || 1;
    const order = [...g.keys()].sort((a, b) => Math.abs(g[b]) - Math.abs(g[a]));
    const part = this.L.parts[pr.part];
    const lits = order.slice(0, top).filter(j => Math.abs(g[j]) > 0).map(j => {
      const id = ids[j];
      const l = this.L.literals[id];
      const inPart = part ? ((l.fn && part.fns.includes(l.fn)) || (l.constName && part.consts.includes(l.constName))) : false;
      // A unit increase moves the surface along the normal by -g / |grad d| (metres).
      return this.literalInfo(id, { grad: r6(g[j]), mmPerMm: r4(-g[j] / gl), inPart });
    });
    const byLine = new Map();
    for (let j = 0; j < K; j++) {
      const l = this.L.literals[ids[j]];
      byLine.set(l.line, (byLine.get(l.line) || 0) + Math.abs(g[j]));
    }
    const tot = [...byLine.values()].reduce((s, x) => s + x, 0) || 1;
    const lineList = [...byLine.entries()].sort((a, b) => b[1] - a[1]).slice(0, lines)
      .map(([ln, w]) => ({ line: ln, share: r4(w / tot), text: this.L.lines[ln - 1] }));
    return {
      ...res, d: r6(pr.d), gradLen: r4(pr.gradLen),
      part: part ? { index: pr.part, name: part.name, line: part.line, fnLine: part.fnLine } : null,
      partValues: this.L.parts.map((p, i) => ({ name: p.name, d: r6(pr.partValues[i]) })),
      literals: lits, lines: lineList,
      grads: { ids, g },
      ms: { total: performance.now() - t0, gpu: tGpu },
    };
  }
  rayFor(at) {
    if (at.ray) return { o: at.ray.o, d: v3.norm(at.ray.d), tmax: at.ray.tmax ?? 40 };
    if (at.point) return { o: at.point, d: [0, 0, 0], snap: true };
    const lay = makeLayout(at.view || 'side', at.size || (at.view === 'sheet' ? 1024 : 512), this.frame);
    return pixelRay(lay, at.pixel[0], at.pixel[1]);
  }

  /** Field value, part values and the winning part at a point (no snapping). */
  async probePoint(p) {
    const [r] = await this.probe([{ o: p, d: [0, 0, 0], snap: false, tmax: 0 }]);
    return r;
  }

  // ---- surface samples (anchors for the drag, check points for locality) ----
  async surfaceSamples(n = 14, offset = 0.5) {
    const key = `${n}:${offset}:${this.version}`;
    if (this._samples && this._samples.key === key) return this._samples;
    const rays = [];
    for (const name of ['side', 'front', 'threeq', 'top']) {
      const v = { ...makeView(name, this.frame), x0: 0, y0: 0 };
      for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
        rays.push(viewRay(v, [((x + offset) / n) * 2 - 1, ((y + offset) / n) * 2 - 1]));
      }
    }
    const res = await this.probe(rays);
    const points = res.filter(r => r.hit === 'surface' && r.point[1] > 0.005).map(r => r.point);
    this._samples = { key, points };
    return this._samples;
  }

  // ---- drag to edit ----
  /**
   * Start a move at a surface point. Options:
   *  - patch (default true): a ring of surface points 6 mm around the click translates with the drag,
   *    so "move this bit of surface" rather than "make the surface pass through the target" (which a
   *    bulge satisfies). patch: false is the single-point constraint.
   *  - select: 'residual' (default) picks the K literals that most reduce the move's residual, once the
   *    target is known; 'influence' picks the K with the largest |d field / d literal| at the click.
   *    `literals` overrides both. Helper-function literals (shared code) are skipped unless includeHelpers.
   *  - anchors (default true): surface samples elsewhere that should stay put.
   *  - symmetry (default true): if the field is mirror-symmetric in x (to 1 cm), the mirror image of the
   *    patch moves with the mirrored drag, and the mirror region isn't anchored.
   */
  async beginMove(from, { k = 8, literals = null, anchors = true, nAnchors = 14, select = 'residual', patch = true, patchR = 0.006, ringWeight = 0.02, symmetry = true, includeHelpers = false, falloff = null } = {}) {
    const t0 = performance.now();
    const q = await this.query(from, { top: 0 });
    if (q.hit !== 'surface') throw new Error(`move: the start point is not on the surface (${q.hit})`);
    const st = { p0: q.point, n0: q.normal, part: q.part, theta0: Float64Array.from(this.values), anchorsOn: anchors, query: q, k, select, sel: null, patchOn: patch, ringWeight, includeHelpers, falloffIn: falloff };
    st.patch = [q.point];
    if (patch) {
      const n = q.normal, a = Math.abs(n[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0];
      const t1 = v3.norm(v3.cross(n, a)), t2 = v3.cross(n, t1);
      const rays = [];
      for (let i = 0; i < 6; i++) {
        const ang = i * Math.PI / 3;
        rays.push({ o: v3.add(q.point, v3.add(v3.mul(t1, patchR * Math.cos(ang)), v3.mul(t2, patchR * Math.sin(ang)))), snap: true });
      }
      for (const r of await this.probe(rays)) if (v3.dist(r.point, q.point) < 3 * patchR) st.patch.push(r.point);
    }
    if (anchors) st.anchors = (await this.surfaceSamples(nAnchors, 0.5)).points;
    const sym = symmetry ? await this.mirrorSymmetric() : null;
    st.symmetry = sym ? { detected: sym.yes, worstMm: r4(sym.worst * 1000) } : null;
    if (sym && sym.yes) {
      st.mirror = [-st.p0[0], st.p0[1], st.p0[2]];
      st.mirrorPatch = st.patch.map(p => [-p[0], p[1], p[2]]);
      const G = await this.grads(st.mirrorPatch, [], { normalized: true });
      st.mirrorR0 = Array.from(G.d, (d, i) => d / Math.max(G.gl[i], 0.25));
    }
    if (literals) await this.setSelection(st, this.resolve(literals));
    else if (select === 'influence') {
      const { ids, g } = q.grads;
      const order = [...g.keys()].filter(j => Math.abs(g[j]) > 1e-5 && (includeHelpers || !this.L.literals[ids[j]].helper)).sort((a, b) => Math.abs(g[b]) - Math.abs(g[a]));
      await this.setSelection(st, order.slice(0, k === 'all' ? order.length : k).map(j => ids[j]));
    }
    st.ms = performance.now() - t0;
    return st;
  }
  async setSelection(st, sel) {
    st.sel = sel;
    st.gp0 = this.selGrad(st.query, sel).map(g => g / Math.max(st.query.gradLen || 1, 0.25));
    if (st.anchorsOn) {
      const G = await this.grads(st.anchors, sel, { normalized: true });
      st.rA0 = Array.from(G.d, (d, i) => d / Math.max(G.gl[i], 0.25));
      st.coupled = this.coupling(G, st.gp0);
    }
  }
  /** Residual selection: score each field literal by |sum_i J_ij r_i| over the displaced patch (the move's steepest-descent direction). */
  async selectForTarget(st, target) {
    const rows = this.moveRows(st, target);
    const ids = this.fieldIds.filter(id => st.includeHelpers || !this.L.literals[id].helper);
    const G = await this.grads(rows.map(r => r.p), ids, { normalized: true });
    const score = ids.map((_, j) => { let s = 0; for (let i = 0; i < G.M; i++) s += G.J[i * G.K + j] * (G.d[i] / Math.max(G.gl[i], 0.25) - rows[i].r0); return Math.abs(s); });
    const order = [...score.keys()].filter(j => score[j] > 0).sort((a, b) => score[b] - score[a]);
    await this.setSelection(st, order.slice(0, st.k === 'all' ? order.length : st.k).map(j => ids[j]));
    st.selectedFor = v3.sub(target, st.p0);
  }
  /** The constrained points of a move to `target`: the displaced patch, and its mirror image when the field is symmetric. r0 is each point's wanted distance estimate. */
  moveRows(st, target) {
    const delta = v3.sub(target, st.p0);
    const rw = st.ringWeight ?? 0.02;
    const rows = st.patch.map((p, i) => ({ p: v3.add(p, delta), r0: 0, w: i ? rw : 1 }));
    if (st.mirrorPatch) {
      const md = [-delta[0], delta[1], delta[2]];
      st.mirrorPatch.forEach((p, i) => rows.push({ p: v3.add(p, md), r0: st.mirrorR0[i], w: i ? rw : 1, mirror: true }));
    }
    return rows;
  }
  selGrad(q, sel) {
    const pos = new Map(q.grads.ids.map((id, j) => [id, j]));
    return sel.map(id => (pos.has(id) ? q.grads.g[pos.get(id)] : 0));
  }
  /** Is the field mirror-symmetric in x, up to `tol` (surface detail such as unmirrored noise)? Checked numerically on surface samples. */
  async mirrorSymmetric(tol = 0.01) {
    if (this._sym && this._sym.version === this.version) return this._sym;
    const pts = (await this.surfaceSamples(14, 0.5)).points;
    const G = await this.grads([...pts, ...pts.map(p => [-p[0], p[1], p[2]])], []);
    let worst = 0;
    for (let i = 0; i < pts.length; i++) worst = Math.max(worst, Math.abs(G.d[i] - G.d[i + pts.length]) / Math.max(G.gl[i], 0.25));
    this._sym = { version: this.version, yes: worst < tol, worst };
    return this._sym;
  }
  /** Distance from a point to the move's origin, or to its mirror image when symmetry is kept. */
  moveDist(st, a) { return st.mirror ? Math.min(v3.dist(a, st.p0), v3.dist(a, st.mirror)) : v3.dist(a, st.p0); }
  /** Points whose gradient over the selected literals matches the dragged point's: the source moves them together. */
  coupling(G, gp0) {
    const K = gp0.length, n0 = Math.hypot(...gp0) || 1e-12, out = new Uint8Array(G.M);
    for (let i = 0; i < G.M; i++) {
      let s = 0;
      for (let j = 0; j < K; j++) { const x = G.J[i * K + j] - gp0[j]; s += x * x; }
      out[i] = Math.sqrt(s) <= 0.15 * n0 ? 1 : 0;
    }
    return out;
  }
  /** Anchors fade in between rIn and rOut from the dragged point (and its mirror). Default rIn: 3 cm or 1.5x the drag; `falloff` (m) sets it. */
  falloff(st, target) {
    const rIn = st.falloffIn || Math.max(0.03, 1.5 * v3.dist(target, st.p0));
    return { rIn, rOut: 3 * rIn };
  }
  anchorWeights(st, target) {
    const { rIn, rOut } = this.falloff(st, target);
    return st.anchors.map((a, i) => (st.coupled[i] ? 0 : smoothstep(rIn, rOut, this.moveDist(st, a))));
  }
  /**
   * Gauss-Newton on  W/|rows| sum_i (r(p_i + delta) - r0_i)^2  +  wA sum_a w_a (r(a) - r0(a))^2  +  lambda^2 |theta - theta0|^2
   * over the selected literals (free = a subset, for rounding), where r = d / |grad d| is the field's
   * distance estimate. Warm-started from the current values, damped toward the values at the start.
   */
  async solveMove(st, target, { iters = 2, W = 1e4, wAnchors = 1e4, lambda2 = 1, free = null, measure = true } = {}) {
    const t0 = performance.now();
    if (!st.sel) await this.selectForTarget(st, target);
    const sel = free || st.sel;
    const K = sel.length;
    const rows = this.moveRows(st, target);
    const nT = rows.length;
    const wA = st.anchorsOn ? this.anchorWeights(st, target) : [];
    const nA = wA.length, wSum = wA.reduce((s, x) => s + x, 0) || 1;
    let before = null;
    for (let it = 0; it < iters && K > 0; it++) {
      const G = await this.grads([...rows.map(r => r.p), ...(st.anchorsOn ? st.anchors : [])], sel, { normalized: true });
      const r = i => G.d[i] / Math.max(G.gl[i], 0.25);
      if (it === 0) before = Math.abs(r(0) - rows[0].r0) * 1000;
      const A = new Float64Array(K * K), b = new Float64Array(K);
      const accum = (row, res, w) => {
        for (let i = 0; i < K; i++) {
          const ji = G.J[row * K + i];
          if (ji === 0) continue;
          b[i] -= w * ji * res;
          for (let j = 0; j < K; j++) A[i * K + j] += w * ji * G.J[row * K + j];
        }
      };
      for (let i = 0; i < nT; i++) accum(i, r(i) - rows[i].r0, W * rows[i].w);
      for (let a = 0; a < nA; a++) if (wA[a] > 0) accum(nT + a, r(nT + a) - st.rA0[a], wAnchors * wA[a] / wSum);
      for (let i = 0; i < K; i++) {
        A[i * K + i] += lambda2;
        b[i] -= lambda2 * (this.values[sel[i]] - st.theta0[sel[i]]);
      }
      const dx = solve(A, b, K);
      for (let i = 0; i < K; i++) if (Number.isFinite(dx[i])) this.values[sel[i]] += dx[i];
      this.upload();
    }
    // measure: false skips a round trip; errMmBefore is the error before this update's step.
    const e = measure ? await this.moveError(rows) : { errMm: before, patchRmsMm: null };
    return { ...e, errMmBefore: before, ms: performance.now() - t0 };
  }
  /** Distance (mm) from the constrained points to where they should be: |d| / |grad d| - r0. errMm is the clicked point's. */
  async moveError(rows) {
    const c = await this.grads(rows.map(r => r.p), []);
    const mm = i => Math.abs(c.d[i] / Math.max(c.gl[i], 0.25) - rows[i].r0) * 1000;
    let s = 0;
    for (let i = 0; i < rows.length; i++) s += mm(i) ** 2;
    return { errMm: mm(0), patchRmsMm: Math.sqrt(s / rows.length) };
  }
  /**
   * Finish a move: drop literals that carry under 5% of the move and re-solve; then round each changed
   * literal to the source's precision, largest change first, re-solving the rest after each so they
   * compensate; then measure the error and how much the rest of the surface moved.
   */
  async finishMove(st, target, { prune = true, policy = 'adaptive', iters = 4, check = true } = {}) {
    const t0 = performance.now();
    let res = await this.solveMove(st, target, { iters: 2 });
    let pruned = [];
    const sel0 = st.sel;
    const rows = this.moveRows(st, target);
    if (prune && st.sel.length > 1) {
      const G = await this.grads(rows.map(r => r.p), st.sel, { normalized: true });
      const contrib = st.sel.map((id, j) => { let c = 0; for (let i = 0; i < G.M; i++) c += Math.abs(G.J[i * G.K + j] * (this.values[id] - st.theta0[id])); return c; });
      const tot = contrib.reduce((s, x) => s + x, 0) || 1;
      const keep = st.sel.filter((_, j) => contrib[j] >= 0.05 * tot);
      if (keep.length && keep.length < st.sel.length) {
        pruned = st.sel.filter(id => !keep.includes(id));
        for (const id of st.sel) this.values[id] = st.theta0[id];
        this.upload();
        await this.setSelection(st, keep);
        res = await this.solveMove(st, target, { iters });
      }
    }
    const solved = res;
    // Round to what the source will hold: the source's own decimals ('keep'), largest change first,
    // re-solving the others; the last literal standing gets adaptive precision.
    let free = st.sel.filter(id => this.values[id] !== st.theta0[id]);
    free.sort((a, b) => Math.abs(this.values[b] - st.theta0[b]) - Math.abs(this.values[a] - st.theta0[a]));
    while (free.length) {
      const id = free.shift();
      const lit = { ...this.L.literals[id], value: st.theta0[id], text: this.literalText(id, st.theta0[id]) };
      this.values[id] = Number(formatLiteral(lit, this.values[id], free.length ? 'keep' : policy).replace(/[fh]$/, ''));
      this.upload();
      if (free.length) await this.solveMove(st, target, { iters: 2, free });
    }
    // A literal rounded back to its original text is no change at all.
    for (const id of st.sel) if (this.literalText(id, this.values[id]) === this.L.literals[id].text) this.values[id] = st.theta0[id];
    this.upload();
    const after = await this.moveError(rows);
    const out = {
      p0: st.p0, target: target.map(r6), moveMm: r4(v3.dist(target, st.p0) * 1000),
      mode: { patch: st.patchOn, patchPoints: st.patch.length, ringWeight: st.ringWeight, select: st.select, anchors: st.anchorsOn, k: st.k, symmetry: st.symmetry, mirrorRows: !!st.mirrorPatch },
      selected: sel0.map(id => this.L.literals[id].name), pruned: pruned.map(id => this.L.literals[id].name),
      errMmSolved: r4(solved.errMm), errMm: r4(after.errMm), patchRmsMm: r4(after.patchRmsMm),
      changes: st.sel.filter(id => this.values[id] !== st.theta0[id]).map(id => ({ ...this.literalInfo(id), from: r6(st.theta0[id]), to: r6(this.values[id]), delta: r6(this.values[id] - st.theta0[id]) })),
    };
    if (check) {
      out.locality = await this.locality(st, target);
      // Did the move split the shape (a small part pulled off, say)? Compare piece counts.
      const now = Float64Array.from(this.values);
      this.values = Float64Array.from(st.theta0); this.upload();
      const p0 = await this.pieces();
      this.values = now; this.upload();
      const p1 = await this.pieces();
      out.pieces = { before: p0.count, after: p1.count, split: p1.count > p0.count, ms: p1.ms };
    }
    out.ms = performance.now() - t0;
    if (this.bounds) await this.computeBounds();
    return out;
  }
  literalText(id, value) {
    const l = this.L.literals[id];
    return value === l.value ? l.text : formatLiteral(l, value, 'adaptive');
  }
  /**
   * How far surface points away from the move travelled (mm), on a denser grid than the anchors.
   * 'far': beyond the falloff radius; 'tied': points whose gradient over the chosen literals equals the
   * dragged point's, or near its mirror image when symmetry is kept, so the source moves them with it;
   * 'near': within the falloff radius.
   */
  async locality(st, target, n = 22) {
    const now = Float64Array.from(this.values);
    this.values = Float64Array.from(st.theta0); this.upload();
    const pts = (await this.surfaceSamples(n, 0.25)).points;
    const G0 = await this.grads(pts, st.sel, { normalized: true });
    const coupled = this.coupling(G0, st.gp0);
    this.values = now; this.upload();
    const G1 = await this.grads(pts, []);
    const { rOut } = this.falloff(st, target);
    const far = [], tied = [], near = [];
    let worst = null;
    for (let i = 0; i < pts.length; i++) {
      const mm = Math.abs(G1.d[i] / Math.max(G1.gl[i], 0.25) - G0.d[i] / Math.max(G0.gl[i], 0.25)) * 1000;
      const nearMirror = st.mirror && v3.dist(pts[i], st.mirror) <= rOut;
      if (coupled[i] || nearMirror) tied.push(mm);
      else if (v3.dist(pts[i], st.p0) > rOut) { far.push(mm); if (!worst || mm > worst.mm) worst = { mm: r4(mm), at: pts[i].map(r4) }; }
      else near.push(mm);
    }
    const stats = a => a.length ? { n: a.length, rmsMm: r4(Math.sqrt(a.reduce((s, x) => s + x * x, 0) / a.length)), maxMm: r4(Math.max(...a)), over1mm: a.filter(x => x > 1).length } : { n: 0 };
    return { falloffMm: r4(rOut * 1000), far: { ...stats(far), worstAt: worst && worst.at }, near: stats(near), tied: stats(tied) };
  }
  targetFor(st, from, to) {
    if (to.delta) return v3.add(st.p0, to.delta);
    if (to.pixel) {
      const r = this.rayFor({ view: to.view || from.view, pixel: to.pixel, size: to.size || from.size });
      const n = r.view ? r.view.f : r.d;   // the plane through p0 facing the camera
      const t = v3.dot(v3.sub(st.p0, r.o), n) / v3.dot(r.d, n);
      return v3.add(r.o, v3.mul(r.d, t));
    }
    if (to.point) return to.point;
    throw new Error('move: `to` needs {delta}, {point} or {view, pixel}');
  }
  /** One call: move the surface at `from` to `to`, in `steps` updates (like a drag), then finish. */
  async move(from, to, opts = {}) {
    const t0 = performance.now();
    const st = await this.beginMove(from, opts);
    const target = this.targetFor(st, from, to);
    if (!st.sel) await this.selectForTarget(st, target);
    const steps = opts.steps || 1;
    const updates = [];
    for (let s = 1; s <= steps; s++) {
      const tt = v3.add(st.p0, v3.mul(v3.sub(target, st.p0), s / steps));
      const u = await this.solveMove(st, tt, { iters: opts.iters || 2 });
      updates.push({ errMm: r4(u.errMm), ms: r4(u.ms) });
    }
    const fin = await this.finishMove(st, target, opts);
    fin.part = st.part && st.part.name;
    fin.updates = updates;
    fin.beginMs = r4(st.ms);
    fin.totalMs = r4(performance.now() - t0);
    this.history.push({ op: 'move', changes: fin.changes.map(c => c.name) });
    return fin;
  }

  // ---- fit to a silhouette ----
  /** Minimum of the field along each pixel's ray, in an orthographic view. Returns m per pixel; the minimizing points stay on the GPU (or come back with `points`). */
  async silhouette(view = 'side', res = 256, { points = false } = {}) {
    const v = { ...makeView(view, this.frame), x0: 0, y0: 0 };
    if (!v.ortho) throw new Error('silhouette needs an orthographic view');
    // GPU safety: row bands of ~16k pixels per submission (each pixel marches up to ~400 steps).
    const band = Math.max(8, Math.floor(16384 / res / 8) * 8);
    for (let y0 = 0; y0 < res; y0 += band) {
      const rect = [0, y0, res, Math.min(band, res - y0)];
      this.writeUniform({ views: [v], span: 1.2, d: [0, 0, 0, 0], e: [res, res, 0, 0], rect });
      const enc = this.device.createCommandEncoder();
      const pass = enc.beginComputePass();
      pass.setPipeline(this.pipe.silmin);
      pass.setBindGroup(0, this.texFor(64).bg);
      pass.dispatchWorkgroups(Math.ceil(rect[2] / 8), Math.ceil(rect[3] / 8));
      pass.end();
      this.device.queue.submit([enc.finish()]);
      if (y0 + band < res) await this.device.queue.onSubmittedWorkDone();
    }
    const reads = [{ buf: this.outb, offset: 0, size: res * res * 4 }];
    if (points) reads.push({ buf: this.pts, offset: 0, size: res * res * 16 });
    const out = await this.readBuffers(this.device.createCommandEncoder(), reads);
    return { m: out.subarray(0, res * res), pts: points ? out.subarray(res * res) : null, res, view: v };
  }
  static iou(m, target) {
    let i = 0, u = 0;
    for (let k = 0; k < m.length; k++) { const a = m[k] < 0, b = target[k] > 0; if (a && b) i++; if (a || b) u++; }
    return u ? i / u : 1;
  }
  /**
   * Fit literals so the orthographic silhouette matches a target mask (Uint8Array res x res, 1 = inside).
   * Loss: hinge residuals (metres) on pixels on the wrong side, or within mu of the boundary; the
   * Jacobian is d field / d literal at each active pixel's minimizing point (envelope theorem).
   * Levenberg-Marquardt with accept/reject.
   */
  async fit({ target, view = 'side', literals, res = 256, maxIter = 40, muPx = 0.5, onIter = null, truth = null, prior = 0, normalized = false } = {}) {
    const t0 = performance.now();
    const sel = this.resolve(literals);
    const K = sel.length;
    const half = makeView(view, this.frame).half;
    const mu = muPx * 2 * half / res;
    const theta0 = Float64Array.from(this.values);
    // Residuals: m itself (default), or with `normalized` the distance estimate m / |grad d| at the
    // minimizing point, which a scale factor on the field can't shrink, but which is ill-conditioned at
    // deep interior minima (measured: it stalled the fit, README). `prior` adds prior * |theta - theta0|^2.
    const priorLoss = () => { let p = 0; for (const id of sel) p += (this.values[id] - theta0[id]) ** 2; return prior * p; };
    const evalSil = async () => {
      const s = await this.silhouette(view, res, { points: true });
      let loss = 0;
      const active = [], resid = [];
      for (let k = 0; k < s.m.length; k++) {
        const m = normalized ? s.m[k] / Math.max(s.pts[k * 4 + 3], 0.5) : s.m[k];
        const r = target[k] ? Math.max(0, m + mu) : Math.min(0, m - mu);
        if (r !== 0) { active.push(k); resid.push(r); loss += r * r; }
      }
      return { s, loss: loss + priorLoss(), active, resid, iou: Lens.iou(s.m, target) };
    };
    let cur = await evalSil();
    const iou0 = cur.iou;
    let lambda = 1e-2;
    const log = [{ iter: 0, iou: r6(cur.iou), loss: cur.loss, active: cur.active.length, ms: r4(performance.now() - t0) }];
    let tConverged = performance.now() - t0, stall = 0, iters = 0;
    for (let it = 1; it <= maxIter; it++) {
      iters = it;
      // Jacobian at the active pixels' minimizing points.
      const na = cur.active.length;
      if (!na) break;
      const pts = new Float32Array(na * 4);
      cur.active.forEach((k, i) => pts.set(cur.s.pts.subarray(k * 4, k * 4 + 4), i * 4));
      this.device.queue.writeBuffer(this.pts, res * res * 16, pts);
      this._ptsCount = na;
      const G = await this.grads(null, sel, { ptsOffset: res * res, upload: false, normalized });
      const JtJ = new Float64Array(K * K), Jtr = new Float64Array(K);
      for (let i = 0; i < na; i++) {
        const r = cur.resid[i];
        for (let a = 0; a < K; a++) {
          const ja = G.J[i * K + a];
          if (ja === 0) continue;
          Jtr[a] += ja * r;
          for (let b = 0; b < K; b++) JtJ[a * K + b] += ja * G.J[i * K + b];
        }
      }
      for (let a = 0; a < K; a++) { JtJ[a * K + a] += prior; Jtr[a] += prior * (this.values[sel[a]] - theta0[sel[a]]); }
      const maxDiag = Math.max(1e-12, ...Array.from({ length: K }, (_, a) => JtJ[a * K + a]));
      let accepted = false;
      for (let tries = 0; tries < 6 && !accepted; tries++) {
        const A = Float64Array.from(JtJ);
        for (let a = 0; a < K; a++) A[a * K + a] += lambda * JtJ[a * K + a] + 1e-6 * maxDiag;
        const dx = solve(A, Jtr.map(x => -x), K);
        const save = sel.map(id => this.values[id]);
        sel.forEach((id, a) => {
          const cap = 0.05 * Math.max(1, Math.abs(this.values[id]));
          this.values[id] += Math.max(-cap, Math.min(cap, dx[a]));
        });
        this.upload();
        const trial = await evalSil();
        if (trial.loss < cur.loss) {
          const rel = (cur.loss - trial.loss) / Math.max(cur.loss, 1e-18);
          cur = trial;
          lambda = Math.max(1e-7, lambda / 3);
          accepted = true;
          if (rel > 1e-3) { tConverged = performance.now() - t0; stall = 0; } else stall++;
        } else {
          sel.forEach((id, a) => { this.values[id] = save[a]; });
          this.upload();
          lambda = Math.min(1e6, lambda * 5);
        }
      }
      log.push({ iter: it, iou: r6(cur.iou), loss: cur.loss, active: cur.active.length, lambda, accepted, ms: r4(performance.now() - t0) });
      if (onIter) await onIter(log[log.length - 1]);
      if (!accepted || stall >= 3) break;
    }
    const result = {
      literals: sel.map(id => ({ ...this.literalInfo(id), from: r6(theta0[id]), to: r6(this.values[id]), truth: truth ? r6(truth[id]) : undefined, errMm: truth ? r4(Math.abs(this.values[id] - truth[id]) * 1000) : undefined })),
      iouBefore: r6(iou0), iouAfter: r6(cur.iou), iterations: iters, convergedMs: r4(tConverged), totalMs: r4(performance.now() - t0),
      res, metresPerPixel: r6(2 * half / res), prior, normalized, log,
    };
    this.history.push({ op: 'fit', literals: sel.length });
    return result;
  }

  // ---- write-back ----
  /** Splice the current values into the source; verify by re-lifting. Does not change the lens until `commit`. */
  writeBack({ policy = 'adaptive' } = {}) {
    const t0 = performance.now();
    const wb = writeBack(this.L, this.values, { policy });
    const ver = verifyWriteBack(this.L, wb, this.values);
    const diff = diffLines(this.L.src, wb.source);
    return {
      source: wb.source, changes: wb.changes, diff: diff.text, changedLines: diff.changedLines, changedChars: diff.changedChars,
      sameLineCount: diff.sameLineCount, verified: ver.ok, problems: ver.problems, maxRoundErr: ver.maxRoundErr,
      ms: { splice: r4(wb.ms), verify: r4(ver.ms), total: r4(performance.now() - t0) }, newLift: ver.lift,
    };
  }
  /** Adopt written-back source as the new truth: spans and base values from the re-lift, no recompile (same structure). */
  commit(wb) {
    if (!wb.verified) throw new Error('write-back did not verify: ' + wb.problems.join('; '));
    this.adopt(wb.newLift);
  }
  adopt(L2) {
    if (L2.structure !== this.L.structure) throw new Error('the new source differs in more than literal values: reload it');
    this.L = L2;
    this.src = L2.src;
    this.base = Float64Array.from(this.L.literals.map(l => l.value));
    this.values = Float64Array.from(this.base);
    this.upload();
  }
  /** Take a source with the same structure (only literal values differ) as the truth, without recompiling. */
  adoptSource(src) { this.adopt(lift(src, { entry: this.L.entry, name: this.L.name })); }
}

// ---- images ----
export async function rgbaToPng(img, draw = null) {
  const c = new OffscreenCanvas(img.width, img.height);
  const g = c.getContext('2d');
  g.putImageData(new ImageData(img.data, img.width, img.height), 0, 0);
  if (draw) draw(g);
  return c.convertToBlob({ type: 'image/png' });
}
export function maskToRgba(mask, res) {
  const data = new Uint8ClampedArray(res * res * 4);
  for (let k = 0; k < res * res; k++) { const v = mask[k] ? 255 : 0; data.set([v, v, v, 255], k * 4); }
  return { data, width: res, height: res };
}
export async function loadMask(url, res) {
  const blob = await (await fetch(url)).blob();
  const bmp = await createImageBitmap(blob);
  const c = new OffscreenCanvas(res, res);
  const g = c.getContext('2d');
  g.imageSmoothingEnabled = false;
  g.drawImage(bmp, 0, 0, res, res);
  const d = g.getImageData(0, 0, res, res).data;
  const mask = new Uint8Array(res * res);
  for (let k = 0; k < res * res; k++) mask[k] = d[k * 4] > 127 ? 1 : 0;
  return mask;
}
