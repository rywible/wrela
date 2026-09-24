"""Spatial visibility with reusable rest-space candidates under affine sway.
Independent reference intersects deformed parallelograms in world space.
This is not nonlinear bending, volumetric needles, or full forest transport.
"""
import sys
from pathlib import Path
import json
import hashlib
import numpy as np
from PIL import Image, ImageDraw
arguments = sys.argv[1:]
sys.argv = sys.argv[:1]
from appearance import branch_geometry, unit

OUT = Path('output/compiler-frontiers/branch-spatial')
OUT.mkdir(parents=True, exist_ok=True)
c,u,v,n,length,width,snow=branch_geometry()
N=len(c)

def deform(p,b,point=False):
    q=p.copy();q[...,1]-=b*(p[...,0]+(1 if point else 0));return q

def intersection(p,d,centers,tangents,bitangents,owners=None,candidates=None):
    """Independent world-plane intersection plus general (nonorthogonal) Gram solve."""
    normals=np.cross(tangents,bitangents)
    denominator=np.sum(d[:,None,:]*normals[None,:,:],axis=-1)
    safe=np.where(abs(denominator)>1e-12,denominator,1)
    t=np.sum((centers[None,:,:]-p[:,None,:])*normals,axis=-1)/safe
    hit=p[:,None,:]+t[:,:,None]*d[:,None,:]-centers
    a=np.sum(tangents*tangents,axis=-1);b=np.sum(tangents*bitangents,axis=-1);cc=np.sum(bitangents*bitangents,axis=-1)
    hu=np.sum(hit*tangents,axis=-1);hv=np.sum(hit*bitangents,axis=-1)
    along=(hu*cc-hv*b)/(a*cc-b*b);across=(hv*a-hu*b)/(a*cc-b*b)
    valid=(t>1e-5)&(abs(denominator)>1e-12)&(abs(along)<=length/2)&(abs(across)<=width/2)
    if owners is not None:valid[np.arange(len(p)),owners]=False
    if candidates is not None:valid &= candidates[owners]
    distances=np.where(valid,t,np.inf)
    ids=distances.argmin(axis=1);dist=distances[np.arange(len(p)),ids]
    return np.where(np.isfinite(dist),ids,-1),dist

def batched(p,d,cc,uu,vv,owners=None,candidates=None):
    ids=[];dist=[]
    for start in range(0,len(p),512):
        stop=start+512
        a,b=intersection(p[start:stop],d[start:stop],cc,uu,vv,None if owners is None else owners[start:stop],candidates)
        ids.extend(a);dist.extend(b)
    return np.array(ids),np.array(dist)

if '--prepare' in arguments:
    plates=[dict(center=c[i].tolist(),tangent=u[i].tolist(),bitangent=v[i].tolist(),halfLength=float(length[i]/2),halfWidth=float(width[i]/2)) for i in range(N)]
    (OUT/'geometry.json').write_text(json.dumps(plates));sys.exit()
product=json.loads((OUT/'product.json').read_text())
mask=np.zeros((N,N),dtype=bool)
for i in range(N):mask[i,product['candidates'][product['offsets'][i]:product['offsets'][i+1]]]=True
rng=np.random.default_rng(6514)
queries=[];reference=[];cases=[]
for bend in [-.12,0,.12,.35]:
    count=2048;owners=rng.integers(0,N,count)
    local=c[owners]+u[owners]*(rng.uniform(-.499,.499,count)*length[owners])[:,None]+v[owners]*(rng.uniform(-.499,.499,count)*width[owners])[:,None]
    angles=rng.uniform(0,2*np.pi,count)
    light=unit(np.column_stack((np.cos(angles),np.full(count,.55),np.sin(angles))))
    # Float32 input is shared with GPU; the reference starts from that exact input.
    local=local.astype(np.float32).astype(float);light=light.astype(np.float32).astype(float);bend=float(np.float32(bend))
    world=deform(local,bend,True);ld=deform(light,-bend)
    ref,_=batched(world,light,deform(c,bend,True),deform(u,bend),deform(v,bend),owners)
    eligible=ld[:,1]>=product['minimumRise']*np.linalg.norm(ld[:,[0,2]],axis=1)+1e-6
    compact,_=batched(local,ld,c,u,v,owners,mask)
    full,_=batched(local,ld,c,u,v,owners)
    actual=np.where(eligible,compact,full)
    mismatch=int(np.count_nonzero((actual>=0)!=(ref>=0)))
    assert mismatch==0
    cases.append(dict(shear=bend,queries=count,fallbacks=int((~eligible).sum()),mismatches=mismatch))
    queries.extend(np.column_stack((local,owners,light,np.full(count,bend))).ravel().tolist())
    reference.extend(np.repeat((ref<0).astype(float),3).tolist())

frames=[];centroid_differences=[]
for frame in range(12):
    bend=.12*np.sin(frame*2*np.pi/12);theta=.45+frame*.09
    cc,uu,vv=deform(c,bend,True),deform(u,bend),deform(v,bend)
    view=unit(np.array([.2,.9,1.]));right=unit(np.cross([0.,1,0],view));up=np.cross(view,right)
    w,h=384,180;yy,xx=np.mgrid[:h,:w]
    origins=((xx.ravel()+.5)/w-.5)[:,None]*right*2.6+(.5-(yy.ravel()+.5)/h)[:,None]*up*(2.6*h/w)+view*4
    rays=np.tile(-view,(len(origins),1))
    ids,dist=batched(origins,rays,cc,uu,vv)
    visible=ids>=0;owner=ids[visible];points=origins[visible]+dist[visible,None]*rays[visible]
    light=unit(np.array([np.cos(theta),.55,np.sin(theta)]));lights=np.tile(light,(len(points),1))
    ref,_=batched(points,lights,cc,uu,vv,owner)
    local=deform(points,-bend,True);ld=deform(lights,-bend)
    selected,_=batched(local,ld,c,u,v,owner,mask)
    assert np.array_equal(ref>=0,selected>=0)
    centroid,_=batched(cc,np.tile(light,(N,1)),cc,uu,vv,np.arange(N))
    centroid_differences.append(float(np.mean((ref>=0)!=(centroid[owner]>=0))))
    normals=unit(np.cross(uu,vv));normals=np.where(normals[:,1,None]<0,-normals,normals)
    diffuse=np.maximum(normals[owner]@light,0)
    albedo=np.array([.04,.15,.08])[None,:]*(1-snow[owner,None])+np.array([.88,.91,.95])[None,:]*snow[owner,None]
    panels=[]
    for title,shadow in [('Centroid shadow',centroid[owner]>=0),('Spatial reference',ref>=0),('Compiled candidates',selected>=0)]:
        pixels=np.tile([.025,.035,.05],(w*h,1));pixels[visible]=albedo*(.2+.8*diffuse*(~shadow))[:,None]
        im=Image.fromarray(np.uint8(np.clip(pixels.reshape(h,w,3),0,1)**(1/2.2)*255))
        canvas=Image.new('RGB',(w,h+30),'#101923');canvas.paste(im,(0,30));ImageDraw.Draw(canvas).text((10,9),title,fill='white');panels.append(canvas)
    canvas=Image.new('RGB',(w*3,h+30));[canvas.paste(im,(i*w,0)) for i,im in enumerate(panels)];frames.append(canvas)
frames[3].save(OUT/'comparison.png');frames[0].save(OUT/'motion.gif',save_all=True,append_images=frames[1:],duration=120,loop=0)
geometry=np.column_stack((c,length/2,u,width/2,v,np.zeros(N),n,snow)).ravel().tolist()
aux=product['offsets']+product['candidates']
aux += [0]*((-len(aux))%4)
(OUT/'gpu-input.json').write_text(json.dumps(dict(geometry=geometry,queries=queries,auxiliary=aux,reference=reference,plates=N,minimumRise=product['minimumRise'])))
result=dict(plates=N,possiblePairs=N*(N-1),candidatePairs=len(product['candidates']),retainedFraction=len(product['candidates'])/(N*(N-1)),byteLength=product['byteLength'],compileMs=product['compileMs'],cases=cases,frames=len(frames),spatialMismatch=0,centroidPixelMismatch=centroid_differences,sourceSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
(OUT/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result,indent=2))
