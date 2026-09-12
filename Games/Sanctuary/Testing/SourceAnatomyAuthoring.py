#!/usr/bin/env python3
"""Isolated native source-only anatomy, correspondence and contact pipeline check.

Run with the harness-provided WRELA_CONTROL_ROOT against an owned Soundstage.
Always restores the complete prior study; never saves a published asset.
"""
import copy, hashlib, json, os, runpy, sys
from pathlib import Path
workspace = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(workspace / 'Tools/AgentTools'))
from creature import CreatureWorkspace, CraftTransaction
s = CreatureWorkspace(); c = s.command
root = runpy.run_path(str(workspace / 'scripts/gardenctl'))['ROOT']
original = c('saveStudy', path='source-anatomy-restore.json')['path']
checks = []; completed = False

def check(name, value):
    checks.append(dict(name=name, passed=bool(value)))
    if not value: raise AssertionError(name)
def pixels(label):
    return hashlib.sha256(Path(c('capture', label=label)['path']).read_bytes()).hexdigest()
def pose(y=0):
    return dict(offset=[0,y,0], rotation=[0,0,0], scale=[1,1,1])
def joint(key, parent=None, pivot=(0,0,0)):
    return dict(id=key, pivot=list(pivot), **({'parent':parent} if parent else {}))
element = CraftTransaction.anatomical_element
try:
    source = dict(version=1, id='source-anatomy-fixture', name='Source anatomy fixture', generator='fields',
        parts=[], craft=dict(joints=[joint('torso'), joint('upper','torso',[0,1,0]),
            joint('lower','upper',[0,.55,.15]), joint('foot','lower',[0,.12,0])],
        anatomy=[dict(id='body-source',part='torso',resolution=24,material=7,roughness=.7,metallic=0,
            elements=[element('shell','thorax',[0,1.18,0],[.42,.35,.3],weights={'torso':1}),
                      element('shin','shin',[0,.65,.12],[.11,.43,.13],weights={'upper':.5,'lower':.5}),
                      element('pad','contact',[0,.10,-.07],[.19,.08,.24],weights={'foot':1})])],
        contactChains=[dict(id='support',parent='torso',upper='upper',lower='lower',foot='foot',pole=[0,0,1],sole=.12)],
        phrases=[dict(id='compress',duration=2,interpolation='spline',samples=[dict(time=0,poses={'torso':pose()}),
            dict(time=1,poses={'torso':pose(-.15)}),dict(time=2,poses={'torso':pose()})],
            contacts=[dict(chain='support',keys=[dict(time=0,position=[0,.12,0],planted=True),dict(time=2,position=[0,.12,0],planted=True)])],references=[])],
        arrangement=dict(clip='study',duration=2,looping=False,layers=[dict(id='acting',phrase='compress',start=0,end=2,
            sourceStart=0,sourceEnd=2,fadeIn=0,fadeOut=0,weight=1,additive=False)])))
    source['surfaceLayers']=[dict(id='crest-pigment',part='torso',center=[0,1.53,0],radius=[.3,.2,.3],
        tint=[.5,.35,.2],amount=.6,frequency=20,roughness=.7,metallic=0,relief=.001,pattern=3,seed=7)]
    path=root/'source-anatomy-fixture.json';path.write_text(json.dumps(source))
    c('assetLoad',path=str(path));c('pause',value=True);c('view',value='quarter');c('rig',value='softbox')
    check('Source-only anatomy compiles into a runtime skinned batch',any(x['id']=='torso' and x['vertices']>100 and 'foot' in x['skinJoints'] for x in s.inspect()['parts']))
    check('Source-authored clips discovered without generator registration','study' in s.inspect()['clips'])
    c('creature',mode='study',seconds=1)
    report=s.report(samples=5)
    check('Authored contact chain runs shared solver',all(row['contacts'] and row['contacts'][0]['residual']<.003 for row in report['samples']))
    c('creature',mode='bind',seconds=0)
    with s.edit() as e:
        e.anatomy_anchor('crest-root','body-source',[0,1.53,0])
        e.guide_chain('crest','torso',[[0,1.53,0],[0,1.70,.04],[0,1.85,.08]])
        e.anatomy('crest-host','crest',[element('host','host',[0,1.53,0],[.002]*3)],resolution=24,material=8)
        e.groom('crest-groom','crest',[['crest-0','crest-1','crest-2']],fibres=8,width=.035,radius=.002)
        e.landmark('crest-marker','torso',[0,1.53,0])
        e.bind_anatomy('crest-binding','crest-root','guideChain','crest')
        e.bind_anatomy('marker-binding','crest-root','landmark','crest-marker')
        e.bind_anatomy('pigment-binding','crest-root','surfaceLayer','crest-pigment')
    guide_before=next(n for n in s.inspect()['guideNodes'] if n['id']=='crest-0')['position']
    before=copy.deepcopy(s.inspect()['source'])
    replacement=copy.deepcopy(before['anatomy'][0]);replacement['elements'][0]['radius'][1]=.38
    with s.edit() as e:e.upsert('anatomy',replacement)
    rebound=s.inspect()['source']['anatomyAnchors'][0]
    check('Compatible field edit preserves semantic anchor',rebound['id']=='crest-root' and abs(rebound['sourcePosition'][1]-1.56)<.002)
    inspection=s.inspect();guide_after=next(n for n in inspection['guideNodes'] if n['id']=='crest-0')['position']
    check('Persistent binding moves actual groom rest root',abs(guide_after[1]-guide_before[1]-.03)<.002)
    check('Persistent binding resolves visible landmark',abs(inspection['resolvedLandmarks'][0]['position'][1]-1.56)<.002)
    check('Persistent binding resolves rendered surface detail',abs(inspection['resolvedSurfaceLayers'][0]['center'][1]-1.56)<.002)
    check('Bound guide compiles actual groom vertices',any(p['id']=='crest' and p['vertices']>100 for p in inspection['parts']))
    stable=s.inspect();replacement['elements'][0]['region']='armored-thorax'
    try:
        with s.edit() as e:e.upsert('anatomy',replacement)
        rejected=False
    except RuntimeError:rejected=True
    check('Structural change rejects atomically with authored anchors',rejected and s.inspect()['revision']==stable['revision'])
    with s.edit() as e:
        e.upsert('anatomy',replacement)
        e.rebind_anatomy('body-source',allow_structural_changes=True)
    check('Explicit bounded recovery publishes new correspondence',s.inspect()['source']['anatomyAnchors'][0]['region']=='armored-thorax')
    c('creature',mode='study',seconds=.5)
    saved=c('saveStudy',path='source-anatomy-replay.json')['path'];first=pixels('source-anatomy-paused')
    c('step',frames=30);future=pixels('source-anatomy-future')
    c('loadStudy',path=saved);check('Source-only study restores exact paused pixels',pixels('source-anatomy-reloaded')==first)
    c('step',frames=30);check('Source-only study restores exact future pixels',pixels('source-anatomy-replayed')==future)
    completed=True
finally:
    c('loadStudy',path=original)
    result=dict(passed=completed and all(x['passed'] for x in checks),checks=checks)
    (root/'source-anatomy-validation.json').write_text(json.dumps(result,indent=2))
    print(json.dumps(result,indent=2))
