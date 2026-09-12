#!/usr/bin/env python3
"""Native GPU validation of atomic local edits, final contacts and exact replay."""
import hashlib,json,os,runpy
from pathlib import Path
os.environ['SANCTUARY_CONTROL_ROOT']='.soundstage'
api=runpy.run_path(str(Path(__file__).resolve().parents[3]/'scripts/gardenctl'));c,root=api['command'],api['ROOT']
original=c('saveStudy',path='creature-tools-restore.json')['path'];checks=[];completed=False
def check(name,value):
 checks.append(dict(name=name,passed=bool(value)))
 if not value:raise AssertionError(name)
def state():return c('status')['state']['workshop']['document']
def author(**values):return c('author',expectedRevision=c('authoring')['authoring']['revision'],**values)
def reject(**values):
 try:c('author',**values)
 except RuntimeError:return True
 return False
def pixels(label):return hashlib.sha256(Path(c('capture',label=label)['path']).read_bytes()).hexdigest()
try:
 c('subject',value='vesper');c('creature',mode='procession',seconds=0);c('view',value='front');c('rig',value='softbox',z=-1.5);c('pause',value=True)
 c('rehearsal',slopeX=0,slopeZ=0,stepHeight=0,stepZ=0,view=False,solving=True)
 before=state();image=pixels('tools-before');a=c('authoring')['authoring'];source=a['source']
 edits=source.get('surfaceEdits',[])+[dict(id='test-cheek',part='mask',targets=['eyes'],center=[-.4,3.1,-2],radius=[.6,.6,.6],offset=[-.08,0,0],dilation=[.1,0,0])]
 result=author(surfaceEdits=edits)
 check('Base geometry reused for local edit',result['authoring']['baseGeometryReused'])
 check('Local edit changes rendered pixels',pixels('tools-local')!=image)
 stable=state();revision=c('authoring')['authoring']['revision']
 check('Stale revision rejected',reject(expectedRevision=a['revision'],surfaceEdits=[]))
 check('Unknown field rejected',reject(expectedRevision=revision,surfaceEdits=[],typo=1))
 bad=dict(edits[-1]);bad['radius']=[-1,1,1]
 check('Invalid deformation rejected',reject(expectedRevision=revision,surfaceEdits=[bad]))
 bad=dict(edits[-1]);bad['targets']=['missing']
 check('Invalid attachment rejected',reject(expectedRevision=revision,surfaceEdits=[bad]))
 bad=dict(edits[-1]);bad['center']=[20,20,20]
 check('Empty brush rejected',reject(expectedRevision=revision,surfaceEdits=[bad]))
 check('Rejection preserves whole document',state()==stable)
 c('undo');check('One undo restores transaction',state()==before)
 c('redo');check('Redo restores edit',state()==stable)
 probe=c('surfaceProbe',part='mask',x=0,y=3,z=-2,radius=.5)['probe']
 check('Surface probe identifies actual geometry',probe['verticesInRadius']>0 and probe['distance']<.5)
 score=json.loads(json.dumps(source['performance']))
 score['tracks']=[t for t in score['tracks'] if not (t['joint']=='body' and t['channel']=='x')]
 score['tracks'].append(dict(joint='body',channel='x',curve=dict(keys=[dict(time=0,value=.25),dict(time=12,value=.25)])))
 author(performance=score)
 c('rehearsal',stepHeight=.3,solving=True)
 saved=state();report=c('poseReport',samples=121)['report']
 check('Pose sampling does not change scene',state()==saved)
 check('Contacts survive additive body correction',report['maximumReachErrorMetres']<.002 and report['maximumPlantedClearanceMetres']<.002)
 check('Planted feet do not slide',report['maximumPlantedSpeedMetresPerSecond']<.005)
 c('rehearsal',solving=False);raw=c('poseReport',samples=31)['report']
 check('Unsolved contacts expose penetration',raw['minimumSoleClearanceMetres']<-.25)
 c('rehearsal',solving=True,slopeX=.12)
 replay=c('saveStudy',path='creature-tools-replay.json')['path']
 c('step',frames=90);future=state();future_image=pixels('tools-future')
 c('loadStudy',path=replay);c('step',frames=90)
 check('Exact future document replay',state()==future)
 check('Exact future rendered replay',pixels('tools-future-replay')==future_image)
 bodies=c('authoring')['authoring']['collisionBodies'];bodies[-1]['radius']=2
 author(collisionBodies=bodies)
 check('Collision sweep flags overlapping proxies',len(c('poseReport',samples=9)['report']['collisions'])>0)
 before_score=state()['source']['performance']
 c('motion',tempo=1.2)
 after_score=state()['source']['performance']
 ratio=before_score['duration']/after_score['duration']
 old_tangents=[k['outTangent'] for t in before_score['tracks'] for k in t['curve']['keys'] if 'outTangent' in k]
 new_tangents=[k['outTangent'] for t in after_score['tracks'] for k in t['curve']['keys'] if 'outTangent' in k]
 check('Tempo rescales authored velocities',bool(old_tangents) and all(abs(b-a*ratio)<.0001 for a,b in zip(old_tangents,new_tangents)))
 check('No GPU errors',not c('status')['state']['gpuErrors'])
 completed=True
finally:
 c('loadStudy',path=original)
 report=dict(passed=completed and bool(checks) and all(x['passed'] for x in checks),checks=checks)
 (root/'creature-tools-validation.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
