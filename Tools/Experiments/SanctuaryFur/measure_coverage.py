#!/usr/bin/env python3
"""Measure actual matched coverage-calibration PNGs, with explicit ROI provenance.
Uses bundled Pillow/numpy; never treats display RGB as linear coverage. Inputs
are captures plus reviewed pixel rectangles, not generated renderer evidence.
"""
import argparse, hashlib, json, pathlib
import numpy as np
from PIL import Image

def sha(path): return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
def srgb_decode(x): return np.where(x<=.04045,x/12.92,((x+.055)/1.055)**2.4)
def srgb_encode(x): return np.where(x<=.0031308,x*12.92,1.055*np.maximum(x,0)**(1/2.4)-.055)
def tone(x): return np.clip(x*(2.51*x+.03)/(x*(2.43*x+.59)+.14),0,1)
def inverse_tone(y):
    # Monotone bisection avoids unstable quadratic roots near black/white.
    lo=np.zeros_like(y);hi=np.full_like(y,100.)
    for _ in range(40):
        mid=(lo+hi)*.5;below=tone(mid)<y;lo=np.where(below,mid,lo);hi=np.where(below,hi,mid)
    return (lo+hi)*.5

def roi(image,box):
    x,y,w,h=map(int,box)
    if w<1 or h<1 or x<0 or y<0 or x+w>image.shape[1] or y+h>image.shape[0]:
        raise ValueError('ROI outside image: '+repr(box))
    return image[y:y+h,x:x+w].reshape(-1,3)

def transfer(image,regions,look,no_bloom):
    black=roi(image,regions['black']).mean(axis=0)
    white=roi(image,regions['white']).mean(axis=0)
    gray=roi(image,regions['gray']).mean(axis=0)
    contrast=float(look['contrast']);saturation=float(look['saturation'])
    # Untouched calibration kinds return linear x; post grade applies gain,
    # warmth, contrast, ACES approximation, saturation and sRGB storage.
    # Saturation1 is required so channels invert independently. White fitting
    # absorbs exposure and warmth per channel. Gray is an independent check.
    failures=[]
    if abs(saturation-1)>1e-6:failures.append('nonunit display saturation')
    if not 0.1<contrast<4:failures.append('unsupported contrast')
    if np.max(white)>=254/255:failures.append('white chip clipped')
    if np.max(black)>2/255:failures.append('black chip exceeds2 encoded levels; bloom/contamination possible')
    if not no_bloom:failures.append('no zero-bloom-input justification supplied')
    ywhite=inverse_tone(srgb_decode(white))
    multiplier=.18*(ywhite/.18)**(1/contrast)
    predicted=srgb_encode(tone(.18*((.18*multiplier)/.18)**contrast))
    gray_error=float(np.max(np.abs(predicted-gray)))
    if gray_error>3/255:failures.append('independent .18 chip mismatch exceeds3 encoded levels')
    def inverse(pixels):
        y=inverse_tone(srgb_decode(pixels))
        return (.18*(y/.18)**(1/contrast))/multiplier
    return {'qualifiedEstimate':not failures,'failures':failures,'blackRGB':black.tolist(),
        'whiteRGB':white.tolist(),'grayRGB':gray.tolist(),'predictedGrayRGB':predicted.tolist(),
        'maximumGrayEncodedError':gray_error,'fittedChannelMultiplier':multiplier.tolist()},inverse

def run(config):
    results=[]
    for pair in config['pairs']:
        reference=pathlib.Path(pair['opaque']);candidate=pathlib.Path(pair['analytic'])
        ma=json.loads(reference.with_suffix('.json').read_text());mb=json.loads(candidate.with_suffix('.json').read_text())
        keys=['camera','sceneLook','sourceDigest','shaderDigest','antialiasing','renderSize','wetness']
        mismatch=[k for k in keys if ma.get(k)!=mb.get(k)]
        if mismatch:raise ValueError('Invalid matched pair '+pair['name']+': '+','.join(mismatch))
        a=np.asarray(Image.open(reference).convert('RGB'),dtype=np.float64)/255
        b=np.asarray(Image.open(candidate).convert('RGB'),dtype=np.float64)/255
        if a.shape!=b.shape:raise ValueError('Image dimensions differ')
        regions=pair['regions']
        ta,ia=transfer(a,regions,ma['sceneLook'],pair.get('justifiedNoBloomInput',False))
        tb,ib=transfer(b,regions,mb['sceneLook'],pair.get('justifiedNoBloomInput',False))
        valid=ta['qualifiedEstimate'] and tb['qualifiedEstimate']
        measurements={}
        for name,box in regions.items():
            if name in ['black','white','gray']:continue
            pa=roi(a,box);pb=roi(b,box)
            r={'rectangle':box,'pixels':len(pa),'opaqueDisplayMean':pa.mean(axis=0).tolist(),
              'analyticDisplayMean':pb.mean(axis=0).tolist(),'displayRMSDifference':float(np.sqrt(np.mean((pa-pb)**2)))}
            if valid:
                la=ia(pa);lb=ib(pb)
                r['estimatedLinearOpaqueMean']=la.mean(axis=0).tolist()
                r['estimatedLinearCoverageMean']=lb.mean(axis=0).tolist()
                r['estimatedLinearCoverageP05P50P95']=np.quantile(lb.mean(axis=1),[.05,.5,.95]).tolist()
            measurements[name]=r
        results.append({'name':pair['name'],'opaque':str(reference),'analytic':str(candidate),
          'opaqueSHA256':sha(reference),'analyticSHA256':sha(candidate),'metadataSHA256':[sha(reference.with_suffix('.json')),sha(candidate.with_suffix('.json'))],
          'metadataMatchedKeys':keys,'referenceTransfer':ta,'candidateTransfer':tb,
          'measurements':measurements,'linearStatus':'calibrated display inversion ESTIMATE; not direct HDR/sample-mask readback' if valid else 'BLOCKED; display-only measurements retained'})
    return {'kind':'actual PNG measurement with reviewed ROIs','config':config,'results':results,
      'limits':['No direct GPU sample masks or HDR resolve values are measured.',
      'ROI selection must exclude edges for interior means and be preserved verbatim.',
      'Four-sample A2C mapping/phase correlation is implementation dependent.',
      'Linear estimates apply only to unlit kinds30/31 over black; never ordinary kind13.',
      'Black/.18/white consistency is necessary, not sufficient to rule out all spatial postprocess contamination.',
      'Boundary ROIs measure visible contrast; cannot by themselves identify normals versus coverage.']}

def selfcheck():
    # Synthetic numbers validate inversion arithmetic only; not a renderer check.
    x=np.linspace(0,1,257);contrast=.98;gain=1.17
    encoded=srgb_encode(tone(.18*((x*gain)/.18)**contrast))
    recovered=.18*(inverse_tone(srgb_decode(encoded))/.18)**(1/contrast)/gain
    error=float(np.max(np.abs(recovered-x)))
    assert error<1e-8
    return {'kind':'synthetic arithmetic check only','maximumInverseError':error}

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--config');p.add_argument('--out');p.add_argument('--self-check',action='store_true');args=p.parse_args()
    if args.self_check:print(json.dumps(selfcheck(),indent=2))
    else:
        if not args.config or not args.out:p.error('--config and --out required')
        result=run(json.loads(pathlib.Path(args.config).read_text()));result['scriptSHA256']=sha(__file__)
        pathlib.Path(args.out).write_text(json.dumps(result,indent=2)+'\n');print(args.out)
