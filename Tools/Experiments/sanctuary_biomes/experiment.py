#!/usr/bin/env python3
"""Bounded causal-biome research; these maps are NOT Sanctuary renderer evidence.

NumPy + Pillow only. World coordinates/metres; synthetic dimensionless climate/soil.
No production imports, saves, assets, native apps or GPU work. See accompanying report.
"""
import argparse
import hashlib
import heapq
import json
import math
import platform
from pathlib import Path
import time

import numpy as np
from PIL import Image, ImageDraw

EXTENT = 16000.0
OFFSETS = [(dy, dx) for dy in (-1, 0, 1) for dx in (-1, 0, 1) if dy or dx]
BIOMES = ['woodland', 'meadow', 'riparian', 'alpine', 'dry scrub', 'warm wet forest']
PALETTE = np.array([[78,116,69],[156,173,91],[66,148,137],[150,159,167],
                    [189,161,101],[37,105,69]], dtype=float)


def macro(x, z, seed):
    """Globally addressed analytic source, independent of grid traversal/tiles."""
    rng = np.random.Generator(np.random.PCG64(seed))
    phases = rng.uniform(-math.pi, math.pi, 7)
    fault_x = -1700 + 1250*np.sin(z/6900 + phases[0]) + 380*np.sin(z/2300 + phases[1])
    fault_d = np.abs(x-fault_x)
    along = 0.78 + 0.22*np.cos(z/4400 + phases[2])
    uplift = 900*np.exp(-(fault_d/1700)**2)*along
    secondary_d = np.abs(0.72*x + 0.69*z - 2300)
    uplift += 460*np.exp(-(secondary_d/1400)**2)*np.exp(-((z-1800)/8200)**2)
    edge = np.clip((EXTENT-np.maximum(np.abs(x),np.abs(z)))/2500, 0, 1)
    edge = edge*edge*(3-2*edge)
    detail = np.zeros(np.broadcast_shapes(x.shape, z.shape))
    for i, (amp, length) in enumerate([(80,2700),(37,1100),(16,480),(6,230)]):
        detail += amp*np.sin((0.73*x+0.68*z)/length+phases[i+1])*np.cos(
            (-0.63*x+0.77*z)/length+phases[i+2])
    bed = -35 + edge*(165 + uplift + detail)
    return bed, uplift*edge, fault_x


def drainage(bed, dx):
    """Priority-flood spill levels; steepest D8 flow, flood-parent tie breaks on flats.

    Spill surface is separate from the terrain bed: depression water does not fill rock.
    Boundary + sea cells are prescribed outlets. Queue index breaks equal-height ties.
    """
    n = len(bed); size = n*n
    h = bed.ravel(); filled = h.copy(); visited = np.zeros(size, bool)
    fallback = np.full(size, -1, np.int32); queue = []; order = []
    for j in range(size):
        y,x = divmod(j,n)
        if x in (0,n-1) or y in (0,n-1) or h[j] <= 0:
            visited[j] = True; filled[j] = max(0,h[j]); heapq.heappush(queue,(filled[j],j))
    while queue:
        level,j = heapq.heappop(queue); order.append(j); y,x = divmod(j,n)
        for oy,ox in OFFSETS:
            yy,xx = y+oy,x+ox
            if not(0<=yy<n and 0<=xx<n): continue
            k=yy*n+xx
            if visited[k]: continue
            visited[k]=True; fallback[k]=j; filled[k]=max(h[k],level)
            heapq.heappush(queue,(filled[k],k))
    order=np.array(order,np.int32); rank=np.empty(size,np.int32); rank[order]=np.arange(size)
    receiver=fallback.copy(); lengths=np.full(size,dx)
    for j in order:
        if fallback[j] < 0: continue
        y,x=divmod(int(j),n); best=0.0
        for oy,ox in OFFSETS:
            yy,xx=y+oy,x+ox
            if not(0<=yy<n and 0<=xx<n): continue
            k=yy*n+xx; length=dx*math.hypot(oy,ox)
            slope=(filled[j]-filled[k])/length
            if slope>best:
                best=slope; receiver[j]=k; lengths[j]=length
        if best==0:
            yy,xx=divmod(int(receiver[j]),n); lengths[j]=dx*math.hypot(y-yy,x-xx)
    area=np.full(size,dx*dx); destination=np.arange(size,dtype=np.int32)
    for j in order:
        if receiver[j]>=0: destination[j]=destination[receiver[j]]
    for j in order[::-1]:
        if receiver[j]>=0: area[receiver[j]]+=area[j]
    valid=receiver>=0
    assert np.all(rank[receiver[valid]] < rank[valid]), 'Non-acyclic flow graph'
    assert len(order)==size
    return filled.reshape(bed.shape),receiver,lengths,order,area.reshape(bed.shape),destination


def evolve(initial, uplift, dx, steps, deadline):
    bed=initial.copy(); history=[]
    for iteration in range(steps):
        if time.monotonic()>deadline: raise RuntimeError('CPU experiment exceeded 110-second budget')
        spill,receiver,lengths,order,area,_=drainage(bed,dx)
        old=bed.ravel(); new=old.copy(); up=12*uplift.ravel()/max(1,uplift.max())
        for j in order:
            if receiver[j]<0: continue
            # Bounded implicit n=1 stream-power relaxation. This dt*K is a design parameter,
            # not calibrated geological time. Depositional sediment transport is omitted.
            c=0.10*math.sqrt(area.ravel()[j])/lengths[j]
            raised=old[j]+up[j]
            new[j]=min(raised,(raised+c*new[receiver[j]])/(1+c))
        history.append({'iteration':iteration,'max_cut_m':float(np.max(old+up-new)),
                        'max_fill_depth_m':float(np.max(spill-bed))})
        bed=new.reshape(bed.shape)
    return bed,history


def climate(bed, dx, eastward=True):
    """Finite moist column advected along x, with delayed rain and lee evaporation.

    Not Smith/Barstad's Fourier airflow solver. All reservoirs are dimensionless;
    local transfer conserves incoming vapor + cloud + rainwater apart from rainout.
    Sea replenishment is explicit. This is a prevailing-climate proxy, not live weather.
    """
    h=bed if eastward else bed[:,::-1]; n=len(h)
    vapor=np.ones(n); cloud=np.zeros(n); drops=np.zeros(n); rain=np.zeros_like(h)
    replenished=np.zeros(n); previous=np.zeros(n)
    for x in range(n):
        sea=h[:,x]<=0
        addition=np.where(sea,np.maximum(0,1-vapor),0)
        vapor+=addition; replenished+=addition
        rise=np.maximum(0,h[:,x]-previous); descent=np.maximum(0,previous-h[:,x])
        condensation=vapor*(1-np.exp(-rise/640))
        background=vapor*(1-np.exp(-dx/90000))
        vapor-=condensation+background; cloud+=condensation+background
        evaporation=cloud*(1-np.exp(-descent/260)); cloud-=evaporation; vapor+=evaporation
        converted=cloud*(1-np.exp(-dx/900)); cloud-=converted; drops+=converted
        evaporation=drops*(1-np.exp(-descent/600)); drops-=evaporation; vapor+=evaporation
        fallout=drops*(1-np.exp(-dx/650)); drops-=fallout; rain[:,x]=fallout
        previous=np.maximum(0,h[:,x])
    residual=1+replenished-vapor-cloud-drops-rain.sum(axis=1)
    result=rain if eastward else rain[:,::-1]
    # Dimensionless moisture index uses a fixed reference, not a per-seed min/max stretch.
    return np.clip(result*7000/dx,0,1),float(np.max(np.abs(residual)))


def ecology(bed,rain,runoff,dx,z):
    gz,gx=np.gradient(bed,dx); slope=np.hypot(gx,gz)
    temp=21-0.0065*np.maximum(0,bed)-4*(z/EXTENT)
    convergent=np.clip(np.log1p(runoff/5e4)/4,0,1)
    moisture=np.clip(0.77*rain+0.34*convergent,0,1)
    # Explicit game-design proxies: weathering/moisture and valley retention versus slope loss.
    soil=np.clip((0.2+0.6*moisture+0.25*convergent)*np.exp(-2.3*slope),0,1)
    cold=np.clip((14-temp)/7,0,1); warm=np.clip((temp-15)/8,0,1)
    wet=np.clip(moisture/0.55,0,1); dry=1-np.clip(moisture/0.36,0,1)
    scores=np.stack([soil*wet*(1-cold)*(1-0.55*warm),
                     soil*(1-0.7*wet)*(1-cold),
                     convergent**3*soil*(1-cold),
                     np.maximum(cold,np.clip((bed-550)/550,0,1))*(0.3+0.7*wet),
                     dry*(0.3+0.7*warm)*(1-cold),soil*wet*warm],axis=-1)
    weights=(scores+0.015)/(scores+0.015).sum(axis=-1,keepdims=True)
    return moisture,soil,temp,slope,weights


def route(bed,dx):
    """Find a grade-constrained coarse route; never flatten terrain to force success."""
    n=len(bed); a=(n//2)*n+n//5; b=(n//2)*n+4*n//5
    dist=np.full(n*n,np.inf); dist[a]=0; parent=np.full(n*n,-1,np.int32); q=[(0,a)]
    while q:
        cost,j=heapq.heappop(q)
        if cost!=dist[j]: continue
        if j==b: break
        y,x=divmod(j,n)
        for oy,ox in OFFSETS:
            yy,xx=y+oy,x+ox
            if not(0<=yy<n and 0<=xx<n) or bed[yy,xx]<=0: continue
            length=dx*math.hypot(oy,ox); grade=abs(bed[yy,xx]-bed[y,x])/length
            if grade>0.55: continue
            k=yy*n+xx; candidate=cost+length*(1+14*grade*grade)
            if candidate<dist[k]: dist[k]=candidate; parent[k]=j; heapq.heappush(q,(candidate,k))
    points=[]; j=b
    if np.isfinite(dist[b]):
        while j>=0: points.append(divmod(j,n)); j=int(parent[j])
        points=points[::-1]
    grades=[]; length=0.0
    for (y,x),(yy,xx) in zip(points,points[1:]):
        segment=dx*math.hypot(y-yy,x-xx); length+=segment
        grades.append(abs(bed[y,x]-bed[yy,xx])/segment)
    return points,{'found':bool(points),'length_m':length,'maximum_edge_grade':max(grades,default=None)}


def palette(values, stops):
    t=np.clip(values,0,1)*(len(stops)-1); lo=np.minimum(t.astype(int),len(stops)-2)
    f=(t-lo)[...,None]; colors=np.array(stops)
    return np.uint8(colors[lo]*(1-f)+colors[lo+1]*f)


def maps(output,seed,x,z,initial,uplift,bed,spill,area,rain,reverse,soil,weights,points):
    terrain=palette((bed+35)/1050,[(30,67,97),(97,135,92),(169,163,127),(191,188,175),(240,242,239)])
    gz,gx=np.gradient(bed,x[0,1]-x[0,0]); light=np.clip((1-gx*.8+gz*.5)/np.sqrt(1+gx*gx+gz*gz),.35,1.15)
    terrain=np.uint8(np.clip(terrain*light[...,None],0,255)); terrain[bed<=0]=[40,81,112]
    panels=[('Analytic uplift constraint / m',palette(uplift/1000,[(25,35,60),(142,75,83),(251,204,106)])),
      ('Initial relief / m',palette((initial+35)/1050,[(30,67,97),(97,135,92),(169,163,127),(240,242,239)])),
      ('Eroded relief + route / m',terrain),
      ('Drainage / log10 catchment m2',palette(np.log10(area)/8,[(241,238,215),(137,180,176),(12,65,121)])),
      ('West wind moisture proxy [0,1]',palette(rain,[(221,192,139),(126,174,130),(39,97,143)])),
      ('Reversed wind (same terrain)',palette(reverse,[(221,192,139),(126,174,130),(39,97,143)])),
      ('Soil retention proxy [0,1]',palette(soil,[(82,79,75),(165,127,85),(142,169,96)])),
      ('Continuous suitability blend',np.uint8(weights@PALETTE)),
      ('Land basin depth / 0-25 m (sea masked)',palette(np.where(bed>0,np.clip((spill-bed)/25,0,1),0),[(232,227,207),(102,173,179),(18,61,119)]))]
    cell=384; pad=20; top=70; canvas=Image.new('RGB',(3*(cell+pad)+pad,3*(cell+52)+top),(246,245,238))
    draw=ImageDraw.Draw(canvas); draw.text((pad,15),f'RESEARCH MAPS - NOT RENDERER EVIDENCE | seed {seed} | {len(bed)} x {len(bed)} | 32 km square',fill=(20,27,33))
    draw.text((pad,35),'Same scales across seeds. North/+Z at top. Wind travels left to right; reverse panel right to left.',fill=(40,50,55))
    for i,(title,rgb) in enumerate(panels):
        px=pad+(i%3)*(cell+pad); py=top+(i//3)*(cell+52)
        tile=Image.fromarray(rgb[::-1]).resize((cell,cell),Image.Resampling.NEAREST)
        canvas.paste(tile,(px,py)); draw.text((px,py+cell+8),title,fill=(20,27,33))
        if i==2 and points:
            line=[(px+xx/(len(bed)-1)*(cell-1),py+(1-yy/(len(bed)-1))*(cell-1)) for yy,xx in points]
            draw.line(line,fill=(245,212,86),width=2)
    canvas.save(output/f'seed-{seed}-research.png')


def experiment(seed,n,steps,output,deadline):
    started=time.monotonic(); axis=np.linspace(-EXTENT,EXTENT,n); x,z=np.meshgrid(axis,axis); dx=axis[1]-axis[0]
    initial,uplift,fault=macro(x,z,seed); bed,history=evolve(initial,uplift,dx,steps,deadline)
    spill,receiver,lengths,order,area,destination=drainage(bed,dx)
    rain,budget=climate(bed,dx); reverse,reverse_budget=climate(bed,dx,False)
    runoff=(rain*dx*dx).ravel().copy()
    for j in order[::-1]:
        if receiver[j]>=0: runoff[receiver[j]]+=runoff[j]
    runoff=runoff.reshape(bed.shape)
    moisture,soil,temp,slope,weights=ecology(bed,rain,runoff,dx,z)
    points,route_metrics=route(bed,dx)
    # Equal-distance, same-row windward/lee bands around the primary fault; exclude sea.
    west=(x>fault-3500)&(x<fault-700)&(bed>0)&(np.abs(z)<10000)
    east=(x>fault+700)&(x<fault+3500)&(bed>0)&(np.abs(z)<10000)
    chunks=[]
    for col in np.array_split(np.arange(n),4): chunks.append(macro(x[:,col],z[:,col],seed)[0])
    tiled=np.concatenate(chunks,axis=1)
    land=bed>0; valid=receiver>=0; flat_spill=spill.ravel()
    terminal_sea=bed.ravel()[destination]<=0
    watersheds,counts=np.unique(destination[land.ravel()],return_counts=True)
    metrics={'seed':seed,'grid':n,'spacing_m':dx,'iterations':steps,
      'elevation_m':{'min':float(bed.min()),'max':float(bed.max())},
      'land_grade':{'p95':float(np.percentile(slope[land],95)),'max':float(slope[land].max())},
      'flow':{'land_cells':int(land.sum()),'routed_fraction':float(np.mean(terminal_sea[land.ravel()])),
        'uphill_spill_edges':int(np.sum(flat_spill[receiver[valid]]>flat_spill[valid]+1e-9)),
        'outlets':len(watersheds),'largest_catchment_km2':float(counts.max()*dx*dx/1e6),
        'filled_land_fraction':float(np.mean((spill-bed)[land]>.01)),
        'maximum_land_basin_depth_m':float((spill-bed)[land].max()),
        'uphill_bed_edges_retained_as_lake_connections':int(np.sum(
            bed.ravel()[receiver[valid]]>bed.ravel()[valid]+1e-9)),
        'source_area_minus_outlet_area_m2':float(n*n*dx*dx-area.ravel()[receiver<0].sum())},
      'rain':{'west_band':float(rain[west].mean()),'east_band':float(rain[east].mean()),
        'reverse_west_band':float(reverse[west].mean()),'reverse_east_band':float(reverse[east].mean()),
        'column_budget_max_residual':budget,'reverse_budget_max_residual':reverse_budget},
      'biome_weight_sum_max_error':float(np.max(np.abs(weights.sum(axis=-1)-1))),
      'dominant_suitability_land_fraction':dict(zip(BIOMES,[float(np.mean(weights.argmax(axis=-1)[land]==i)) for i in range(len(BIOMES))])),
      'analytic_tile_max_error_m':float(np.abs(tiled-initial).max()),'route':route_metrics,
      'height_change_from_initial_rms_m':float(np.sqrt(np.mean((bed-initial)**2))),
      'history':history,'seconds':time.monotonic()-started}
    arrays={'bed_m':bed,'initial_m':initial,'uplift_constraint_m':uplift,'spill_m':spill,
      'receiver':receiver.reshape(bed.shape),'catchment_m2':area,'rain_index':rain,
      'runoff_area_times_rain_index':runoff,
      'reverse_rain_index':reverse,'moisture_index':moisture,'soil_index':soil,'temperature_proxy_c':temp,
      'biome_weights':weights,'route_yx':np.array(points,dtype=np.int32)}
    digest=hashlib.sha256()
    for key,array in sorted(arrays.items()): digest.update(key.encode()); digest.update(array.tobytes())
    metrics['array_sha256']=digest.hexdigest()
    np.savez_compressed(output/f'seed-{seed}.npz',**arrays)
    maps(output,seed,x,z,initial,uplift,bed,spill,area,rain,reverse,soil,weights,points)
    (output/f'seed-{seed}.json').write_text(json.dumps(metrics,indent=2)+'\n')
    return metrics


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--seeds',type=int,nargs='+',default=[82317,1447,9001])
    parser.add_argument('--grid',type=int,default=192)
    parser.add_argument('--steps',type=int,default=6)
    args=parser.parse_args()
    if not(64<=args.grid<=256 and 1<=len(args.seeds)<=3 and 1<=args.steps<=8): parser.error('Bounded experiment limits exceeded')
    args.output.mkdir(parents=True,exist_ok=False)
    start=time.monotonic(); deadline=start+110
    results=[experiment(seed,args.grid,args.steps,args.output,deadline) for seed in args.seeds]
    manifest={'purpose':'Research maps, not game renderer or production simulation evidence',
      'source_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
      'numpy':np.__version__,'python':platform.python_version(),'machine':platform.machine(),
      'total_seconds':time.monotonic()-start,'parameters':{'seeds':args.seeds,'grid':args.grid,'steps':args.steps},
      'results':results}
    (args.output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(json.dumps(manifest,indent=2))


if __name__=='__main__': main()
