#!/usr/bin/env python3
"""Fit Vesper's authored body proxies for garment and groom interaction.

These capsules deliberately remain inspectable approximations. In particular,
zero capsule penetration is not a certificate of zero rendered-skin overlap.
"""
import json
import sys
from pathlib import Path

sys.path.insert(0,str(Path(__file__).resolve().parents[3]/'Tools/AgentTools'))
from creature import CreatureWorkspace

def capsule(key,joint,a,b,radius):
    return dict(id=key,joint=joint,a=a,b=b,radius=radius,ignore=[])

def bodies():
    result=[
        capsule('ribcage','spine-1',[0,1.95,-.52],[0,1.90,.23],.70),
        capsule('loin','body',[0,1.69,.50],[0,1.64,1.02],.55),
        capsule('pelvis','haunch',[0,1.60,1.32],[0,1.60,1.64],.61),
        capsule('neck','neck',[0,2.13,-.84],[0,2.88,-1.53],.47),
        capsule('mask','mask',[-.24,3.17,-1.83],[.24,3.17,-1.83],.39),
    ]
    for front in (True,False):
        for side in (-1,1):
            name=('fore-' if front else 'hind-')+('left' if side<0 else 'right')
            a,b,c=([side*.64,2.25,-.85],[side*.82,1.18,.05],[side*.93,.18,-1.3]) if front else ([side*.58,1.5,1.55],[side*.82,.84,2.2],[side*.87,.18,1.45])
            def at(t): return [a[i]+(b[i]-a[i])*t for i in range(3)]
            result.append(capsule(name+'-muscle',name+'-upper',at(.15),at(.52),.32 if front else .35))
            result.append(capsule(name+'-elbow',name+'-upper',at(.52),b,.21))
            result.append(capsule(name+'-forearm',name+'-lower',b,c,.18))
    return result

if __name__=='__main__':
    s=CreatureWorkspace(); a=s.command('authoring')['authoring']
    if a['source']['id']!='vesper': raise ValueError('Select Vesper before fitting its collision proxies')
    s.command('author',expectedRevision=a['revision'],collisionBodies=bodies())
    print(json.dumps({'bodyProxies':len(bodies()),'published':False,
        'scope':'Authored kinematic capsules for one-way secondary contact; inspect dense surfaces and motion separately'}))
