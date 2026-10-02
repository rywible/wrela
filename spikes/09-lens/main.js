// Spike 09's page: the human UI by default; #quick (smoke test), #run (measured tests, run.js),
// #api=<job> (run a command file for lens.py), #serve=<session> (live command queue for lens.py).

import { LensGPU, Lens, makeLayout, pixelRay, project, rgbaToPng, loadMask, v3 } from './lens.js';
import { Api } from './api.js';

const $ = id => document.getElementById(id);
const log = (...a) => { const el = $('log'); if (el) { el.textContent += a.join(' ') + '\n'; el.scrollTop = 1e9; } console.log(...a); };
const r4 = x => Math.round(x * 1e4) / 1e4;
async function save(name, body) {
  try {
    const r = await fetch(`results/${name}`, { method: 'PUT', body });
    if (!r.ok) console.error('save failed', name, r.status);
  } catch (e) { console.error('save failed', name, e.message); }
  return `spikes/09-lens/results/${name}`;
}
const fetchText = async url => { const r = await fetch(url); if (!r.ok) throw new Error(`${url}: ${r.status}`); return r.text(); };

const hash = decodeURIComponent(location.hash || '');
main();

async function main() {
  let gpu;
  try {
    gpu = await LensGPU.create();
    if (hash === '#run') { const { runAll } = await import('./run.js'); await runAll(gpu, { save, log }); }
    else if (hash === '#quick' || hash === '#perf') await quick(gpu);
    else if (hash.startsWith('#api=')) await apiJob(gpu, hash.slice(5));
    else if (hash.startsWith('#serve=')) await serve(gpu, hash.slice(7));
    else if (hash === '#uitest') await uitest(gpu);
    else await ui(gpu);
  } catch (e) {
    console.error('FAILED:', e.stack || e.message);
    log('FAILED:', e.message);
    if (hash.startsWith('#')) await save('DONE', 'failed: ' + e.message);
  }
}

// ---- #quick: render, query, move, write back, once each ----
async function quick(gpu) {
  const lens = new Lens(gpu);
  const t = await lens.load(await fetchText('subjects/wolf.wgsl'), { name: 'wolf' });
  log(`loaded: lift ${r4(t.liftMs)} ms, compile ${r4(t.compileMs)} ms; ${lens.L.literals.length} lifted, ${lens.L.unlifted.length} unlifted, parts ${lens.L.parts.map(p => p.name).join(' ')}`);
  if (hash === '#perf') {
    const ref = new Lens(gpu);
    const tr = await ref.load(await fetchText('subjects/wolf.wgsl'), { name: 'wolf-unlifted', lifted: false });
    log(`unlifted compile ${r4(tr.compileMs)} ms`);
    const litConst = new Lens(gpu);
    await litConst.load(await fetchText('subjects/wolf.wgsl'), { name: 'wolf-litconst', unrollLits: true });
    const uni = new Lens(gpu);
    await uni.load(await fetchText('subjects/wolf.wgsl'), { name: 'wolf-uniform', uniform: true });
    const rows = {};
    const variants = [['unlifted (source as written)', ref], ['lifted, literals in a storage buffer (default)', lens], ['lifted, influence-view module (one literal perturbable)', lens, 'influence'],
      ['lifted, literals in a uniform buffer', uni], ['lifted structure, literals baked as constants', litConst]];
    for (let round = 0; round < 3; round++) {
      for (const [name, l, mode] of variants) {
        const r = await l.render({ view: 'side', size: 512, mode: mode || 'shaded', shadows: true, literal: mode ? 'J_HEAD.y' : null, tileSize: l === uni ? 64 : null });
        (rows[name] = rows[name] || []).push(r.ms);
      }
    }
    const med = a => [...a].sort((x, y) => x - y)[a.length >> 1];
    const out = Object.fromEntries(Object.entries(rows).map(([k, v]) => [k, r4(med(v))]));
    for (const [k, v] of Object.entries(out)) log(`perf side 512 shaded: ${v} ms  ${k}`);
    await save(`perf-${new Date().toISOString().replace(/[:.]/g, '-')}.json`, JSON.stringify({ note: 'wall-clock ms per 512x512 side render with shadows, summed over tiles (256x256; 128 for the influence module, 64 for the uniform variant), median of 3; run under the GPU lock', device: gpu.adapterInfo, out }, null, 1));
    await save('DONE', 'perf');
    return;
  }
  const r0 = await lens.render({ view: 'sheet', mode: 'shaded' });
  await save('quick-sheet.png', await rgbaToPng(await lens.readImage(r0)));
  log(`render sheet: ${r4(r0.ms)} ms in ${r0.tiles} tiles`);
  const lay = makeLayout('side', 512);
  const q = await lens.query({ view: 'side', pixel: project(lay, [0.02, 0.87, 0.80]), size: 512 });
  log(`query nose: part ${q.part && q.part.name} in ${r4(q.ms.total)} ms; top: ${q.literals.slice(0, 4).map(l => `${l.name}=${l.text} (${l.grad})`).join(', ')}`);
  const tips = (await lens.probe(Array.from({ length: 99 }, (_, i) => ({ o: [0.02 + 0.008 * (i % 11), 1.4, 0.48 + 0.012 * Math.floor(i / 11)], d: [0, -1, 0], tmax: 40 }))))
    .filter(h => h.hit === 'surface' && h.part === 0).sort((a, b) => b.point[1] - a.point[1]);
  for (const [name, from, delta] of [['ear tip up 15 mm', tips[0].point, [0, 0.015, 0]], ['nose tip forward 30 mm', (await lens.query({ ray: { o: [0, 0.87, 2], d: [0, 0, -1] } })).point, [0, 0, 0.030]]]) {
    lens.reset();
    const m = await lens.move({ point: from }, { delta }, { steps: 4 });
    log(`move ${name}: ${m.errMm} mm off; ${m.changes.map(c => `${c.name} ${c.from}->${c.to}`).join(', ')}; far rms ${m.locality.far.rmsMm} max ${m.locality.far.maxMm} mm; pieces ${m.pieces.before} -> ${m.pieces.after} (${m.pieces.ms} ms); ${Math.round(m.totalMs)} ms`);
  }
  const wb = lens.writeBack();
  log(`write-back: ${wb.changes.length} literals, ${wb.changedLines} lines, verified ${wb.verified}, ${wb.ms.total} ms\n${wb.diff}`);
  lens.reset();
  const tl = new Lens(gpu);
  await tl.load(await fetchText('subjects/wolf-target.wgsl'), { name: 'target' });
  const mask = Uint8Array.from((await tl.silhouette('side', 256)).m, m => (m < 0 ? 1 : 0));
  tl.destroy();
  const f = await lens.fit({ target: mask, literals: ['J_HEAD.y', 'J_HEAD.z', 'EAR_H'], maxIter: 10 });
  log(`fit: IoU ${f.iouBefore} -> ${f.iouAfter} in ${f.convergedMs} ms: ${f.literals.map(l => `${l.name} ${l.from}->${l.to}`).join(', ')}`);
  await save('DONE', 'quick');
}

// ---- #api=<job>: run jobs/<job>/job.json ----
async function apiJob(gpu, id) {
  const base = `jobs/${id}/`;
  const job = JSON.parse(await fetchText(base + 'job.json'));
  const api = new Api(gpu, { save: (name, body) => save(`api-${id}-${name}`, body), base });
  const outputs = [];
  if (job.source) outputs.push(await api.exec({ op: 'load', source: job.source, name: job.name || job.source }));
  for (const cmd of job.commands || []) outputs.push(await api.exec(cmd));
  await save(`api-${id}.json`, JSON.stringify({ job: id, outputs }, null, 1));
  await save('DONE', 'api');
}

// ---- #serve=<session>: poll jobs/<session>/queue.json for {seq, cmd} ----
async function serve(gpu, sid) {
  const base = `jobs/${sid}/`;
  const api = new Api(gpu, { save: (name, body) => save(`serve-${sid}-${name}`, body), base });
  const meta = JSON.parse(await fetchText(base + 'session.json'));
  const loaded = await api.exec({ op: 'load', source: meta.source, name: meta.name || meta.source });
  await save(`serve-${sid}-0.json`, JSON.stringify(loaded));
  let last = 0, idleSince = performance.now();
  for (;;) {
    let q = null;
    try { q = JSON.parse(await fetchText(base + 'queue.json')); } catch { q = null; }
    if (q && q.seq > last) {
      last = q.seq;
      idleSince = performance.now();
      if (q.cmd.op === 'quit') { await save(`serve-${sid}-${q.seq}.json`, '{"op":"quit"}'); break; }
      const out = await api.exec(q.cmd);
      await save(`serve-${sid}-${q.seq}.json`, JSON.stringify(out));
    } else {
      // An idle session still holds the GPU lock; give it back after 10 minutes.
      if (performance.now() - idleSince > 600e3) { console.error('serve: idle for 10 minutes, quitting'); break; }
      await new Promise(r => setTimeout(r, 20));
    }
  }
  await save('DONE', 'serve');
}

// ---- the human UI ----
async function ui(gpu) {
  const S = { view: 'side', mode: 'shaded', part: null, literal: null, k: 8, anchors: true, shadows: true, undo: [], q: null, drag: null, marks: [] };
  const canvas = $('view'), overlay = $('overlay');
  const ctx = canvas.getContext('webgpu');
  ctx.configure({ device: gpu.device, format: 'rgba8unorm', usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_DST, alphaMode: 'opaque' });
  const lens = new Lens(gpu);
  const t = await lens.load(await fetchText('subjects/wolf.wgsl'), { name: 'wolf.wgsl' });
  log(`wolf.wgsl: ${lens.L.literals.length} literals lifted (${lens.fieldIds.length} reach field), ${lens.L.unlifted.length} not lifted; compile ${Math.round(t.compileMs)} ms`);
  $('partSel').innerHTML = '<option value="">(none)</option>' + lens.L.parts.map(p => `<option>${p.name}</option>`).join('');
  const size = () => (S.view === 'sheet' ? 1024 : 512);

  // Rendering is coalesced: at most one in flight; a newer request replaces a queued one.
  let busy = false, queued = null;
  async function redraw(fast = false) {
    if (busy) { queued = { fast }; return; }
    busy = true;
    try {
      const sz = size();
      if (canvas.width !== sz) { canvas.width = canvas.height = overlay.width = overlay.height = sz; }
      const r = await lens.render({ view: S.view, size: sz, mode: S.mode, part: S.part, literal: S.literal, shadows: S.shadows && !fast, wait: false });
      const enc = gpu.device.createCommandEncoder();
      enc.copyTextureToTexture({ texture: r.tex.tex }, { texture: ctx.getCurrentTexture() }, [sz, sz]);
      gpu.device.queue.submit([enc.finish()]);
      await gpu.device.queue.onSubmittedWorkDone();
      S.lastRenderMs = performance.now() - (S.renderT0 || performance.now());
      drawOverlay();
    } finally {
      busy = false;
      if (queued) { const q = queued; queued = null; redraw(q.fast); }
    }
  }
  function drawOverlay() {
    const g = overlay.getContext('2d');
    g.clearRect(0, 0, overlay.width, overlay.height);
    const lay = makeLayout(S.view, size(), lens.frame);
    const s = size() / 512;
    g.lineWidth = 2 * s;
    g.font = `${12 * s}px system-ui`;
    if (S.q && S.q.hit === 'surface' && !S.drag) {
      for (const v of lay.views) {
        const p = project(lay, S.q.point, v.name);
        g.strokeStyle = '#ff3b30';
        g.beginPath(); g.arc(p[0], p[1], 6 * s, 0, 7); g.stroke();
      }
    }
    if (S.drag && S.drag.target) {
      for (const v of lay.views) {
        const a = project(lay, S.drag.st.p0, v.name), b = project(lay, S.drag.target, v.name);
        g.strokeStyle = '#ffcc00'; g.fillStyle = '#ffcc00';
        g.beginPath(); g.moveTo(a[0], a[1]); g.lineTo(b[0], b[1]); g.stroke();
        g.beginPath(); g.arc(b[0], b[1], 5 * s, 0, 7); g.fill();
      }
      g.fillStyle = '#fff';
      g.fillText(`${r4(S.drag.err || 0)} mm off · solve ${Math.round(S.drag.solveMs || 0)} ms`, 10 * s, 20 * s);
    }
  }
  const canvasPx = e => {
    const rc = overlay.getBoundingClientRect();
    return [(e.clientX - rc.left) / rc.width * overlay.width, (e.clientY - rc.top) / rc.height * overlay.height];
  };

  // ---- query panel and source panel ----
  function showQuery(q) {
    S.q = q;
    const el = $('query');
    if (q.hit !== 'surface') { $('qTitle').textContent = 'Query'; el.innerHTML = `<span class="hint">Hit: ${q.hit}. No source made this pixel.</span>`; drawOverlay(); return; }
    $('qTitle').textContent = `Part: ${q.part ? q.part.name : '?'} (line ${q.part ? q.part.fnLine : '?'}) · ${Math.round(q.ms.total)} ms`;
    const rows = q.literals.map(l => `<tr class="lit" data-id="${l.id}"><td>${l.name}</td><td class="num">${l.value}</td><td class="num">${l.grad}</td><td class="num">${l.mmPerMm}</td><td>${l.line}${l.inPart ? '' : ' ·'}</td></tr>`).join('');
    el.innerHTML = `<div class="hint">at (${q.point.map(x => x.toFixed(3)).join(', ')}) m · literals ranked by |∂d/∂literal|; "surface/unit" is how far the surface moves along its normal per unit increase.</div>
      <table><tr><th>literal</th><th>value</th><th>∂d/∂θ</th><th>surface/unit</th><th>line</th></tr>${rows}</table>
      <div class="hint" style="margin-top:6px">Lines by share of influence:</div>
      <pre>${q.lines.map(l => `${String(l.line).padStart(4)} ${Math.round(l.share * 100).toString().padStart(3)}%  ${esc(l.text.trim())}`).join('\n')}</pre>
      <div class="bar" id="litEdit"></div>`;
    el.querySelectorAll('tr.lit').forEach(tr => tr.addEventListener('click', () => selectLiteral(Number(tr.dataset.id))));
    showSource(new Set(q.lines.map(l => l.line)));
    drawOverlay();
  }
  function showMove(m) {
    $('qTitle').textContent = `Moved ${m.moveMm} mm · ${m.errMm} mm off · ${m.changes.length} literals`;
    const loc = m.locality;
    $('query').innerHTML = `<div class="hint">Selected: ${m.selected.join(', ')}${m.pruned.length ? `; pruned: ${m.pruned.join(', ')}` : ''}</div>
      <table><tr><th>literal</th><th>from</th><th>to</th><th>line</th></tr>${m.changes.map(c => `<tr class="lit" data-id="${c.id}"><td>${c.name}</td><td class="num">${c.from}</td><td class="num">${c.to}</td><td>${c.line}</td></tr>`).join('')}</table>
      <div class="hint" style="margin-top:6px">Elsewhere (beyond ${Math.round(loc.falloffMm)} mm): RMS ${loc.far.rmsMm} mm, max ${loc.far.maxMm} mm over ${loc.far.n} points. Tied to the point by the source (moved by construction, e.g. the mirror image): ${loc.tied.n} points, RMS ${loc.tied.rmsMm ?? 0} mm.</div>`;
    $('query').querySelectorAll('tr.lit').forEach(tr => tr.addEventListener('click', () => selectLiteral(Number(tr.dataset.id))));
  }
  function selectLiteral(id) {
    S.literal = id;
    S.mode = 'influence';
    $('modeSel').value = 'influence';
    const l = lens.L.literals[id];
    const box = $('litEdit') || $('query');
    const ed = document.createElement('div');
    ed.className = 'bar';
    ed.innerHTML = `<label>${l.name} (line ${l.line})</label><input type="number" step="0.001" value="${lens.values[id]}" style="width:8em"> <span class="hint">influence view shows where it acts</span>`;
    const old = $('litEditRow'); if (old) old.remove();
    ed.id = 'litEditRow';
    box.appendChild(ed);
    ed.querySelector('input').addEventListener('change', ev => {
      pushUndo();
      lens.values[id] = Number(ev.target.value);
      lens.upload();
      redraw();
      showDiff();
    });
    showSource(new Set([l.line]), id);
    redraw();
  }
  function showSource(hot = new Set(), selId = null) {
    const wb = lens.writeBack();
    const L = wb.newLift;
    const src = wb.source;
    // Wrap literal spans, line by line.
    const byLine = new Map();
    for (const l of L.literals) { if (!byLine.has(l.line)) byLine.set(l.line, []); byLine.get(l.line).push(l); }
    const lines = src.split('\n');
    let html = '';
    lines.forEach((text, i) => {
      const ln = i + 1;
      let out = '', pos = 0;
      for (const l of (byLine.get(ln) || []).sort((a, b) => a.col - b.col)) {
        const c0 = l.col - 1, c1 = c0 + (l.e - l.s);
        out += esc(text.slice(pos, c0)) + `<span class="lt${l.id === selId ? ' sel' : ''}" data-id="${l.id}" title="${l.name}">${esc(text.slice(c0, c1))}</span>`;
        pos = c1;
      }
      out += esc(text.slice(pos));
      html += `<span id="L${ln}" class="${hot.has(ln) ? 'hot' : ''}"><span class="ln">${ln}</span>${out}</span>\n`;
    });
    $('source').innerHTML = html;
    $('srcTitle').textContent = `Source: ${lens.name}${wb.changes.length ? ` (${wb.changes.length} unsaved edits)` : ''}`;
    $('source').querySelectorAll('.lt').forEach(s => s.addEventListener('click', () => selectLiteral(Number(s.dataset.id))));
    const first = [...hot].sort((a, b) => a - b)[0];
    if (first) { const el = $('L' + first); if (el) $('source').scrollTop = el.offsetTop - $('source').offsetTop - 40; }
  }
  function showDiff() {
    const wb = lens.writeBack();
    $('diff').innerHTML = wb.changes.length ? wb.diff.split('\n').map(l => l.startsWith('+') ? `<span class="add">${esc(l)}</span>` : l.startsWith('-') ? `<span class="del">${esc(l)}</span>` : esc(l)).join('\n') : 'No edits.';
    $('diff').className = wb.changes.length ? '' : 'hint';
  }
  function pushUndo() { S.undo.push(Float64Array.from(lens.values)); if (S.undo.length > 50) S.undo.shift(); }

  // ---- pointer: click to query, drag to move ----
  let down = null;
  overlay.addEventListener('pointerdown', async e => {
    try { overlay.setPointerCapture(e.pointerId); } catch { /* synthetic events in #uitest */ }
    const px = canvasPx(e);
    const at = { view: S.view, pixel: px, size: size() };
    down = { px, moved: false, qp: lens.query(at, { iso: S.mode === 'isolate' && S.part ? lens.partIndex(S.part) : -1 }) };
    const q = await down.qp;
    showQuery(q);
    log(`query ${q.hit}${q.part ? ' ' + q.part.name : ''}: ${Math.round(q.ms.total)} ms`);
  });
  let solving = false, pendingTarget = null;
  async function solveTo(target) {
    if (solving) { pendingTarget = target; return; }
    solving = true;
    try {
      const r = await lens.solveMove(S.drag.st, target, { iters: 1, measure: false });
      Object.assign(S.drag, { target, err: r.errMm, solveMs: r.ms });
      await redraw(true);
    } finally {
      solving = false;
      if (pendingTarget) { const t2 = pendingTarget; pendingTarget = null; solveTo(t2); }
    }
  }
  overlay.addEventListener('pointermove', async e => {
    if (!down || !(e.buttons & 1)) return;
    const d0 = down;
    const px = canvasPx(e);
    if (!d0.moved && Math.hypot(px[0] - d0.px[0], px[1] - d0.px[1]) < 4) return;
    if (!d0.moved) {
      d0.moved = true;
      d0.starting = (async () => {
        const q = await d0.qp;
        if (q.hit !== 'surface') return null;
        pushUndo();
        S.drag = { st: null, target: null };
        S.drag.st = await lens.beginMove({ view: S.view, pixel: d0.px, size: size() }, { k: S.k === 'all' ? 'all' : Number(S.k), anchors: S.anchors });
        log(`drag started in ${Math.round(S.drag.st.ms)} ms`);
        return S.drag.st;
      })();
    }
    const st = await d0.starting;
    if (!st || !S.drag || down !== d0) return;
    const lay = makeLayout(S.view, size(), lens.frame);
    const ray = pixelRay(lay, px[0], px[1]);
    const n = ray.view.f;
    const t = v3.dot(v3.sub(S.drag.st.p0, ray.o), n) / v3.dot(ray.d, n);
    solveTo(v3.add(ray.o, v3.mul(ray.d, t)));
  });
  overlay.addEventListener('pointerup', async () => {
    const d = down;
    down = null;
    if (!d || !d.moved) return;
    const st0 = await d.starting;
    if (!st0 || !S.drag) return;
    while (solving || pendingTarget) await new Promise(r => setTimeout(r, 5));
    const st = S.drag.st, target = S.drag.target || st.p0;
    const m = await lens.finishMove(st, target);
    S.drag = null;
    S.lastMove = m;
    log(`moved ${m.moveMm} mm: ${m.errMm} mm off, ${m.changes.length} literals: ${m.changes.map(c => `${c.name} ${c.from}→${c.to}`).join(', ')}`);
    showMove(m);
    showDiff();
    showSource(new Set(m.changes.map(c => c.line)));
    redraw();
  });

  // ---- controls ----
  $('viewSel').addEventListener('change', e => { S.view = e.target.value; S.q = null; redraw(); });
  $('modeSel').addEventListener('change', e => { S.mode = e.target.value; if (S.mode === 'influence' && S.literal == null && S.q && S.q.literals) S.literal = S.q.literals[0].id; redraw(); });
  $('partSel').addEventListener('change', e => { S.part = e.target.value || null; if (S.part && (S.mode === 'shaded')) { S.mode = 'tint'; $('modeSel').value = 'tint'; } redraw(); });
  $('kSel').addEventListener('change', e => { S.k = e.target.value; });
  $('anchorsChk').addEventListener('change', e => { S.anchors = e.target.checked; });
  $('shadowChk').addEventListener('change', e => { S.shadows = e.target.checked; redraw(); });
  $('undoBtn').addEventListener('click', () => { if (!S.undo.length) return; lens.setValues(S.undo.pop()); showDiff(); showSource(); redraw(); });
  $('resetBtn').addEventListener('click', () => { pushUndo(); lens.reset(); showDiff(); showSource(); redraw(); });
  $('writeBtn').addEventListener('click', async () => {
    const wb = lens.writeBack();
    if (!wb.changes.length) { $('status').textContent = 'nothing to write'; return; }
    if (!wb.verified) { $('status').textContent = 'write-back did not verify: ' + wb.problems.join('; '); return; }
    lens.commit(wb);
    const path = await save('ui-edited-wolf.wgsl', wb.source);
    const a = $('dl');
    a.href = URL.createObjectURL(new Blob([wb.source], { type: 'text/plain' }));
    a.download = 'wolf.wgsl';
    a.style.display = '';
    $('status').textContent = `wrote ${wb.changes.length} literals on ${wb.changedLines} lines in ${wb.ms.total} ms → ${path}`;
    log(`write-back:\n${wb.diff}`);
    showDiff();
    showSource(new Set(wb.changes.map(c => c.line)));
  });
  $('fitBtn').addEventListener('click', async () => {
    $('status').textContent = 'fitting the side silhouette to subjects/wolf-target.wgsl…';
    const tl = new Lens(gpu);
    await tl.load(await fetchText('subjects/wolf-target.wgsl'), { name: 'target' });
    const s = await tl.silhouette('side', 256);
    const mask = Uint8Array.from(s.m, m => (m < 0 ? 1 : 0));
    tl.destroy();
    pushUndo();
    S.view = 'side'; $('viewSel').value = 'side';
    const f = await lens.fit({ target: mask, literals: ['J_HEAD.y', 'J_HEAD.z', 'EAR_H', 'HEAD_PITCH', 'HEAD_SCALE', 'J_NECK.y', 'J_NECK.z', '@105:0.122', '@105:0.075'],
      onIter: async it => { $('status').textContent = `fit iteration ${it.iter}: IoU ${it.iou}`; await redraw(true); } });
    $('status').textContent = `fit: IoU ${f.iouBefore} → ${f.iouAfter} in ${Math.round(f.convergedMs)} ms`;
    log(`fit: ${f.literals.map(l => `${l.name} ${l.from}→${l.to}`).join(', ')}`);
    showDiff();
    showSource(new Set(f.literals.map(l => l.line)));
    redraw();
  });
  showSource();
  await redraw();
  window.__lens = lens;
  window.__ui = { S, redraw, canvas, overlay, size };
}

// ---- #uitest: drive the human UI with synthetic pointer events (headless check of the UI code) ----
async function uitest(gpu) {
  await ui(gpu);
  const { S, overlay, canvas } = window.__ui;
  const lens = window.__lens;
  const out = { steps: [] };
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const until = async (f, ms = 5000) => { const t = performance.now(); while (!f()) { if (performance.now() - t > ms) return false; await sleep(10); } return true; };
  const lay = makeLayout('side', 512);
  const client = p => { const rc = overlay.getBoundingClientRect(); const px = project(lay, p); return [rc.left + px[0] / 512 * rc.width, rc.top + px[1] / 512 * rc.height]; };
  const fire = (type, xy, buttons) => overlay.dispatchEvent(new PointerEvent(type, { clientX: xy[0], clientY: xy[1], buttons, pointerId: 1, bubbles: true }));
  // 1. click the nose
  const nose = [0.02, 0.87, 0.81];
  let t0 = performance.now();
  fire('pointerdown', client(nose), 1);
  fire('pointerup', client(nose), 0);
  out.steps.push({ step: 'click nose', ok: await until(() => S.q && S.q.part), part: S.q && S.q.part && S.q.part.name, title: $('qTitle').textContent, ms: Math.round(performance.now() - t0) });
  // 2. drag the nose tip 30 mm forward in 8 moves (12 mm is under the 4 px drag threshold at this zoom)
  const tip = (await lens.query({ ray: { o: [0, 0.87, 2], d: [0, 0, -1] } })).point;
  t0 = performance.now();
  fire('pointerdown', client(tip), 1);
  await sleep(30);
  for (let i = 1; i <= 8; i++) { fire('pointermove', client(v3.add(tip, [0, 0, 0.030 * i / 8])), 1); await sleep(40); }
  fire('pointerup', client(v3.add(tip, [0, 0, 0.030])), 0);
  const moved = await until(() => S.lastMove, 8000);
  out.steps.push({ step: 'drag nose +30 mm', ok: moved, changes: S.lastMove && S.lastMove.changes.map(c => `${c.name}: ${c.from} -> ${c.to}`), errMm: S.lastMove && S.lastMove.errMm, ms: Math.round(performance.now() - t0), diff: $('diff').textContent.slice(0, 400) });
  await sleep(200);
  const blob = await new Promise(r => canvas.toBlob(r, 'image/png'));
  if (blob) await save('uitest-canvas.png', blob);
  // 3. select a literal from the table: influence view
  const row = document.querySelector('tr.lit');
  if (row) row.click();
  out.steps.push({ step: 'select literal', ok: S.mode === 'influence' && S.literal != null, literal: S.literal != null ? lens.L.literals[S.literal].name : null });
  // 4. write back
  $('writeBtn').click();
  out.steps.push({ step: 'write back', ok: await until(() => /wrote/.test($('status').textContent)), status: $('status').textContent });
  // 5. undo, views and modes
  $('undoBtn').click();
  for (const [v, m] of [['sheet', 'parts'], ['front', 'tint'], ['side', 'isolate']]) {
    $('viewSel').value = v; $('viewSel').dispatchEvent(new Event('change'));
    $('modeSel').value = m; $('modeSel').dispatchEvent(new Event('change'));
    if (m !== 'parts') { $('partSel').value = 'sd_head'; $('partSel').dispatchEvent(new Event('change')); }
    await sleep(400);
  }
  out.steps.push({ step: 'views and modes', ok: true });
  out.ok = out.steps.every(s => s.ok);
  out.log = $('log').textContent.split('\n').slice(-12);
  await save('uitest.json', JSON.stringify(out, null, 1));
  log('uitest', out.ok ? 'passed' : 'FAILED', JSON.stringify(out.steps.map(s => [s.step, s.ok])));
  await save('DONE', 'uitest');
}

function esc(s) { return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
