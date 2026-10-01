//! The same CPU benchmarks, compiled natively, for two comparisons: the determinism hash (it
//! must equal the browser's) and wasm-vs-native speed. Also computes the uniform-grid ground
//! truth for the adaptive mass integration.
//!
//! Usage: native <params.f32>   (328 little-endian f32s, written by `node params.mjs`)

use grazer_cpu::*;
use std::time::Instant;

fn time<T>(min_ms: f64, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut r = f();
    let t0 = Instant::now();
    let mut n = 0;
    while t0.elapsed().as_secs_f64() * 1e3 < min_ms {
        r = f();
        n += 1;
    }
    (t0.elapsed().as_secs_f64() * 1e3 / n.max(1) as f64, r)
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: native <params.f32>");
    let bytes = std::fs::read(path).expect("read params");
    assert_eq!(bytes.len(), PARAM_FLOATS * 4, "expected {PARAM_FLOATS} f32s");
    let mut p = [0f32; PARAM_FLOATS];
    for (i, c) in bytes.chunks_exact(4).enumerate() {
        p[i] = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
    }
    let g = Grazer::new(p);
    let t = Terrain::new(7);

    let n = 200_000;
    let (full, _) = time(400.0, || bench_eval(&g, n, false));
    let (pruned, _) = time(400.0, || bench_eval(&g, n, true));
    println!("evals_per_ms_full    {:.0}", n as f64 / full);
    println!("evals_per_ms_pruned  {:.0}", n as f64 / pruned);

    let (max, mean, over) = lipschitz_probe(&g, 200_000);
    println!("lipschitz max {max:.3} mean {mean:.3} share_over_{CLASSIFY_L} {over:.5}");

    for finest in [0.02f32, 0.01, 0.005] {
        let (ms, m) = time(200.0, || integrate(&g, finest));
        let c = m.com();
        println!(
            "mass_adaptive finest {finest} ms {ms:.2} mass {:.3} volume {:.5} com [{:.4} {:.4} {:.4}] evals {} leaves {} inside {}",
            m.mass, m.volume, c[0], c[1], c[2], m.evals, m.leaves, m.inside_nodes
        );
    }
    for cell in [0.02f32, 0.01] {
        let t0 = Instant::now();
        let m = integrate_uniform(&g, cell);
        let c = m.com();
        println!(
            "mass_uniform cell {cell} ms {:.0} mass {:.3} volume {:.5} com [{:.4} {:.4} {:.4}] evals {}",
            t0.elapsed().as_secs_f64() * 1e3, m.mass, m.volume, c[0], c[1], c[2], m.evals
        );
    }

    let rays = 20_000;
    let (ms, (evals, hits)) = time(400.0, || raycast_bench(&t, rays));
    println!(
        "raycast us_per_ray {:.3} avg_evals {:.2} hit_share {:.3}",
        ms * 1e3 / rays as f64, evals as f64 / rays as f64, hits as f64 / rays as f64
    );

    let t0 = Instant::now();
    let h = det_hash(&g, &t);
    println!("det_hash {:016x} ({:.0} ms)", h, t0.elapsed().as_secs_f64() * 1e3);
}
