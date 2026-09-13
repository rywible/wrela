#!/usr/bin/env python3
"""Bounded source-space chart/coverage risk probe, never renderer evidence."""
import argparse, hashlib, json, math, pathlib, time
import numpy as np

SEED = 37011
CENTERS = np.array([[0,.355,.21],[0,.405,-.12],[0,.575,-.235]])
RADII = np.array([[.255,.29,.325],[.205,.255,.255],[.158,.18,.15]])

def body(p):
    # Exact default Shape.sphere.stretched and polynomial smoothUnion formulas.
    d = (np.linalg.norm((p[...,None,:]-CENTERS)/RADII,axis=-1)-1)*RADII.min(axis=1)
    a=d[...,0]
    for b,k in [(d[...,1],.07),(d[...,2],.055)]:
        h=np.clip(.5+.5*(b-a)/k,0,1); a=b+(a-b)*h-k*h*(1-h)
    return a

def gradient(p):
    e=1e-5
    return np.stack([(body(p+np.eye(3)[i]*e)-body(p-np.eye(3)[i]*e))/(2*e) for i in range(3)],-1)

def chart():
    # Exactly 2,048 triangles including two 32-triangle polar fans.
    z=-.385+.92*(1-np.cos(np.linspace(.02,math.pi-.02,32)))/2
    y=np.linspace(.05,.76,512)
    candidates=np.stack(np.broadcast_arrays(np.zeros((32,512)),y[None,:],z[:,None]),-1)
    cy=y[np.argmin(body(candidates),axis=1)]
    theta=np.arange(32)*2*math.pi/32
    origins=np.stack(np.broadcast_arrays(np.zeros((32,32)),cy[:,None],z[:,None]),-1)
    directions=np.stack([np.cos(theta),np.sin(theta),theta*0],-1)[None,:,:]
    radial=np.linspace(0,.7,257)
    values=body(origins[:,:,None,:]+directions[:,:,None,:]*radial[None,None,:,None])
    exits=((values[:,:,:-1]<=0)&(values[:,:,1:]>0)).sum(axis=2)
    lo=np.zeros((32,32));hi=lo+.7
    for _ in range(32):
        mid=(lo+hi)/2;inside=body(origins+directions*mid[...,None])<0
        lo=np.where(inside,mid,lo);hi=np.where(inside,hi,mid)
    vertices=(origins+directions*((lo+hi)/2)[...,None]).reshape(-1,3)
    vertices=np.vstack([vertices,[0,.575,-.385],[0,.355,.535]])
    triangles=[]
    for j in range(31):
        for i in range(32):
            a=j*32+i;b=j*32+(i+1)%32;c=a+32;d=b+32
            triangles.extend([[a,b,c],[b,d,c]])
    for i in range(32):
        triangles.extend([[1024,(i+1)%32,i],[1025,31*32+i,31*32+(i+1)%32]])
    triangles=np.array(triangles)
    # Project triangle center and edge midpoints to the exact source zero set.
    p=vertices[triangles];samples=np.concatenate([p.mean(1),(p[:,0]+p[:,1])/2,(p[:,1]+p[:,2])/2,(p[:,2]+p[:,0])/2])
    q=samples.copy()
    for _ in range(12):
        g=gradient(q);q-=g*(body(q)/np.maximum((g*g).sum(-1),1e-12))[:,None]
    err=np.linalg.norm(q-samples,axis=-1)
    areas=np.linalg.norm(np.cross(p[:,1]-p[:,0],p[:,2]-p[:,0]),axis=-1)/2
    return vertices,triangles,dict(vertices=len(vertices),triangles=len(triangles),sampled_rays=int(exits.size),
        non_single_exit_rays=int((exits!=1).sum()),outside_origins=int((body(origins)>=0).sum()),
        maximum_vertex_field_residual_m=float(np.max(abs(body(vertices)))),minimum_triangle_area_m2=float(areas.min()),
        maximum_sample_projection_error_m=float(err.max()),p95_sample_projection_error_m=float(np.quantile(err,.95)),
        chart_single_exit_gate=bool(np.all(exits==1)),sampled_one_mm_gate=bool(err.max()<=.001),
        limitations='Double precision transcription, not Swift compilation. Sampled projection is not a conservative Hausdorff bound. Single exits checked at finite radial samples; no global proof.')

def coverage():
    rng=np.random.default_rng(SEED);tile=.016;depth=.008
    roots=rng.uniform(0,tile,(128,2));bend=rng.uniform(-1,1,(128,2))*.0006
    origins=rng.uniform(0,tile,(96,2));rows=[]
    def sigma(xy,h):
        total=np.zeros(h.shape)
        for root,b in zip(roots,bend):
            center=root+np.stack([h*.002,np.zeros_like(h)],-1)+np.sin(h*math.pi)[...,None]*b
            delta=(xy-center+tile/2)%tile-tile/2
            total+=np.exp(-(delta*delta).sum(-1)/(2*.00015**2))
        return 1200*total*(1-.65*h)
    for angle in [0,60,80]:
        cs=math.cos(math.radians(angle));tan=math.tan(math.radians(angle))
        def samples(n):
            h=np.broadcast_to((np.arange(n)+.5)/n,(96,n))
            xy=origins[:,None,:]+np.stack([h*depth*tan,h*0],-1)
            return sigma(xy,h)*depth/(n*cs)
        ref=1-np.exp(-samples(1024).sum(-1))
        for n in [4,8,12]:
            tau=samples(n);alpha=1-np.exp(-tau);linear=1-np.exp(-tau.sum(-1))
            # Explicit correlated-mask counterexample, NOT the Metal A2C implementation.
            nested=(alpha[...,None]>np.array([.125,.375,.625,.875])).any(axis=1).mean(axis=-1)
            rows.append(dict(angle_degrees=angle,layers=n,reference_mean=float(ref.mean()),linear_mean=float(linear.mean()),
                mean_absolute_error=float(abs(linear-ref).mean()),maximum_error=float(abs(linear-ref).max()),
                nested_four_sample_mask_mean=float(nested.mean()),nested_mask_mean_absolute_error=float(abs(nested-ref).mean()),
                mean_error_within_five_points=bool(abs(linear-ref).mean()<=.05)))
    homogeneous=[]
    for n in [4,8,12]:
        alpha=1-math.exp(-1.6/n)
        homogeneous.append(dict(layers=n,reference_coverage=1-math.exp(-1.6),per_layer_alpha=alpha,
            nested_mask_coverage=float((alpha>np.array([.125,.375,.625,.875])).mean())))
    return dict(seed=SEED,root_count=128,tile_metres=tile,root_density_per_m2=128/tile**2,guide_sigma_radius_m=.00015,
        coat_depth_m=depth,rays=96,reference_samples=1024,rows=rows,homogeneous_nested_mask_counterexample=homogeneous,
        limitations='Analytic Gaussian guide extinction with midpoint integration. No native rasterization, shading, texture filtering, fin rendering, A2C sample-pattern measurement or fur-identity claim.')

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--out',type=pathlib.Path,required=True);a=ap.parse_args();start=time.perf_counter()
    repository=pathlib.Path(__file__).resolve().parents[3]
    source=repository/'Games/Sanctuary/Project/SanctuarySunhareDesign.swift'
    field=repository/'Engine/FieldCore/Field.swift'
    required=['V3(0, 0.355, 0.21), V3(0.255 * p.bodyRoundness, 0.29, 0.325)',
        'V3(0, 0.405, -0.12), V3(0.205, 0.255, 0.255)',
        'V3(0, 0.575, -0.235), V3(0.158, 0.18, 0.15)']
    if not all(fragment in source.read_text() for fragment in required):
        raise RuntimeError('Default body source changed: re-audit transcription before replay')
    a.out.mkdir(parents=True,exist_ok=True);v,t,g=chart();c=coverage()
    np.savez_compressed(a.out/'body-chart.npz',vertices=v,triangles=t)
    result={'version':1,'geometry':g,'coverage':c,'runtime_seconds':time.perf_counter()-start,
        'script_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
        'source_sha256':{str(p.relative_to(repository)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [source,field]},
        'numpy_version':np.__version__,'decision':'HOLD production adoption; evaluate geometry cap and coverage convergence separately from appearance.'}
    (a.out/'metrics.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))

if __name__=='__main__':main()
