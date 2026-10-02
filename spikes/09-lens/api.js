// The agent API: every lens operation as a JSON command. The page runs a list of commands from a job
// file (#api=<job>) or a live queue (#serve=<session>); lens.py is the CLI. Same calls the UI makes.

import { Lens, makeLayout, project, metresPerPixel, rgbaToPng, maskToRgba, loadMask, v3 } from './lens.js';

const r4 = x => Math.round(x * 1e4) / 1e4;

/** Commands, for `help`. */
export const COMMANDS = {
  info: 'counts of lifted and unlifted literals (with reasons), the parts, compile time',
  literals: '{filter?: selector | {fn, const, line, inField}} -> the literal table (id, name, value, line, sites)',
  render: '{view: side|front|top|threeq|sheet, size?, mode?: shaded|parts|tint|isolate|influence|silhouette, part?, literal?, shadows?, name?} -> PNG path and the view geometry',
  isolate: '{part, view?, size?} -> render of that part alone',
  query: '{view, pixel:[x,y], size?} | {point:[x,y,z]} | {ray:{o,d}} -> part, literals ranked by |d field/d literal|, source lines',
  probe: '{point:[x,y,z]} -> field value, the winning part and every part value there (no snapping)',
  ray: '{o:[x,y,z], d:[x,y,z]} -> first surface hit (a measuring tape)',
  project: '{point, view?, size?} -> pixel',
  move: '{from: {view,pixel}|{point}, to: {point}|{delta}|{view,pixel}, k? (8), literals? [selectors], anchors? (true), patch? (true), select?: residual|influence, falloff? (m), symmetry? (true), steps?} -> literal changes, error (mm), locality, diff stats',
  set: '{literal: selector, value}',
  pieces: '{cell? (0.008 m)} -> how many separate pieces the shape has (a floating part shows up here)',
  reset: 'discard unsaved edits',
  fit: '{target: {png} | {source}, literals: [selectors], view? (side), res? (256), maxIter? (40), prior? (0)} -> fitted literals, IoU, convergence log',
  diff: 'the diff the current edits would write',
  write: 'write the edits back into the source (the CLI saves the file), verify by re-lifting',
  help: 'this list',
};

export class Api {
  constructor(gpu, { save, base = '' }) {
    this.gpu = gpu;
    this.save = save;          // async (name, blob|string) -> path
    this.base = base;          // URL prefix for files the job references
    this.n = 0;
    this.lens = null;
  }

  async loadSource(url, name) {
    const src = await (await fetch(url)).text();
    if (this.lens) this.lens.destroy();
    this.lens = new Lens(this.gpu);
    const t = await this.lens.load(src, { name });
    return t;
  }

  async exec(cmd) {
    const t0 = performance.now();
    let out;
    try {
      out = await this.run(cmd);
    } catch (e) {
      console.error(`api ${cmd.op}: ${e.message}`);
      out = { error: e.message };
    }
    return { op: cmd.op, ...out, ms: r4(performance.now() - t0) };
  }

  async run(c) {
    const lens = this.lens;
    const need = () => { if (!lens) throw new Error('no source loaded'); };
    switch (c.op) {
      case 'help': return { commands: COMMANDS };
      case 'load': {
        const t = await this.loadSource(this.base + c.source, c.name || c.source);
        return { loaded: c.source, ...t, ...this.info() };
      }
      case 'info': need(); return this.info();
      case 'literals': {
        need();
        let ids = lens.L.literals.map(l => l.id);
        if (c.filter && typeof c.filter !== 'object') ids = lens.resolve(c.filter, { fieldOnly: false });
        else if (c.filter) {
          const f = c.filter;
          ids = ids.filter(id => {
            const l = lens.L.literals[id];
            return (!f.fn || l.fn === f.fn) && (!f.const || l.constName === f.const) && (!f.line || l.line === f.line) && (f.inField == null || l.inField === f.inField);
          });
        }
        return { count: ids.length, literals: ids.map(id => ({ ...lens.literalInfo(id), inField: lens.L.literals[id].inField, source: lens.L.literals[id].source })) };
      }
      case 'render':
      case 'isolate': {
        need();
        const mode = c.op === 'isolate' ? 'isolate' : (c.mode || 'shaded');
        const r = await lens.render({ view: c.view || (c.op === 'isolate' ? 'side' : 'sheet'), size: c.size, mode, part: c.part, literal: c.literal, shadows: c.shadows !== false, scale: c.scale || 1 });
        const img = await lens.readImage(r);
        const name = c.name || `${++this.n}-${mode}-${r.layout.name}${c.part != null ? '-' + c.part : ''}`;
        const path = await this.save(`${name}.png`, await rgbaToPng(img));
        return { image: path, width: img.width, height: img.height, views: viewsInfo(r.layout), gpuMs: r4(r.ms) };
      }
      case 'query': {
        need();
        const q = await lens.query(c, { top: c.top || 12 });
        delete q.grads;
        return q;
      }
      case 'probe': {
        need();
        const p = await lens.probePoint(c.point);
        const parts = lens.L.parts.map((x, i) => ({ name: x.name, d: r4(p.partValues[i]) }));
        return { point: c.point, d: r4(p.d), inside: p.d < 0, winningPart: lens.L.parts[p.part] ? lens.L.parts[p.part].name : null, parts, gradLen: r4(p.gradLen) };
      }
      case 'ray': {
        need();
        const [h] = await lens.probe([{ o: c.o, d: v3.norm(c.d), tmax: c.tmax || 40 }]);
        return { hit: h.hit, point: h.point.map(r4), distance: r4(h.t), normal: h.normal.map(r4), part: lens.L.parts[h.part] ? lens.L.parts[h.part].name : null };
      }
      case 'project': {
        need();
        const lay = makeLayout(c.view || 'side', c.size || (c.view === 'sheet' ? 1024 : 512), lens.frame);
        return { pixel: project(lay, c.point, c.tile || null).map(x => Math.round(x * 10) / 10) };
      }
      case 'move': {
        need();
        return await lens.move(c.from, c.to, { k: c.k || 8, literals: c.literals || null, anchors: c.anchors !== false, steps: c.steps || 1, prune: c.prune !== false,
          patch: c.patch !== false, select: c.select || 'residual', falloff: c.falloff || null, symmetry: c.symmetry !== false, includeHelpers: !!c.includeHelpers });
      }
      case 'set': {
        need();
        const ids = lens.resolve(c.literal, { fieldOnly: false });
        for (const id of ids) lens.values[id] = c.value;
        lens.upload();
        return { set: ids.map(id => lens.L.literals[id].name), value: c.value };
      }
      case 'reset': need(); lens.reset(); return { reset: true };
      case 'pieces': need(); return await lens.pieces(c.cell || 0.008);
      case 'fit': {
        need();
        const res = c.res || 256;
        let mask;
        if (c.target.png) mask = await loadMask(this.base + c.target.png, res);
        else if (c.target.source) {
          const tl = new Lens(this.gpu);
          await tl.load(await (await fetch(this.base + c.target.source)).text(), { name: 'target' });
          const s = await tl.silhouette(c.view || 'side', res);
          mask = Uint8Array.from(s.m, m => (m < 0 ? 1 : 0));
          tl.destroy();
        } else throw new Error('fit: target needs {png} or {source}');
        const f = await lens.fit({ target: mask, view: c.view || 'side', literals: c.literals, res, maxIter: c.maxIter || 40, prior: c.prior || 0 });
        return f;
      }
      case 'diff': {
        need();
        const wb = lens.writeBack({ policy: c.policy || 'adaptive' });
        return { changes: wb.changes, diff: wb.diff, changedLines: wb.changedLines, verified: wb.verified };
      }
      case 'write': {
        need();
        const wb = lens.writeBack({ policy: c.policy || 'adaptive' });
        if (!wb.verified) throw new Error('write-back did not verify: ' + wb.problems.join('; '));
        lens.commit(wb);
        const path = await this.save(c.name || 'written.wgsl', wb.source);
        return { written: path, source: wb.source, changes: wb.changes, diff: wb.diff, changedLines: wb.changedLines, changedChars: wb.changedChars, ms: wb.ms };
      }
      default: throw new Error(`unknown op '${c.op}'; try {"op":"help"}`);
    }
  }

  info() {
    const L = this.lens.L;
    const byReason = {};
    for (const u of L.unlifted) byReason[u.reason] = (byReason[u.reason] || 0) + 1;
    return {
      name: L.name, entry: L.entry,
      literals: { lifted: L.literals.length, reachField: L.literals.filter(l => l.inField).length, unlifted: L.unlifted.length, unliftedByReason: byReason,
        libraryFloatLiterals: this.gpu.libFloatLiterals, libraryNote: 'lib.wgsl is shared code; its literals are not lifted' },
      unlifted: L.unlifted.map(u => ({ line: u.line, col: u.col, text: u.text, reason: u.reason })),
      parts: L.parts.map(p => ({ index: p.index, name: p.name, callLine: p.line, fnLine: p.fnLine, fns: p.fns, consts: p.consts })),
      notes: L.notes, liftMs: r4(L.ms), compileMs: r4(this.lens.compileMs),
    };
  }
}

export function viewsInfo(lay) {
  return lay.views.map(v => ({
    name: v.name, x0: v.x0, y0: v.y0, size: lay.tile, ortho: v.ortho,
    metresPerPixel: v.ortho ? r4(2 * v.half / lay.tile * 1e3) / 1e3 : null,
    centre: v.o.map(r4), forward: v.f.map(r4), right: v.r.map(r4), up: v.u.map(r4),
    note: v.ortho ? 'orthographic: pixel (x, y) = centre + right*((x - x0)/size*2-1)*half - up*((y - y0)/size*2-1)*half' : 'perspective',
  }));
}

export { metresPerPixel, maskToRgba };
