#!/usr/bin/env python3
"""Compare research replay, resolution and the current Swift radial-weight formula.

This parses landmark constants; it does NOT execute production Swift or establish
native equivalence. Run after experiment.py's run-02, replay-82317, refinement-256.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re

import numpy as np
from PIL import Image, ImageDraw
from experiment import EXTENT, PALETTE, ecology


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,required=True)
    parser.add_argument('--repo',type=Path,required=True)
    args=parser.parse_args(); root=args.root
    reference=np.load(root/'run-02/seed-82317.npz')
    replay=np.load(root/'replay-82317/seed-82317.npz')
    errors={key:float(np.max(np.abs(reference[key]-replay[key]))) for key in reference.files}
    assert max(errors.values())==0, 'Deterministic replay changed'
    manifest=json.loads((root/'run-02/manifest.json').read_text())
    refined=json.loads((root/'refinement-256/manifest.json').read_text())
    source=args.repo/'Games/Sanctuary/Content/BiomeGeography.swift'; text=source.read_text()
    records=re.findall(r'\.init\(id: "([^"\n]+)", name: "[^"\n]+", coordinate: SIMD2\(([^,]+), ([^)]+)\), biome: \.([^)]+)\)',text)
    radius_block=text.split('private static let radii:')[1].split(']')[1]
    radii={name:float(number.replace('_','')) for name,number in re.findall(r'\.(\w+): ([\d_]+)',radius_block)}
    assert len(records)==len(radii)==11
    n=len(reference['bed_m']); axis=np.linspace(-EXTENT,EXTENT,n); x,z=np.meshgrid(axis,axis); dx=axis[1]-axis[0]
    logweights=[]
    for _,cx,cz,name in records:
        logweights.append(-0.5*((x-float(cx.replace('_','')))**2+(z-float(cz.replace('_','')))**2)/radii[name]**2)
    logweights=np.stack(logweights,axis=-1); logweights-=logweights.max(axis=-1,keepdims=True)
    radial=np.exp(logweights); radial/=radial.sum(axis=-1,keepdims=True)
    receiver=reference['receiver'].ravel(); flow=(reference['reverse_rain_index']*dx*dx).ravel().copy()
    # Obtain a topological ordering from the already-validated receiver DAG.
    children=np.bincount(receiver[receiver>=0],minlength=n*n); ready=list(np.flatnonzero(children==0))
    for j in ready:
        k=receiver[j]
        if k<0: continue
        flow[k]+=flow[j]; children[k]-=1
        if children[k]==0: ready.append(int(k))
    assert len(ready)==n*n
    reverse_weights=ecology(reference['bed_m'],reference['reverse_rain_index'],flow.reshape((n,n)),dx,z)[-1]
    land=reference['bed_m']>0
    causal_delta=float(np.sqrt(np.mean((reference['biome_weights'][land]-reverse_weights[land])**2)))
    colors=np.array([[81,115,64],[166,177,86],[71,141,129],[97,150,111],[52,110,153],
      [156,164,177],[199,163,99],[38,108,65],[185,178,137],[82,159,156],[34,82,126]])
    panels=[('Current radial weights (formula translation)',np.uint8(radial@colors)),
      ('Experiment suitability, west wind',np.uint8(reference['biome_weights']@PALETTE)),
      ('Same experiment terrain, east wind',np.uint8(reverse_weights@PALETTE))]
    canvas=Image.new('RGB',(1260,530),(246,245,238)); draw=ImageDraw.Draw(canvas)
    draw.text((20,14),'RESEARCH COMPARISON - NOT RENDERER EVIDENCE | same 32 km domain, different geography source',fill=(20,25,30))
    for i,(label,rgb) in enumerate(panels):
        px=20+i*415; canvas.paste(Image.fromarray(rgb[::-1]).resize((390,390)),(px,50))
        draw.text((px,450),label,fill=(20,25,30))
    draw.text((20,485),'Radial colors blend 11 named regions; experiment colors blend 6 ecological suitability classes. Palettes are not equivalent.',fill=(30,35,40))
    draw.text((20,505),'Current radial weights have no wind input. Experiment moisture and suitability change without moving the ridges.',fill=(30,35,40))
    canvas.save(root/'comparison-research.png')
    provenance={}
    for name in ['Terrain.swift','BiomeGeography.swift','BiomeHabitatField.swift']:
        path=args.repo/'Games/Sanctuary/Content'/name; provenance[str(path.relative_to(args.repo))]=hashlib.sha256(path.read_bytes()).hexdigest()
    result={'replay_array_max_errors':errors,'radial_formula_records':records,
      'radial_source_note':'Exact translated distance-weight formula and parsed constants; no production Swift execution.',
      'wind_reversal_radial_weights_change_by_definition':0,
      'wind_reversal_experiment_suitability_rms_change':causal_delta,
      'refinement_seed_82317':{'192':manifest['results'][0],'256':refined['results'][0]},
      'production_source_sha256_at_read':provenance,
      'experiment_source_sha256':manifest['source_sha256'],
      'compare_source_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    (root/'comparison.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'replay_max_error':max(errors.values()),'causal_rms_change':causal_delta},indent=2))


if __name__=='__main__': main()
