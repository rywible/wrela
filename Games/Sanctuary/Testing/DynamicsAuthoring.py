"""Vesper exercises the generic guide, dynamics, source and GPU replay contracts."""
import hashlib,json,os,runpy
from pathlib import Path
api=runpy.run_path(str(Path(__file__).resolve().parents[3]/'scripts/gardenctl'));c,root=api['command'],api['ROOT']
checks=[]
def check(name,value):
 checks.append(dict(name=name,passed=bool(value)))
 if not value:raise AssertionError(name)
def pixels():return hashlib.sha256(Path(c('capture',label='dynamics-check')['path']).read_bytes()).hexdigest()
def doc():return c('status')['state']['workshop']['document']
def author(**values):return c('author',expectedRevision=c('authoring')['authoring']['revision'],**values)
c('project',value='sanctuary');c('subject',value='vesper');c('rig',value='softbox',z=-1.5);c('view',value='quarter');c('pause',value=True)
c('creature',mode='procession',seconds=3)
try:
 a=c('authoring')['authoring'];check('Registered reusable guide graph',len(a['secondaryRig']['nodes'])==135)
 original=pixels();document=doc()
 c('secondary',enabled=False);check('Simulation visibly deforms the groom and cloth',pixels()!=original)
 c('undo');check('Dynamics undo restores exact image',pixels()==original)
 for values in [dict(gravity=-1),dict(enabled=1),dict(edgeContacts=1),dict(friction=-.1),dict(friction=2.1),dict(iterations=0),dict(typo=2)]:
  rejected=False
  try:c('secondary',**values)
  except RuntimeError:rejected=True
  check('Invalid settings rejected '+str(values),rejected)
 check('Rejected settings preserve document',doc()==document)
 bad_settings=dict(a['source']['secondary'],misspelledContact=True)
 rejected=False
 try:author(secondary=bad_settings)
 except RuntimeError:rejected=True
 check('Nested secondary source rejects unknown fields atomically',rejected and doc()==document)
 c('secondary',edgeContacts=not a['source']['secondary'].get('edgeContacts',False),friction=.7)
 c('undo');check('Contact controls undo restores exact image',pixels()==original)
 offsets=dict(a['source'].get('guideOffsets',{}));offsets['mane-guide-2-3']=[-.12,.12,0]
 result=author(guideOffsets=offsets)
 check('Guide sculpt reuses base recipe',result['authoring']['baseGeometryReused'])
 check('Guide sculpt changes only affected draw batch',result['authoring']['rebuiltBatches']==1)
 check('Guide sculpt changes rendered pixels',pixels()!=original)
 c('undo');check('Guide sculpt undo restores exact image',pixels()==original)
 rejected=False
 try:author(guideOffsets={'missing':[0,0,0]})
 except RuntimeError:rejected=True
 check('Unknown guide rejected',rejected)
 for radii in [{'missing':.1},{'mane-guide-0-1':-.1}]:
  rejected=False
  try:author(guideRadii=radii)
  except RuntimeError:rejected=True
  check('Invalid contact envelope rejected',rejected)
 radii=dict(a['source'].get('guideRadii',{}));radii['mane-guide-0-1']=.3
 result=author(guideRadii=radii)
 check('Contact envelopes reuse all geometry',result['authoring']['rebuiltBatches']==0)
 c('undo');check('Envelope undo restores exact image',pixels()==original)
 frames=dict(a['source'].get('guideFrames',{}));frames['fabric-4-2']='rotation' if frames.get('fabric-4-2')=='surface' else 'surface'
 result=author(guideFrames=frames)
 check('Surface frame edit reuses all geometry',result['authoring']['rebuiltBatches']==0)
 c('undo');check('Surface frame undo restores exact image',pixels()==original)
 rejected=False
 try:author(guideFrames={'fabric-4-2':'misspelled'})
 except RuntimeError:rejected=True
 check('Invalid guide frame rejected atomically',rejected and doc()==document)
 layer=dict(id='test-finish',part='mask',center=[0,3,-2],radius=[2,2,2],tint=[.1,.3,.7],amount=1,frequency=30,roughness=.9,metallic=0,relief=.001,pattern=1,seed=0)
 result=author(surfaceLayers=[layer])
 check('Surface finish reuses every geometry batch',result['authoring']['rebuiltBatches']==0)
 check('Surface finish changes rendered pixels',pixels()!=original)
 c('undo');check('Finish undo restores exact image',pixels()==original)
 for bad in [dict(layer,part='missing'),dict(layer,radius=[0,1,1]),dict(layer,pattern=4)]:
  rejected=False
  try:author(surfaceLayers=[bad])
  except RuntimeError:rejected=True
  check('Invalid finish rejected '+str(bad),rejected)
 check('Rejected finish preserves source',doc()==document)
 before=doc();r=c('secondaryReport',samples=61)['report']
 check('Dynamics report leaves working document unchanged',doc()==before)
 check('Structural strain remains below 3 percent',max(x['metrics']['maximumStretch'] for x in r['samples'])<.03)
 check('Guide particles clear collision proxies',max(x['metrics']['maximumPenetration'] for x in r['samples'])<.0005)
 c('secondary',edgeContacts=True,friction=.4)
 before=doc();image_before=pixels();dense=c('surfaceContacts',samples=121)['report']
 check('Dense audit checks actual vertices and leaves scene untouched',sum(x['contact']['vertices'] for x in dense['samples'])>10000 and doc()==before and pixels()==image_before)
 rejected=False
 try:c('surfaceContacts',part='missing')
 except RuntimeError:rejected=True
 check('Dense audit rejects unknown part',rejected)
 check('Dense secondary surfaces clear body proxies through the full dance',dense['maximumPenetration']<.0005)
 c('creature',seconds=8);forward=pixels()
 c('creature',seconds=1);c('creature',seconds=8)
 check('Reverse scrubbing replays exact dynamics pixels',pixels()==forward)
 saved=c('saveStudy',path='dynamics-replay.json')['path'];c('step',frames=60);future=pixels()
 c('creature',seconds=0);c('loadStudy',path=saved);c('step',frames=60)
 check('Saved study restores exact future dynamics pixels',pixels()==future)
 c('meshlets',value=False);vertex_before=pixels()
 author(surfaceLayers=[layer]);check('Vertex renderer applies local finish fields',pixels()!=vertex_before)
 c('undo');check('Vertex renderer finish undo restores exact pixels',pixels()==vertex_before)
 c('meshlets',value=True)
 check('No GPU errors',not c('status')['state']['gpuErrors'])
finally:
 (root/'dynamics-validation.json').write_text(json.dumps(dict(checks=checks),indent=2))
 print(json.dumps(checks,indent=2))
