// Scene: a mossy masonry tower rising from damp ground, golden sun raking across the stones.

const G_YMIN: f32 = -0.35;
const G_YMAX: f32 = 0.2;
const G_SLOPE: f32 = 0.3;
const G_HALF: f32 = 80.0;
const FOG: f32 = 0.006;
const PUDDLE_R: f32 = 0.3;
const LAYER_S: f32 = 1.0;

const RT: f32 = 3.2;          // the wall face's radius
const TOP: f32 = 13.0;        // the tower's height
const HC: f32 = 0.30;         // course height
const NST: i32 = 36;          // stones per course: the circumference divides evenly, so no seam
const LS: f32 = 0.55850536;   // 2π·RT / NST
const GAP: f32 = 0.011;       // half a joint's width
const RECESS: f32 = 0.04;     // how far the mortar sits behind the stone faces
const PLINTH_C: i32 = 3;      // courses below this one step out: a plinth
const PLINTH_Y: f32 = 0.9;    // = PLINTH_C · HC
const PLINTH_W: f32 = 0.16;
const LEDGE_Y: f32 = 4.2;     // a string course

fn ground_h(x: f32, z: f32) -> f32 {
  let q = vec3f(x * 0.55, 0.0, z * 0.55);
  return 0.045 * sin(0.7 * x + 0.3) * cos(0.6 * z) + 0.14 * noise3(q) - 0.06 * noise3(q * 2.1 + vec3f(5.0, 0.0, 1.0));
}

fn course_r(ci: i32) -> f32 { return RT + select(0.0, PLINTH_W, ci < PLINTH_C); }

fn wall_r(y: f32) -> f32 { return course_r(i32(floor(y / HC))); }

fn imod(a: i32, n: i32) -> i32 { return ((a % n) + n) % n; }

/** Boundary of stone j in course ci, along the course (arc length at RT, before the course offset). */
fn bnd(ci: i32, j: i32) -> f32 {
  return (f32(j) + 0.62 * (hash2u(ci, imod(j, NST)) - 0.5)) * LS;
}

/** The stone containing arc position s in course ci: (index, start, end), offsets included. */
fn stone_at(ci: i32, s: f32) -> vec3f {
  let off = hash2u(ci, 977) * LS;
  let x = s + off;
  var j = i32(floor(x / LS));
  if (x < bnd(ci, j)) { j -= 1; } else if (x >= bnd(ci, j + 1)) { j += 1; }
  return vec3f(f32(j), bnd(ci, j) - off, bnd(ci, j + 1) - off);
}

/** One rounded stone: local coordinates along the course (s), up (y) and outward (w). */
fn stone_d(ci: i32, st: vec3f, s: f32, y: f32, w: f32) -> f32 {
  let j = imod(i32(st.x), NST);
  let hsh = hash_i3(vec3i(ci, j, 5));
  let prot = 0.03 * hsh;
  let rnd = 0.035 + 0.025 * hash_i3(vec3i(ci, j, 9));
  let cx = 0.5 * (st.y + st.z);
  let hx = 0.5 * (st.z - st.y) - GAP;
  let yc = (f32(ci) + 0.5) * HC;
  // each stone sits a little askew
  let tilt = 0.05 * hash_i3(vec3i(ci, j, 13));
  let lx = s - cx;
  let ly = y - yc;
  let lw = w - prot + 0.25 - tilt * lx - 0.4 * tilt * ly;
  return sd_round_box(vec3f(lx, ly, lw), vec3f(hx, 0.5 * HC - GAP, 0.25), rnd);
}

fn tower(p: vec3f) -> f32 {
  let r = length(p.xz);
  var outer = RT + 0.08;
  if (p.y < PLINTH_Y + 0.5) { outer += PLINTH_W; }
  if (abs(p.y - (LEDGE_Y + 0.11)) < 0.6) { outer += 0.13; }
  let outside = r - outer;
  let top = p.y - TOP;
  if (outside > 0.15 || top > 0.15) { return max(outside, top); }
  let s = atan2(p.z, p.x) * RT;
  let ci = i32(floor(p.y / HC));
  let yc = (f32(ci) + 0.5) * HC;
  let cj = select(ci - 1, ci + 1, p.y > yc);
  let wi = r - course_r(ci);
  let a = stone_at(ci, s);
  var d = stone_d(ci, a, s, p.y, wi);
  // the neighbour along the course that's nearer
  let mid = 0.5 * (a.y + a.z);
  let jn = select(a.x - 1.0, a.x + 1.0, s > mid);
  let off = hash2u(ci, 977) * LS;
  let nb = vec3f(jn, bnd(ci, i32(jn)) - off, bnd(ci, i32(jn) + 1) - off);
  d = min(d, stone_d(ci, nb, s, p.y, wi));
  if (cj >= 0) { d = min(d, stone_d(cj, stone_at(cj, s), s, p.y, r - course_r(cj))); }
  // weathered faces: a small displacement of the stones only
  if (d < 0.06) { d -= 0.016 * fbm3(p * 4.5, 3); }
  // mortar: behind the faces, and behind the plinth's faces below PLINTH_Y
  let pq = vec2f(r - (RT + PLINTH_W - RECESS), p.y - PLINTH_Y);
  let mortar = min(r - (RT - RECESS), length(max(pq, vec2f(0.0))) + min(max(pq.x, pq.y), 0.0));
  d = smin(d, mortar, 0.012);
  // the string course
  let lq = vec2f(r - (RT + 0.07), p.y - (LEDGE_Y + 0.11));
  let ledge = length(max(abs(lq) - vec2f(0.1, 0.09), vec2f(0.0))) + min(max(abs(lq.x) - 0.1, abs(lq.y) - 0.09), 0.0) - 0.02;
  if (ledge - 0.0045 < d) { d = min(d, ledge - 0.006 * fbm3(p * 6.0, 2)); }
  return max(0.8 * d, top);
}

fn obj(p: vec3f) -> f32 { return tower(p); }

fn obj_range(o: vec3f, d: vec3f) -> vec2f {
  // vertical cylinder of radius RT + 0.4 between y = −0.4 and TOP + 0.2
  let R = RT + PLINTH_W + 0.3;
  let a = dot(d.xz, d.xz);
  let b = dot(o.xz, d.xz);
  let c = dot(o.xz, o.xz) - R * R;
  let h = b * b - a * c;
  if (h < 0.0 || a < 1e-9) { return vec2f(INF, -INF); }
  let sq = sqrt(h);
  var t0 = (-b - sq) / a;
  var t1 = (-b + sq) / a;
  let iy = safe_inv(d.y);
  let ya = (-0.4 - o.y) * iy;
  let yb = (TOP + 0.2 - o.y) * iy;
  t0 = max(t0, min(ya, yb));
  t1 = min(t1, max(ya, yb));
  if (t1 < max(t0, 0.0)) { return vec2f(INF, -INF); }
  return vec2f(max(t0, 0.0), t1);
}

fn part_field(p: vec3f, key: u32) -> f32 { return tower(p); }

fn obj_except(p: vec3f, key: u32) -> f32 { return 1e5; }
fn obj_except_k(p: vec3f, key: u32) -> vec2f { return vec2f(1e5, 0.0); }
fn part_key(p: vec3f) -> u32 { return 0u; }
fn part_medium(key: u32) -> u32 { return 0u; }

fn obj_id(p: vec3f) -> u32 {
  let w = length(p.xz) - wall_r(p.y);
  // Joints: the mortar surface sits RECESS behind the stone faces.
  if (w < -RECESS + 0.012 && abs(p.y - (LEDGE_Y + 0.11)) > 0.13) { return M_MORTAR; }
  return M_STONE;
}

fn eye_frame(p: vec3f) -> EyeF { return EyeF(vec3f(0.0), vec3f(0.0, 0.0, 1.0), vec3f(1.0, 0.0, 0.0), vec3f(0.0, 1.0, 0.0), 1.0, 0.0); }

fn wet_of(id: u32) -> f32 { return select(0.0, 1.0, id == M_GROUND || id == M_STONE || id == M_MORTAR); }

fn surf(p: vec3f, id: u32, fp: f32) -> Surf {
  if (id == M_GROUND) {
    let nz = fbm_f(p, 1.5, 5, fp);
    let gr = smoothstep(-0.1, 0.25, nz + 0.15 * sin(p.x * 0.4));
    var s = surf0(mix(vec3f(0.11, 0.085, 0.06), vec3f(0.055, 0.085, 0.025), gr), 0.9);
    let dt = detail(p, 6.0, 0.006, 6, fp);   // pebbles and clods
    s.dg = dt.g;
    s.dvar = dt.var_;
    return s;
  }
  let ci = i32(floor(p.y / HC));
  let st = stone_at(ci, atan2(p.z, p.x) * RT);
  let j = imod(i32(st.x), NST);
  let h1 = hash_i3(vec3i(ci, j, 21));
  let h2 = hash_i3(vec3i(ci, j, 33));
  var s: Surf;
  if (id == M_MORTAR) {
    s = surf0(vec3f(0.46, 0.44, 0.40), 0.95);
  } else {
    let base = mix(vec3f(0.42, 0.385, 0.33), vec3f(0.35, 0.345, 0.33), 0.5 + 0.5 * h2);
    let grain = fbm_f(p, 9.0, 4, fp);
    s = surf0(base * (0.78 + 0.22 * h1) * (0.88 + 0.25 * grain), 0.72);
    s.moss = 0.25 * h1;
  }
  // damp near the ground: moss climbs the lowest courses
  s.moss += 0.55 * (1.0 - smoothstep(0.2, 2.2, p.y));
  let dt = detail(p, 30.0, 0.0012, 5, fp);   // grain: wavelengths from 33 mm down to 2 mm
  s.dg = dt.g;
  s.dvar = dt.var_;
  return s;
}
