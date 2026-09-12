#!/usr/bin/env python3
"""Validate native performance authoring: real GPU images, atomic edits and replay."""
import hashlib,json,os,runpy
from pathlib import Path
os.environ['SANCTUARY_CONTROL_ROOT']='.soundstage'
client=runpy.run_path(str(Path(__file__).resolve().parents[3]/'scripts/gardenctl'))
c,root=client['command'],client['ROOT'];checks=[]
saved=c('saveStudy',path='performance-validation-restore.json')['path']
def doc():return c('status')['state']['workshop']['document']
def image(label):return hashlib.sha256(Path(c('capture',label=label)['path']).read_bytes()).hexdigest()
def check(name,result):
    checks.append(dict(name=name,passed=bool(result)))
    if not result:raise AssertionError(name)
def reject(**kwargs):
    try:c('performanceKey',**kwargs)
    except RuntimeError:return True
    return False
try:
    c('subject',value='vesper');c('rig',value='softbox');c('view',value='quarter');c('pause',value=True)
    c('creature',mode='procession',seconds=4)
    before=doc();pixels=image('performance-original')
    check('Deforming surfaces registered',len(c('performance')['performance']['skinnedParts'])>=3)
    c('performanceKey',joint='mask',channel='yaw',seconds=0,value=0)
    c('performanceKey',joint='mask',channel='yaw',seconds=4,value=25)
    c('performanceKey',joint='mask',channel='yaw',seconds=12,value=0)
    check('Key visibly changes renderer',image('performance-key')!=pixels)
    score=doc()['source']['performance'];stable=doc()
    check('Invalid joint rejected',reject(joint='missing',channel='yaw',seconds=4,value=1))
    check('Invalid channel rejected',reject(joint='mask',channel='banana',seconds=4,value=1))
    check('Invalid time rejected',reject(joint='mask',channel='yaw',seconds=-1,value=1))
    check('Invalid value rejected',reject(joint='mask',channel='yaw',seconds=4,value=1000))
    check('Malformed remove rejected',reject(joint='mask',channel='yaw',seconds=4,remove='yes'))
    check('Rejection is atomic',doc()==stable)
    replay=c('saveStudy',path='performance-validation-replay.json')['path']
    c('step',frames=60);future=doc();future_pixels=image('performance-future')
    c('loadStudy',path=replay);c('step',frames=60)
    check('Exact future document replay',doc()==future)
    check('Exact future GPU replay',image('performance-future-replay')==future_pixels)
    c('loadStudy',path=replay)
    c('performanceKey',joint='mask',channel='yaw',seconds=4,value=0)
    c('undo');check('Undo restores source score',doc()['source']['performance']==score)
    c('redo');check('Redo reapplies key',doc()['source']['performance']!=score)
    c('performanceBeat',name='Sweep');check('Named beat seeks exact time',abs(c('performance')['performance']['time']-6.5)<0.001)
    c('motion',tempo=1.2);check('Tempo rescales score',abs(doc()['source']['performance']['duration']-10)<0.001)
    c('meshlets',value=False);c('capture',label='performance-indexed')
    c('meshlets',value=True)
    check('No GPU errors',not c('status')['state']['gpuErrors'])
finally:
    c('loadStudy',path=saved)
    report=dict(passed=bool(checks) and all(x['passed'] for x in checks),checks=checks)
    (root/'performance-validation.json').write_text(json.dumps(report,indent=2))
    print(json.dumps(report,indent=2))
