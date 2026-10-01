//! The grazer and terrain fields on the CPU, as the compiler would emit them for
//! `@deterministic` code (D-074).
//!
//! Strict IEEE f32: no fused multiply-add, no fast-math, no transcendental functions. Only
//! correctly rounded operations (+ − × ÷ sqrt) and exact ones (floor, min, max, abs, comparisons).
//! The hypothesis is that every platform then produces the same bits; `det_hash` checks it
//! between this crate compiled to wasm32 (in the browser) and compiled natively.
//!
//! The field mirrors field.wgsl's primal (`*_d`) functions. GPU and CPU results aren't expected
//! to match each other bit for bit: only CPU results feed the simulation (D-052).

use core::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 { v3(self.x + o.x, self.y + o.y, self.z + o.z) }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 { v3(self.x - o.x, self.y - o.y, self.z - o.z) }
}
impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, s: f32) -> V3 { v3(self.x * s, self.y * s, self.z * s) }
}
impl Mul<V3> for V3 {
    type Output = V3;
    fn mul(self, o: V3) -> V3 { v3(self.x * o.x, self.y * o.y, self.z * o.z) }
}
impl Div<V3> for V3 {
    type Output = V3;
    fn div(self, o: V3) -> V3 { v3(self.x / o.x, self.y / o.y, self.z / o.z) }
}
impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 { v3(-self.x, -self.y, -self.z) }
}
impl V3 {
    pub fn dot(self, o: V3) -> f32 { self.x * o.x + self.y * o.y + self.z * o.z }
    pub fn length(self) -> f32 { self.dot(self).sqrt() }
}

/// WGSL's `sign`: zero for ±0 (unlike `f32::signum`).
fn sign(x: f32) -> f32 {
    if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 }
}

/// WGSL's `mix`, with the same formula so results match the shader's intent.
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    mix(b, a, h) - k * h * (1.0 - h)
}

fn smin_h(a: f32, b: f32, k: f32) -> (f32, f32) {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    (mix(b, a, h) - k * h * (1.0 - h), h)
}

// ---- noise ----------------------------------------------------------------------------------

fn hash_u(x: u32) -> u32 {
    let mut h = x.wrapping_mul(747796405).wrapping_add(2891336453);
    h = ((h >> ((h >> 28) + 4)) ^ h).wrapping_mul(277803737);
    (h >> 22) ^ h
}

fn lattice(x: i32, y: i32, z: i32) -> f32 {
    let h = hash_u((x as u32).wrapping_add(hash_u((y as u32).wrapping_add(hash_u(z as u32)))));
    (h as f32) * (2.0 / 4294967295.0) - 1.0
}

pub fn noise(p: V3) -> f32 {
    let (fx, fy, fz) = (p.x.floor(), p.y.floor(), p.z.floor());
    let (ix, iy, iz) = (fx as i32, fy as i32, fz as i32);
    let f = v3(p.x - fx, p.y - fy, p.z - fz);
    let q = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let u = v3(q(f.x), q(f.y), q(f.z));
    let l = |dx: i32, dy: i32, dz: i32| lattice(ix.wrapping_add(dx), iy.wrapping_add(dy), iz.wrapping_add(dz));
    mix(
        mix(mix(l(0, 0, 0), l(1, 0, 0), u.x), mix(l(0, 1, 0), l(1, 1, 0), u.x), u.y),
        mix(mix(l(0, 0, 1), l(1, 0, 1), u.x), mix(l(0, 1, 1), l(1, 1, 1), u.x), u.y),
        u.z,
    )
}

// ---- primitives -----------------------------------------------------------------------------

fn ellipsoid(p: V3, r: V3) -> f32 {
    let k0 = (p / r).length();
    let k1 = (p / (r * r)).length().max(1e-9);
    k0 * (k0 - 1.0) / k1
}

fn round_cone(p: V3, a: V3, b: V3, r1: f32, r2: f32) -> f32 {
    let ba = b - a;
    let l2 = ba.dot(ba);
    let rr = r1 - r2;
    let a2 = l2 - rr * rr;
    let il2 = 1.0 / l2;
    let pa = p - a;
    let y = pa.dot(ba);
    let z = y - l2;
    let xv = pa * l2 - ba * y;
    let x2 = xv.dot(xv);
    let y2 = y * y * l2;
    let z2 = z * z * l2;
    let k = sign(rr) * rr * rr * x2;
    if sign(z) * a2 * z2 > k {
        return (x2 + z2).sqrt() * il2 - r2;
    }
    if sign(y) * a2 * y2 < k {
        return (x2 + y2).sqrt() * il2 - r1;
    }
    ((x2 * a2 * il2).sqrt() + y * rr) * il2 - r1
}

// ---- the grazer -----------------------------------------------------------------------------

pub const PARTS: usize = 20;
pub const PARAM_FLOATS: usize = 8 + PARTS * 16;
pub const HIDE_DENSITY: f32 = 1050.0; // kg/m³ (sketch 01 §1)
pub const HOOF_DENSITY: f32 = 1300.0;

pub struct Grazer {
    p: [f32; PARAM_FLOATS],
    /// Per part: a bounding sphere and a factor turning distance-to-sphere into a lower bound on
    /// the part's field value. 1 for exact parts; 0.5 for ellipsoid-based ones, whose bound
    /// formula isn't a true distance (an assumption, not derived).
    sphere: [(V3, f32, f32); PARTS],
    pub lo: V3,
    pub hi: V3,
}

impl Grazer {
    pub fn new(p: [f32; PARAM_FLOATS]) -> Grazer {
        let mut g = Grazer { p, sphere: [(v3(0.0, 0.0, 0.0), 0.0, 1.0); PARTS], lo: v3(0.0, 0.0, 0.0), hi: v3(0.0, 0.0, 0.0) };
        let amp = g.amp_total();
        let mut lo = v3(f32::MAX, f32::MAX, f32::MAX);
        let mut hi = v3(f32::MIN, f32::MIN, f32::MIN);
        for i in 0..PARTS {
            let (c, r, f) = match i {
                0 => {
                    let (c1, r1, c2, r2) = (g.xyz(0, 0), g.xyz(0, 1), g.xyz(0, 2), g.xyz(0, 3));
                    let c = (c1 + c2) * 0.5;
                    let r = (c1 - c2).length() * 0.5 + r1.x.max(r1.y).max(r1.z).max(r2.x.max(r2.y).max(r2.z)) + amp;
                    (c, r, 0.5)
                }
                2 => {
                    let (ec, er, a, b) = (g.xyz(2, 0), g.xyz(2, 1), g.xyz(2, 2), g.xyz(2, 3));
                    let rm = er.x.max(er.y).max(er.z);
                    let (cc, cr) = ((a + b) * 0.5, (b - a).length() * 0.5 + g.w(2, 2).max(g.w(2, 3)));
                    let c = (ec + cc) * 0.5;
                    let r = (ec - c).length() + rm.max((cc - c).length() + cr);
                    (c, r.max((cc - c).length() + cr), 0.5)
                }
                _ => {
                    let (a, b) = (g.xyz(i, 0), g.xyz(i, 1));
                    ((a + b) * 0.5, (b - a).length() * 0.5 + g.w(i, 0).max(g.w(i, 1)), 1.0)
                }
            };
            g.sphere[i] = (c, r, f);
            lo = v3(lo.x.min(c.x - r), lo.y.min(c.y - r), lo.z.min(c.z - r));
            hi = v3(hi.x.max(c.x + r), hi.y.max(c.y + r), hi.z.max(c.z + r));
        }
        g.lo = lo;
        g.hi = hi;
        g
    }

    fn at(&self, i: usize, k: usize) -> usize { 8 + i * 16 + k * 4 }
    fn xyz(&self, i: usize, k: usize) -> V3 {
        let o = self.at(i, k);
        v3(self.p[o], self.p[o + 1], self.p[o + 2])
    }
    fn w(&self, i: usize, k: usize) -> f32 { self.p[self.at(i, k) + 3] }
    fn k(&self) -> f32 { self.p[0] }
    fn amp_total(&self) -> f32 { self.p[1] * 1.875 }

    fn fbm(&self, p: V3) -> f32 {
        let mut s = 0.0;
        let mut a = 1.0;
        let mut fr = self.p[2];
        for o in 0..self.p[3] as u32 {
            s += a * noise(p * fr + v3(o as f32 * 17.0, o as f32 * 17.0, o as f32 * 17.0));
            a *= 0.5;
            fr *= 2.0;
        }
        s
    }

    /// One part's field value (unfiltered: the CPU uses the exact definition).
    pub fn part(&self, q: V3, i: usize) -> f32 {
        match i {
            0 => {
                let base = smin(ellipsoid(q - self.xyz(0, 0), self.xyz(0, 1)), ellipsoid(q - self.xyz(0, 2), self.xyz(0, 3)), self.w(0, 0));
                base + self.p[1] * self.fbm(q + v3(self.p[4], self.p[5], self.p[6]))
            }
            2 => smin(
                ellipsoid(q - self.xyz(2, 0), self.xyz(2, 1)),
                round_cone(q, self.xyz(2, 2), self.xyz(2, 3), self.w(2, 2), self.w(2, 3)),
                self.w(2, 0),
            ),
            16..=19 => round_cone(q, self.xyz(i, 0), self.xyz(i, 1), self.w(i, 0), self.w(i, 1)).max(self.p[self.at(i, 2)] - q.y),
            _ => round_cone(q, self.xyz(i, 0), self.xyz(i, 1), self.w(i, 0), self.w(i, 1)),
        }
    }

    /// The creature: all 20 parts, smooth-unioned in field.wgsl's fold order.
    pub fn distance(&self, q: V3) -> f32 {
        let k = self.k();
        let mut d = 1e6;
        for i in 0..PARTS {
            d = smin(d, self.part(q, i), k);
        }
        d
    }

    /// Distance and blended density (a `Blend` channel, sketch 01 §1).
    pub fn sample(&self, q: V3, pruned: bool) -> (f32, f32) {
        let k = self.k();
        let (live, best, est) = if pruned { self.live_parts(q) } else { ((1 << PARTS) - 1, PARTS, 0.0) };
        let (mut d, mut rho) = (1e6f32, HIDE_DENSITY);
        for i in 0..PARTS {
            if live & (1 << i) == 0 {
                continue;
            }
            let v = if i == best { est } else { self.part(q, i) };
            let (nd, h) = smin_h(d, v, k);
            let prho = if i >= 16 { HOOF_DENSITY } else { HIDE_DENSITY };
            rho = mix(prho, rho, h);
            d = nd;
        }
        (d, rho)
    }

    pub fn distance_pruned(&self, q: V3) -> f32 {
        self.sample(q, true).0
    }

    /// CPU pruning: lower bounds from bounding spheres. The part with the lowest bound is
    /// evaluated first; any part whose bound is at least k above that value can't affect the
    /// smooth union (smin is then exact). Returns (mask, that part, its value).
    fn live_parts(&self, q: V3) -> (u32, usize, f32) {
        let mut lb = [0f32; PARTS];
        let mut best = 0;
        for i in 0..PARTS {
            let (c, r, f) = self.sphere[i];
            lb[i] = ((q - c).length() - r) * f;
            if lb[i] < lb[best] {
                best = i;
            }
        }
        let est = self.part(q, best);
        let k = self.k();
        let mut live = 0u32;
        for i in 0..PARTS {
            if i == best || lb[i] < est + k {
                live |= 1 << i;
            }
        }
        (live, best, est)
    }
}

// ---- adaptive mass integration (sketch 01 §4, T3) ---------------------------------------------

#[derive(Default, Debug, Clone, Copy)]
pub struct Mass {
    pub mass: f64,
    pub volume: f64,
    pub moment: [f64; 3],
    pub evals: u64,
    pub leaves: u64,
    pub inside_nodes: u64,
}

impl Mass {
    pub fn com(&self) -> [f64; 3] {
        [self.moment[0] / self.mass, self.moment[1] / self.mass, self.moment[2] / self.mass]
    }
}

/// Lipschitz constant used to classify octree nodes: sketch 01's `@assert(lipschitz <= 1.5)`.
pub const CLASSIFY_L: f32 = 1.5;

pub fn integrate(g: &Grazer, finest: f32) -> Mass {
    let c = (g.lo + g.hi) * 0.5;
    let e = g.hi - g.lo;
    let h = e.x.max(e.y).max(e.z) * 0.5;
    let mut m = Mass::default();
    node(g, c, h, finest, &mut m);
    m
}

fn node(g: &Grazer, c: V3, h: f32, finest: f32, m: &mut Mass) {
    m.evals += 1;
    let (d, rho) = g.sample(c, true);
    let r = h * 1.732_050_8 * CLASSIFY_L;
    let add = |m: &mut Mass| {
        let vol = (2.0 * h as f64).powi(3);
        let mass = vol * rho as f64;
        m.volume += vol;
        m.mass += mass;
        m.moment[0] += mass * c.x as f64;
        m.moment[1] += mass * c.y as f64;
        m.moment[2] += mass * c.z as f64;
    };
    if d > r {
        return;
    }
    if d < -r {
        m.inside_nodes += 1;
        add(m);
        return;
    }
    if 2.0 * h <= finest {
        m.leaves += 1;
        if d < 0.0 {
            add(m);
        }
        return;
    }
    let q = h * 0.5;
    for i in 0..8 {
        let o = v3(
            if i & 1 != 0 { q } else { -q },
            if i & 2 != 0 { q } else { -q },
            if i & 4 != 0 { q } else { -q },
        );
        node(g, c + o, q, finest, m);
    }
}

/// Ground truth for the adaptive version: midpoint rule on a uniform grid.
pub fn integrate_uniform(g: &Grazer, cell: f32) -> Mass {
    let e = g.hi - g.lo;
    let (nx, ny, nz) = ((e.x / cell).ceil() as i32, (e.y / cell).ceil() as i32, (e.z / cell).ceil() as i32);
    let mut m = Mass::default();
    let vol = (cell as f64).powi(3);
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                let c = g.lo + v3((x as f32 + 0.5) * cell, (y as f32 + 0.5) * cell, (z as f32 + 0.5) * cell);
                m.evals += 1;
                let (d, rho) = g.sample(c, true);
                if d < 0.0 {
                    let mass = vol * rho as f64;
                    m.volume += vol;
                    m.mass += mass;
                    m.moment[0] += mass * c.x as f64;
                    m.moment[1] += mass * c.y as f64;
                    m.moment[2] += mass * c.z as f64;
                }
            }
        }
    }
    m
}

// ---- terrain (sketch 03: a base field plus a list of edits, looped over) -----------------------

pub struct Terrain {
    edits: Vec<(V3, f32, bool)>,
}

pub struct XorShift(pub u32);
impl XorShift {
    pub fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 * (1.0 / 16_777_216.0)
    }
}

impl Terrain {
    pub fn new(seed: u32) -> Terrain {
        let mut r = XorShift(seed | 1);
        let mut t = Terrain { edits: Vec::new() };
        for i in 0..32 {
            let x = r.unit() * 120.0 - 60.0;
            let z = r.unit() * 120.0 - 60.0;
            let y = t.height(x, z);
            t.edits.push((v3(x, y, z), 1.0 + 3.0 * r.unit(), i % 3 == 0));
        }
        t
    }

    pub fn height(&self, x: f32, z: f32) -> f32 {
        let mut s = 0.0;
        let mut a = 4.0;
        let mut f = 0.02;
        for _ in 0..5 {
            s += a * noise(v3(x * f, 0.5, z * f));
            a *= 0.5;
            f *= 2.0;
        }
        s
    }

    /// A bound: the heightfield term is scaled by 0.5 for slopes up to ~1.7.
    pub fn distance(&self, p: V3) -> f32 {
        let mut d = (p.y - self.height(p.x, p.z)) * 0.5;
        for &(c, r, add) in &self.edits {
            let s = (p - c).length() - r;
            d = if add { smin(d, s, 0.5) } else { -smin(-d, s, 0.5) };
        }
        d
    }

    /// Sphere tracing. Returns (t, evaluations); t < 0 means no hit within `max_t`.
    pub fn raycast(&self, from: V3, dir: V3, max_t: f32) -> (f32, u32) {
        let mut t = 0.0;
        for i in 1..=128 {
            let d = self.distance(from + dir * t);
            if d < 1e-3 {
                return (t, i);
            }
            t += d.max(1e-3);
            if t > max_t {
                return (-1.0, i);
            }
        }
        (-1.0, 128)
    }
}

// ---- benchmarks shared by the wasm exports and the native binary -------------------------------

pub fn random_point(g: &Grazer, r: &mut XorShift) -> V3 {
    let e = g.hi - g.lo;
    g.lo + v3(r.unit() * e.x, r.unit() * e.y, r.unit() * e.z)
}

pub fn bench_eval(g: &Grazer, n: u32, pruned: bool) -> f64 {
    let mut r = XorShift(12345);
    let mut s = 0.0f64;
    for _ in 0..n {
        let q = random_point(g, &mut r);
        s += if pruned { g.distance_pruned(q) } else { g.distance(q) } as f64;
    }
    s
}

/// Empirical Lipschitz constant: central differences at random points. Returns (max, mean, share
/// of points above `CLASSIFY_L`).
pub fn lipschitz_probe(g: &Grazer, n: u32) -> (f64, f64, f64) {
    let mut r = XorShift(777);
    let h = 1e-3f32;
    let (mut max, mut sum, mut over) = (0f64, 0f64, 0u32);
    for _ in 0..n {
        let q = random_point(g, &mut r);
        let d = |o: V3| g.distance(q + o) as f64 - g.distance(q - o) as f64;
        let gx = d(v3(h, 0.0, 0.0)) / (2.0 * h as f64);
        let gy = d(v3(0.0, h, 0.0)) / (2.0 * h as f64);
        let gz = d(v3(0.0, 0.0, h)) / (2.0 * h as f64);
        let m = (gx * gx + gy * gy + gz * gz).sqrt();
        max = max.max(m);
        sum += m;
        if m > CLASSIFY_L as f64 {
            over += 1;
        }
    }
    (max, sum / n as f64, over as f64 / n as f64)
}

/// Raycasts straight down from 1.5 m above random ground points. Returns (total evaluations, hits).
pub fn raycast_bench(t: &Terrain, n: u32) -> (u64, u32) {
    let mut r = XorShift(4242);
    let (mut evals, mut hits) = (0u64, 0u32);
    for _ in 0..n {
        let x = r.unit() * 100.0 - 50.0;
        let z = r.unit() * 100.0 - 50.0;
        let (hit, e) = t.raycast(v3(x, t.height(x, z) + 1.5, z), v3(0.0, -1.0, 0.0), 10.0);
        evals += e as u64;
        if hit >= 0.0 {
            hits += 1;
        }
    }
    (evals, hits)
}

struct Fnv(u64);
impl Fnv {
    fn eat(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}

/// One hash over everything the simulation could observe: 1M evaluations (full and pruned),
/// a mass integration, and 10k raycasts. Equal hashes across platforms = the same bits.
pub fn det_hash(g: &Grazer, t: &Terrain) -> u64 {
    let mut h = Fnv(0xcbf29ce484222325);
    let mut r = XorShift(99);
    for _ in 0..500_000 {
        let q = random_point(g, &mut r);
        h.eat(&g.distance(q).to_bits().to_le_bytes());
        h.eat(&g.distance_pruned(q).to_bits().to_le_bytes());
    }
    let m = integrate(g, 0.01);
    for v in [m.mass, m.volume, m.moment[0], m.moment[1], m.moment[2]] {
        h.eat(&v.to_bits().to_le_bytes());
    }
    h.eat(&m.evals.to_le_bytes());
    let mut r = XorShift(5);
    for _ in 0..10_000 {
        let x = r.unit() * 100.0 - 50.0;
        let z = r.unit() * 100.0 - 50.0;
        let (hit, e) = t.raycast(v3(x, t.height(x, z) + 1.5, z), v3(0.0, -1.0, 0.0), 10.0);
        h.eat(&hit.to_bits().to_le_bytes());
        h.eat(&e.to_le_bytes());
    }
    h.0
}

// ---- wasm exports -------------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
mod exports {
    use super::*;
    use core::ptr::addr_of_mut;

    static mut PARAMS: [f32; PARAM_FLOATS] = [0.0; PARAM_FLOATS];
    static mut OUT: [f64; 16] = [0.0; 16];
    static mut STATE: Option<(Grazer, Terrain)> = None;

    fn state() -> &'static (Grazer, Terrain) {
        unsafe { (&*addr_of_mut!(STATE)).as_ref().expect("call load() first") }
    }
    fn out(vals: &[f64]) {
        unsafe { (&mut *addr_of_mut!(OUT))[..vals.len()].copy_from_slice(vals) }
    }

    #[no_mangle]
    pub extern "C" fn params_ptr() -> *mut f32 {
        addr_of_mut!(PARAMS) as *mut f32
    }
    #[no_mangle]
    pub extern "C" fn out_ptr() -> *const f64 {
        addr_of_mut!(OUT) as *const f64
    }
    #[no_mangle]
    pub extern "C" fn load() {
        unsafe { *addr_of_mut!(STATE) = Some((Grazer::new(*addr_of_mut!(PARAMS)), Terrain::new(7))) }
    }
    #[no_mangle]
    pub extern "C" fn bench_eval_js(n: u32, pruned: u32) -> f64 {
        bench_eval(&state().0, n, pruned != 0)
    }
    #[no_mangle]
    pub extern "C" fn lipschitz_probe_js(n: u32) -> f64 {
        let (max, mean, over) = lipschitz_probe(&state().0, n);
        out(&[max, mean, over]);
        max
    }
    #[no_mangle]
    pub extern "C" fn mass_js(finest: f32) -> f64 {
        let m = integrate(&state().0, finest);
        let c = m.com();
        out(&[m.mass, m.volume, c[0], c[1], c[2], m.evals as f64, m.leaves as f64, m.inside_nodes as f64]);
        m.mass
    }
    #[no_mangle]
    pub extern "C" fn raycast_bench_js(n: u32) -> f64 {
        let (evals, hits) = raycast_bench(&state().1, n);
        out(&[evals as f64 / n as f64, hits as f64 / n as f64]);
        evals as f64
    }
    #[no_mangle]
    pub extern "C" fn det_hash_js() -> f64 {
        let h = det_hash(&state().0, &state().1);
        out(&[(h >> 32) as f64, (h & 0xffff_ffff) as f64]);
        0.0
    }
}
