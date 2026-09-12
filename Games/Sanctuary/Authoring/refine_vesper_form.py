#!/usr/bin/env python3
"""Shape Vesper's large/medium anatomy through public, reversible craft edits.

Run after author_vesper_anatomy.py. Coordinates are bind metres; IDs name
artistic decisions rather than compiled vertices. This never publishes a source.
"""
import copy
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'Tools/AgentTools'))
from creature import CreatureWorkspace


def author(edit):
    body = copy.deepcopy(next(s for s in edit.snapshot['source']['anatomy'] if s['part'] == 'body'))
    elements = {e['id']: e for e in body['elements']}
    # Connect the long trunk's big masses before carving superficial landmarks.
    elements['loin'].update(radius=[.54,.54,.87], blend=.26)
    elements['pelvis'].update(radius=[.64,.62,.64], blend=.22)
    elements['sternum'].update(radius=[.37,.45,.35], blend=.16)
    for side in ('left', 'right'):
        elements['scapula-'+side]['radius'] = [.255,.49,.46]
        elements['pectoral-'+side]['radius'] = [.275,.345,.235]
        elements['hip-'+side]['radius'] = [.30,.43,.42]
        # Full upper masses taper into a readable elbow instead of a round tube.
        for prefix in ('fore-', 'hind-'):
            key = prefix+side+'-upper-'
            elements[key+'extensor']['radius'][0] *= .94
            elements[key+'flexor']['radius'][0] *= .90
    edit.upsert('anatomy', body)
    edit.sculpt('vesper-scapular-plane', 'body', [[.745,2.26,-.64],[.765,2.05,-.31]],
                brush='flatten', radius=.37, strength=.70, direction=[1,.10,-.20], mirror=True)
    edit.sculpt('vesper-deltoid-separation', 'body', [[.74,2.03,-.54],[.87,1.79,-.33],[.91,1.59,-.13]],
                brush='crease', radius=.125, strength=.13, direction=[1,0,-.10], mirror=True)
    edit.sculpt('vesper-sternal-cleft', 'body', [[0,2.04,-1.125],[0,1.84,-1.15],[0,1.65,-1.08]],
                brush='crease', radius=.16, strength=.14, direction=[0,0,-1])
    edit.sculpt('vesper-neck-tendon-valley', 'body', [[.32,2.35,-1.36],[.32,2.56,-1.55],[.30,2.77,-1.69]],
                brush='crease', radius=.115, strength=.10, direction=[.55,0,-1], mirror=True)
    edit.sculpt('vesper-iliac-plane', 'body', [[.74,1.72,1.43],[.76,1.48,1.70]],
                brush='flatten', radius=.33, strength=.60, direction=[1,.2,.2], mirror=True)
    edit.sculpt('vesper-abdominal-tuck', 'body', [[.48,1.26,.47],[.46,1.20,.82]],
                brush='grab', radius=.34, strength=.055, direction=[-.4,1,0], mirror=True)
    # Landmarks make the authored regions directly discoverable for later edits.
    for key,point in [('scapular-plane',[.745,2.26,-.64]),('sternum',[0,1.84,-1.15]),
                      ('neck-tendon',[.32,2.56,-1.55]),('iliac-plane',[.74,1.72,1.43])]:
        edit.landmark('vesper-'+key,'body',point,'Large/medium form review landmark')


if __name__ == '__main__':
    studio = CreatureWorkspace()
    with studio.edit() as edit:
        if not any(s['id'] == 'vesper-body' for s in edit.snapshot['source']['anatomy']):
            raise ValueError('Select Vesper with the connected anatomy source')
        author(edit)
    print(json.dumps({'authored':'Vesper planar anatomy and trunk transitions','published':False}))
