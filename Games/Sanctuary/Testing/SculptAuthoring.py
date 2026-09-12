#!/usr/bin/env python3
"""Native agent sculpting: actual pixels, fitting, rejection and matched replay.

Owns a source-only fixture in the isolated harness. No published assets change.
"""
import copy, hashlib, json, runpy, sys, time
from pathlib import Path
workspace=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(workspace/'Tools/AgentTools'))
from creature import CreatureWorkspace, CraftTransaction
s=CreatureWorkspace();c=s.command
root=runpy.run_path(str(workspace/'scripts/gardenctl'))['ROOT']
original=c('saveStudy',path='sculpt-restore.json')['path'];checks=[];completed=False
def check(name,value):
    checks.append(dict(name=name,passed=bool(value)))
    if not value:raise AssertionError(name)
def document():return c('status')['state']['workshop']['document']
def digest(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def reject(action,**kwargs):
    try:c(action,**kwargs)
    except RuntimeError:return True
    return False
try:
    fixture=dict(version=1,id='sculpt-fixture',name='Sculpt workflow fixture',generator='fields',parts=[],
        craft=dict(joints=[dict(id='shell'),dict(id='hidden')],anatomy=[dict(id='shell-source',part='shell',resolution=32,material=7,roughness=.7,metallic=0,
            elements=[CraftTransaction.anatomical_element('shell','form',[0,.5,0],[.45,.5,.4],weights={'shell':1})]),
            dict(id='hidden-source',part='hidden',resolution=24,material=7,roughness=.7,metallic=0,
            elements=[CraftTransaction.anatomical_element('hidden-mass','hidden',[0,5,0],[2,3,2],weights={'hidden':1})])]))
    path=root/'sculpt-fixture.json';path.write_text(json.dumps(fixture))
    c('assetLoad',path=str(path));c('pause',value=True);c('view',value='front');c('rig',value='softbox');c('surfaceReview',mode='clay',hiddenParts=['hidden'])
    framed=s.frame_visible()
    check('Visible framing excludes the large hidden surface',framed['parts']==['shell'] and .98<framed['heightMetres']<1.02)
    c('part',value='shell');part_baseline=document()
    c('partEdit',x=.025,pivotY=.3,roughness=.44)
    authored=s.inspect()['source']
    check('Native part edit targets the authoritative craft joint',next(x for x in authored['joints'] if x['id']=='shell')['offset'][0]>.024 and abs(next(x for x in authored['joints'] if x['id']=='shell')['pivot'][1]-.3)<.0001)
    check('Native material edit targets authoritative anatomy',abs(next(x for x in authored['anatomy'] if x['part']=='shell')['roughness']-.44)<.0001)
    check('Authoritative edits do not create shadowed recipe overrides',not document()['source'].get('partOverrides'))
    c('undo');check('Native part edit undo restores the full study',document()==part_baseline)
    baseline=document();observation=s.observe('sculpt-baseline')
    selection=s.select(observation,[[960,540]],part='shell',explain=True);hit=selection['hits'][0]
    check('Image pixel resolves source element and triangle',hit['source']=='shell-source' and hit['element']=='shell' and abs(sum(hit['barycentric'])-1)<1e-5)
    explanation=hit['explanation'];responses={x['parameter']:x['normalResponse'] for x in explanation['contributors'][0]['controls']}
    check('Actual pixel explains authoritative field controls without changing source',explanation['contributors'][0]['element']=='shell' and abs(explanation['fieldValue'])<.003 and abs(responses['radius.z'])>.8 and document()==baseline)
    check('Nonboolean explanation flag rejects',reject('sculptProbe',frame=selection['frame'],expectedRevision=selection['revision'],pixels=[[960,540]],explain=1))
    check('Boolean pixel rejects',reject('sculptProbe',frame=selection['frame'],expectedRevision=selection['revision'],pixels=[[True,540]]))
    check('Out of frame pixel rejects',reject('sculptProbe',frame=selection['frame'],expectedRevision=selection['revision'],pixels=[[-1,540]]))
    check('Occluded or wrong part rejects',reject('sculptProbe',frame=selection['frame'],expectedRevision=selection['revision'],pixels=[[960,540]],part='absent'))
    p=hit['point'];q=[p[i]+hit['normal'][i]*.018 for i in range(3)]
    controls=[dict(center=p,radius=.35)];targets=[dict(id='move',point=p,target=q,tolerance=.0002)]
    field=copy.deepcopy(next(x for x in s.inspect()['source']['anatomy'] if x['part']=='shell'))
    field['elements'].append(CraftTransaction.anatomical_element('socket','socket',[.2,.55,.35],[.22]*3,operation='cut',blend=0,cut_blend=.035))
    invalid=copy.deepcopy(field);invalid['elements'][-1]['cutBlend']=True
    alternatives=s.explore_edits([dict(key='rounded-source-cut',operations=[dict(op='upsert',collection='anatomy',value=field)]),
        dict(key='invalid-source-cut',operations=[dict(op='upsert',collection='anatomy',value=invalid)])],root/'sculpt-source-alternatives')
    outcomes=json.loads(Path(alternatives['report']).read_text())
    check('Source alternatives render accepted field and archive strict rejection',outcomes['results'][0]['accepted'] and not outcomes['results'][1]['accepted'] and digest(outcomes['records'][0]['path'])!=digest(outcomes['records'][1]['path']))
    check('Source exploration restores complete document and exact pixels',alternatives['restored'] and document()==baseline and digest(s.observe('sculpt-source-restored')['path'])==digest(observation['path']))
    counts={x['id']:x['vertices'] for x in s.inspect()['parts']}
    with s.edit() as e:e.refine_local('local-detail','shell',p,[.08]*3,.006,6000)
    refined=s.inspect();new_counts={x['id']:x['vertices'] for x in refined['parts']}
    check('Local detail adds geometry only to the selected part',new_counts['shell']>counts['shell'] and new_counts['hidden']==counts['hidden'])
    fine=s.select(s.observe('sculpt-local-detail'),[[960,540]],part='shell')['hits'][0]
    check('Local detail meets the requested edge scale on the actual selected triangle',fine['triangleEdgeMetres']<=.0061)
    bad_detail=dict(op='upsert',collection='detailPatches',value=dict(id='local-detail',part='shell',center=p,radius=[.08]*3,edgeLength=.001,maximumNewVertices=1))
    check('Detail budget rejection preserves source',reject('craft',expectedRevision=refined['revision'],operations=[bad_detail]) and s.inspect()['revision']==refined['revision'])
    c('undo');check('Undo restores local detail and complete study',document()==baseline)
    check('Undo refinement restores exact paused pixels',digest(s.observe('sculpt-local-detail-undone')['path'])==digest(observation['path']))
    s.preview_sculpt('shell',controls=controls,protect=[dict(id='protected',center=p,radius=[.12]*3,innerFraction=.4)])
    check('Native influence overlay changes display without editing source',s.inspect()['revision']==selection['revision'] and digest(s.observe('sculpt-mask')['path'])!=digest(observation['path']))
    s.preview_sculpt()
    check('Clearing overlay restores exact clay pixels and document',document()==baseline and digest(s.observe('sculpt-mask-cleared')['path'])==digest(observation['path']))
    start=time.monotonic()
    with s.edit() as e:e.fit_sculpt('intent','shell',controls,targets,maximum_displacement=.025)
    fit=e.result['sculptFits'][0];fit['seconds']=time.monotonic()-start
    check('Fit reports residuals and full-surface displacement',fit['residuals'][0]['error']<=.0002 and .017<fit['maximumDisplacement']<=.025)
    changed=document();after=s.observe('sculpt-after')
    check('Fitted source changes actual rendered pixels',digest(after['path'])!=digest(observation['path']))
    check('Stale capture rejects after source edit',reject('sculptProbe',frame=selection['frame'],expectedRevision=selection['revision'],pixels=[[960,540]]))
    c('undo');check('Undo restores complete source and study',document()==baseline)
    check('Undo restores exact paused pixels',digest(s.observe('sculpt-undone')['path'])==digest(observation['path']))
    c('redo');check('Redo restores complete source and study',document()==changed)
    fresh=s.select(s.observe('sculpt-fresh'),[[960,540]],part='shell');p=fresh['hits'][0]['point']
    stable=s.inspect()['revision']
    bad=dict(op='fitSculpt',key='conflict',part='shell',controls=[dict(center=p,radius=.35)],targets=[dict(id='hold',point=p,target=p,tolerance=.0001),dict(id='move',point=p,target=[p[0],p[1]+.02,p[2]],tolerance=.0001)])
    check('Contradictory intent rejects',reject('craft',expectedRevision=stable,operations=[bad]))
    typo=copy.deepcopy(bad);typo['controls'][0]['raduis']=.4
    check('Nested fit typo rejects',reject('craft',expectedRevision=stable,operations=[typo]))
    check('Rejections preserve revision and document',s.inspect()['revision']==stable and document()==changed)
    exploratory=[dict(key='small-candidate',part='shell',controls=[dict(center=p,radius=.35)],targets=[dict(id='move',point=p,target=[p[0],p[1]+.008,p[2]],tolerance=.0002)]),
                 dict(key='distorted-candidate',part='shell',controls=[dict(center=p,radius=.35)],targets=[dict(id='move',point=p,target=[p[0],p[1]+.04,p[2]],tolerance=.0002)],maximum_normal_change_degrees=.1)]
    exploration=s.explore_sculpt(exploratory,root/'sculpt-exploration')
    outcomes=json.loads(Path(exploration['report']).read_text())['results']
    check('Bounded exploration archives acceptance and distortion rejection',outcomes[0]['accepted'] and not outcomes[1]['accepted'] and 'normalChange' in outcomes[1]['error'])
    check('Exploration retains no candidate and restores complete study',exploration['restored'] and document()==changed)
    comparison=s.compare_sculpt_layer('intent',root/'sculpt-alternatives')
    report=json.loads(Path(comparison['report']).read_text())
    check('Bounded alternatives restore complete study',comparison['restored'] and document()==changed)
    records=report['records'];check('Three rendered intensities differ',len({digest(r['path']) for r in records})==3)
    c('loadStudy',path=str(Path(comparison['report']).parent/records[1]['study']))
    check('Archived alternative replays exact pixels',digest(s.observe('sculpt-replay')['path'])==digest(records[1]['path']))
    before=document();invalid=copy.deepcopy(before);invalid['sculptReviewFrame']['maximum']=invalid['sculptReviewFrame']['minimum']
    path=root/'sculpt-invalid-frame.json';path.write_text(json.dumps(invalid))
    check('Invalid comparison frame rejects atomically',reject('loadStudy',path=str(path)) and document()==before)
    completed=True
finally:
    c('loadStudy',path=original)
    report=dict(passed=completed and all(x['passed'] for x in checks),checks=checks)
    (root/'sculpt-validation.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
