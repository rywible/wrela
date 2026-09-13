#!/usr/bin/env python3
"""Matched periodic/localized surface signal comparison; never renderer evidence.
Requires the existing bundled NumPy. No install, geometry, source-world or energy-model change.
"""
import argparse, hashlib, json, math, pathlib, time
import numpy as np
import experiment as reference

MATRICES = np.array([[[116, 87, 0], [-87, 116, 0], [0, 0, 62]],
                     [[-143.4, 191.2, 0], [-191.2, -143.4, 0], [12.36, 0, 103]]])
SEEDS = [37011, 9973]
AMPLITUDES = [.00016, .00008]
OFFSETS = np.array([[0, 0, 0], [17.31, -8.7, 41.6]])

def smooth(a, b, x):
    t = np.clip((x-a)/(b-a), 0, 1)
    return t*t*(3-2*t)

def hash_value(cell, seed):
    p = cell.astype(np.uint32)
    with np.errstate(over='ignore'):
        h = p[..., 0]*np.uint32(0x8da6b343) ^ p[..., 1]*np.uint32(0xd8163841) ^ p[..., 2]*np.uint32(0xcb1ab31f) ^ np.uint32(seed)
        h ^= h >> 16; h *= np.uint32(0x7feb352d)
        h ^= h >> 15; h *= np.uint32(0x846ca68b); h ^= h >> 16
    return (h >> 8).astype(np.float64)*(2/16777215)-1

def value(p, seed):
    cell = np.floor(p).astype(np.int64); t = p-cell
    t = t*t*t*(t*(t*6-15)+10)
    out = np.zeros(p.shape[:-1])
    for z in range(2):
        for y in range(2):
            for x in range(2):
                weight = (t[..., 0] if x else 1-t[..., 0])*(t[..., 1] if y else 1-t[..., 1])*(t[..., 2] if z else 1-t[..., 2])
                out += hash_value(cell + [x, y, z], seed)*weight
    return out

def attenuation(matrix, dx, dy):
    a, b = matrix@dx, matrix@dy
    footprint = max(np.linalg.norm(a), np.linalg.norm(b))
    return float((1-smooth(.18, .55, footprint))*math.exp(-1.5*(a@a+b@b)))

def localized(p, dx, dy, filtered=True):
    out = np.zeros(p.shape[:-1])
    for matrix, seed, amplitude, offset in zip(MATRICES, SEEDS, AMPLITUDES, OFFSETS):
        out += value(p@matrix.T+offset, seed)*amplitude*(attenuation(matrix, dx, dy) if filtered else 1)
    return out

def periodic(p, dx, dy, filtered=True):
    out = np.zeros(p.shape[:-1])
    for mode, phase in zip(reference.MODES, reference.PHASES):
        out += mode[3]*np.sin(2*np.pi*(p@np.array(mode[:3]))+phase)*(reference.attenuation(mode, dx, dy) if filtered else 1)
    return out

def rms(a): return float(np.sqrt(np.mean(a*a)))

def spectrum(a, spacing):
    n = a.shape[0]; window = np.outer(np.hanning(n), np.hanning(n))
    power = abs(np.fft.fft2((a-a.mean())*window))**2
    f = np.fft.fftfreq(n, spacing); fx, fy = np.meshgrid(f, f)
    power[fx*fx+fy*fy < 8**2] = 0
    total = power.sum()
    if total < 1e-25: return power, fx, fy, 0.0
    return power, fx, fy, float(np.sort(power.ravel())[-8:].sum()/total)

def patch(n, spacing, plane):
    u, v = np.meshgrid((np.arange(n)+.5)*spacing, (np.arange(n)+.5)*spacing)
    if plane == 'head-front-XY': return np.stack([u-.25, v+.61, np.full_like(u, -.47)], -1)
    return np.stack([np.full_like(u, .20), u+.12, v-.25], -1)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--out', type=pathlib.Path, required=True)
    args = parser.parse_args(); args.out.mkdir(parents=True, exist_ok=True)
    start = time.perf_counter(); spectral = []; rows = []; images = {}
    for plane in ['head-front-XY', 'flank-YZ']:
        p = patch(256, .5/256, plane)
        for label, function in [('periodic', periodic), ('localized', localized)]:
            raw = function(p, np.zeros(3), np.zeros(3), False)
            _, _, _, peak = spectrum(raw, .5/256)
            spectral.append(dict(plane=plane, source=label, top_eight_bin_energy_fraction=peak, raw_rms_m=rms(raw)))
            images[plane+'-'+label] = raw
        for depth in [.5, 1, 2, 4, 8]:
            pixel = 2*depth*math.tan(1.05/2)/1080
            dx = np.array([pixel, 0, 0] if plane == 'head-front-XY' else [0, pixel, 0])
            dy = np.array([0, pixel, 0] if plane == 'head-front-XY' else [0, 0, pixel])
            p = patch(256, pixel/4, plane)
            for label, function in [('periodic', periodic), ('localized', localized)]:
                raw = function(p, dx, dy, False); filtered = function(p, dx, dy)
                shifted = function(p+dx*.25, dx, dy)
                power, fx, fy, _ = spectrum(filtered, pixel/4)
                alias = power[(abs(fx) >= .5/pixel) | (abs(fy) >= .5/pixel)].sum()
                fraction = float(alias/power.sum()) if power.sum() > 1e-25 else 0.0
                rows.append(dict(plane=plane, source=label, distance_m=depth,
                    filtered_rms_m=rms(filtered), raw_rms_m=rms(raw),
                    quarter_pixel_delta_m=rms(shifted-filtered),
                    raw_quarter_pixel_delta_m=rms(function(p+dx*.25, dx, dy, False)-raw),
                    above_nyquist_power_fraction=fraction,
                    caveat='Finite-window FFT leakage and quintic spectral tails are retained; this is not rendered shimmer.'))
    # Crossing actual integer noise-cell faces, including negative coordinates.
    rng = np.random.default_rng(37011); points = rng.uniform(-20, 20, (2048, 3))
    points[:, 0] = np.round(points[:, 0]); epsilon = np.array([1e-5, 0, 0])
    center = value(points, 37011)
    left, right = value(points-epsilon, 37011), value(points+epsilon, 37011)
    seam = dict(maximum_value_jump=float(np.max(abs(right-left))),
        maximum_first_derivative_jump=float(np.max(abs((right-center)/1e-5-(center-left)/1e-5))))
    # Diagnostic PGM maps retain signal values without a generated illustration or lighting.
    for label, raw in images.items():
        gray = np.clip(128+raw/.00034*110, 0, 255).astype(np.uint8)
        (args.out/(label+'.pgm')).write_bytes(b'P5\n256 256\n255\n'+gray.tobytes())
    result = dict(seed=37011, numpy=np.__version__, seconds=time.perf_counter()-start,
        spectral_comparison=spectral, filter_comparison=rows, cell_boundary_continuity=seam,
        height_bound_m=sum(AMPLITUDES), sheen='Unchanged fixed roughness0.70 Charlie energy model',
        gates=dict(reduced_discrete_spectral_peaks=all(spectral[i+1]['top_eight_bin_energy_fraction'] < spectral[i]['top_eight_bin_energy_fraction']*.5 for i in [0, 2]),
            cell_boundary_continuous=seam['maximum_value_jump'] < 1e-7 and seam['maximum_first_derivative_jump'] < 1e-5),
        limitations=['No native rendering/cost/softness conclusion.', 'Two value-noise layers may still reveal lattice bias or look pebbled; actual same-camera A/B must judge.',
            'Cell fade approximates filtering; it does not exactly integrate stochastic noise over a pixel.',
            'Generic bind-space elongated clusters are not anatomically combed fur.', 'No silhouette, opacity or strand shadows.'])
    result['sources'] = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in
        [pathlib.Path(__file__), pathlib.Path(__file__).with_name('LocalizedNap.metal'), pathlib.Path(reference.__file__)]}
    (args.out/'metrics.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(dict(seconds=result['seconds'], gates=result['gates'], spectral=spectral, seam=seam)))

if __name__ == '__main__': main()
