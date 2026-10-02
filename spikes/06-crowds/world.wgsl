// The static world as an analytic field: terrain, a stone tower, a town wall with a turret, houses.
// Plus the edit log (CSG primitives applied in order) and the material query used in shading.
// Needs `edits: array<vec4f>` declared by the including pass. (The analytic world with the whole
// log, through a spatial edit list, is in trace.wgsl: only that pass has the list.)
//
// The cook pass bakes base + edits into bricks; the analytic variants and the reference evaluate it
// directly. It's written as a spike would, not as the language would emit it.

// ---- primitives ---------------------------------------------------------------------------------

fn sd_box(p: vec3f, b: vec3f) -> f32 {
  let q = abs(p) - b;
  return length(max(q, vec3f(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

/** Capped cone centred at the origin: height 2h along y, radius r1 at the bottom and r2 at the top
 *  (Quílez). Exact. */
fn sd_capped_cone(p: vec3f, h: f32, r1: f32, r2: f32) -> f32 {
  let q = vec2f(length(p.xz), p.y);
  let k1 = vec2f(r2, h);
  let k2 = vec2f(r2 - r1, 2.0 * h);
  let ca = vec2f(q.x - min(q.x, select(r2, r1, q.y < 0.0)), abs(q.y) - h);
  let cb = q - k1 + k2 * clamp(dot(k1 - q, k2) / dot(k2, k2), 0.0, 1.0);
  let s = select(1.0, -1.0, cb.x < 0.0 && ca.y < 0.0);
  return s * sqrt(min(dot(ca, ca), dot(cb, cb)));
}

/** Distance to the terrain surface, normalized by the local slope: (y − h) / √(1 + |∇h|²). It's
 *  exact for a plane and close for ground this gentle; the reference comparison checks it. */
fn terrain_sdf(p: vec3f) -> f32 {
  let hg = terrain_hg(p.x, p.z);
  return (p.y - hg.x) * inverseSqrt(1.0 + dot(hg.yz, hg.yz));
}

// ---- the tower ------------------------------------------------------------------------------------

const TOWER = vec2f(0.0, 27.0);

/** x: masonry, y: roof. */
fn tower_sd(p: vec3f) -> vec2f {
  let q = vec3f(p.x - TOWER.x, p.y, p.z - TOWER.y);
  let r = length(q.xz);
  // Body: a slightly tapered drum, then a corbel flaring out under the parapet.
  var body = sd_capped_cone(q - vec3f(0.0, 7.5, 0.0), 10.5, 4.35, 3.97);
  body = min(body, sd_capped_cone(q - vec3f(0.0, 17.3, 0.0), 0.7, 4.0, 4.75));
  // Parapet: a ring wall with 14 crenels.
  var ring = max(max(r - 4.75, 4.1 - r), abs(q.y - 19.3) - 1.3);
  let a = atan2(q.z, q.x);
  let sec = 2.0 * PI / 14.0;
  let al = a - sec * round(a / sec);
  ring = max(ring, -sd_box(vec3f(r * cos(al) - 4.42, q.y - 20.55, r * sin(al)), vec3f(0.5, 0.75, 0.42)));
  body = min(body, ring);
  // Slit windows: three levels of four, each level turned.
  let k = clamp(round((q.y - 5.0) / 4.5), 0.0, 2.0);
  let a2 = a + 0.6 * k;
  let s4 = PI * 0.5;
  let al2 = a2 - s4 * round(a2 / s4);
  body = max(body, -sd_box(vec3f(r * cos(al2) - 4.15, q.y - 5.0 - 4.5 * k, r * sin(al2)), vec3f(0.6, 0.8, 0.16)));
  // An arched door facing the square (−z).
  let dp = vec3f(q.x, q.y - 1.0, q.z + 4.3);
  let door = max(min(sd_box(dp, vec3f(0.8, 1.6, 2.0)), length(dp.xy - vec2f(0.0, 1.6)) - 0.8), abs(dp.z) - 0.45);
  body = max(body, -door);
  // A conical slate roof inside the parapet.
  let roof = sd_capped_cone(q - vec3f(0.0, 23.25, 0.0), 5.25, 3.95, 0.04);
  return vec2f(body, roof);
}

// ---- the town wall and its turret ---------------------------------------------------------------------

const TURRET = vec2f(24.0, 27.0);

fn wall_pt(i: u32) -> vec2f {
  switch (i) {
    case 0u: { return vec2f(4.0, 28.5); }
    case 1u: { return vec2f(24.0, 27.0); }
    case 2u: { return vec2f(40.0, 11.0); }
    default: { return vec2f(44.5, -12.0); }
  }
}

/** One straight wall segment: a 1.5 m thick wall up to 7 m, merlons to 8.3 m. x: distance, y: along. */
fn wall_seg(p: vec3f, a: vec2f, b: vec2f) -> vec2f {
  let ab = b - a;
  let len = length(ab);
  let dir = ab / len;
  let rel = p.xz - a;
  let u = dot(rel, dir);
  let v = dot(rel, vec2f(-dir.y, dir.x));
  let w = sd_box(vec3f(u - 0.5 * len, p.y - 2.65, v), vec3f(0.5 * len + 0.75, 5.65, 0.75));
  let um = u - 2.2 * round(u / 2.2);
  let gap = sd_box(vec3f(um, p.y - 8.05, v), vec3f(0.5, 0.75, 1.0));
  return vec2f(max(w, -gap), u);
}

/** x: wall and turret masonry, y: turret roof. */
fn wall_sd(p: vec3f) -> vec2f {
  var d = wall_seg(p, wall_pt(0u), wall_pt(1u)).x;
  d = min(d, wall_seg(p, wall_pt(1u), wall_pt(2u)).x);
  d = min(d, wall_seg(p, wall_pt(2u), wall_pt(3u)).x);
  let q = vec3f(p.x - TURRET.x, p.y, p.z - TURRET.y);
  d = min(d, max(length(q.xz) - 2.4, abs(q.y - 3.5) - 6.5));
  let roof = sd_capped_cone(q - vec3f(0.0, 12.25, 0.0), 2.25, 2.85, 0.04);
  return vec2f(d, roof);
}

// ---- houses ------------------------------------------------------------------------------------------

const NHOUSES: u32 = 9u;

/** (centre x, centre z, half-length along the ridge, half-depth). */
fn house_a(i: u32) -> vec4f {
  switch (i) {
    case 0u: { return vec4f(-26.0, -9.0, 3.6, 4.2); }
    case 1u: { return vec4f(-26.5, -1.0, 3.8, 4.4); }
    case 2u: { return vec4f(-26.0, 7.0, 3.5, 4.0); }
    case 3u: { return vec4f(-26.5, 14.5, 3.4, 4.3); }
    case 4u: { return vec4f(-13.0, 26.0, 4.5, 3.8); }
    case 5u: { return vec4f(-23.5, 25.0, 3.8, 3.6); }
    case 6u: { return vec4f(-26.0, -17.5, 3.4, 4.2); }
    case 7u: { return vec4f(-15.0, -27.0, 4.0, 3.6); }
    default: { return vec4f(12.5, 22.5, 4.0, 3.4); }
  }
}

/** (wall height, roof pitch, ridge along z?, seed). */
fn house_b(i: u32) -> vec4f {
  switch (i) {
    case 0u: { return vec4f(5.2, 0.85, 1.0, 0.13); }
    case 1u: { return vec4f(6.0, 0.95, 1.0, 0.71); }
    case 2u: { return vec4f(5.0, 0.80, 1.0, 0.37); }
    case 3u: { return vec4f(6.4, 1.00, 1.0, 0.92); }
    case 4u: { return vec4f(5.6, 0.90, 0.0, 0.55); }
    case 5u: { return vec4f(4.8, 0.85, 0.0, 0.24); }
    case 6u: { return vec4f(5.0, 0.90, 1.0, 0.66); }
    case 7u: { return vec4f(4.6, 0.80, 0.0, 0.48); }
    default: { return vec4f(5.6, 0.95, 0.0, 0.81); }
  }
}

/** House i in its own frame (ridge along local x). */
fn house_local(p: vec3f, i: u32) -> vec3f {
  let a = house_a(i);
  let l = vec3f(p.x - a.x, p.y, p.z - a.y);
  if (house_b(i).z > 0.5) { return vec3f(l.z, l.y, -l.x); }
  return l;
}

/** x: walls and chimney, y: roof, z: chimney. */
fn house_sd(p: vec3f, i: u32) -> vec3f {
  let a = house_a(i);
  let b = house_b(i);
  let l = house_local(p, i);
  let hx = a.z;
  let hz = a.w;
  let wh = b.x;
  let walls = sd_box(l - vec3f(0.0, 0.5 * (wh - 3.0), 0.0), vec3f(hx, 0.5 * (wh + 3.0), hz));
  // A gable roof as a solid wedge: under both slopes, above the eaves, within the gable ends, and
  // cut back to a 25 cm fascia at the eaves (a knife edge would be thinner than a voxel).
  let eave = wh - 0.25;
  let ridge = eave + (hz + 0.4) * b.y + 0.25;
  let slope = (abs(l.z) * b.y + (l.y - ridge)) * inverseSqrt(1.0 + b.y * b.y);
  let roof = max(max(max(slope, eave - l.y), abs(l.x) - (hx + 0.35)), abs(l.z) - (hz + 0.4));
  let chim = sd_box(l - vec3f(0.55 * hx, ridge - 0.2, 0.3 * hz), vec3f(0.35, 1.3, 0.35));
  return vec3f(min(walls, chim), roof, chim);
}

// ---- the base world --------------------------------------------------------------------------------

const GROUND_BLEND: f32 = 0.3;

/** Structures only (no terrain): their union. A structure whose bound is at least `limit` away
 *  is skipped; that's exact for the caller's smooth union with the ground (limit = ground + blend). */
fn structures(p: vec3f, limit: f32) -> f32 {
  var s = BIG;
  let tq = length(p.xz - TOWER) - 5.0;
  if (tq < limit) { let t = tower_sd(p); s = min(s, min(t.x, t.y)); }
  let wb = sd_box(p - vec3f(24.25, 5.8, 8.25), vec3f(21.05, 8.8, 21.05));
  if (wb < limit) { let w = wall_sd(p); s = min(s, min(w.x, w.y)); }
  for (var i = 0u; i < NHOUSES; i++) {
    let a = house_a(i);
    let rough = length(p.xz - a.xy) - (length(a.zw) + 0.6);
    if (rough < min(limit, s)) {
      let h = house_sd(p, i);
      s = min(s, min(h.x, h.y));
    }
  }
  return s;
}

fn base_field(p: vec3f) -> f32 {
  let d = terrain_sdf(p);
  let s = structures(p, d + GROUND_BLEND);
  return smin(d, s, GROUND_BLEND);
}

// ---- edits ---------------------------------------------------------------------------------------------
// Each edit is two vec4s: (centre, radius), (op, blend k, material, 0).
//   op 0: smooth subtraction of a sphere (crater, blast, dig); op 1: smooth union of a squashed
//   sphere (rubble). material: +1 earth, −1 broken stone (for the "disturbed" shading channel).

fn edit_shape(p: vec3f, e0: vec4f, op: f32) -> f32 {
  if (op < 0.5) { return length(p - e0.xyz) - e0.w; }
  // Rubble: an ellipsoid squashed to 0.7 in y; scaled so it stays a distance bound.
  return (length((p - e0.xyz) / vec3f(1.0, 0.7, 1.0)) - e0.w) * 0.7;
}

/** Applies edit i to (distance, disturbed). */
fn edit_apply(p: vec3f, dg: vec2f, i: u32) -> vec2f {
  let e0 = edits[2u * i];
  let e1 = edits[2u * i + 1u];
  let s = edit_shape(p, e0, e1.x);
  var d = dg.x;
  if (e1.x < 0.5) { d = smax(d, -s, e1.y); } else { d = smin(d, s, e1.y); }
  // Disturbed: the new surface, plus a splash of thrown earth around a crater.
  var w = 1.0 - smoothstep(0.5 * e1.y, 0.5 * e1.y + 0.3, abs(s));
  if (e1.z > 0.0 && e1.x < 0.5) { w = max(w, 0.55 * (1.0 - smoothstep(0.9 * e0.w, 1.9 * e0.w, length(p - e0.xyz)))); }
  var g = dg.y;
  if (w > abs(g)) { g = w * e1.z; }
  return vec2f(d, g);
}

/** Distance only: edit i applied to d (the march doesn't need the disturbed channel). */
fn edit_apply_d(p: vec3f, d: f32, i: u32) -> f32 {
  let e0 = edits[2u * i];
  let e1 = edits[2u * i + 1u];
  let s = edit_shape(p, e0, e1.x);
  if (e1.x < 0.5) { return smax(d, -s, e1.y); }
  return smin(d, s, e1.y);
}

// ---- materials -------------------------------------------------------------------------------------------

const M_GRASS: u32 = 0u;
const M_COBBLE: u32 = 1u;
const M_TOWER: u32 = 2u;      // masonry in cylindrical coordinates
const M_WALL: u32 = 3u;       // masonry along a wall
const M_SLATE: u32 = 4u;
const M_PLASTER: u32 = 5u;
const M_TILE: u32 = 6u;
const M_DOOR: u32 = 7u;

struct Mat { id: u32, uv: vec2f, seed: f32 }

/** Which base component is nearest p, with surface coordinates for its pattern. */
fn base_material(p: vec3f) -> Mat {
  let terr = terrain_sdf(p);
  var m = Mat(M_GRASS, p.xz, 0.0);
  if (rect_sd(p.x, p.z, PLAZA) < 0.0) { m.id = M_COBBLE; }
  var best = terr + 0.05;   // ties near the ground blend go to the structure
  var t = vec2f(BIG);
  if (length(p.xz - TOWER) - 5.0 < best) { t = tower_sd(p); }
  if (t.x < best) {
    best = t.x;
    let q = vec2f(p.x - TOWER.x, p.z - TOWER.y);
    m = Mat(M_TOWER, vec2f(atan2(q.y, q.x) * 4.2, p.y), 0.0);
    let dp = vec3f(q.x, p.y - 1.0, q.y + 4.3);
    if (abs(dp.x) < 0.85 && dp.y < 2.45 + sqrt(max(0.64 - dp.x * dp.x, 0.0)) && dp.z > -0.05 && dp.z < 0.5) { m.id = M_DOOR; }
  }
  if (t.y < best) { best = t.y; m = Mat(M_SLATE, vec2f(atan2(p.z - TOWER.y, p.x - TOWER.x) * 2.5, p.y), 0.0); }
  if (sd_box(p - vec3f(24.25, 5.8, 8.25), vec3f(21.05, 8.8, 21.05)) < best) {
    let w = wall_sd(p);
    if (w.x < best) {
      best = w.x;
      // Coordinates along the nearest segment.
      var u = 0.0;
      var bd = BIG;
      for (var i = 0u; i < 3u; i++) {
        let s = wall_seg(p, wall_pt(i), wall_pt(i + 1u));
        if (s.x < bd) { bd = s.x; u = s.y + f32(i) * 37.0; }
      }
      if (length(p.xz - TURRET) < 2.6) { u = atan2(p.z - TURRET.y, p.x - TURRET.x) * 2.4; }
      m = Mat(M_WALL, vec2f(u, p.y), 0.0);
    }
    if (w.y < best) { best = w.y; m = Mat(M_SLATE, vec2f(atan2(p.z - TURRET.y, p.x - TURRET.x) * 2.0, p.y), 0.0); }
  }
  for (var i = 0u; i < NHOUSES; i++) {
    let a = house_a(i);
    if (length(p.xz - a.xy) - (length(a.zw) + 0.6) > best) { continue; }
    let h = house_sd(p, i);
    let l = house_local(p, i);
    let seed = house_b(i).w;
    if (h.x < best) {
      best = h.x;
      // Walls: u runs around the house; the chimney is masonry.
      let u = select(l.x, l.z, abs(l.x) > a.z - 0.05) + f32(i) * 11.0;
      m = Mat(M_PLASTER, vec2f(u, l.y), seed);
      if (h.z <= h.x + 1e-4) { m = Mat(M_WALL, vec2f(l.x + l.z, l.y), seed); }
    }
    if (h.y < best) {
      best = h.y;
      let b = house_b(i);
      m = Mat(M_TILE, vec2f(l.x, abs(l.z) * sqrt(1.0 + b.y * b.y)), seed);
    }
  }
  return m;
}
