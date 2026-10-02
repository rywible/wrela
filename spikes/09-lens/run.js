// #run: scripted tests of every operation, through the same calls the UI and the agent API make.
// Saves results/run-<timestamp>.json, screenshots, and finally results/DONE.

import { Lens, makeLayout, project, rgbaToPng, maskToRgba, loadMask, v3, DEFAULT_FRAME } from './lens.js';
import { Api } from './api.js';
import { writeBack, diffLines } from './lift.js';

const r3 = x => Math.round(x * 1e3) / 1e3;
const r4 = x => Math.round(x * 1e4) / 1e4;
const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[s.length >> 1] : NaN; };
const pct = (a, p) => { const s = [...a].sort((x, y) => x - y); return s.length ? s[Math.min(s.length - 1, Math.floor(p * s.length))] : NaN; };
const stats = a => ({ n: a.length, median: r3(median(a)), p90: r3(pct(a, 0.9)), min: r3(Math.min(...a)), max: r3(Math.max(...a)) });
const fetchText = async url => (await fetch(url)).text();

export async function runAll(gpu, { save, log }) {
  const T0 = performance.now();
  const started = new Date().toISOString();
  const stamp = started.replace(/[:.]/g, '-');
  const R = {
    started, device: { ...gpu.adapterInfo, userAgent: navigator.userAgent, timestampQuery: gpu.timestamp },
    note: 'Latencies are wall-clock in the page (call to result), indicative: other spikes share the GPU while this one is built.',
  };
  const shot = async (lens, r, name, draw = null) => {
    const img = await lens.readImage(r);
    await save(`${name}.png`, await rgbaToPng(img, draw));
    return img;
  };
  const section = async (name, f) => {
    const t = performance.now();
    try { R[name] = await f(); } catch (e) { console.error(`${name} FAILED: ${e.stack || e.message}`); R[name] = { error: e.message }; }
    log(`${name}: ${Math.round(performance.now() - t)} ms`);
    await save(`run-${stamp}.json`, JSON.stringify(R, null, 1));
  };

  const wolfSrc = await fetchText('subjects/wolf.wgsl');
  const lens = new Lens(gpu);
  let ref = null;

  // ---- 1. lifting ----
  await section('lift', async () => {
    const t = await lens.load(wolfSrc, { name: 'wolf.wgsl', salt: true });
    const L = lens.L;
    const byReason = {};
    for (const u of L.unlifted) byReason[u.reason] = (byReason[u.reason] || 0) + 1;
    const noEdit = writeBack(L, lens.values);
    const tRound = performance.now();
    const roundTrip = noEdit.source === wolfSrc;
    return {
      sourceLines: wolfSrc.split('\n').length, liftMs: r3(t.liftMs), compileMs: r3(t.compileMs),
      compileNote: 'cold: the shader is salted so no cache can serve it; two modules (fast, finite-difference), six pipelines',
      literalsLifted: L.literals.length, reachField: L.literals.filter(l => l.inField).length,
      fromConsts: L.literals.filter(l => l.kind === 'const').length, inFunctions: L.literals.filter(l => l.kind === 'fn').length,
      unlifted: L.unlifted.length, unliftedByReason: byReason, unliftedList: L.unlifted.map(u => `${u.line}:${u.col} ${u.text}`),
      libraryFloatLiteralsNotLifted: gpu.libFloatLiterals,
      constsLifted: L.consts.filter(c => c.lifted).length, constsTotal: L.consts.length,
      parts: L.parts.map(p => ({ name: p.name, callLine: p.line, fns: p.fns, consts: p.consts })),
      sharedHelperLiterals: L.literals.filter(l => l.inField && l.sites > 4).map(l => ({ name: l.name, text: l.text, sites: l.sites })),
      notes: L.notes, roundTripIdentical: roundTrip, roundTripMs: r3(noEdit.ms + performance.now() - tRound),
    };
  });

  // ---- 2. correctness and the cost of lifting ----
  await section('correctness', async () => {
    ref = new Lens(gpu);
    const tr = await ref.load(wolfSrc, { name: 'wolf (unlifted)', lifted: false, salt: true });
    const a = await lens.readImage(await lens.render({ view: 'sheet', size: 1024 }));
    const b = await ref.readImage(await ref.render({ view: 'sheet', size: 1024 }));
    let sum = 0, over8 = 0, mx = 0;
    for (let i = 0; i < a.data.length; i += 4) {
      let m = 0;
      for (let c = 0; c < 3; c++) { const d = Math.abs(a.data[i + c] - b.data[i + c]); sum += d; m = Math.max(m, d); }
      if (m > 8) over8++;
      mx = Math.max(mx, m);
    }
    const n = a.data.length / 4;
    await save('sheet-shaded.png', await rgbaToPng(a));
    const nb = await lens.readImage(await lens.render({ view: 'sheet', size: 1024, bounds: false }));
    let bDiff = 0;
    for (let i = 0; i < a.data.length; i += 4) if (Math.max(Math.abs(a.data[i] - nb.data[i]), Math.abs(a.data[i + 1] - nb.data[i + 1]), Math.abs(a.data[i + 2] - nb.data[i + 2])) > 8) bDiff++;
    const times = {};
    const timeIt = async (name, l, opts, k = 5) => { const ts = []; for (let i = 0; i < k; i++) ts.push((await l.render(opts)).ms); times[name] = stats(ts); };
    await timeIt('unlifted side 512 shaded+shadows', ref, { view: 'side', size: 512 });
    await timeIt('lifted side 512 shaded+shadows', lens, { view: 'side', size: 512 });
    await timeIt('lifted side 512 shaded+shadows, no bounds culling', lens, { view: 'side', size: 512, bounds: false }, 3);
    await timeIt('unlifted side 512 no shadows', ref, { view: 'side', size: 512, shadows: false });
    await timeIt('lifted side 512 no shadows (drag preview)', lens, { view: 'side', size: 512, shadows: false });
    await timeIt('lifted side 384 no shadows', lens, { view: 'side', size: 384, shadows: false });
    await timeIt('lifted sheet 1024 shaded+shadows', lens, { view: 'sheet', size: 1024 }, 3);
    await timeIt('lifted side 512 influence view (finite-difference module)', lens, { view: 'side', size: 512, mode: 'influence', literal: 'J_HEAD.y' }, 3);
    ref.destroy();
    return {
      vsUnlifted: { meanAbs255: r4(sum / (n * 3)), shareOver8: r4(over8 / n), max255: mx, pixels: n },
      boundsCullingVsNone: { pixelsOver8: bDiff, bounds: lens.bounds },
      unliftedCompileMs: r3(tr.compileMs), renderMs: times,
      note: 'Render times: wall-clock from submit to onSubmittedWorkDone, median of 5 (3 for the sheet).',
    };
  });

  // ---- 3. views: parts, isolate, tint, influence ----
  await section('views', async () => {
    const out = {};
    await shot(lens, await lens.render({ view: 'sheet', mode: 'parts' }), 'sheet-parts');
    const tiles = [];
    for (const p of lens.L.parts) tiles.push(await lens.readImage(await lens.render({ view: 'side', size: 384, mode: 'isolate', part: p.name })));
    const c = new OffscreenCanvas(384 * 3, 384 * 2), g = c.getContext('2d');
    tiles.forEach((im, i) => {
      g.putImageData(new ImageData(im.data, 384, 384), (i % 3) * 384, Math.floor(i / 3) * 384);
      g.fillStyle = '#000'; g.font = '16px system-ui'; g.fillText(`isolate: ${lens.L.parts[i].name}`, (i % 3) * 384 + 8, Math.floor(i / 3) * 384 + 20);
    });
    await save('side-isolate-parts.png', await c.convertToBlob({ type: 'image/png' }));
    await shot(lens, await lens.render({ view: 'side', mode: 'tint', part: 'sd_hindleg' }), 'side-tint-hindleg');
    for (const [lit, scale] of [['J_HEAD.y', 1], ['EAR_H', 1], ['@39:1.0', 0.3], ['J_STIFLE.z', 1]]) {
      const r = await lens.render({ view: 'side', mode: 'influence', literal: lit, scale });
      const id = lens.resolve(lit, { fieldOnly: false })[0];
      const name = lens.L.literals[id].name;
      await shot(lens, r, `side-influence-${name.replace(/[^\w.]/g, '_')}`, gg => { gg.fillStyle = '#000'; gg.font = '15px system-ui'; gg.fillText(`influence of ${name} (line ${lens.L.literals[id].line}, ${lens.L.literals[id].sites} sites): orange = surface moves out as it grows`, 8, 20); });
      out[name] = { gpuMs: r3(r.ms) };
    }
    return { influence: out, isolated: lens.L.parts.map(p => p.name) };
  });

  // ---- 4. click to source ----
  const clicks = [
    ['nose', [0.02, 0.87, 0.80], 'sd_head'], ['eye', [0.045, 0.919, 0.654], 'sd_head'], ['ear', [0.07, 1.02, 0.56], 'sd_head'],
    ['throat', [0.05, 0.74, 0.45], 'sd_neck'], ['ribcage', [0.13, 0.62, 0.10], 'sd_torso'], ['loin', [0.06, 0.70, -0.18], 'sd_torso'],
    ['forearm', [0.08, 0.25, 0.265], 'sd_foreleg'], ['forepaw', [0.07, 0.03, 0.33], 'sd_foreleg'], ['thigh', [0.10, 0.50, -0.42], 'sd_hindleg'],
    ['hock', [0.08, 0.20, -0.50], 'sd_hindleg'], ['tail middle', [0.0, 0.50, -0.70], 'sd_tail'], ['tail tip', [0.0, 0.30, -0.69], 'sd_tail'],
  ];
  await section('clickToSource', async () => {
    const lay = makeLayout('side', 512);
    const rows = [], times = [];
    const cold = await lens.query({ view: 'side', pixel: [256, 256], size: 512 });
    for (const [name, p, expect] of clicks) {
      const px = project(lay, p);
      let q;
      for (let k = 0; k < 3; k++) { q = await lens.query({ view: 'side', pixel: px, size: 512 }); times.push(q.ms.total); }
      const top = q.literals[0];
      rows.push({
        landmark: name, pixel: px.map(x => Math.round(x)), hit: q.hit, point: q.point.map(r4), part: q.part && q.part.name, expected: expect, partOk: !!q.part && q.part.name === expect,
        topLiteral: top && `${top.name} = ${top.text} (line ${top.line})`, topInPart: !!top && top.inPart,
        top5: q.literals.slice(0, 5).map(l => `${l.name} ${l.grad}`), topLines: q.lines.slice(0, 3).map(l => `${l.line} (${Math.round(l.share * 100)}%): ${l.text.trim().slice(0, 90)}`),
      });
    }
    const r = await lens.render({ view: 'side', size: 512 });
    await shot(lens, r, 'side-clicks', g => {
      g.font = '11px system-ui';
      for (const row of rows) {
        const [x, y] = row.pixel;
        g.strokeStyle = row.partOk ? '#00e05a' : '#ff3b30'; g.lineWidth = 2;
        g.beginPath(); g.arc(x, y, 5, 0, 7); g.stroke();
        g.fillStyle = '#000'; g.fillText(`${row.part}: ${(row.topLiteral || '').split(' (')[0]}`, x + 7, y - 4);
      }
    });
    return {
      partCorrect: `${rows.filter(r => r.partOk).length}/${rows.length}`, topLiteralInPart: `${rows.filter(r => r.topInPart).length}/${rows.length}`,
      latencyMs: stats(times), coldFirstQueryMs: r3(cold.ms.total), literalsDifferentiated: lens.fieldIds.length, rows,
    };
  });

  // ---- 5. drag to edit ----
  const findPoint = async (o, d) => { const [h] = await lens.probe([{ o, d: v3.norm(d), tmax: 40 }]); return h.hit === 'surface' ? h.point : null; };
  const earTip = async () => {
    const rays = [];
    for (let i = 0; i <= 10; i++) for (let j = 0; j <= 12; j++) rays.push({ o: [0.02 + 0.008 * i, 1.4, 0.48 + 0.012 * j], d: [0, -1, 0], tmax: 40 });
    const hs = (await lens.probe(rays)).filter(h => h.hit === 'surface' && h.part === 0);
    return hs.sort((a, b) => b.point[1] - a.point[1])[0].point;
  };
  const cases = async () => [
    { name: 'ear tip up 15 mm', from: await earTip(), delta: [0, 0.015, 0], view: 'side' },
    { name: 'nose forward 12 mm', from: await findPoint([0, 0.87, 2], [0, 0, -1]), delta: [0, 0, 0.012], view: 'side' },
    { name: 'belly down 10 mm', from: await findPoint([0, 0.1, 0.0], [0, 1, 0]), delta: [0, -0.010, 0], view: 'side' },
    { name: 'tail tip down 20 mm', from: await findPoint([0, 0.0, -0.69], [0, 1, 0]), delta: [0, -0.020, 0], view: 'side' },
    { name: 'back of thigh back 10 mm', from: await findPoint([0.08, 0.50, -2], [0, 0, 1]), delta: [0, 0, -0.010], view: 'side' },
    { name: 'ribcage side out 10 mm (mirrored)', from: await findPoint([2, 0.60, 0.10], [-1, 0, 0]), delta: [0.010, 0, 0], view: 'front' },
    { name: 'ear side up 15 mm (tangential to the surface)', from: await findPoint([2, 1.035, 0.56], [-1, 0, 0]), delta: [0, 0.015, 0], view: 'side' },
  ];
  let dragCases;
  const doDrag = async (c, opts = {}, { render = false, steps = 15 } = {}) => {
    lens.reset();
    const before = Float64Array.from(lens.values);
    const st = await lens.beginMove({ point: c.from }, { k: 8, ...opts });
    const target = v3.add(st.p0, c.delta);
    const solveMs = [], renderMs = [], errs = [];
    if (!st.sel) { const t = performance.now(); await lens.selectForTarget(st, target); st.selectMs = performance.now() - t; }
    for (let s = 1; s <= steps; s++) {
      const tt = v3.add(st.p0, v3.mul(c.delta, s / steps));
      const u = await lens.solveMove(st, tt, { iters: 1, measure: false });
      solveMs.push(u.ms); errs.push(u.errMm);
      if (render) { const r = await lens.render({ view: c.view, size: 512, shadows: false }); renderMs.push(r.ms); }
    }
    const fin = await lens.finishMove(st, target);
    const wb = lens.writeBack();
    const anyNaN = Array.from(lens.values).some(v => !Number.isFinite(v));
    const changedElsewhere = Array.from(lens.values).filter((v, i) => v !== before[i] && !st.sel.includes(i)).length;
    return {
      case: c.name, options: { k: 8, patch: true, select: 'residual', anchors: true, ...opts }, from: st.p0.map(r4), part: st.part && st.part.name,
      selected: fin.selected, pruned: fin.pruned, changes: fin.changes.map(x => `${x.name}: ${x.from} -> ${x.to} (line ${x.line})`),
      literalsChanged: fin.changes.length, maxAbsDelta: r4(Math.max(0, ...fin.changes.map(x => Math.abs(x.delta)))), unselectedChanged: changedElsewhere,
      errMmSolved: fin.errMmSolved, errMmAfterRounding: fin.errMm, patchRmsMm: fin.patchRmsMm, locality: fin.locality, pieces: fin.pieces,
      diff: { changedLines: wb.changedLines, changedChars: wb.changedChars, verified: wb.verified, text: wb.diff },
      latency: { beginMs: r3(st.ms), selectMs: st.selectMs != null ? r3(st.selectMs) : null, solveUpdateMs: stats(solveMs), renderUpdateMs: renderMs.length ? stats(renderMs) : null, finishMs: r3(fin.ms) },
      anyNaN,
    };
  };
  await section('drags', async () => {
    dragCases = await cases();
    const out = [];
    for (const c of dragCases) {
      if (!c.from) { out.push({ case: c.name, error: 'landmark ray missed' }); continue; }
      const res = await doDrag(c, {}, { render: true });
      out.push(res);
      log(`  ${c.name}: err ${res.errMmAfterRounding} mm, ${res.literalsChanged} literals, far rms ${res.locality.far.rmsMm} max ${res.locality.far.maxMm} mm, solve ${res.latency.solveUpdateMs.median} ms, render ${res.latency.renderUpdateMs.median} ms`);
      if (c.name.startsWith('ear tip') || c.name.startsWith('nose') || c.name.startsWith('ribcage')) {
        const slug = c.name.split(' ')[0];
        const lay = makeLayout(c.view, 512);
        const mark = g => {
          const a = project(lay, res.from), b = project(lay, v3.add(res.from, c.delta));
          g.strokeStyle = '#ffcc00'; g.lineWidth = 2; g.beginPath(); g.arc(a[0], a[1], 5, 0, 7); g.stroke();
          g.fillStyle = '#ffcc00'; g.beginPath(); g.arc(b[0], b[1], 3, 0, 7); g.fill();
          g.fillStyle = '#000'; g.font = '13px system-ui'; g.fillText(`${c.name}: ${res.changes.join('; ')}`.slice(0, 120), 8, 18);
        };
        await shot(lens, await lens.render({ view: c.view, size: 512 }), `drag-${slug}-after`, mark);
        lens.reset();
        await shot(lens, await lens.render({ view: c.view, size: 512 }), `drag-${slug}-before`, mark);
      }
    }
    return out;
  });

  // Held out: these drags were added after the solver's weights were tuned on the seven above, and
  // weren't used for tuning. They're the fairer measure of the default settings.
  await section('dragsHeldOut', async () => {
    const held = [
      { name: 'forepaw toe forward 8 mm', from: await findPoint([0.07, 0.012, 2], [0, 0, -1]), delta: [0, 0, 0.008], view: 'side' },
      { name: 'hock back 10 mm', from: await findPoint([0.078, 0.21, -2], [0, 0, 1]), delta: [0, 0, -0.010], view: 'side' },
      { name: 'withers up 10 mm', from: await findPoint([0, 2, 0.2], [0, -1, 0]), delta: [0, 0.010, 0], view: 'side' },
      { name: 'croup down 10 mm', from: await findPoint([0, 2, -0.47], [0, -1, 0]), delta: [0, -0.010, 0], view: 'side' },
      { name: 'cheek ruff out 8 mm (front view)', from: await findPoint([2, 0.86, 0.55], [-1, 0, 0]), delta: [0.008, 0, 0], view: 'front' },
      { name: 'chest front forward 10 mm', from: await findPoint([0, 0.56, 2], [0, 0, -1]), delta: [0, 0, 0.010], view: 'side' },
    ];
    const out = [];
    for (const c of held) {
      if (!c.from) { out.push({ case: c.name, error: 'landmark ray missed' }); continue; }
      const r = await doDrag(c, {}, { render: false });
      out.push(r);
      log(`  held out: ${c.name}: err ${r.errMmAfterRounding} mm, ${r.literalsChanged} literals, far rms ${r.locality.far.rmsMm} max ${r.locality.far.maxMm} mm`);
    }
    return out;
  });

  await section('dragAblations', async () => {
    const out = [];
    const variants = [
      ['point constraint, influence-ranked (the brief taken literally)', { patch: false, select: 'influence' }],
      ['point constraint, residual-ranked', { patch: false, select: 'residual' }],
      ['patch, influence-ranked', { patch: true, select: 'influence' }],
      ['patch as a hard constraint (ring weight 1)', { ringWeight: 1 }],
      ['no symmetry rows', { symmetry: false }],
      ['default, anchors off (plain damped least squares)', { anchors: false }],
      ['default, K = 4', { k: 4 }], ['default, K = 16', { k: 16 }], ['default, K = every field literal', { k: 'all' }],
    ];
    for (const c of dragCases.filter(c => c.from && (c.name.startsWith('ear tip') || c.name.startsWith('belly') || c.name.startsWith('nose')))) {
      for (const [label, opts] of variants) {
        const r = await doDrag(c, opts, { steps: 5 });
        out.push({ variant: label, case: c.name, err: r.errMmAfterRounding, patchRmsMm: r.patchRmsMm, literalsChanged: r.literalsChanged, changes: r.changes, farRmsMm: r.locality.far.rmsMm, farMaxMm: r.locality.far.maxMm, nearRmsMm: r.locality.near.rmsMm, tiedRmsMm: r.locality.tied.rmsMm, changedLines: r.diff.changedLines, solveMedianMs: r.latency.solveUpdateMs.median, anyNaN: r.anyNaN });
      }
    }
    lens.reset();
    return out;
  });

  // ---- 6. write-back: several moves, one diff, then load the written file fresh and compare ----
  await section('writeBack', async () => {
    lens.reset();
    for (const c of dragCases.slice(0, 4)) {
      const st = await lens.beginMove({ point: c.from }, { k: 8 });
      const target = v3.add(st.p0, c.delta);
      await lens.solveMove(st, target, { iters: 2 });
      await lens.finishMove(st, target, { check: false });
    }
    const memImg = await lens.readImage(await lens.render({ view: 'sheet', size: 1024 }));
    const times = [];
    let wb;
    for (let i = 0; i < 5; i++) { wb = lens.writeBack(); times.push(wb.ms.total); }
    const tPut = performance.now();
    await save('edited-wolf.wgsl', wb.source);
    const putMs = performance.now() - tPut;
    await save('edited-wolf.diff', wb.diff + '\n');
    // The file is the truth: compile the written source from scratch and compare renders.
    const fresh = new Lens(gpu);
    const tf = await fresh.load(wb.source, { name: 'edited-wolf.wgsl' });
    const freshImg = await fresh.readImage(await fresh.render({ view: 'sheet', size: 1024 }));
    let sum = 0, over8 = 0;
    for (let i = 0; i < memImg.data.length; i += 4) {
      let m = 0;
      for (let c = 0; c < 3; c++) { const d = Math.abs(memImg.data[i + c] - freshImg.data[i + c]); sum += d; m = Math.max(m, d); }
      if (m > 8) over8++;
    }
    await save('sheet-edited.png', await rgbaToPng(freshImg));
    fresh.destroy();
    lens.commit(wb);
    const n = memImg.data.length / 4;
    return {
      moves: dragCases.slice(0, 4).map(c => c.name), changes: wb.changes.map(c => `${c.name}: ${c.from} -> ${c.to} (line ${c.line})`),
      changedLiterals: wb.changes.length, changedLines: wb.changedLines, changedChars: wb.changedChars, sameLineCount: wb.sameLineCount,
      verified: wb.verified, problems: wb.problems, maxRoundingErr: r4(wb.maxRoundErr),
      spliceAndVerifyMs: stats(times), putMs: r3(putMs), freshCompileMs: r3(tf.compileMs),
      freshVsInMemory: { meanAbs255: r4(sum / (n * 3)), shareOver8: r4(over8 / n) }, diff: wb.diff,
    };
  });
  lens.adoptSource(wolfSrc);   // back to the original file for the remaining tests

  // ---- 7. fit to a reference silhouette ----
  const HEADNECK = ['fn:sd_head', 'fn:sd_neck', 'fn:sd_muzzle', 'fn:sd_ear', 'fn:sd_ear_cavity', 'fn:ear_local', 'fn:head_local', 'const:J_HEAD', 'const:J_NECK', 'const:HEAD_PITCH', 'const:HEAD_SCALE', 'const:EAR_H', 'const:EAR_BASE', 'const:EYE_C'];
  await section('fit', async () => {
    const res = 256;
    const tgt = new Lens(gpu);
    const targetSrc = await fetchText('subjects/wolf-target.wgsl');
    await tgt.load(targetSrc, { name: 'wolf-target.wgsl' });
    const sameStructure = tgt.L.structure === lens.L.structure;
    const truth = Float64Array.from(tgt.values);
    const s = await tgt.silhouette('side', res);
    const mask0 = Uint8Array.from(s.m, m => (m < 0 ? 1 : 0));
    await save('target-side-silhouette.png', await rgbaToPng(maskToRgba(mask0, res)));
    const hi = 512;   // a second check at twice the resolution, against the target file itself
    const tm = Uint8Array.from((await tgt.silhouette('side', hi)).m, m => (m < 0 ? 1 : 0));
    tgt.destroy();
    const mask = await loadMask('results/target-side-silhouette.png', res);
    let same = 0;
    for (let k = 0; k < mask.length; k++) if (mask[k] === mask0[k]) same++;
    const trueChanged = [...truth.keys()].filter(i => truth[i] !== lens.base[i]).map(i => lens.L.literals[i].name);
    const subsets = [
      ['A: the 3 changed literals', ['J_HEAD.y', 'J_HEAD.z', 'EAR_H']],
      ['B: changed + 6 distractors that also shape the head and neck', ['J_HEAD.y', 'J_HEAD.z', 'EAR_H', 'HEAD_PITCH', 'HEAD_SCALE', 'J_NECK.y', 'J_NECK.z', '@105:0.122', '@105:0.075']],
      ['C: every literal in the head and neck code', HEADNECK],
      ['D: every literal in the head and neck code, with a prior toward the source values', HEADNECK, { prior: 1e-2 }],
    ];
    const out = [];
    for (const [label, sel, extra] of subsets) {
      lens.reset();
      const before = await lens.silhouette('side', res);
      const f = await lens.fit({ target: mask, literals: sel, res, maxIter: label.startsWith('C') || label.startsWith('D') ? 25 : 40, truth, ...(extra || {}) });
      const after = await lens.silhouette('side', res);
      const iouHi = Lens.iou((await lens.silhouette('side', hi)).m, tm);
      const truthRows = f.literals.filter(l => trueChanged.includes(l.name));
      const others = f.literals.filter(l => !trueChanged.includes(l.name));
      const moved = others.map(l => Math.abs(l.to - l.from)).sort((a, b) => b - a);
      out.push({
        subset: label, literals: f.literals.length, prior: f.prior, iouBefore: f.iouBefore, iouAfter: f.iouAfter, iouAfterAt512: r4(iouHi),
        iterations: f.iterations, convergedMs: f.convergedMs, totalMs: f.totalMs, metresPerPixel: f.metresPerPixel,
        recovered: truthRows.map(l => ({ name: l.name, from: l.from, fitted: l.to, truth: l.truth, errMm: l.errMm })),
        others: { count: others.length, changedOver1mm: moved.filter(x => x > 0.001).length, maxAbsChange: r4(moved[0] || 0),
          biggest: others.sort((a, b) => Math.abs(b.to - b.from) - Math.abs(a.to - a.from)).slice(0, 6).map(l => `${l.name}: ${l.from} -> ${l.to}`) },
        log: f.log.map(x => `${x.iter}: IoU ${x.iou} loss ${x.loss.toExponential(2)} active ${x.active} ${x.ms} ms`),
      });
      const c = new OffscreenCanvas(res * 2, res), g = c.getContext('2d');
      for (const [k, sil] of [[0, before], [1, after]]) {
        const img = new ImageData(res, res);
        for (let i = 0; i < res * res; i++) {
          const a = sil.m[i] < 0, b = mask[i] > 0;
          img.data.set(a && b ? [235, 235, 235, 255] : b ? [230, 60, 50, 255] : a ? [50, 120, 240, 255] : [0, 0, 0, 255], i * 4);
        }
        g.putImageData(img, k * res, 0);
      }
      g.fillStyle = '#ffcc00'; g.font = '11px system-ui';
      g.fillText(`before: IoU ${f.iouBefore}`, 6, 14); g.fillText(`after (${label.split(':')[0]}): IoU ${f.iouAfter}`, res + 6, 14);
      g.fillText('red: target only · blue: fit only · white: both', 6, res - 6);
      await save(`fit-${label[0]}.png`, await c.convertToBlob({ type: 'image/png' }));
      log(`  fit ${label}: IoU ${f.iouBefore} -> ${f.iouAfter} in ${f.convergedMs} ms (${f.iterations} it)`);
    }
    lens.reset();
    return { resolution: res, targetStructureMatches: sameStructure, truthChanged: trueChanged, pngRoundTripAgreement: r4(same / mask.length), subsets: out };
  });

  // ---- 8. robustness ----
  await section('robustness', async () => {
    const out = {};
    const nose = dragCases[1].from;
    // Literals with zero gradient at the point: nothing should change, nothing should blow up.
    lens.reset();
    let m = await lens.move({ point: nose }, { delta: [0, 0, 0.012] }, { literals: ['J_TAIL3.y', 'J_TAIL3.z'], steps: 3 });
    out.zeroGradient = { literals: ['J_TAIL3.y', 'J_TAIL3.z'], changes: m.changes.length, errMm: m.errMm, anyNaN: Array.from(lens.values).some(v => !Number.isFinite(v)) };
    // Collinear: the head bone's height and the cranium's offset both lift the top of the head.
    lens.reset();
    const crown = await findPoint([0, 1.5, 0.57], [0, -1, 0]);
    m = await lens.move({ point: crown }, { delta: [0, 0.008, 0] }, { literals: ['J_HEAD.y', '@149:0.010'], steps: 3, prune: false, anchors: false });
    out.collinear = { at: crown && crown.map(r4), literals: ['J_HEAD.y', 'cranium offset y (line 149)'], changes: m.changes.map(c => `${c.name}: ${c.from} -> ${c.to}`), errMm: m.errMm };
    // A large drag on a small primitive: the nose pad, 30 mm forward (the UI test does this). Does it detach?
    lens.reset();
    m = await lens.move({ point: nose }, { delta: [0, 0, 0.030] }, { steps: 6 });
    out.bigNoseDrag = { changes: m.changes.map(c => `${c.name}: ${c.from} -> ${c.to}`), errMm: m.errMm, pieces: m.pieces };
    // The agent names the literal; the lens computes its value.
    lens.reset();
    m = await lens.move({ point: dragCases[0].from }, { delta: [0, 0.015, 0] }, { literals: ['EAR_H'], steps: 3 });
    out.explicitLiteral = { case: 'ear tip up 15 mm, literals: [EAR_H]', changes: m.changes.map(c => `${c.name}: ${c.from} -> ${c.to}`), errMm: m.errMm, farRmsMm: m.locality.far.rmsMm, farMaxMm: m.locality.far.maxMm };
    // A larger falloff lets a big primitive move as a whole (the ribcage drag).
    lens.reset();
    m = await lens.move({ point: dragCases[5].from }, { delta: [0.010, 0, 0] }, { falloff: 0.15, steps: 3 });
    out.ribcageFalloff15cm = { changes: m.changes.map(c => `${c.name}: ${c.from} -> ${c.to}`), errMm: m.errMm, far: m.locality.far, near: m.locality.near, tied: m.locality.tied };
    // Every literal that reaches the field (~390) in one solve, no pruning.
    lens.reset();
    m = await lens.move({ point: nose }, { delta: [0, 0, 0.012] }, { k: 'all', steps: 3, prune: false });
    out.allLiterals = { selected: m.selected.length, changed: m.changes.length, errMm: m.errMm, farRmsMm: m.locality.far.rmsMm, farMaxMm: m.locality.far.maxMm, solveMedianMs: median(m.updates.map(u => u.ms)), anyNaN: Array.from(lens.values).some(v => !Number.isFinite(v)) };
    // Shared helper literal: the 1.0 in sd_ell's (k0 - 1.0) is used by 19 ellipsoids.
    out.sharedHelper = { literal: '@39:1.0', sites: lens.L.literals[lens.resolve('@39:1.0')[0]].sites, note: 'see the influence screenshot: it acts on every ellipsoid at once' };
    lens.reset();
    out.mirrorSymmetricDetected = await lens.mirrorSymmetric();
    return out;
  });

  // ---- 9. the agent API, in-page ----
  await section('api', async () => {
    const api = new Api(gpu, { save: (name, body) => save(`api-selftest-${name}`, body), base: '' });
    const cmds = [
      { op: 'load', source: 'subjects/wolf.wgsl' },
      { op: 'query', view: 'side', pixel: project(makeLayout('side', 512), [0.02, 0.87, 0.80]).map(Math.round), size: 512 },
      { op: 'probe', point: [0.045, 0.919, 0.654] },
      { op: 'ray', o: [0.3, 0.919, 0.654], d: [-1, 0, 0] },
      { op: 'project', point: [0, 0.87, 0.8], view: 'side' },
      { op: 'literals', filter: 'const:J_HEAD' },
      { op: 'isolate', part: 'sd_head', view: 'side', size: 256 },
      { op: 'move', from: { point: dragCases[1].from }, to: { delta: [0, 0, 0.012] } },
      { op: 'diff' },
      { op: 'render', view: 'side', size: 256, mode: 'parts', name: 'parts' },
    ];
    const outs = [];
    for (const c of cmds) outs.push(await api.exec(c));
    await save('api-selftest.json', JSON.stringify(outs, null, 1));
    return { commands: outs.map(o => ({ op: o.op, ms: o.ms, error: o.error || null })) };
  });

  R.totalMs = r3(performance.now() - T0);
  await save(`run-${stamp}.json`, JSON.stringify(R, null, 1));
  log(`run done in ${Math.round(R.totalMs / 1000)} s → results/run-${stamp}.json`);
  await save('DONE', `run-${stamp}.json`);
}
