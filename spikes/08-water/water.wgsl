// The water: a bandlimited surface field (summed waves plus noise, each faded by the pixel
// footprint projected along its own direction), its intersection, foam, and the water body's
// optics (absorption, in-scattering, caustics on the bed). Appended to world + scene.

const ETA: f32 = 0.7518797;                 // air → water, 1/1.33
const SIG = vec3f(0.42, 0.15, 0.20);        // extinction per metre: a green-brown forest lake
const SCAT = vec3f(0.011, 0.024, 0.019);    // deep-water colour per unit of light above the surface
const BASE_VAR: f32 = 0.0004;               // slope variance always present (σ = 0.02)
const STREAM_L: f32 = 0.35;                 // slope bound of the stream's ripples
const STREAM_A: f32 = 0.04;                // bound on the stream's ripple height: 0.010·(0.6 + 0.4·4.1)·(1 + 0.5 + 0.25) = 0.039
const FALL_L: f32 = 5.1;                    // slope bound of the stream's level in the chute (2.0 · 1.5 / 0.6)

struct WS { h: f32, g: vec2f, vpar: f32, vperp: f32 }   // height, gradient, slope variance filtered out (along / across the view)

fn wave_off() -> u32 { return select(select(18u, 6u, WAVES == 1u), 0u, WAVES == 0u); }
fn wave_cnt() -> u32 { return select(select(24u, 12u, WAVES == 1u), 6u, WAVES == 0u); }
fn noise_oct() -> u32 { return select(select(4u, 2u, WAVES == 1u), 0u, WAVES == 0u); }

/** Calm and ruffled patches ("cat's paws") drifting with the wind. */
fn wind_patch(xz: vec2f) -> f32 {
  return 0.12 + 0.88 * smoothstep(-0.3, 0.45, tnoise2(xz * 0.055 + F.wind.xy * F.misc.x * 0.03 + 11.0));
}

/** The pixel's footprint on the water at distance t: along the view (stretched by 1/|d.y|) and
 *  across it. */
fn footprints(t: f32, d: vec3f, theta: f32) -> vec2f {
  let fc = t * theta;
  return vec2f(fc / max(abs(d.y), 0.015), fc);
}

/** The footprint on the water's macro-surface: horizontal for the lake; for the stream, tilted by
 *  its level's slope (in the chute it faces the viewer, so it isn't stretched). */
fn surface_fp(p: vec3f, d: vec3f, t: f32, theta: f32, kind: u32) -> vec2f {
  if (kind != 2u) { return footprints(t, d, theta); }
  let n0 = normalize(vec3f(-stream_level_d(stream_sl(p.xz).x) * S_U.x, 1.0, -stream_level_d(stream_sl(p.xz).x) * S_U.y));
  let fc = t * theta;
  return vec2f(fc / max(abs(dot(d, n0)), 0.015), fc);
}

fn view_xz(d: vec3f) -> vec2f {
  let l = length(d.xz);
  return select(vec2f(1.0, 0.0), d.xz / max(l, 1e-6), l > 1e-6);
}

fn lake_waves(xz: vec2f, fpa: f32, fpc: f32, vd: vec2f) -> WS {
  var o = WS(0.0, vec2f(0.0), 0.0, 0.0);
  let t = F.misc.x;
  let m = wind_patch(xz) * F.wind.z;
  let off = wave_off();
  let n = wave_cnt();
  for (var i = 0u; i < n; i++) {
    let w = WV[off + i];
    let dir = w.a.xy;
    let k = w.a.z;
    let amp = w.a.w * select(1.0, m, k > 3.0);       // ripples shorter than 2 m follow the wind
    let c = dot(dir, vd);
    let fpk = sqrt(fpa * fpa * c * c + fpc * fpc * (1.0 - c * c));
    let wt = band(fpk, k * 0.15915494);
    let sv = amp * k;
    let rem = (1.0 - wt * wt) * 0.5 * sv * sv;
    o.vpar += rem * c * c;
    o.vperp += rem * (1.0 - c * c);
    if (wt <= 0.0) { continue; }                     // filtered out: only its slope variance remains
    let ph = k * dot(dir, xz) - w.b.y * t + w.b.x;
    let a = amp * wt;
    o.h += a * sin(ph);
    o.g += (a * k * cos(ph)) * dir;
  }
  // Fine ripples: noise octaves drifting downwind, filtered by the longer footprint axis.
  let fpm = max(fpa, fpc);
  var f = 6.0;
  var amp = 0.0026 * m;
  for (var j = 0u; j < noise_oct(); j++) {
    let wt = band(fpm, f);
    let sv = amp * f * 1.2;
    let rem = (1.0 - wt * wt) * 0.25 * sv * sv;
    o.vpar += rem;
    o.vperp += rem;
    if (wt > 0.0) {
      let q = (xz - F.wind.xy * (0.55 - 0.08 * f32(j)) * t) * f + vec2f(f32(j) * 13.7, f32(j) * 5.1);
      let ng = tnoise2_g(q);
      o.h += amp * wt * ng.x;
      o.g += (amp * wt * f) * ng.yz;
    }
    f *= 2.0;
    amp *= 0.55;
  }
  return o;
}

fn stream_speed(s: f32) -> f32 { return 0.5 + 1.2 * clamp(stream_level_d(s) * 8.0, 0.0, 3.0); }

/** The stream's surface: its level (with the chute) plus ripples stretched along the flow. */
fn stream_waves(xz: vec2f, sl: vec2f, fp: f32) -> WS {
  var o = WS(stream_level(sl.x), stream_level_d(sl.x) * S_U, 0.0, 0.0);
  let t = F.misc.x;
  let speed = stream_speed(sl.x);
  var f = 2.2;
  var amp = 0.010 * (0.6 + 0.4 * speed);
  for (var j = 0u; j < 3u; j++) {
    let wt = band(fp, f);
    let q = vec2f((sl.x + speed * t) * f * 0.55, sl.y * f) + vec2f(f32(j) * 7.3, f32(j) * 2.9);
    let ng = tnoise2_g(q);
    o.h += amp * wt * ng.x;
    o.g += (amp * wt * f) * (ng.y * 0.55 * S_U + ng.z * S_V);
    let sv = amp * f * 1.2;
    let rem = (1.0 - wt * wt) * 0.25 * sv * sv;
    o.vpar += rem;
    o.vperp += rem;
    f *= 2.1;
    amp *= 0.5;
  }
  return o;
}

fn water_at(p: vec3f, kind: u32, fp: vec2f, vd: vec2f) -> WS {
  if (kind == 2u) { return stream_waves(p.xz, stream_sl(p.xz), max(fp.x, fp.y)); }
  return lake_waves(p.xz, fp.x, fp.y, vd);
}

fn water_n(w: WS) -> vec3f { return normalize(vec3f(-w.g.x, 1.0, -w.g.y)); }

// ---- the intersection --------------------------------------------------------------------------------

/** Newton steps on the filtered wave field from a first guess (the mean plane's hit), kept inside
 *  the wave slab. */
fn lake_refine(o: vec3f, d: vec3f, t_in: f32, theta: f32, newton: u32) -> f32 {
  let amax = F.wv.x;
  let tlo = max((amax - o.y) / d.y, 0.0);
  let thi = (-amax - o.y) / d.y;
  let vd = view_xz(d);
  var t = t_in;
  for (var i = 0u; i < newton; i++) {
    C.wsteps += 1u;
    let p = o + d * t;
    let fp = footprints(t, d, theta);
    let w = lake_waves(p.xz, fp.x, fp.y, vd);
    let f = p.y - w.h;
    let df = min(d.y - dot(w.g, d.xz), -1e-3);
    t = clamp(t - f / df, tlo, thi);
  }
  return t;
}

/** The lake. Fast path: the mean plane, then `newton` Newton steps. The reference marches down
 *  through the wave slab with the slope bound instead (exact to 2e-5 m). */
fn lake_hit(o: vec3f, d: vec3f, theta: f32, newton: u32) -> f32 {
  if (d.y >= -1e-5) { return INF; }
  let t0 = -o.y / d.y;
  if (t0 > 400.0) { return INF; }
  if (!REF) { return lake_refine(o, d, t0, theta, newton); }
  let vd = view_xz(d);
  var t = max((F.wv.x - o.y) / d.y, 0.0);
  let rate = -d.y + F.wv.y * length(d.xz);
  for (var i = 0u; i < W_STEPS; i++) {
    C.wsteps += 1u;
    let p = o + d * t;
    let fp = footprints(t, d, theta);
    let f = p.y - lake_waves(p.xz, fp.x, fp.y, vd).h;
    if (f < 2e-5) { return t; }
    t += f / rate * STEP;
  }
  C.wcaps += 1u;
  return t;
}

/** The stream: a heightfield march in its box, with a scoped slope bound (steep only in the chute). */
fn stream_hit(o: vec3f, d: vec3f, theta: f32) -> f32 {
  let q = o.xz - S_MOUTH;
  let os = vec2f(dot(q, S_U), dot(q, S_V));
  let ds = vec2f(dot(d.xz, S_U), dot(d.xz, S_V));
  let w = S_HALF + 3.0;
  let sb = slab2(os, ds, vec2f(-0.5, -w), vec2f(S_END, w));
  let ymax = stream_level(S_END) + 0.15;
  let iy = safe_inv(d.y);
  let ya = (-0.15 - o.y) * iy;
  let yb = (ymax - o.y) * iy;
  let t0 = max(max(sb.x, min(ya, yb)), 0.0);
  let t1 = min(sb.y, max(ya, yb));
  if (t1 <= t0) { return INF; }
  let fall = slab2(os, ds, vec2f(S_FALL - 0.35, -w), vec2f(S_FALL + 0.35, w));
  let lxz = length(d.xz);
  let rflat = -d.y + S_SLOPE * abs(ds.x) + STREAM_L * lxz;
  let rfall = -d.y + FALL_L * abs(ds.x) + STREAM_L * lxz;
  var t = t0;
  var tp = t;
  var fprev = INF;
  var full = false;
  for (var i = 0u; i < W_STEPS; i++) {
    C.wsteps += 1u;
    let p = o + d * t;
    let sl = stream_sl(p.xz);
    let fp = t * theta;
    // Far from the surface, march the bare level lowered by the ripples' amplitude bound (no noise
    // to evaluate); within that band, the rippled surface.
    var f = p.y - stream_level(sl.x) - STREAM_A;
    if (f < STREAM_A) { full = true; }
    if (full) { f = p.y - stream_waves(p.xz, sl, fp).h; }
    let eps = select(max(0.1 * fp, 2e-4), 2e-5, REF);
    if (f < eps) {
      if (!REF && fprev < INF && fprev > f) { t = tp + (t - tp) * fprev / (fprev - f); }
      let sl2 = stream_sl((o + d * t).xz);
      if (abs(sl2.y) > stream_half(sl2.x) || sl2.x < -0.5) { return INF; }
      return t;
    }
    let in_fall = t >= fall.x && t <= fall.y;
    var s: f32;
    if (in_fall) {
      if (rfall <= 0.0) { return INF; }
      s = f / rfall * STEP;
    } else {
      let nx = select(INF, fall.x, fall.x > t);
      if (rflat <= 0.0) {
        if (nx >= t1) { return INF; }
        s = nx - t + 1e-3;
      } else {
        s = min(f / rflat * STEP, nx - t + 1e-3);
      }
    }
    tp = t;
    fprev = f;
    t += s;
    if (t > t1) { return INF; }
  }
  C.wcaps += 1u;
  return INF;
}

struct WHit { t: f32, kind: u32 }

fn water_hit(o: vec3f, d: vec3f, theta: f32, newton: u32) -> WHit {
  var r = WHit(INF, 0u);
  let tl = lake_hit(o, d, theta, newton);
  if (tl > 0.0 && tl < r.t) { r = WHit(tl, 1u); }
  let ts = stream_hit(o, d, theta);
  if (ts < r.t) { r = WHit(ts, 2u); }
  return r;
}

// ---- foam --------------------------------------------------------------------------------------------

fn rocks_near(p: vec3f) -> f32 {
  var best = INF;
  for (var k = 0u; k < F.rocks.x; k++) {
    let a = objs[ROCK0 + 2u * k];
    if (distance(p, a.xyz) < rock_radius(k) + 0.6) { best = min(best, rock_eval(k, p, 0.0, M_BOUND) + ROCK_DISP * a.w); }
  }
  return best;
}

/** Foam coverage where the water is fast (the chute, the plunge pool, wakes around rocks), at
 *  the inflow, and in a thin line where the lake is very shallow. Filtered: crisp up close,
 *  average coverage far away. */
fn foam(p: vec3f, kind: u32, fp: f32) -> f32 {
  let t = F.misc.x;
  var amount = 0.0;
  var q: vec2f;
  if (kind == 2u) {
    let sl = stream_sl(p.xz);
    let s = sl.x;
    let speed = stream_speed(s);
    // Streaks run along the flow: the along-coordinate is arc length, so they stretch down the fall.
    amount = 0.1 + 0.45 * smoothstep(0.25, 1.5, stream_level_d(s));
    amount += 0.7 * exp(-max(S_FALL - 0.35 - s, 0.0) / 1.5) * smoothstep(S_FALL - 0.2, S_FALL - 0.45, s);
    let chute = smoothstep(0.4, 1.2, stream_level_d(s));
    amount += 0.7 * (1.0 - smoothstep(0.0, 0.5, rocks_near(p))) * (1.0 - chute);
    amount = min(amount, 0.8);
    q = vec2f((s + stream_level(s) - speed * t) * 0.9, sl.y * 5.5);
  } else {
    amount = 0.45 * exp(-distance(p.xz, S_MOUTH) / 4.5);
    if (lake_rn(p.xz) > 0.9) {
      let depth = p.y - terrain_h(p.xz);
      amount += 0.3 * (1.0 - smoothstep(0.0, 0.04, depth));
    }
    if (amount < 0.01) { return 0.0; }
    q = (p.xz + S_U * 0.25 * t) * 3.0;
  }
  let w0 = band(fp, 4.0);
  let w1 = band(fp, 9.0);
  let n = 0.5 + 0.5 * (0.65 * w0 * tnoise2(q) + 0.35 * w1 * tnoise2(q * vec2f(1.3, 2.6) + 4.0));
  let soft = 0.06 + 0.5 * (1.0 - w0);
  let a = clamp(amount, 0.0, 1.0);
  return smoothstep(1.0 - a - soft, 1.0 - a + soft, n) * smoothstep(0.0, 0.1, a);
}

// ---- under the surface ------------------------------------------------------------------------------

/** Light just under the surface: sun (through Fresnel) and sky. */
fn water_light() -> vec3f {
  return F.sun_col.rgb * max(F.sun.y, 0.0) * 0.95 + sky_amb(1.0);
}

/** Bed or rock radiance seen through `len` metres of water, plus in-scattering (depth fog). */
fn through_water(c: vec3f, len: f32) -> vec3f {
  let tr = exp(-SIG * len);
  return c * tr + SCAT * water_light() * (1.0 - tr);
}

fn caustics(xz: vec2f, depth: f32, fp: f32) -> f32 {
  let t = F.misc.x;
  let q = xz * 1.7;
  let n1 = tnoise2(q + vec2f(t * 0.35, t * 0.22));
  let n2 = tnoise2(q * 1.37 + vec2f(-t * 0.27, t * 0.31) + 5.0);
  let r = pow(1.0 - min(abs(n1 + n2), 1.0), 5.0);
  let w = band(fp, 3.5) * (1.0 - smoothstep(0.2, 1.8, depth)) * smoothstep(0.0, 0.1, depth);
  return 1.0 + w * (r - 0.13) * 1.6;
}

fn bed_albedo(p: vec3f, fp: f32, depth: f32) -> vec3f {
  let xz = p.xz;
  let n = tnoise2(xz * 0.4 + 2.0) * 0.5 + 0.5;
  let peb = 0.5 + 0.5 * band(fp, 5.0) * tnoise2(xz * 5.0 + 9.0);
  let sand = mix(vec3f(0.16, 0.135, 0.095), vec3f(0.075, 0.068, 0.055), smoothstep(0.35, 0.75, peb));
  let silt = vec3f(0.05, 0.045, 0.03) * (0.8 + 0.4 * n);
  var c = mix(sand, silt, smoothstep(0.25, 1.2, depth + 0.5 * n));
  let leaf = smoothstep(0.62, 0.8, tnoise2(xz * 1.7 + 3.0) * 0.5 + 0.5) * band(fp, 1.7);  // sunken leaves and twigs
  c = mix(c, vec3f(0.06, 0.04, 0.022), leaf * 0.7);
  let weed = smoothstep(0.55, 0.8, tnoise2(xz * 0.25 + 17.0) * 0.5 + 0.5) * smoothstep(0.6, 1.2, depth);
  c = mix(c, vec3f(0.03, 0.055, 0.02), weed * 0.8);
  return c;
}

/** Bed (or submerged rock) lit through the water: sun attenuated along its refracted path, with
 *  procedural caustics, and attenuated sky light. */
fn shade_under(h: Hit, p: vec3f, fp: f32, level: f32) -> vec3f {
  let depth = max(level - p.y, 0.0);
  var n = vec3f(0.0, 1.0, 0.0);
  var alb: vec3f;
  if (h.kind == K_ROCK) {
    n = obj_normal(h, p, fp);
    alb = rock_albedo(h.id, p, n, fp);
  } else {
    n = terrain_nh(p.xz, fp);
    alb = bed_albedo(p, fp, depth);
  }
  let sw = refract(-F.sun.xyz, vec3f(0.0, 1.0, 0.0), ETA);
  let path = depth / max(-sw.y, 0.2);
  let ndl = max(dot(n, -sw), 0.0);
  let sun = F.sun_col.rgb * ndl * exp(-SIG * path) * caustics(p.xz, depth, fp) * 0.95;
  let skyl = sky_amb(n.y) * exp(-SIG * depth * 1.3) * 0.9;
  return alb * (sun + skyl);
}

/** Underwater: terrain (the bed) and rocks, from p along d, up to len. */
fn trace_under(o: vec3f, d: vec3f, len: f32, cone: Cone) -> Hit {
  var best = Hit(INF, K_SKY, 0u);
  var lim = len;
  var dummy = 1.0;
  let tt = terrain_trace(o, d, 0.0, lim, cone);
  if (tt < lim) { best = Hit(tt, K_TERRAIN, 0u); lim = tt; }
  let hr = trace_rocks(o, d, 0.0, lim, cone, false, &dummy);
  if (hr.t < lim) { best = hr; }
  return best;
}
