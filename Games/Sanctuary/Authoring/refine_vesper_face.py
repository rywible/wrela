#!/usr/bin/env python3
"""Quiet ivory finish and broad carved planes, using public source transactions."""
import copy
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[3]/'Tools/AgentTools'))
from creature import CreatureWorkspace

def author(studio):
    a=studio.command('authoring')['authoring']
    if a['source']['id']!='vesper':raise ValueError('Select Vesper')
    layers=copy.deepcopy(a['source']['surfaceLayers'])
    for layer in layers:
        if layer['id']=='old-ivory':layer.update(amount=.18,frequency=6,relief=.00012)
        if layer['id']=='ivory-grain':layer.update(amount=.12,frequency=180,pattern=1,relief=.00008,tint=[.60,.54,.42])
        if layer['id']=='chin-weathering':layer.update(amount=.26,frequency=8,relief=.00012)
        if layer['id'].startswith('muzzle-'):layer.update(amount=.24,relief=.00008)
    studio.command('author',expectedRevision=a['revision'],surfaceLayers=layers)
    with studio.edit() as e:
        e.sculpt('vesper-frontal-plane','mask',[[0,3.38,-2.015]],brush='flatten',radius=.34,strength=.25,direction=[0,.15,-1])
        e.sculpt('vesper-orbital-plane','mask',[[.37,3.345,-2.055]],brush='flatten',radius=.19,strength=.24,direction=[.2,.32,-1],mirror=True)
        e.sculpt('vesper-malar-plane','mask',[[.51,3.05,-1.94]],brush='flatten',radius=.24,strength=.30,direction=[.8,.05,-.6],mirror=True)
        e.landmark('vesper-brow','mask',[.37,3.345,-2.055],'Carved orbital shelf')
        e.landmark('vesper-malar','mask',[.51,3.05,-1.94],'Cheek plane')

if __name__=='__main__':
    author(CreatureWorkspace())
    print('Authored Vesper face planes and finish; not published')
