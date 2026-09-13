#!/usr/bin/env python3
"""Scalar transcription of existing GroomCoverage, NOT a Wrela rendered image.
No dependencies; no renderer changes. Reproduce expected single-patch coverage
and projection scales before requesting a native material diagnostic.
"""
import argparse, csv, hashlib, json, math, pathlib, time

def clamp(x, a=0., b=1.): return min(b,max(a,x))
def smooth(a,b,x):
    t=clamp((x-a)/max(.000001,b-a));return t*t*(3-2*t)
def random(seed):
    x=(seed+0x9e3779b9)&0xffffffff
    x=((x^(x>>16))*2246822519)&0xffffffff
    x=((x^(x>>13))*3266489917)&0xffffffff
    return ((x^(x>>16))&0xffffff)/16777216.
def coverage(x,t,density,variation,dx,dt):
    if density<=0:return 1.
    t=clamp(t);phase=x+.12*math.sin(t*11)*math.sin(t*math.pi)
    span=max(.002,abs(dx));duty=density*(1-.55*smooth(.55,1,t))
    def integral(a):return math.floor(a)*duty+clamp(a-math.floor(a)-.5+duty*.5,0,duty)
    local=phase-math.floor(phase)
    pulse=clamp((integral(local+span*.5)-integral(local-span*.5))/span)
    variation=max(.001,variation);end=1-variation*random(max(0,math.floor(phase)))
    feather=max(.008,abs(dt));individual=1-smooth(end-feather,end,t)
    mean=clamp((1-t)/variation);blend=smooth(.6,1.6,span)
    return clamp(pulse*(individual*(1-blend)+mean*blend))

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--out',required=True);args=ap.parse_args()
    start=time.perf_counter();out=pathlib.Path(args.out);out.mkdir(parents=True,exist_ok=True)
    rows=[]
    for density in [.25,.5,.75,.94]:
      for footprint in [.05,.25,.5,1,2,3.05,8]:
       for t in [.25,.5,.7,.9,1]:
        values=[coverage(370+i/512,t,density,.18,footprint,.03) for i in range(512)]
        rows.append(dict(density=density,footprintPeriods=footprint,t=t,minimum=min(values),maximum=max(values),mean=sum(values)/len(values)))
    with (out/'coverage-sweep.csv').open('w') as f:
      w=csv.DictWriter(f,fieldnames=rows[0]);w.writeheader();w.writerows(rows)
    # Use an exact integer footprint to independently check pulse integration.
    integerError=max(abs(coverage(123+i/32,.25,d,.18,8,.001)-d) for i in range(32) for d in [.25,.5,.75,.94])
    tipMax=max(coverage(370+i/32,1,.94,.18,dx,.03) for i in range(32) for dx in [.05,3.05,8])
    assert integerError<1e-12 and tipMax==0
    focal=1080/(2*math.tan(math.radians(60.1605682)/2))
    projection=[]
    for distance in [.25,1,1.8288466,2.2761991,4]:
      pixelMetres=distance/focal
      projection.append({'distanceMetres':distance,'frontoparallelMetresPerPixel':pixelMetres,'periodsPerPixel':pixelMetres/.0008,'bodyPatchWidthPixels':.055/pixelMetres,'bodyPatchLengthPixels':.067/pixelMetres,'headPatchWidthPixels':.035/pixelMetres})
    # Side geometry is abruptly clipped while along-tip coverage is filtered.
    # These are source-domain values, not a sample mask measurement.
    sideJump=coverage(370,.25,.94,.18,3.05,.03)
    rowsAtNative=[r for r in rows if r['density']==.94 and r['footprintPeriods']==3.05]
    sources=['Engine/FieldCore/GroomCoverage.swift','Engine/FieldEngine/Resources/Surface.metal','Games/Sanctuary/Project/SanctuarySunhareCoatDesign.swift']
    receipt={'kind':'CPU scalar expectation, not native or GPU evidence','scriptSHA256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),'sourceSHA256':{p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest() for p in sources},'projectionAssumptions':'frontoparallel surface at stated camera-target distance; real depth/foreshortening/fwidth require native diagnostic','projection':projection,'nativeApproximation':rowsAtNative,'sourceEdgeCoverageJumpAtT025':sideJump,'integerFootprintMeanError':integerError,'tipMaximum':tipMax,'MSAALimitation':'4 samples give only5 sample-count levels, but implementation mapping/dither/order unknown; no simulated mask claimed','decision':'Mean .94 coverage at unresolved scales is expected to look almost opaque. Current interface has no transverse patch envelope. Do not increase density or change patch shape before single-patch A2C/normal controls.','elapsedSeconds':time.perf_counter()-start}
    (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(receipt,indent=2))
if __name__=='__main__':main()
