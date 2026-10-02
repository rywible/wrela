// The whole frame in one compute kernel: ground, sky, the wolf, its fur, sun shadows and ambient
// occlusion. No triangles. Appended to common.wgsl + wolf.wgsl + posed.wgsl.

@group(0) @binding(1) var<storage, read> pose: array<vec4f>;
@group(0) @binding(2) var<storage, read> tiles: array<u32>;
@group(0) @binding(3) var<storage, read> lights: array<u32>;
@group(0) @binding(4) var<storage, read> volm: array<u32>;
@group(0) @binding(5) var out_tex: texture_storage_2d<rgba16float, write>;
@group(0) @binding(6) var<storage, read_write> stats: array<atomic<u32>, 32>;
@group(0) @binding(7) var strands: texture_2d_array<f32>;
@group(0) @binding(8) var samp: sampler;
@group(0) @binding(9) var<storage, read_write> accum: array<vec4f>;
@group(0) @binding(10) var<uniform> band: vec4u;   // x: first tile row, y: first tile column of this dispatch (frames run in bands)

// Variants are pipeline-overridable constants: each one compiles to its own kernel.
override CREATURE: bool = true;     // false: ground and sky only (subtracted to get creature time)
override FUR: u32 = 1u;             // 0: surface fur shading only; 1: volumetric fur shell
override FUR_LOD: bool = true;      // footprint LOD: strand mip, step size, and no volume when far
override REF: bool = false;         // brute-force reference: every part, no masks, fine steps
override STATS: bool = false;
override VIEW: u32 = 0u;            // 1: march + fur step heat map; 2: fur steps only
override STEP_SCALE: f32 = 0.9;     // hypothesis: the noise-free wolf field's L ≤ 1.1 (checked by the reference)
override EPS_SCALE: f32 = 0.25;     // hit tolerance, in pixel footprints
override MAX_STEPS: u32 = 192u;
override SHADOW_STEPS: u32 = 40u;
override SHADOW_SCALE: f32 = 1.0;
override FUR_STEPS: f32 = 12.0;     // volume steps across the shell at normal incidence
override AO: bool = true;
override SHADOWS: bool = true;
override RELAX: f32 = 1.0;          // over-relaxation ω (Keinert et al. 2014), falls back to 1 on a gap
override LOOK0: f32 = 0.1;          // initial lookahead for the active-part mask (m)
override FUR_EVAL: u32 = 1u;        // evaluate the field every Nth volume step; extrapolate (secant) in between
override FUR_SURF: f32 = 0.17;      // fur LOD 2 (surface shading only) when the footprint exceeds this × shell

const SOFT: f32 = 12.0;             // soft-shadow sharpness (penumbra ∝ distance / SOFT)
const SIGMA: f32 = 1100.0;          // strand extinction at full coverage (1/m)
const LEAN: f32 = 2.2;              // strand lean: comb displacement per unit of height (~24° from the skin)
const UNDER: f32 = 0.85;            // underfur coverage near the skin
const STRAND_TEXELS: f32 = 512.0;
const LAYERS: f32 = 16.0;

const S_CREATURE_PX: u32 = 0u;
const S_STEPS: u32 = 1u;
const S_EVALS: u32 = 2u;
const S_CAPS: u32 = 3u;
const S_FUR: u32 = 4u;
const S_FUR_CAPS: u32 = 5u;
const S_SEGS: u32 = 6u;
const S_SHADOW: u32 = 7u;
const S_SHADOW_CAPS: u32 = 8u;
const S_RAYS: u32 = 9u;             // pixels whose per-ray mask isn't empty
const S_RAY_PARTS: u32 = 10u;       // sum of per-ray mask sizes
const S_TILE_PARTS: u32 = 11u;      // sum of tile mask sizes over pixels
const S_SURF_PX: u32 = 12u;         // creature pixels shaded as a surface (fur LOD 2 or fur off)
const S_HIT_STEPS: u32 = 13u;       // march + fur steps on creature pixels
const S_STOPS: u32 = 14u;           // march steps cut short at a box entry
const S_JUMPS: u32 = 15u;           // empty stretches skipped between boxes

struct Counters {
  steps: u32, evals: u32, caps: u32, fur: u32, fur_caps: u32, segs: u32, sh: u32, sh_caps: u32,
  ray_parts: u32, tile_parts: u32, surf: u32, stops: u32, jumps: u32,
}

// ---- lookups into the bins -----------------------------------------------------------------------

fn light_mask(p: vec3f) -> u32 {
  let rel = p - F.lc.xyz;
  let g = f32(F.grid.x);
  let e = F.lc.w;
  let l = (vec2f(dot(rel, F.lr.xyz), dot(rel, F.lu.xyz)) + e) / (2.0 * e) * g;
  if (any(l < vec2f(0.0)) || any(l >= vec2f(g))) { return 0u; }
  return lights[u32(l.y) * F.grid.x + u32(l.x)];
}

fn vol_mask(p: vec3f) -> u32 {
  let c = (p - F.vol.xyz) / F.vol.w;
  if (any(c < vec3f(0.0)) || any(c >= vec3f(F.vdims.xyz))) { return 0u; }
  let ci = vec3u(c);
  return volm[(ci.z * F.vdims.y + ci.y) * F.vdims.x + ci.x];
}

// ---- shadows and occlusion --------------------------------------------------------------------------

/** Sun visibility from p: a soft-shadow march through the parts in p's light-grid column. */
fn shadow(p: vec3f, c: ptr<function, Counters>) -> f32 {
  if (!SHADOWS) { return 1.0; }
  let d = F.sun.xyz;
  var mask = F.vdims.w;
  if (!REF) { mask = light_mask(p); }
  if (mask == 0u) { return 1.0; }
  var pm = 0u;
  var t0 = INF;
  var t1 = 0.0;
  var L = 1.0;
  var m = mask;
  loop {
    if (m == 0u) { break; }
    let i = firstTrailingBit(m);
    m &= m - 1u;
    if (REF) { pm |= 1u << i; L = max(L, part_l(i)); continue; }
    let iv = part_interval(i, p, d);
    if (iv.y > 0.0 && iv.x <= iv.y) {
      pm |= 1u << i;
      t0 = min(t0, iv.x);
      t1 = max(t1, iv.y);
      L = max(L, part_l(i));
    }
  }
  if (pm == 0u) { return 1.0; }
  if (REF) { t0 = 0.0; t1 = 3.0; }
  let np = countOneBits(pm);
  var res = 1.0;
  var t = max(t0, 0.0);
  var ph = 1e10;
  for (var i = 0u; i < SHADOW_STEPS; i++) {
    (*c).sh += 1u;
    (*c).evals += np;
    let h = field(pm, p + d * t);
    if (h < 1e-4) { return 0.0; }
    // Closest approach between this sample and the last (Quílez's improved soft shadows).
    let y = h * h / (2.0 * ph);
    let dd = sqrt(max(h * h - y * y, 0.0));
    res = min(res, SOFT * dd / max(t - y, 1e-3));
    if (res < 0.004) { return 0.0; }
    ph = h;
    t += max(h / L * STEP_SCALE * SHADOW_SCALE, 0.003 * SHADOW_SCALE);
    if (t > t1) { return smoothstep(0.0, 1.0, res); }
  }
  (*c).sh_caps += 1u;
  return smoothstep(0.0, 1.0, res);
}

fn occ_mask(p: vec3f) -> u32 {
  if (REF) { return F.vdims.w; }
  return vol_mask(p);
}

fn ground_ao(p: vec3f, c: ptr<function, Counters>) -> f32 {
  if (!AO) { return 1.0; }
  let mask = occ_mask(p);
  if (mask == 0u) { return 1.0; }
  (*c).evals += 2u * countOneBits(mask);
  let a = field(mask, p + vec3f(0.0, 0.08, 0.0));
  let b = field(mask, p + vec3f(0.0, 0.25, 0.0));
  return (0.45 + 0.55 * clamp(a / 0.08, 0.0, 1.0)) * (0.40 + 0.60 * clamp(b / 0.25, 0.0, 1.0));
}

fn body_ao(p: vec3f, n: vec3f, c: ptr<function, Counters>) -> f32 {
  if (!AO) { return 1.0; }
  let mask = occ_mask(p);
  if (mask == 0u) { return 1.0; }
  (*c).evals += 2u * countOneBits(mask);
  let a = field(mask, p + n * 0.03);
  let b = field(mask, p + n * 0.10);
  return (0.45 + 0.55 * clamp(a / 0.03, 0.0, 1.0)) * (0.35 + 0.65 * clamp(b / 0.10, 0.0, 1.0));
}

// ---- shading frame at a hit --------------------------------------------------------------------------

struct FurFrame {
  p: vec3f,          // where the frame was taken (world)
  n: vec3f,          // surface normal (world)
  q: vec3f,          // rest-space position at p (blended across parts)
  nr: vec3f,         // normal in rest space
  ct: vec3f,         // comb direction in rest space, in the tangent plane
  rinv: mat3x3f,     // world → rest rotation (blended across parts)
  alb: vec3f,
  len: f32,          // strand length, as a fraction of the shell
  depth: f32,        // skin depth, as a fraction of the shell's inner depth (0: bare)
  T: vec3f,          // strand direction (world), for Kajiya-Kay
  head: f32,         // head weight (eyes and nose are checked per step on head rays)
  sh: f32,
  ao: f32,
  w: vec3f,          // triplanar weights
}

fn hit_frame(pm: u32, p: vec3f, raw: f32, fp: f32, c: ptr<function, Counters>) -> FurFrame {
  let np = countOneBits(pm);
  (*c).evals += 5u * np;
  var fr: FurFrame;
  fr.p = p;
  fr.n = field_n(pm, p, max(5e-4, 0.5 * fp));
  let s = field_s(pm, p);
  fr.rinv = transpose(s.rot);
  fr.q = s.q;
  fr.nr = fr.rinv * fr.n;
  let qs = s.q - fr.nr * raw;               // on the authored surface (rest space)
  let ps = p - fr.n * raw;                  // and in the world
  fr.alb = wolf_albedo(qs, s.ch);
  var comb = comb_dir(qs, s.ch);
  comb += 0.35 * vec3f(noise3(qs * 18.0), noise3(qs * 18.0 + 11.0), noise3(qs * 18.0 + 23.0));
  fr.ct = normalize(comb - fr.nr * dot(comb, fr.nr) + vec3f(1e-5, 0.0, 0.0));
  fr.T = normalize(s.rot * (fr.ct * LEAN + fr.nr));
  let coat = fur_coat(qs, s.ch);
  fr.len = coat.x;
  fr.depth = coat.y;
  fr.head = s.ch.y;
  fr.sh = 1.0;
  if (dot(fr.n, F.sun.xyz) > -0.35) { fr.sh = shadow(ps + fr.n * 0.012, c); }
  fr.ao = body_ao(ps, fr.n, c);
  var w = pow(abs(fr.nr), vec3f(4.0));
  w = select(w, vec3f(0.0), w < vec3f(0.02 * (w.x + w.y + w.z)));
  fr.w = w / (w.x + w.y + w.z);
  return fr;
}

/** Strand coverage and brightness at a point in the shell: the strand texture's layer for this
 *  height, sampled triplanar at the strand's root (projected to the skin, then back along the comb).
 *  `along`/`across` (m): the filter footprint along the comb (a volume step's sweep) and across it
 *  (the pixel footprint). Zero for both means the finest level, unfiltered (the reference). */
fn strand(fr: FurFrame, pf: vec3f, rf: f32, h: f32, along: f32, across: f32) -> vec2f {
  let hl = h / fr.len;
  if (hl >= 1.0) { return vec2f(0.0, 0.5); }
  let shell = F.fur.x + F.fur.y;
  let qs = fr.q + fr.rinv * (pf - fr.p);
  let root = qs - fr.nr * (rf + F.fur.x) - fr.ct * (LEAN * h * shell);
  let layer = i32(min(hl * LAYERS, LAYERS - 1.0));
  let s = 1.0 / F.fur.z;
  var r = vec2f(0.0);
  if (fr.w.x > 0.0) { r += fr.w.x * tri(root.zy * s, vec2f(fr.ct.z, fr.ct.y), layer, along * s, across * s); }
  if (fr.w.y > 0.0) { r += fr.w.y * tri(root.xz * s, vec2f(fr.ct.x, fr.ct.z), layer, along * s, across * s); }
  if (fr.w.z > 0.0) { r += fr.w.z * tri(root.xy * s, vec2f(fr.ct.x, fr.ct.y), layer, along * s, across * s); }
  return r;
}

/** One triplanar tap, anisotropically filtered along the comb's direction in this plane. */
fn tri(uv: vec2f, comb: vec2f, layer: i32, along: f32, across: f32) -> vec2f {
  if (along <= 0.0) { return textureSampleLevel(strands, samp, uv, layer, 0.0).rg; }
  let l = length(comb);
  let c = select(vec2f(1.0, 0.0), comb / l, l > 1e-4);
  let pp = vec2f(-c.y, c.x);
  return textureSampleGrad(strands, samp, uv, layer, c * max(along, across), pp * across).rg;
}

// ---- the wolf -------------------------------------------------------------------------------------------

struct Hit { col: vec3f, T: f32, t: f32 }   // premultiplied colour, transmittance, first-touch distance

/** Parts whose box the ray is inside at t, and where the next box starts. Parts outside their box
 *  are at least their margin away, so leaving them out of the fold is exact there; a step never
 *  crosses into the next box unevaluated. */
struct Active { m: u32, next: f32, L: f32 }

/** `look`: also take parts whose box starts within this distance ahead, so a step isn't cut short
 *  at every box entry. The step must still stop at `next`. */
fn active_at(pm: u32, ivs: ptr<function, array<vec2f, 22>>, t: f32, look: f32) -> Active {
  var a = Active(0u, INF, 1.0);
  var m = pm;
  loop {
    if (m == 0u) { break; }
    let i = firstTrailingBit(m);
    m &= m - 1u;
    if (REF) { a.m |= 1u << i; a.L = max(a.L, part_l(i)); continue; }
    let v = (*ivs)[i];
    if (v.x <= t + look) {
      if (v.y >= t) { a.m |= 1u << i; a.L = max(a.L, part_l(i)); }
    } else {
      a.next = min(a.next, v.x);
    }
  }
  return a;
}

fn creature(o: vec3f, d: vec3f, tile: u32, tmax: f32, dither: f32, c: ptr<function, Counters>) -> Hit {
  var out = Hit(vec3f(0.0), 1.0, INF);
  // Per-ray PartMask and per-part intervals: exact ray intervals against each part's boxes.
  var ivs: array<vec2f, 22>;
  var pm = 0u;
  var t0 = INF;
  var t1 = 0.0;
  var L = 1.0;
  var m = tile;
  loop {
    if (m == 0u) { break; }
    let i = firstTrailingBit(m);
    m &= m - 1u;
    if (REF) { pm |= 1u << i; L = max(L, part_l(i)); continue; }
    let iv = part_interval(i, o, d);
    if (iv.y > max(iv.x, 0.0) && iv.x < tmax) {
      pm |= 1u << i;
      ivs[i] = iv;
      t0 = min(t0, iv.x);
      t1 = max(t1, iv.y);
      L = max(L, part_l(i));
    }
  }
  (*c).tile_parts = countOneBits(tile);
  if (pm == 0u) { return out; }
  if (REF) { t0 = 0.0; t1 = min(tmax, 40.0); }
  (*c).ray_parts = countOneBits(pm);
  let hin = F.fur.x;
  let shell = F.fur.x + F.fur.y;
  let tend = min(t1, tmax);
  var t = max(t0, 0.0);
  // Fur LOD 2: a footprint over half the shell gets no volume, only surface fur shading.
  let surf = FUR == 0u || (FUR_LOD && t * F.eye.w > FUR_SURF * shell);
  let hout = select(F.fur.y, 0.0, surf);
  var have = false;
  var look = LOOK0;
  var w = RELAX;
  var t_prev = t;
  var r_prev = 0.0;
  // With fur, the entry into the shell only needs the volume step's precision.
  let eps_env = select(0.0, 0.4 * shell / FUR_STEPS, !surf);
  var fr: FurFrame;
  var direct = 0.0;
  var spec = vec3f(0.0);
  var amb = vec3f(0.0);
  var skin = vec3f(0.0);
  var i = 0u;
  loop {
    if (i >= MAX_STEPS) { (*c).caps += 1u; break; }
    let a = active_at(pm, &ivs, t, look);
    if (a.m == 0u) {
      if (a.next >= tend) { break; }
      t = a.next;
      (*c).jumps += 1u;
      continue;
    }
    i++;
    (*c).steps += 1u;
    (*c).evals += countOneBits(a.m);
    let p = o + d * t;
    let raw = field(a.m, p);
    let fp = t * F.eye.w;
    let eps = max(max(EPS_SCALE * fp, 1e-4), eps_env);
    let g = raw - hout;
    let stepk = STEP_SCALE / a.L;
    let r = max(g, 0.0) * stepk;
    if (w > 1.0 && r_prev > 0.0 && r + r_prev < t - t_prev) {
      // The relaxed step left a gap between the two unbounding spheres: take the safe step instead.
      t = t_prev + r_prev;
      w = 1.0;
      r_prev = 0.0;
      continue;
    }
    if (g >= eps) {
      let st = r * w;
      look = 1.25 * st;
      t_prev = t;
      r_prev = r;
      let tn = t + st;
      if (tn > a.next) { t = a.next; r_prev = 0.0; (*c).stops += 1u; } else { t = tn; }
      if (t > tend) { break; }
      continue;
    }
    if (out.t == INF) { out.t = t; }
    if (surf) {
      (*c).surf = 1u;
      let f2 = hit_frame(a.m, p, raw, fp, c);
      out.col = shade_fur_surface(f2.alb, f2.n, f2.T, -d, f2.sh, f2.ao);
      out.T = 0.0;
      return out;
    }

    // In the fur shell. One shading frame per ray, retaken only if this entry is far from it
    // (another body region, such as the far leg seen past the near one).
    if (!have || distance(p, fr.p) > 0.04) {
      (*c).segs += 1u;
      fr = hit_frame(a.m, p, raw, fp, c);
      have = true;
      let fl = fur_light(fr.T, fr.n, -d);
      direct = fl.diff * fl.lam * fr.sh;
      spec = (0.07 * fl.spec1 + 0.16 * fl.spec2 * fr.alb * 3.0) * fl.lam * fr.sh;
      amb = ambient(fr.n) * fr.ao;
      // The skin under a coat matches the deepest part of the pile, so where rays reach it doesn't show.
      skin = (fr.alb * (SUN_COL * direct + amb) + SUN_COL * spec) * 0.35;
    }
    let cosn = abs(dot(fr.n, d));
    // Steps sized to the pile actually there (short coats are thin), so thin piles aren't banded.
    let pile = max(fr.len, 0.15) * shell;
    var dt = pile / max(cosn, 0.25) / FUR_STEPS;
    var along = 0.0;
    var across = 0.0;
    if (FUR_LOD) {
      dt = max(dt, 0.75 * fp);
      // A step moves the lookup LEAN·dt along the comb, so filter over that sweep along the comb
      // (anisotropically) and over the pixel footprint across it; unfiltered, strands thinner than
      // the sweep alias into blotches.
      along = 0.5 * LEAN * dt;
      across = fp;
    }
    let top = fr.len * shell - hin;           // strand tips (field value); nothing above
    let skin_at = -hin * fr.depth;            // the skin (field value)
    var tf = t + dither * dt;                 // a fixed per-pixel offset turns residual banding into grain
    var k = 0u;
    let kmax = u32(FUR_STEPS * 3.0);
    var since = FUR_EVAL;
    var rf_last = raw;
    var t_last = t;
    var slope = dot(fr.n, d);                  // d(field)/dt along the ray, until two samples exist
    loop {
      if (k >= kmax) { (*c).fur_caps += 1u; break; }
      k++;
      let af = active_at(pm, &ivs, tf, dt);
      (*c).fur += 1u;
      let pf = o + d * tf;
      var rf: f32;
      if (since >= FUR_EVAL) {
        (*c).evals += countOneBits(af.m);
        rf = field(af.m, pf);
        if (tf > t_last + 1e-6) { slope = (rf - rf_last) / (tf - t_last); }
        rf_last = rf;
        t_last = tf;
        since = 0u;
      } else {
        rf = rf_last + slope * (tf - t_last);
      }
      since++;
      var bare = 0.0;
      if (fr.head > 0.3) { bare = bare_mask(fr.q + fr.rinv * (pf - fr.p)); }
      if (rf < mix(skin_at, 0.0, bare)) {      // reached the skin (eyes and nose: the surface itself)
        if (out.T > 0.05 && (fr.depth < 0.3 || bare > 0.5)) {
          // Bare or thin-coated skin (eyes, nose, claws, ears): shade it from its own frame, since
          // the entry frame was taken a shell's height away from these small features.
          let f2 = hit_frame(af.m, pf, rf, fp, c);
          let fl2 = fur_light(f2.T, f2.n, -d);
          let hh = normalize(F.sun.xyz - d);
          let gloss = pow(max(dot(f2.n, hh), 0.0), 60.0) * 0.6 * (1.0 - smoothstep(0.0, 0.1, f2.depth));
          out.col += out.T * (f2.alb * (SUN_COL * fl2.lam * f2.sh + ambient(f2.n) * f2.ao) + SUN_COL * gloss * f2.sh);
        } else {
          out.col += out.T * skin;
        }
        out.T = 0.0;
        return out;
      }
      if (rf > F.fur.y + max(eps, 0.002)) { break; }   // left the shell: march on
      if (rf > top) {                          // above the strand tips: skip the empty part
        tf += min(max(dt, (rf - top) * STEP_SCALE / af.L), af.next - tf + 1e-5);
        if (tf > tend) { break; }
        continue;
      }
      let h = clamp((rf + hin) / shell, 0.0, 1.0);
      var st = strand(fr, pf, rf, h, along, across);
      // Short coats (face, legs) read as velvet: an even pile instead of visible locks.
      let velvet = 1.0 - smoothstep(0.32, 0.55, fr.len);
      st = mix(st, vec2f(select(0.0, 0.55, h < fr.len), 0.5), velvet);
      let under = UNDER * (1.0 - smoothstep(0.12, 0.42, h)) * min(fr.depth * 4.0, 1.0);
      let cov = max(st.x, under) * (1.0 - bare);
      let al = 1.0 - exp(-SIGMA * cov * dt);
      let deep = 0.35 + 0.65 * h;             // the coat shadows itself toward the skin
      let alb = fr.alb * (2.0 * st.y);
      let lit = (alb * (SUN_COL * direct + amb) + SUN_COL * spec) * deep;
      out.col += out.T * al * lit;
      out.T *= 1.0 - al;
      if (out.T < 0.01) { out.T = 0.0; return out; }
      tf += min(dt, max(af.next - tf, 1e-5));
      if (tf > tend) { break; }
    }
    t = tf;
    look = 0.0;
    r_prev = 0.0;
    if (t > tend) { break; }
  }
  return out;
}

// ---- environment --------------------------------------------------------------------------------------

fn env(o: vec3f, d: vec3f, tg: f32, c: ptr<function, Counters>) -> vec3f {
  if (tg < INF) {
    let p = o + d * tg;
    var sh = 1.0;
    var ao = 1.0;
    if (CREATURE) {
      sh = shadow(p + vec3f(0.0, 0.002, 0.0), c);
      ao = ground_ao(p, c);
    }
    return shade_ground(p, d, tg, sh, ao);
  }
  return sky(d);
}

fn heat(x: f32) -> vec3f {
  let t = clamp(x, 0.0, 1.0);
  return clamp(vec3f(1.5 * t, 1.5 * t * t, 0.5 - t) + vec3f(0.0, 0.0, 0.2) * (1.0 - t), vec3f(0.0), vec3f(1.0));
}

// ---- the kernel -----------------------------------------------------------------------------------------

@compute @workgroup_size(8, 8)
fn trace(@builtin(global_invocation_id) gid0: vec3u, @builtin(workgroup_id) wg0: vec3u) {
  var gid = gid0;
  var wg = wg0;
  gid.y += band.x * 8u;     // frames run as row bands, one submission each (spikes/README.md GPU rules)
  wg.y += band.x;
  gid.x += band.y * 8u;     // references may cover a crop
  wg.x += band.y;
  if (gid.x >= F.dims.x || gid.y >= F.dims.y) { return; }
  var c = Counters(0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u);
  var jit = vec2f(0.5);
  if (REF && F.grid.z > 1u) {                // stratified 4×4 jitter, one stratum per dispatch
    let k = F.grid.y;
    let r = vec2f(hash2(gid.xy * 7u + vec2u(k, 3u * k)), hash2(gid.yx * 13u + vec2u(5u * k, k)));
    jit = (vec2f(f32(k % 4u), f32(k / 4u)) + r) / 4.0;
  }
  let u = (f32(gid.x) + jit.x) / f32(F.dims.x) * 2.0 - 1.0;
  let v = 1.0 - (f32(gid.y) + jit.y) / f32(F.dims.y) * 2.0;
  let o = F.eye.xyz;
  let d = normalize(F.cf.xyz + F.cr.xyz * u + F.cu.xyz * v);
  var tg = INF;
  if (d.y < -1e-6) { tg = -o.y / d.y; }

  var hit = Hit(vec3f(0.0), 1.0, INF);
  if (CREATURE) {
    var mask = F.vdims.w;
    if (!REF) { mask = tiles[wg.y * F.dims.z + wg.x]; }
    if (mask != 0u) { hit = creature(o, d, mask, tg, hash2(gid.xy), &c); }
  }
  var col = hit.col;
  if (hit.T > 0.0) { col += hit.T * env(o, d, tg, &c); }
  col = tonemap(col);
  if (VIEW == 1u && hit.t < INF) { col = heat(f32(c.steps + c.fur) / 64.0); }
  if (VIEW == 2u && hit.t < INF) { col = heat(f32(c.fur) / 48.0); }
  if (VIEW == 3u && c.ray_parts > 0u) { col = heat(f32(c.steps) / 32.0); }

  if (REF) {
    let idx = gid.y * F.dims.x + gid.x;
    accum[idx] += vec4f(col, 1.0) / f32(max(F.grid.z, 1u));
  } else {
    textureStore(out_tex, vec2i(gid.xy), vec4f(col, 1.0));
  }

  if (STATS) {
    let creature_px = hit.T < 0.5;
    if (creature_px) {
      atomicAdd(&stats[S_CREATURE_PX], 1u);
      atomicAdd(&stats[S_HIT_STEPS], c.steps + c.fur);
      atomicAdd(&stats[S_SURF_PX], c.surf);
    }
    atomicAdd(&stats[S_STEPS], c.steps);
    atomicAdd(&stats[S_EVALS], c.evals);
    atomicAdd(&stats[S_CAPS], c.caps);
    atomicAdd(&stats[S_FUR], c.fur);
    atomicAdd(&stats[S_FUR_CAPS], c.fur_caps);
    atomicAdd(&stats[S_SEGS], c.segs);
    atomicAdd(&stats[S_SHADOW], c.sh);
    atomicAdd(&stats[S_SHADOW_CAPS], c.sh_caps);
    if (c.ray_parts > 0u) { atomicAdd(&stats[S_RAYS], 1u); }
    atomicAdd(&stats[S_RAY_PARTS], c.ray_parts);
    atomicAdd(&stats[S_TILE_PARTS], c.tile_parts);
    if (creature_px) { atomicAdd(&stats[S_STOPS], c.stops); atomicAdd(&stats[S_JUMPS], c.jumps); }
  }
}
