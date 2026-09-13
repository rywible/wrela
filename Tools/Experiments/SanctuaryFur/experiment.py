#!/usr/bin/env python3
"""One short-coat source experiment. Numeric swatches are NOT Wrela renderer evidence.
No third-party dependencies. Run with --out to retain an isolated deterministic result.
"""
import argparse, csv, hashlib, json, math, pathlib, time

SEED = 37011
ROUGHNESS = .70
AMOUNT = .16
# Cycles/metre in bind XYZ, height amplitude in metres. Deliberate elongated groups,
# not individual hairs; three-dimensional phases avoid a UV seam or world-space swimming.
MODES = [(14, 5, 11, .00010), (-19, 7, 13, .00008),
         (87, 13, 46, .000055), (-113, 18, 71, .000045),
         (157, 23, -102, .000035), (-211, 29, -134, .000025)]
PHASES = [((SEED * 1664525 + i * 1013904223) & 0xffffffff) / 2**32 * 2*math.pi
          for i in range(len(MODES))]

def smooth(a, b, x):
    t = min(1, max(0, (x-a)/(b-a)))
    return t*t*(3-2*t)

def attenuation(mode, dx, dy):
    fx = sum(mode[i]*dx[i] for i in range(3))
    fy = sum(mode[i]*dy[i] for i in range(3))
    # Conservative directional band limit; Gaussian pixel filter below it.
    cycles = max(abs(fx), abs(fy))
    return (1-smooth(.30, .50, cycles))*math.exp(-(2*math.pi)**2*(fx*fx+fy*fy)/24)

def nap(p, dx, dy, wet=0, filtered=True):
    h = 0
    for mode, phase in zip(MODES, PHASES):
        wave = 2*math.pi*sum(mode[i]*p[i] for i in range(3))+phase
        h += mode[3]*math.sin(wave)*(attenuation(mode, dx, dy) if filtered else 1)
    return h*(1-.70*wet)

def sheen(nv, nl, nh):
    if nv <= 0 or nl <= 0:
        return 0
    inv = 1/(ROUGHNESS*ROUGHNESS)
    d = (2+inv)*max(0, 1-nh*nh)**(inv*.5)/(2*math.pi)
    # Charlie NDF with the inexpensive Neubelt visibility approximation.
    return d/(4*max(nl+nv-nl*nv, 1e-8))

def hemisphere(nv, count_z=48, count_phi=96):
    vx = math.sqrt(max(0, 1-nv*nv))
    weight = 2*math.pi/(count_z*count_phi)
    for j in range(count_z):
        nl = (j+.5)/count_z
        r = math.sqrt(1-nl*nl)
        for i in range(count_phi):
            phi = (i+.5)*2*math.pi/count_phi
            lx, ly = r*math.cos(phi), r*math.sin(phi)
            hn = math.sqrt((lx+vx)**2+ly*ly+(nl+nv)**2)
            yield nl, (nl+nv)/hn, weight

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--out', type=pathlib.Path,
                        default=pathlib.Path('.build/sanctuary-fur-experiment'))
    args = parser.parse_args(); args.out.mkdir(parents=True, exist_ok=True)
    started = time.perf_counter()
    nodes = [max(.0001, i/32) for i in range(33)]
    lut = [sum(sheen(nv,nl,nh)*nl*w for nl,nh,w in hemisphere(nv)) for nv in nodes]
    def energy(nv):
        x = min(32,max(0,nv*32)); i = min(31,int(x)); t=x-i
        return lut[i]*(1-t)+lut[i+1]*t
    energy_rows=[]
    for nv in [.0001,.03,.08,.2,.4,.7,1]:
        for wet in [0,1]:
            amount=AMOUNT*(1-.80*wet)
            total=0
            for nl,nh,w in hemisphere(nv):
                scale=max(0,1-amount*max(energy(nv),energy(nl)))
                total+=(scale/math.pi+amount*sheen(nv,nl,nh))*nl*w
            energy_rows.append(dict(nv=nv,wet=wet,white_lambert_plus_sheen=total))
    # Pixel footprint from actual engine FOV1.05 and 1080 vertical pixels.
    filter_rows=[]; swatches=[]
    for depth in [.5,1,2,4,8]:
        pixel=2*depth*math.tan(1.05/2)/1080
        dx=(pixel,0,0);dy=(0,pixel,0)
        values=[];unfiltered=[];shifted=[]
        for j in range(64):
            for i in range(64):
                p=(i*pixel,j*pixel,.037)
                values.append(nap(p,dx,dy));unfiltered.append(nap(p,dx,dy,filtered=False))
                shifted.append(nap((p[0]+pixel*.25,p[1],p[2]),dx,dy))
        rms=lambda v:math.sqrt(sum(x*x for x in v)/len(v))
        raw_delta=rms([nap((i%64*pixel+pixel*.25,i//64*pixel,.037),dx,dy,filtered=False)-v
                       for i,v in enumerate(unfiltered)])
        filtered_delta=rms([a-b for a,b in zip(values,shifted)])
        filter_rows.append(dict(distance_m=depth,pixel_m=pixel,rms_height_m=rms(values),
                                quarter_pixel_delta_m=filtered_delta,raw_delta_m=raw_delta,
                                alias_band_amplitude=sum(abs(m[3]*attenuation(m,dx,dy)) for m in MODES
                                  if max(abs(m[0]*pixel),abs(m[1]*pixel))>=.5)))
        swatches.append((depth,values))
    with open(args.out/'filter.csv','w') as f:
        writer=csv.DictWriter(f,fieldnames=filter_rows[0]);writer.writeheader();writer.writerows(filter_rows)
    svg=['<svg xmlns="http://www.w3.org/2000/svg" width="1120" height="220">',
         '<rect width="1120" height="220" fill="#eee"/>',
         '<text x="15" y="20">NUMERICAL HEIGHT SWATCHES — not a native render; contrast magnified</text>']
    for index,(depth,values) in enumerate(swatches):
        x0=15+index*220
        svg.append(f'<text x="{x0}" y="43">{depth:g} m / 1080p footprint</text>')
        for j in range(64):
            for i in range(64):
                gray=round(min(255,max(0,128+values[j*64+i]/.00034*100)))
                svg.append(f'<rect x="{x0+i*3}" y="{53+j*2}" width="3" height="2" fill="rgb({gray},{gray},{gray})"/>')
    svg.append('</svg>');(args.out/'height-swatches.svg').write_text('\n'.join(svg))
    (args.out/'ShortCoatLUT.generated.metal').write_text(
        '// Generated representation: experiment.py, seed37011, roughness.70.\n'
        'constant float shortCoatEnergy[33] = {'+', '.join(f'{x:.9f}f' for x in lut)+'};\n')
    max_energy=max(r['white_lambert_plus_sheen'] for r in energy_rows)
    source_hash=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()
    result=dict(seed=SEED,source_sha256=source_hash,seconds=time.perf_counter()-started,
                modes=MODES,phases=PHASES,roughness=ROUGHNESS,amount=AMOUNT,
                sheen_directional_energy=lut,energy_samples=energy_rows,filter_samples=filter_rows,
                gates=dict(sampled_energy_le_one=max_energy<=1.0005,
                           maximum_sampled_energy=max_energy,
                           zero_nyquist_band_residue=all(r['alias_band_amplitude']==0 for r in filter_rows)),
                limitations=['Numerical swatches are not renderer evidence.',
                 'Scalar integration checks this fixed sheen approximation, not every light/environment.',
                 'Surface nap cannot change silhouette or cast fibre shadows.',
                 'CPU timing does not estimate M4 GPU shader cost.'])
    (args.out/'metrics.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(dict(seconds=result['seconds'],gates=result['gates'],out=str(args.out))))

if __name__=='__main__': main()
