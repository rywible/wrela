// The full run (#run): coverage search, the timing sweep, fur knobs, the raster hybrid, the reference
// comparison, screenshots, 60 Hz pacing. Saves results/run-<timestamp>.json, then results/DONE.

const COVERAGES = [0.01, 0.03, 0.10, 0.25, 0.50, 0.75, 1.00];
const TRACED = [['s', 'rigid'], ['s', 'warp'], ['v', 'rigid'], ['v', 'warp']];
const TRACED_HALF = [['s', 'rigid'], ['v', 'rigid'], ['v', 'warp']];   // 540p: surface-only and both fur configurations
const BUDGET = 2.5;

const r3 = x => Math.round(x * 1000) / 1000;
const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[s.length >> 1] : NaN; };
const pct = (a, q) => { const s = [...a].sort((x, y) => x - y); return s[Math.min(s.length - 1, Math.floor(s.length * q))]; };
const mean = a => a.reduce((s, x) => s + x, 0) / a.length;

export async function runAll(api) {
  const { setup, R, log, err, save, measure, measureRaster, stats, grab, reference, diff, draw, drawRaster, snapshot,
    paced, findDistance, coverageOf, saveDiff, savePixels, cropPixels, SCENES, covCam, FULL, HALF, T_SNAP } = api;
  const t0 = performance.now();
  const lap = label => log(`[${((performance.now() - t0) / 1000).toFixed(1)} s] ${label}`);
  try {
    await setup();
    lap('setup');

    // ---- 1. coverage: camera distances along a fixed direction, found by search ----
    R.coverage = {};
    for (const c of COVERAGES) {
      const dist = await findDistance(c >= 0.999 ? 0.999 : c);
      const covs = [];
      for (const dt of [-0.6, -0.2, 0.2, 0.6]) covs.push(await coverageOf(dist, FULL, T_SNAP + dt));   // checked at 1080p
      R.coverage[c] = { distance_m: r3(dist), coverage_mean: r3(mean(covs)), coverage_min: r3(Math.min(...covs)), coverage_max: r3(Math.max(...covs)) };
      log(`coverage ${c}`, JSON.stringify(R.coverage[c]));
    }
    lap('coverage search');

    // ---- 2. the sweep: coverage × resolution × joints × fur, plus the raster hybrid ----
    R.frames = {};
    R.creature_ms = {};
    R.raster = {};
    for (const res of [FULL, HALF]) {
      for (const c of COVERAGES) {
        const cam = covCam(R.coverage[c].distance_m);
        const key = `${res.name}/${c}`;
        const env = await measure(cam, res, 'env', 'rigid');
        R.frames[`${key}/env`] = strip(env);
        R.creature_ms[key] = {};
        for (const [v, mode] of (res === FULL ? TRACED : TRACED_HALF)) {
          const m = await measure(cam, res, v, mode);
          R.frames[`${key}/${mode}-${v}`] = strip(m);
          const per = m.perFrame.map(x => x - env.trace);
          R.creature_ms[key][`${mode}-${v}`] = { median: r3(median(per)), p95: r3(pct(per, 0.95)), refit: m.refit, bins: m.bins, trace_minus_env: r3(m.trace - env.trace), warp_L_max: m.warp_L_max, cpu_pose_ms: m.cpu_pose_ms };
        }
        const rEnv = await measureRaster(cam, res, false, false);
        const rMesh = await measureRaster(cam, res, false);
        const rShell = await measureRaster(cam, res, true);
        R.raster[key] = { ground_only: rEnv.ground, mesh: rMesh, mesh_shells: rShell };
        log(key, JSON.stringify(R.creature_ms[key]), 'raster', JSON.stringify({ mesh: rMesh.creature, shells: rShell.creature }));
      }
      lap(`sweep ${res.name}`);
    }

    // ---- 3. fur knobs at 100% coverage, 1080p, warp ----
    {
      const cam = covCam(R.coverage[1].distance_m);
      const env = R.frames[`1080p/1/env`];
      R.fur_knobs = {};
      for (const v of ['v', 'v_nolod', 'v6', 'v24', 'v_noao', 'v_nosh', 'v_e3', 's', 's_nosh_noao']) {
        const m = await measure(cam, FULL, v, 'warp');
        R.fur_knobs[v] = { creature_median: r3(m.total - env.trace), trace: m.trace, timed_frames: m.timed_frames };
      }
      log('fur knobs', JSON.stringify(R.fur_knobs));
      // Fixed overhead: the wolf entirely out of view (camera looking away), so no tile has a part.
      const away = root => ({ eye: [root[0], 1.0, root[2] + 3.0], target: [root[0] + 2.0, 0.0, root[2] + 12.0] });
      const envAway = await measure(away, FULL, 'env', 'rigid');
      R.overhead_out_of_view = {};
      for (const [v, mode] of [['s', 'rigid'], ['v', 'rigid'], ['v', 'warp']]) {
        const m = await measure(away, FULL, v, mode);
        R.overhead_out_of_view[`${mode}-${v}`] = { creature_median: r3(m.total - envAway.trace), env_trace: envAway.trace, trace: m.trace };
      }
      log('overhead out of view', JSON.stringify(R.overhead_out_of_view));
      lap('fur knobs');
    }

    // ---- 4. stats: what each pixel costs, per coverage (1080p) ----
    R.stats = {};
    for (const c of COVERAGES) {
      const cam = covCam(R.coverage[c].distance_m);
      for (const [v, mode] of TRACED) {
        const s = await stats(cam, FULL, `stats_${v}`, mode);
        R.stats[`1080p/${c}/${mode}-${v}`] = s;
      }
    }
    for (const [name, sc] of Object.entries(SCENES)) {
      for (const [v, mode] of TRACED) R.stats[`${name}/${mode}-${v}`] = await stats(sc.cam, FULL, `stats_${v}`, mode, sc.t ?? T_SNAP);
    }
    R.stats['1080p/1/warp-v_nolod'] = await stats(covCam(R.coverage[1].distance_m), FULL, 'stats_v_nolod', 'warp');
    lap('stats');

    // ---- 5. correctness against the brute-force reference ----
    R.quality = {};
    const qScenes = [
      ['cov100', covCam(R.coverage[1].distance_m), T_SNAP],
      ['elbow', SCENES.elbow.cam, SCENES.elbow.t],
      ['hock', SCENES.hock.cam, SCENES.hock.t],
    ];
    for (const [name, cam, t] of qScenes) {
      R.quality[name] = {};
      for (const mode of ['rigid', 'warp']) {
        const st = await stats(cam, FULL, 'stats_s', mode, t);
        // Joint close-ups: the centre 960 × 544 (the joint is at the centre); the 100% view: whole frame.
        const crop = name === 'cov100' ? null : [60, 34, 120, 68];
        const ref = await reference(cam, FULL, 'ref_s', mode, 1, t, crop);
        let fast = await grab(cam, FULL, 's', mode, t);
        let rpx = ref.px;
        let cpx = st.creature_px;
        if (crop) { fast = cropPixels(fast, FULL, crop); rpx = cropPixels(rpx, FULL, crop); cpx = null; }
        R.quality[name][`${mode}-s`] = { ...diff(fast, rpx, cpx), crop, ref_ms: r3(ref.ms), ref_worst_band_ms: r3(ref.worst_band_wall_ms), caps_share: st.cap_share_of_creature_px, creature_px: st.creature_px };
        if (name === 'hock') await saveDiff(`diff-${name}-${mode}-s`, fast, rpx, 960, 544);
        if (name === 'cov100') await saveDiff(`diff-${name}-${mode}-s`, fast, rpx, FULL.w, FULL.h);
      }
      log(`quality ${name}`, JSON.stringify(R.quality[name]));
    }
    // Volumetric fur: 16 jittered samples per pixel, full-detail strands, 16× the volume steps (so a
    // step's sweep along a leaning strand is below its width), on a 240 × 136 crop at 100% coverage.
    {
      const [name, cam, t] = qScenes[0];
      const crop = [105, 59, 30, 17];   // 240 × 136 px at the centre
      const cut = px => cropPixels(px, FULL, crop);
      const st = await stats(cam, FULL, 'stats_v', 'warp', t);
      const pxShare = st.creature_px / (FULL.w * FULL.h);
      const cropPx = Math.round(pxShare * 240 * 136);
      const ref16 = await reference(cam, FULL, 'ref_v', 'warp', 16, t, crop);
      const ref1 = await reference(cam, FULL, 'ref_v', 'warp', 1, t, crop);
      const fast = await grab(cam, FULL, 'v', 'warp', t);
      const nolod = await grab(cam, FULL, 'v_nolod', 'warp', t);
      const e3 = await grab(cam, FULL, 'v_e3', 'warp', t);
      const v24 = await grab(cam, FULL, 'v24', 'warp', t);
      const r16 = cut(ref16.px);
      R.quality[name]['warp-v'] = {
        crop_px: [240, 136],
        fast_vs_ref16: diff(cut(fast), r16, cropPx),
        ref1_vs_ref16: diff(cut(ref1.px), r16, cropPx),
        nolod_vs_ref16: diff(cut(nolod), r16, cropPx),
        e3_vs_ref16: diff(cut(e3), r16, cropPx),
        steps24_vs_ref16: diff(cut(v24), r16, cropPx),
        ref16_ms: r3(ref16.ms), ref16_worst_band_ms: r3(ref16.worst_band_wall_ms), caps_share: st.cap_share_of_creature_px, fur_caps: st.fur_caps,
      };
      await savePixels(`${name}-warp-v-ref16-crop`, r16, 240, 136);
      await savePixels(`${name}-warp-v-ref1-crop`, cut(ref1.px), 240, 136);
      await savePixels(`${name}-warp-v-fast-crop`, cut(fast), 240, 136);
      await saveDiff(`diff-${name}-warp-v-crop`, cut(fast), r16, 240, 136);
      log(`quality ${name} fur`, JSON.stringify(R.quality[name]['warp-v']));
    }
    lap('reference comparison');

    // ---- 6. screenshots ----
    for (const c of COVERAGES) {
      const cam = covCam(R.coverage[c].distance_m);
      await draw(cam, FULL, 'v', 'warp');
      await snapshot(`cov${Math.round(c * 100)}-warp-v`);
    }
    {
      const cam = covCam(R.coverage[1].distance_m);
      await draw(cam, FULL, 's', 'rigid'); await snapshot('cov100-rigid-s');
      await draw(cam, FULL, 'heat_v', 'warp'); await snapshot('cov100-heat-v');
      await draw(cam, FULL, 'heat_s', 'rigid'); await snapshot('cov100-heat-s');
      await draw(cam, HALF, 'v', 'warp'); await snapshot('cov100-warp-v-540p', HALF);
      await drawRaster(cam, FULL, false); await snapshot('cov100-raster-mesh');
      await drawRaster(cam, FULL, true); await snapshot('cov100-raster-shells');
    }
    for (const [name, sc] of Object.entries(SCENES)) {
      const t = sc.t ?? T_SNAP;
      for (const [v, mode] of TRACED) { await draw(sc.cam, FULL, v, mode, t); await snapshot(`${name}-${mode}-${v}`); }
      if (name === 'hero' || name === 'portrait') {
        await drawRaster(sc.cam, FULL, false, t); await snapshot(`${name}-raster-mesh`);
        await drawRaster(sc.cam, FULL, true, t); await snapshot(`${name}-raster-shells`);
        await draw(sc.cam, HALF, 'v', 'warp', t); await snapshot(`${name}-warp-v-540p`, HALF);
      }
    }
    await draw(SCENES.hero.cam, FULL, 'heat_v', 'warp'); await snapshot('hero-heat-v');
    lap('screenshots');

    // ---- 7. 60 Hz pacing ----
    R.paced = {};
    R.paced['1080p/0.1/warp-v'] = await paced(covCam(R.coverage[0.1].distance_m), FULL, 'v', 'warp');
    R.paced['540p/0.1/warp-v'] = await paced(covCam(R.coverage[0.1].distance_m), HALF, 'v', 'warp');
    log('paced', JSON.stringify(R.paced));
    lap('paced');

    R.summary = summarize(R);
    log('summary', JSON.stringify(R.summary));
  } catch (e) {
    err('FAILED:', e.stack || e.message);
    R.error = String(e.stack || e.message);
  }
  R.finished = new Date().toISOString();
  R.run_seconds = r3((performance.now() - t0) / 1000);
  await save(`run-${R.started.replace(/[:.]/g, '-')}.json`, JSON.stringify(R, null, 2));
  await save('DONE', R.error ? 'failed' : 'ok');
}

function strip(m) { const o = { ...m }; delete o.perFrame; return o; }

/** Where each configuration crosses 2.5 and 5 ms (linear in coverage between measured points). */
function crossing(points, limit) {
  for (let i = 0; i < points.length; i++) {
    if (points[i][1] > limit) {
      if (i === 0) return `< ${points[0][0]}`;
      const [c0, m0] = points[i - 1], [c1, m1] = points[i];
      return r3(c0 + (c1 - c0) * (limit - m0) / (m1 - m0));
    }
  }
  return `> ${points[points.length - 1][0]} (never crosses)`;
}

function summarize(R) {
  const out = { budget_ms: BUDGET, curves: {}, crossings: {} };
  for (const res of ['1080p', '540p']) {
    for (const [v, mode] of (res === '1080p' ? TRACED : TRACED_HALF)) {
      const k = `${mode}-${v}`;
      const pts = COVERAGES.map(c => [R.coverage[c].coverage_mean, R.creature_ms[`${res}/${c}`][k].median]);
      out.curves[`${res}/${k}`] = pts.map(([c, m]) => [r3(c), m]);
      out.crossings[`${res}/${k}`] = { at_2_5ms: crossing(pts, BUDGET), at_5ms: crossing(pts, 2 * BUDGET) };
    }
    for (const sh of ['mesh', 'mesh_shells']) {
      const pts = COVERAGES.map(c => [R.coverage[c].coverage_mean, R.raster[`${res}/${c}`][sh].creature]);
      out.curves[`${res}/raster-${sh}`] = pts.map(([c, m]) => [r3(c), m]);
      out.crossings[`${res}/raster-${sh}`] = { at_2_5ms: crossing(pts, BUDGET), at_5ms: crossing(pts, 2 * BUDGET) };
    }
  }
  const full = R.creature_ms['1080p/1'];
  out.at_100pct_1080p = Object.fromEntries(Object.entries(full).map(([k, v]) => [k, v.median]));
  out.warp_cost_ratio_100pct = r3(full['warp-v'].median / full['rigid-v'].median);
  out.warp_cost_ratio_100pct_surface = r3(full['warp-s'].median / full['rigid-s'].median);
  return out;
}
