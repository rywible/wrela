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
    lens_baseline=document();wide=s.observe('sculpt-wide-lens')
    s.lens(20);long_document=document();long=s.observe('sculpt-long-lens')
    check('Long lens is saved and moves back while preserving source',long_document['verticalFOVDegrees']==20 and long_document['distance']>lens_baseline['distance']*3 and long_document['source']==lens_baseline['source'])
    check('Actual render and selection token respond to lens change',digest(wide['path'])!=digest(long['path']) and wide['frame']['token']!=long['frame']['token'] and s.select(long,[[960,540]],part='shell')['hits'][0]['part']=='shell')
    check('Invalid and boolean lens reject atomically',reject('camera',verticalFOVDegrees=1) and reject('camera',verticalFOVDegrees=True) and document()==long_document)
    lens_study=c('saveStudy',path='sculpt-long-lens.json')['path']
    c('undo');check('Lens undo restores full document and exact image',document()==lens_baseline and digest(s.observe('sculpt-lens-undone')['path'])==digest(wide['path']))
    baseline_lens_study=c('saveStudy',path='sculpt-original-lens.json')['path']
    c('loadStudy',path=lens_study)
    check('Long lens replay restores full document and exact image',document()==long_document and digest(s.observe('sculpt-lens-replay')['path'])==digest(long['path']))
    c('loadStudy',path=baseline_lens_study)
    baseline=document();observation=s.observe('sculpt-baseline')
    selection=s.select(observation,[[960,540]],part='shell',explain=True);hit=selection['hits'][0]
    check('Image pixel resolves source element and triangle',hit['source']=='shell-source' and hit['element']=='shell' and abs(sum(hit['barycentric'])-1)<1e-5)
    explanation=hit['explanation'];responses={x['parameter']:x['normalResponse'] for x in explanation['contributors'][0]['controls']}
    extraction=next(x for x in s.inspect()['anatomy'] if x['part']=='shell')['extraction']
    check('Source inspection distinguishes orientation from edge incidence',extraction['available'] and extraction['dualInvertedTriangles']==0 and extraction['dualOrientedEdgeIncidence'] and extraction['triangles']>0)
    check('Actual pixel explains authoritative field controls without changing source',explanation['contributors'][0]['element']=='shell' and abs(explanation['fieldValue'])<.003 and abs(responses['radius.z'])>.8 and document()==baseline)
    check('Nonboolean explanation flag rejects',reject('sculptProbe',frame=selection['frame'],expectedRevision=selection['revision'],pixels=[[960,540]],explain=1))
    check('Boolean pixel rejects',reject('sculptProbe',frame=selection['frame'],expectedRevision=selection['revision'],pixels=[[True,540]]))
    check('Out of frame pixel rejects',reject('sculptProbe',frame=selection['frame'],expectedRevision=selection['revision'],pixels=[[-1,540]]))
    check('Occluded or wrong part rejects',reject('sculptProbe',frame=selection['frame'],expectedRevision=selection['revision'],pixels=[[960,540]],part='absent'))
    source_controls=[dict(id='depth',terms=[dict(element='shell',parameter='radius.z',scale=1)],minimum=-.05,maximum=.05)]
    source_targets=[dict(id='front',position=[0,.5,.425],tolerance=.0001),dict(id='top',position=[0,1,0],tolerance=.0001),dict(id='side',position=[.45,.5,0],tolerance=.0001)]
    with s.edit() as e:e.fit_anatomy('shell-source',source_controls,source_targets)
    source_fit=e.result['anatomyFits'][0]
    check('Source fitting reports bounded parameter changes and held surface targets',abs(source_fit['changes'][0]['delta']-.025)<.0001 and all(x['linearizedDistance']<=x['tolerance'] for x in source_fit['residuals']))
    fitted_observation=s.observe('sculpt-fitted-anatomy');fitted_hit=s.select(fitted_observation,[[960,540]],part='shell')['hits'][0]
    check('Fitted source compiles and changes actual rendered surface',fitted_hit['point'][2]>hit['point'][2]+.02 and digest(fitted_observation['path'])!=digest(observation['path']))
    c('undo');check('Source fit undo restores exact source and pixels',document()==baseline and digest(s.observe('sculpt-source-fit-undone')['path'])==digest(observation['path']))
    tight=copy.deepcopy(source_controls);tight[0]['maximum']=.005
    preceding=dict(op='upsert',collection='landmarks',value=dict(id='atomic-before-conflict',part='shell',position=[0,.5,.4],note='must not survive rejection'))
    check('Conflict rejects every operation in the transaction',reject('craft',expectedRevision=s.inspect()['revision'],operations=[preceding,dict(op='fitAnatomy',source='shell-source',controls=tight,targets=source_targets)]) and document()==baseline)
    malformed=copy.deepcopy(source_controls);malformed[0]['terms'][0]['scale']=True
    check('Boolean source control rejects without mutation',reject('craft',expectedRevision=s.inspect()['revision'],operations=[dict(op='fitAnatomy',source='shell-source',controls=malformed,targets=source_targets)]) and document()==baseline)
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
    loft=copy.deepcopy(next(x for x in s.inspect()['source']['anatomy'] if x['part']=='shell'))
    sections=[dict(id='rear',z=-.4,center=[0,0],radius=[.2,.25]),dict(id='middle',z=0,center=[0,0],radius=[.45,.5]),dict(id='front',z=.4,center=[0,0],radius=[.2,.25])]
    loft['elements']=[CraftTransaction.anatomical_element('envelope','form',[0,.5,0],[1,1,1],primitive='loft',sections=sections,weights={'shell':1})]
    with s.edit() as e:e.upsert('anatomy',loft)
    loft_document=document();loft_capture=s.observe('sculpt-loft')
    check('Section envelope changes actual geometry and pixels',digest(loft_capture['path'])!=digest(observation['path']))
    section_controls=[dict(id='middle-width',terms=[dict(element='envelope',parameter='section.middle.radius.x',scale=1)],minimum=-.08,maximum=.08)]
    section_targets=[dict(id='wide-side',position=[.49,.5,0],tolerance=.0001),dict(id='held-top',position=[0,1,0],tolerance=.0001)]
    with s.edit() as e:e.fit_anatomy('shell-source',section_controls,section_targets)
    check('Named section fits through the ordinary source solver',abs(e.result['anatomyFits'][0]['changes'][0]['delta']-.04)<.0002)
    c('undo');check('Section fitting undo restores exact pixels',document()==loft_document and digest(s.observe('sculpt-loft-undone')['path'])==digest(loft_capture['path']))
    broken=copy.deepcopy(loft);broken['elements'][0]['sections'][1]['z']=-.5
    check('Unordered sections reject atomically',reject('craft',expectedRevision=s.inspect()['revision'],operations=[preceding,dict(op='upsert',collection='anatomy',value=broken)]) and document()==loft_document)
    broken=copy.deepcopy(loft);broken['elements'][0]['sections'][1]['radius'][0]=True
    check('Boolean section radius rejects atomically',reject('craft',expectedRevision=s.inspect()['revision'],operations=[dict(op='upsert',collection='anatomy',value=broken)]) and document()==loft_document)
    loft_study=c('saveStudy',path='sculpt-loft-study.json')['path'];c('undo')
    check('Loft replacement undo restores source baseline',document()==baseline)
    c('loadStudy',path=loft_study)
    check('Loft source and exact rendered pixels replay',document()==loft_document and digest(s.observe('sculpt-loft-replay')['path'])==digest(loft_capture['path']))
    with s.edit() as e:
        pending_loft=copy.deepcopy(loft);pending_loft['skinBlend']=.01
        e.upsert('anatomy',pending_loft)
        e.sample_anatomy(loft['id'],base_edge_length=.12,regions=[dict(id='front-detail',center=[0,.5,.35],radius=[.3,.3,.2],edgeLength=.025)],maximum_tetrahedra=100000)
    check('Sampling composes with pending source edits',abs(next(x for x in s.inspect()['source']['anatomy'] if x['id']==loft['id'])['skinBlend']-.01)<.000001)
    sampled_document=document();sampled_capture=s.observe('sculpt-volume-sampled')
    sampled_report=next(x for x in s.inspect()['anatomy'] if x['id']==loft['id'])['extraction']
    check('Native source reports conforming local volume resampling',sampled_report['algorithm']=='conforming-octree-fans' and sampled_report['sampling']['cellSubdivisions']>0)
    check('Source volume resampling changes actual geometry',digest(sampled_capture['path'])!=digest(loft_capture['path']))
    sampled_study=c('saveStudy',path='sculpt-sampled-study.json')['path']
    bad_sampling=copy.deepcopy(next(x for x in s.inspect()['source']['anatomy'] if x['id']==loft['id']));bad_sampling['sampling']['baseEdgeLength']=.006;bad_sampling['sampling']['regions']=[];bad_sampling['sampling']['maximumTetrahedra']=1000
    check('Sampling budget rejection rolls back preceding edits',reject('craft',expectedRevision=s.inspect()['revision'],operations=[preceding,dict(op='upsert',collection='anatomy',value=bad_sampling)]) and document()==sampled_document)
    bad_sampling['sampling']['maximumTetrahedra']=True
    check('Boolean sampling budget rejects atomically',reject('craft',expectedRevision=s.inspect()['revision'],operations=[dict(op='upsert',collection='anatomy',value=bad_sampling)]) and document()==sampled_document)
    c('undo');check('Sampling undo restores complete study and exact pixels',document()==loft_document and digest(s.observe('sculpt-volume-undone')['path'])==digest(loft_capture['path']))
    c('loadStudy',path=sampled_study)
    check('Sampled source reproduces exact pixels on replay',document()==sampled_document and digest(s.observe('sculpt-volume-replay')['path'])==digest(sampled_capture['path']))
    c('loadStudy',path=baseline_lens_study)
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
    with s.edit() as e:e.refine_local('projected-detail','shell',hit['point'],[.08]*3,.006,6000,projection=dict(maximumDistance=.01,tolerance=.000001,retriangulate=True))
    projected=s.select(s.observe('sculpt-field-projected'),[[960,540]],part='shell',explain=True)['hits'][0]
    check('Local projection resolves the actual field surface',abs(projected['explanation']['fieldValue'])<.00002 and projected['triangleEdgeMetres']<.0065)
    c('undo');check('Projection undo restores complete study and exact pixels',document()==baseline and digest(s.observe('sculpt-projection-undone')['path'])==digest(observation['path']))
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
    multi_origin=c('saveStudy',path='sculpt-before-multiview.json')['path']
    trial_layer=copy.deepcopy(next(x for x in s.inspect()['source']['sculptLayers'] if x['id']=='intent'));trial_layer['opacity']=.7
    multi=s.explore_edits([dict(key='reduced-layer',operations=[dict(op='upsert',collection='sculptLayers',value=trial_layer)])],root/'sculpt-multiview',
        views=[dict(key='front',orbit=0,elevation=.1),dict(key='quarter',orbit=.8,elevation=.1),dict(key='side',orbit=1.57,elevation=.1)])
    multi_report=json.loads(Path(multi['report']).read_text());multi_records=multi_report['records']
    check('Multiview exploration compiles one candidate and archives six matched images',len(multi_report['results'])==1 and multi_report['results'][0]['accepted'] and len(multi_records)==6)
    check('Multiview exploration restores exact source and camera',multi['restored'] and document()==changed)
    check('Matched front quarter side renders differ',len({digest(x['path']) for x in multi_records[:3]})==3)
    c('loadStudy',path=str(Path(multi['report']).parent/multi_records[4]['study']))
    check('A multiview candidate replays exact pixels',digest(s.observe('sculpt-multiview-replay')['path'])==digest(multi_records[4]['path']))
    c('loadStudy',path=multi_origin)
    check('Invalid camera label rejects atomically',reject('camera',orbit=.4,viewName=False) and document()==changed)
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
