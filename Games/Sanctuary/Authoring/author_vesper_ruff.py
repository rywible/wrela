#!/usr/bin/env python3
"""A rooted, layered collar rather than identical rearward wedges.

Uses only the public authoring transaction and existing guide identities.
Run VesperFinery.py --contacts-only afterward, then inspect actual motion.
"""
import copy
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'Tools/AgentTools'))
from creature import CreatureWorkspace


def author(studio):
    snapshot=studio.command('authoring')['authoring']
    source=snapshot['source']
    if source['id']!='vesper':
        raise ValueError('Select Vesper before authoring its ruff')
    craft=copy.deepcopy(source['craft'])
    offsets=copy.deepcopy(source.get('guideOffsets',{}))
    frames=copy.deepcopy(source.get('guideFrames',{}))
    nodes={n['id']:n['position'] for n in snapshot['secondaryRig']['nodes']}
    rhythm=[1.12,.91,1.04,.88,1.10,.83,.96,.89,1.08,.93,1.14,.88,.98,1.10,.85,1.18]
    for groom in craft['grooms']:
        if groom['part'] not in ('mane','beard'):
            continue
        mane=groom['part']=='mane'
        groom.update(width=.235 if mane else .107, fibres=40 if mane else 28,
                     radius=.0014, curl=.009, frequency=1.4, clump=.72,
                     lengthVariation=.28, flyaways=.09,
                     rootColor=[.30,.29,.26], tipColor=[.58,.57,.53],
                     envelope=dict(coverage=1.12,taper=1.12,flatten=.72,ridge=.035,layers=3,clumps=6))
        count=len(groom['guides'])
        for i,guide in enumerate(groom['guides']):
            u=i/max(1,count-1)
            if mane:
                angle=-.48+u*(math.pi+.96)
                c,s=math.cos(angle),math.sin(angle)
                crest=max(0,s)
                length=rhythm[min(len(rhythm)-1,round(u*(len(rhythm)-1)))]
                root=[c*.565,3.10+s*.43,-1.56+.035*math.sin(i*2.1)]
                # Side locks fall past the neck; crown locks travel over it.
                controls=[root,
                    [root[0]+c*.075,root[1]-.15+crest*.21,root[2]+.20],
                    [root[0]+c*.18,root[1]-(.48-.13*crest)*length,root[2]+(.44+.28*crest)*length],
                    [root[0]+c*(.20+.07*math.sin(i*1.7)),root[1]-(1.04-.36*crest)*length,root[2]+(.76+.42*crest)*length]]
            else:
                x=(u-.5)*.43
                length=1.03-.24*abs(u-.5)*2+.08*math.sin(i*2.3)
                root=[x,2.65,-2.26+.025*math.sin(i)]
                controls=[root,[x*1.08,2.43,-2.32],
                          [x*1.25,2.18,-2.22],[x*1.38,2.65-.87*length,-2.02]]
            for j,node in enumerate(guide):
                frames[node]='curve'
                parameter=j/max(1,len(guide)-1)*3
                k=min(2,int(parameter));v=parameter-k
                target=[controls[k][axis]*(1-v)+controls[k+1][axis]*v for axis in range(3)]
                base=[nodes[node][axis]-offsets.get(node,[0,0,0])[axis] for axis in range(3)]
                offsets[node]=[round(target[axis]-base[axis],6) for axis in range(3)]
    edits=[e for e in source.get('surfaceEdits',[]) if e['id'] not in ('mane-left-crest','mane-right-sweep')]
    studio.command('author',expectedRevision=snapshot['revision'],craft=craft,
                   guideOffsets=offsets,guideFrames=frames,surfaceEdits=edits)


if __name__=='__main__':
    author(CreatureWorkspace())
    print(json.dumps({'authored':'Layered falling ruff','published':False}))
