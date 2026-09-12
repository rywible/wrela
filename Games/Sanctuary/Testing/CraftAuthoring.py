#!/usr/bin/env python3
"""Native craft contracts: all five source families, rejection, history, runtime rendering and replay."""
import copy,hashlib,json,os,runpy,sys
from pathlib import Path
workspace=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(workspace/'Tools/AgentTools'))
from creature import CreatureWorkspace
s=CreatureWorkspace();c=s.command
api=runpy.run_path(str(workspace/'scripts/gardenctl'));root=api['ROOT']
original=c('saveStudy',path='craft-restore.json')['path'];checks=[];completed=False

def check(name,value):
 checks.append(dict(name=name,passed=bool(value)))
 if not value:raise AssertionError(name)
def state():return c('status')['state']['workshop']['document']
def pixels(label):return hashlib.sha256(Path(c('capture',label=label)['path']).read_bytes()).hexdigest()
def rejected(operations,revision=None):
 try:c('craft',expectedRevision=revision or s.inspect()['revision'],operations=operations)
 except RuntimeError:return True
 return False
try:
 c('subject',value='vesper');c('creature',mode='procession',seconds=0);c('pause',value=True);c('view',value='quarter');c('rig',value='softbox',z=-1.5)
 # Published content evolves; this contract fixture owns an empty craft layer.
 # The user's exact working document is restored by the outer finally block.
 c('author',expectedRevision=s.inspect()['revision'],craft={})
 baseline=state();image=pixels('craft-before');revision=s.inspect()['revision']
 with s.edit() as e:
  e.landmark('test-cheek','mask',[-.5,3,-2])
  e.sculpt_landmark('test-plane','test-cheek',brush='flatten',radius=.4,strength=.15,direction=[0,0,-1],mirror=True)
  e.refine('body',1)
  e.chain('test-spine','body',[[0,1.8,.2],[0,2,-.2]],reparent=['neck'])
  e.skin('test-skin','body',[0,1.8,0],[1,1,1],{'test-spine-0':.4,'body':.6},.5)
  e.corrective('test-bend','body','neck',0,-1,1,[0,1.8,0],[1,1,1],dilation=[.08,0,.04])
  e.mass('body',[0,1.8,0],100)
 changed=state();check('Atomic craft changes actual pixels',pixels('craft-after')!=image)
 check('Status source cache reflects the accepted transaction',changed['source']['craft']==s.inspect()['source'])
 check('Recipe geometry reused',c('authoring')['authoring']['baseGeometryReused'])
 c('undo');check('One undo restores complete craft transaction',state()==baseline)
 c('redo');check('Redo restores complete craft transaction',state()==changed)
 frozen=state();rev=s.inspect()['revision']
 check('Stale revision rejected',rejected([dict(op='set',collection='balance',value=None)],revision))
 check('Nested typo rejected',rejected([dict(op='upsert',collection='landmarks',value=dict(id='wrong',part='mask',position=[0,0,0],note='',typo=1))]))
 check('Unknown record removal rejected',rejected([dict(op='remove',collection='strokes',key='not-present')]))
 check('Whole transaction rolls back on later failure',rejected([dict(op='remove',collection='strokes',key='test-plane'),dict(op='set',collection='refinement',value={'body':3})]))
 check('All rejection paths preserve source and study',state()==frozen and s.inspect()['revision']==rev)
 with s.edit() as e:e.capture_phrase('test-reference',12,25)
 check('Captured phrase spans all rig joints',len(s.inspect()['source']['phrases'][-1]['samples'][0]['poses'])==len(s.inspect()['joints']))
 with s.edit() as e:
  e.fit_pose('test-reference',0,[dict(joint='body',point=[0,1.7,.5],target=[.12,1.7,.5],weight=1)],
             [dict(joint='body',channel='x',minimum=-.5,maximum=.5)])
 fitted=next(p for p in s.inspect()['source']['phrases'] if p['id']=='test-reference')
 check('Landmark fitting authors bounded joint pose',abs(fitted['samples'][0]['poses']['body']['offset'][0]-.12)<.002)
 stable=state()
 check('Unreachable fit rejects without applying approximation',rejected([dict(op='fitPose',key='test-reference',time=0,targets=[dict(joint='body',point=[0,1.7,.5],target=[10,1.7,.5],weight=1)],freedoms=[dict(joint='body',channel='x',minimum=-.5,maximum=.5)])]))
 check('Rejected fit preserves source',state()==stable)
 check('Huge fit iteration count rejects safely',rejected([dict(op='fitPose',key='test-reference',time=0,iterations=1e30,targets=[dict(joint='body',point=[0,1.7,.5],target=[.1,1.7,.5],weight=1)],freedoms=[dict(joint='body',channel='x',minimum=-.5,maximum=.5)])]))
 check('Huge support count rejects safely',rejected([dict(op='footsteps',key='test-reference',minimumSupport=1e30,steps=[])]))

 with s.edit() as e:
  e.operation('mirrorPhrase',key='test-reference',newKey='test-mirror',jointMap={},chainMap={})
  e.operation('retimePhrase',key='test-mirror',duration=6)
  e.set('arrangement',dict(clip='procession',duration=12,looping=True,layers=[dict(id='test-layer',phrase='test-reference',start=0,end=12,sourceStart=0,sourceEnd=12,fadeIn=0,fadeOut=0,weight=1,additive=False)]))
 check('Motion library retime persists',next(p for p in s.inspect()['source']['phrases'] if p['id']=='test-mirror')['duration']==6)
 with s.edit() as e:
  e.footsteps('test-reference',[dict(chain='fore-left',lift=2,land=3,target=[-1,.18,-1.5],height=.3)],minimum_support=3)
 check('Footstep planner rejects unintended support loss',rejected([dict(op='footsteps',key='test-reference',minimumSupport=3,steps=[dict(chain='fore-left',lift=2,land=3,target=[-1,.18,-1.5],height=.3),dict(chain='fore-right',lift=2.1,land=3.1,target=[1,.18,-1.5],height=.3)])]))
 stable=state();report=s.report(samples=13,part='body')
 check('Stress audit preserves current scene',state()==stable)
 check('Audit includes mass, trajectories and deformed surfaces',report['hasMassModel'] and bool(report['samples'][0]['deformation']) and report['maximumJointSpeed']>0)
 single=s.report(samples=2,part='body',start=2.5,end=2.5)
 check('Single-pose audit reads the current compiled surface',len(single['samples'])==1 and abs(single['samples'][0]['time']-2.5)<.0001 and
       single['samples'][0]['deformation'][0]['metrics']['vertices']==next(p['vertices'] for p in s.inspect()['parts'] if p['id']=='body'))
 interval=s.report(samples=3,start=2.1,end=2.9)
 check('Bounded audit includes only its selected interval',all(2.09<=row['time']<=2.91 for row in interval['samples']))
 for invalid in [dict(start=-1),dict(start=3,end=2),dict(end=100),dict(samples=1e30)]:
  failed=False
  try:c('craftReport',**invalid)
  except RuntimeError:failed=True
  check('Invalid audit rejects '+str(invalid),failed)
 check('Bounded and rejected audits preserve the complete scene',state()==stable)
 # Select an observed posed vertex from the exact renderer audit, then make a
 # small visible-space correction. The stored result remains a source field.
 point=single['samples'][0]['deformation'][0]['metrics']['worstPosition']
 with s.edit() as e:
  e.posed_corrective('test-visible-fit','body',point,[.006,.004,0],.5,
                    driver='body',axis=0,start=-360,end=-359,time=2.5)
 fit=e.result['posedFits'][0]
 check('Visible-space correction fits the final contact-constrained skin',fit['residual']<.0001 and fit['selectionDistance']<.0001)
 check('Visible-space correction stores an ordinary source field',any(x['id']=='test-visible-fit' for x in s.inspect()['source']['correctives']))
 c('undo');check('Undo restores the complete pre-fit scene',state()==stable)
 check('Missed visible-surface selection rejects atomically',rejected([dict(op='posedCorrective',key='miss',part='body',point=[90,90,90],offset=[.006,0,0],radius=.5,driver='body',axis=0,start=-360,end=-359,time=2.5)]))
 check('Rejected visible-surface fit preserves source and scene',state()==stable)
 guides=[[f'beard-guide-{g}-{i}' for i in range(4)] for g in range(4)]
 before=pixels('craft-groom-before')
 with s.edit() as e:e.groom('test-beard','beard',guides,fibres=24,width=.055,clump=.9,curl=.02)
 check('Shared groom compiler changes actual render',pixels('craft-groom-after')!=before)
 with s.edit() as e:
  e.guide_chain('test-guide','jaw',[[-.1,2.65,-2.2],[-.1,2.4,-2.25],[-.05,2.2,-2.2]])
  e.groom('test-beard','beard',[['test-guide-0','test-guide-1','test-guide-2']],fibres=16)
 check('New guides need no generator rebuild',any(x['id']=='test-guide-1' for x in s.inspect()['guideNodes']))
 # Replace a small existing cloth batch with a source-authored 2x2 panel, then
 # restore it after proving compile and binding. No species condition in tools.
 with s.edit() as e:
  e.cloth_panel('test-panel','trim',2,2,[[-.4,2.5,0],[.4,2.5,0],[-.4,2.2,.5],[.4,2.2,.5]],['body']*4,[0,1],resolution=8)
 panel=next(p for p in s.inspect()['parts'] if p['id']=='trim')
 check('Source-authored cloth owns its geometry and skin',panel['vertices']==81 and panel['skinJoints']==['test-panel-0','test-panel-1','test-panel-2','test-panel-3'])
 with s.edit() as e:e.remove('clothPanels','test-panel')

 # Project a short stitch line to two nearby points on the actual cloak surface.
 a=s.probe('mantle',[.9,2,0],.4)['point'];b=s.probe('mantle',[.9,2,.12],.4)['point']
 with s.edit() as e:e.seam('test-seam','mantle',[a,b],radius=.006,spacing=.025)
 check('Surface-attached seam compiles',any(x['id']=='test-seam' for x in s.inspect()['source']['seams']))
 c('creature',seconds=2.5);saved=c('saveStudy',path='craft-replay.json')['path'];same=state();same_pixels=pixels('craft-replay-start')
 c('step',frames=45);future=state();future_pixels=pixels('craft-replay-future')
 c('loadStudy',path=saved);check('Exact document load',state()==same);check('Exact paused pixels',pixels('craft-replay-same')==same_pixels)
 c('step',frames=45);check('Exact future document',state()==future);check('Exact future pixels',pixels('craft-replay-again')==future_pixels)
 before_duration=s.inspect()['source']['arrangement']['duration']
 c('motion',tempo=1.2)
 check('Global tempo retimes arrangement',abs(s.inspect()['source']['arrangement']['duration']-before_duration/1.2)<.001)
 check('Published timeline duration follows the arrangement',abs(c('status')['state']['workshop']['animationDuration']-before_duration/1.2)<.001)
 check('No GPU errors',not c('status')['state']['gpuErrors'])
 completed=True
finally:
 c('loadStudy',path=original)
 report=dict(passed=completed and all(x['passed'] for x in checks),checks=checks)
 (root/'craft-validation.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
