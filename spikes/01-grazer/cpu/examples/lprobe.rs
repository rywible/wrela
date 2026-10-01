// Where does the field's gradient exceed 1.5? Prints the worst points and which part is nearest.
use grazer_cpu::*;
fn main() {
    let bytes = std::fs::read("params-seed1.f32").unwrap();
    let mut p = [0f32; PARAM_FLOATS];
    for (i, c) in bytes.chunks_exact(4).enumerate() { p[i] = f32::from_le_bytes([c[0], c[1], c[2], c[3]]); }
    let g = Grazer::new(p);
    let mut r = XorShift(777);
    let h = 1e-3f32;
    let mut worst: Vec<(f64, V3, f32, usize)> = vec![];
    for _ in 0..200_000 {
        let q = random_point(&g, &mut r);
        let d = |o: V3| g.distance(q + o) as f64 - g.distance(q - o) as f64;
        let gx = d(v3(h, 0.0, 0.0)) / 0.002; let gy = d(v3(0.0, h, 0.0)) / 0.002; let gz = d(v3(0.0, 0.0, h)) / 0.002;
        let m = (gx * gx + gy * gy + gz * gz).sqrt();
        if m > 1.5 {
            let mut best = 0; let mut bd = f32::MAX;
            for i in 0..PARTS { let v = g.part(q, i); if v < bd { bd = v; best = i; } }
            worst.push((m, q, g.distance(q), best));
        }
    }
    worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let mut by_part = [0u32; PARTS];
    for w in &worst { by_part[w.3] += 1; }
    println!("over 1.5: {} points; by nearest part: {:?}", worst.len(), by_part);
    let inside = worst.iter().filter(|w| w.2 < 0.0).count();
    println!("of which inside (d<0): {inside}; |d| < 5cm: {}", worst.iter().filter(|w| w.2.abs() < 0.05).count());
    for w in worst.iter().take(8) { println!("|g|={:.2} at ({:.3},{:.3},{:.3}) d={:.4} nearest part {}", w.0, w.1.x, w.1.y, w.1.z, w.2, w.3); }
}
