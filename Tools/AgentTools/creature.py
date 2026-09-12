"""Agent-native creature authoring client. All mutations use one revision-checked transaction.

    studio = CreatureWorkspace()
    with studio.edit() as edit:
        edit.landmark('cheek', 'mask', [-.5, 3.1, -2])
        edit.sculpt('cheek-plane', 'mask', [[-.5, 3.1, -2]], brush='flatten',
                    radius=.3, strength=.15, direction=[0, 0, -1], mirror=True)

No automatic retry of stale revisions, no writes to published assets, no hidden
approval step. Save explicitly after inspecting an actual-render review.
"""
import json
import os
import runpy
import copy
import time
import shutil
from pathlib import Path

class CreatureWorkspace:
    def __init__(self, command=None):
        if command is None:
            os.environ['SANCTUARY_CONTROL_ROOT'] = '.soundstage'
            command = runpy.run_path(str(Path(__file__).resolve().parents[2] / 'scripts/gardenctl'))['command']
        self.command = command

    def inspect(self):
        return self.command('craftInspect')['craft']

    def edit(self):
        return CraftTransaction(self, self.inspect())

    def report(self, samples=25, part=None, start=None, end=None):
        arguments={k:v for k,v in dict(part=part,start=start,end=end).items() if v is not None}
        return self.command('craftReport', samples=samples, **arguments)['report']

    def probe(self, part, point, radius=.25):
        return self.command('surfaceProbe', part=part, x=point[0], y=point[1], z=point[2], radius=radius)['probe']

    def save(self):
        return self.command('assetSave')

    def observe(self, label='sculpt-observation'):
        """Capture the actual engine image with a native surface-selection token."""
        capture=self.command('capture',label=label)
        metadata=json.loads(Path(capture['metadata']).read_text())
        return dict(capture,frame=metadata['sculptFrame'])

    def select(self, observation, pixels, part=None, *, explain=False):
        frame=observation['frame']
        return self.command('sculptProbe',frame=frame['token'],expectedRevision=frame['revision'],
                            pixels=pixels,explain=explain,**({} if part is None else dict(part=part)))['selection']

    def preview_sculpt(self, part=None, controls=(), protect=()):
        """Orange: editable influence; blue: protected interior/feather.
        Omit part to clear. This changes study display, never the source.
        """
        return self.command('sculptOverlay',expectedRevision=self.inspect()['revision'],
            value=None if part is None else dict(part=part,controls=list(controls),protect=list(protect)))

    def frame_visible(self, part=None):
        return self.command('frameVisible',expectedRevision=self.inspect()['revision'],
                            **({} if part is None else dict(part=part)))['bounds']

    def compare_sculpt_layer(self, key, folder, opacities=(0, .5, 1)):
        """Archive matched actual renders and replay studies; restore the source.

        Requires a paused study. Never auto-accepts a variant. A revision change
        from another editor stops the run; the original study remains on disk.
        """
        folder=Path(folder).resolve()
        folder.mkdir(parents=True,exist_ok=False)
        if not 2<=len(opacities)<=8 or any(isinstance(x,bool) or not 0<=x<=1 for x in opacities):
            raise ValueError('Choose 2…8 layer intensities in 0…1')
        snapshot=self.inspect()
        original=next((x for x in snapshot['source']['sculptLayers'] if x['id']==key),None)
        if original is None:raise ValueError('Unknown sculpt layer: '+key)
        document=self.command('status')['state']['workshop']['document']
        if not document['paused']:raise RuntimeError('Pause the studio before comparing sculpt alternatives')
        self.command('saveStudy',path=str(folder/'original.json'))
        locked=bool(document.get('sculptReviewFrame'))
        revision=snapshot['revision'];records=[];restored=False
        report=dict(layer=key,opacities=list(opacities),records=records,restored=False)
        try:
            self.command('sculptFrameLock',enabled=True,expectedRevision=revision)
            settings=None
            for i,opacity in enumerate(opacities):
                candidate=dict(original,opacity=opacity,enabled=True)
                start=time.monotonic()
                result=self.command('craft',expectedRevision=revision,operations=[dict(op='upsert',collection='sculptLayers',value=candidate)])
                revision=result['craft']['revision']
                elapsed=time.monotonic()-start
                capture=self.observe('sculpt-variant-'+str(i))
                if capture['frame']['revision']!=revision:raise RuntimeError('Source changed during comparison; inspect original.json before recovering')
                metadata=json.loads(Path(capture['metadata']).read_text())
                current=copy.deepcopy(metadata['workshop']['document']);current.pop('source')
                if settings is None:settings=current
                if current!=settings:raise RuntimeError('Study settings changed during comparison; captures are not matched')
                stem=f'variant-{i:02d}'
                self.command('saveStudy',path=str(folder/(stem+'-study.json')))
                shutil.copyfile(capture['path'],folder/(stem+'.png'))
                shutil.copyfile(capture['metadata'],folder/(stem+'.json'))
                records.append(dict(variant=f'{key} · {opacity:g}',condition='Matched lighting',view=document.get('viewName','Current'),frame=0,
                    path=str(folder/(stem+'.png')),metadata=str(folder/(stem+'.json')),image=stem+'.png',meta=stem+'.json',study=stem+'-study.json',
                    editSeconds=elapsed,sourceRevision=revision))
        finally:
            # CAS restoration cannot overwrite a newer source edit. Keep the
            # full original checkpoint if restoration itself is rejected.
            try:
                self.command('craft',expectedRevision=revision,operations=[dict(op='upsert',collection='sculptLayers',value=original)])
                restored=self.inspect()['source']==snapshot['source']
                if not locked:self.command('sculptFrameLock',enabled=False,expectedRevision=snapshot['revision'])
            finally:
                report['restored']=restored
                (folder/'report.json').write_text(json.dumps(report,indent=2))
        viewer=runpy.run_path(str(Path(__file__).resolve().parents[2]/'scripts/workshop_viewer.py'))
        viewer['write_viewer'](folder,records)
        return dict(viewer=str(folder/'index.html'),report=str(folder/'report.json'),restored=restored)

    def explore_sculpt(self, intents, folder):
        """Evaluate bounded fits through the same ordinary edit exploration loop."""
        snapshot=self.inspect();keys=[x['key'] for x in intents]
        if len(set(keys))!=len(keys) or any(x['id'] in keys for x in snapshot['source']['sculptLayers']):
            raise ValueError('Each candidate needs a new unique layer key')
        candidates=[]
        for intent in intents:
            transaction=CraftTransaction(self,snapshot);transaction.fit_sculpt(**intent)
            candidates.append(dict(key=intent['key'],operations=transaction.operations))
        return self.explore_edits(candidates,folder,expected_revision=snapshot['revision'])

    def explore_edits(self, candidates, folder, *, expected_revision=None):
        """Compare 1…8 atomic craft operation lists from one unchanged baseline.

        Works for field anatomy, sculpting, rigs and other craft source. Archives
        original inputs, actual renders, replay studies and rejection diagnostics;
        retains no candidate. Restoration uses the complete original craft and
        compare-and-swap revision, never an unconditional study load.
        """
        if not 1<=len(candidates)<=8:raise ValueError('Choose 1…8 bounded edit candidates')
        if any(set(x)!={'key','operations'} or not isinstance(x['key'],str) or not x['key']
               or not isinstance(x['operations'],list) or not 1<=len(x['operations'])<=64 for x in candidates):
            raise ValueError('Candidates require a name and 1…64 ordinary craft operations')
        if len({x['key'] for x in candidates})!=len(candidates):raise ValueError('Candidate names must be unique')
        snapshot=self.inspect()
        if expected_revision is not None and expected_revision!=snapshot['revision']:
            raise RuntimeError('Exploration baseline is stale; inspect the source again')
        document=self.command('status')['state']['workshop']['document']
        if not document['paused'] or document.get('sculptOverlay'):
            raise RuntimeError('Pause and clear the influence overlay before comparing final surfaces')
        folder=Path(folder).resolve();folder.mkdir(parents=True,exist_ok=False)
        self.command('saveStudy',path=str(folder/'original.json'))
        records=[];results=[];locked=bool(document.get('sculptReviewFrame'));revision=snapshot['revision'];active=None
        report=dict(candidates=candidates,results=results,records=records,restored=False)
        settings=None
        def capture(variant):
            nonlocal settings
            observation=self.observe('sculpt-explore-'+str(len(records)))
            if observation['frame']['revision']!=revision:raise RuntimeError('Source changed during exploration; original.json is preserved')
            metadata=json.loads(Path(observation['metadata']).read_text())
            current=copy.deepcopy(metadata['workshop']['document']);current.pop('source')
            if settings is None:settings=current
            if settings!=current:raise RuntimeError('Study settings changed during exploration; captures cannot be compared')
            stem=f'candidate-{len(records):02d}'
            self.command('saveStudy',path=str(folder/(stem+'-study.json')))
            shutil.copyfile(observation['path'],folder/(stem+'.png'))
            shutil.copyfile(observation['metadata'],folder/(stem+'.json'))
            records.append(dict(variant=variant,condition='Matched lighting',view=document.get('viewName','Current'),frame=0,
                path=str(folder/(stem+'.png')),metadata=str(folder/(stem+'.json')),image=stem+'.png',meta=stem+'.json',study=stem+'-study.json'))
        try:
            self.command('sculptFrameLock',enabled=True,expectedRevision=revision)
            capture('Baseline')
            for candidate in candidates:
                start=time.monotonic()
                try:
                    response=self.command('craft',expectedRevision=snapshot['revision'],operations=candidate['operations'])
                except RuntimeError as error:
                    if self.inspect()['revision']!=snapshot['revision']:raise
                    results.append(dict(key=candidate['key'],accepted=False,error=str(error),seconds=time.monotonic()-start))
                    continue
                revision=response['craft']['revision'];active=candidate['key']
                results.append(dict(key=active,accepted=True,fit=response.get('sculptFits',[]),seconds=time.monotonic()-start))
                capture(active)
                self.command('author',expectedRevision=revision,craft=snapshot['source'])
                revision=self.inspect()['revision'];active=None
                if revision!=snapshot['revision']:raise RuntimeError('Restoring a candidate did not reproduce its source baseline')
        finally:
            try:
                if active:
                    self.command('author',expectedRevision=revision,craft=snapshot['source'])
                    revision=self.inspect()['revision']
                report['restored']=self.inspect()['revision']==snapshot['revision']
                if report['restored'] and not locked:self.command('sculptFrameLock',enabled=False,expectedRevision=snapshot['revision'])
            finally:
                (folder/'report.json').write_text(json.dumps(report,indent=2))
        viewer=runpy.run_path(str(Path(__file__).resolve().parents[2]/'scripts/workshop_viewer.py'))
        viewer['write_viewer'](folder,records)
        return dict(viewer=str(folder/'index.html'),report=str(folder/'report.json'),restored=report['restored'])

class CraftTransaction:
    def __init__(self, workspace, snapshot):
        self.workspace, self.snapshot = workspace, snapshot
        self.operations = []
        self.result = None
        self.closed = False

    def __enter__(self):
        return self

    def __exit__(self, kind, value, traceback):
        if kind is None:
            self.commit()
        self.closed = True

    def operation(self, op, **values):
        if self.closed:
            raise RuntimeError('Transaction is closed; inspect the source before beginning another edit')
        self.operations.append(dict(op=op, **values))
        return self

    def commit(self):
        if self.closed:
            raise RuntimeError('Transaction already closed')
        if not self.operations:
            self.closed = True
            return None
        # Close even after rejection: edits must be reviewed against a new snapshot.
        self.closed = True
        self.result = self.workspace.command('craft', expectedRevision=self.snapshot['revision'], operations=self.operations)
        return self.result

    def upsert(self, collection, value):
        return self.operation('upsert', collection=collection, value=value)

    def remove(self, collection, key):
        return self.operation('remove', collection=collection, key=key)

    def set(self, collection, value):
        return self.operation('set', collection=collection, value=value)

    def landmark(self, key, part, position, note=''):
        return self.upsert('landmarks', dict(id=key, part=part, position=position, note=note))

    def sculpt(self, key, part, points, *, brush='grab', radius=.2, strength=.02,
               direction=(0, 1, 0), mirror=False):
        return self.upsert('strokes', dict(id=key, part=part, brush=brush, points=points,
                           radius=radius, strength=strength, direction=list(direction), mirrorX=mirror))

    def sculpt_layer(self, key, strokes, opacity=1, enabled=True):
        return self.upsert('sculptLayers', dict(id=key, strokes=strokes, opacity=opacity, enabled=enabled))

    def sculpt_selected(self, key, selection, index=0, *, brush='scrape', radii=(.2,.2,.1),
                        strength=.3, plane_offset=0, tangent=None, hardness=0, protect=(), mirror=False):
        """Create a source layer directly from a pixel-selected surface frame.

        Radii are tangent/bitangent/depth metres. Protection volumes are bind
        ellipsoids whose inner 70% is held exactly and outer 30% feathers.
        """
        if selection['revision']!=self.snapshot['revision']:
            raise RuntimeError('Selection is stale; capture and select the edited surface again')
        hit=selection['hits'][index];normal=hit['normal']
        if tangent is None:
            axis=min(range(3),key=lambda i:abs(normal[i]))
            tangent=[int(i==axis) for i in range(3)]
        stroke=dict(id=key+'-stamp',part=hit['part'],brush=brush,points=[hit['point']],radius=max(radii),
                    strength=strength,direction=normal,mirrorX=mirror,
                    profile=dict(tangent=list(tangent),radii=list(radii),hardness=hardness,
                                 planeOffset=plane_offset,protect=list(protect)))
        return self.sculpt_layer(key,[stroke])

    def fit_sculpt(self, key, part, controls, targets, maximum_displacement=.15, iterations=24, protect=(), maximum_normal_change_degrees=180):
        return self.operation('fitSculpt', key=key, part=part, controls=controls, targets=targets,
                              maximumDisplacement=maximum_displacement, iterations=iterations,protect=list(protect),maximumNormalChangeDegrees=maximum_normal_change_degrees)

    def sculpt_landmark(self, key, landmark, **options):
        anchors = {x['id']: x for x in self.snapshot.get('resolvedLandmarks', self.snapshot['source']['landmarks'])}
        for op in self.operations:
            if op.get('collection') == 'landmarks' and op['op'] == 'upsert':
                anchors[op['value']['id']] = op['value']
        anchor = anchors[landmark]
        return self.sculpt(key, anchor['part'], [anchor['position']], **options)

    def refine(self, part, levels=1):
        values = dict(self.snapshot['source']['refinement'])
        for op in self.operations:
            if op.get('collection') == 'refinement' and op['op'] == 'set':
                values = dict(op['value'])
        values[part] = levels
        return self.set('refinement', values)

    def refine_local(self, key, part, center, radius, edge_length, maximum_new_vertices=50000):
        return self.upsert('detailPatches',dict(id=key,part=part,center=list(center),radius=list(radius),
                          edgeLength=edge_length,maximumNewVertices=maximum_new_vertices))

    def skin(self, key, part, center, radius, weights, strength=1):
        return self.upsert('skinFields', dict(id=key, part=part, center=center,
                           radius=radius, jointWeights=weights, strength=strength))

    def corrective(self, key, part, driver, axis, start, end, center, radius,
                   offset=(0, 0, 0), dilation=(0, 0, 0)):
        return self.upsert('correctives', dict(id=key, driver=driver, axis=axis, start=start, end=end,
            field=dict(id=key, part=part, center=center, radius=radius, offset=list(offset), dilation=list(dilation))))

    def posed_corrective(self, key, part, point, offset, radius, *, driver, axis, start, end,
                        time=None, maximum_projection=.1):
        return self.operation('posedCorrective',key=key,part=part,point=list(point),offset=list(offset),
            radius=radius,driver=driver,axis=axis,start=start,end=end,maximumProjection=maximum_projection,
            **({} if time is None else dict(time=time)))

    def mass(self, joint, center, kilograms):
        return self.upsert('masses', dict(joint=joint, center=center, kilograms=kilograms))

    def balance(self, joint, strength=1, maximum_shift=.2, transition_seconds=.25):
        return self.set('balance', dict(joint=joint, strength=strength, maximumShift=maximum_shift, transitionSeconds=transition_seconds))

    def chain(self, key, parent, points, reparent=()):
        return self.operation('rigChain', key=key, parent=parent, points=points, reparent=list(reparent))

    def capture_phrase(self, key, duration, samples=25):
        return self.operation('capturePhrase', key=key, duration=duration, samples=samples)

    def phrase(self, key, duration, samples, contacts=(), references=(), interpolation="spline"):
        return self.upsert('phrases', dict(id=key, duration=duration, samples=samples,
                           contacts=list(contacts), references=list(references), interpolation=interpolation))

    def fit_pose(self, phrase, time, targets, freedoms, *, tolerance=.002, iterations=32, require_convergence=True):
        return self.operation('fitPose', key=phrase, time=time, targets=targets, freedoms=freedoms,
                              tolerance=tolerance, iterations=iterations, requireConvergence=require_convergence)

    def footsteps(self, phrase, steps, minimum_support=1):
        return self.operation('footsteps', key=phrase, steps=steps, minimumSupport=minimum_support)

    def guide_chain(self, key, joint, points, *, pins=1, inverse_mass=40, radius=.025, compliance=.00001):
        return self.upsert('guideChains', dict(id=key, joint=joint, points=points, pins=pins,
                           inverseMass=inverse_mass, radius=radius, compliance=compliance))

    def cloth_panel(self, key, part, columns, rows, points, attachments, pins, *,
                    inverse_mass=20, radius=.025, compliance=.00001, resolution=48, color=(.3,.4,.36)):
        return self.upsert('clothPanels', dict(id=key, part=part, columns=columns, rows=rows,
            points=points, attachments=attachments, pins=pins, inverseMass=inverse_mass,
            radius=radius, compliance=compliance, resolution=resolution, color=list(color)))

    def groom(self, key, part, guides, *, fibres=48, width=.06, radius=.003, clump=.8,
              curl=.02, frequency=2, length_variation=.2, flyaways=.05, seed=17,
              root_color=(.35, .24, .10), tip_color=(.6, .46, .24), envelope=None):
        return self.upsert('grooms', dict(id=key, part=part, guides=guides, fibres=fibres,
            width=width, radius=radius, clump=clump, curl=curl, frequency=frequency,
            lengthVariation=length_variation, flyaways=flyaways, seed=seed,
            rootColor=list(root_color), tipColor=list(tip_color),
            **({"envelope": envelope} if envelope is not None else {})))

    def seam(self, key, part, points, *, radius=.004, spacing=.02, lift=.003, color=(.6, .45, .2)):
        return self.upsert('seams', dict(id=key, part=part, points=points, radius=radius,
                           spacing=spacing, lift=lift, color=list(color)))

    def anatomy(self, key, part, elements, *, resolution=64, material=7, roughness=.65, metallic=0, replaces=(), skin_blend=None):
        """Replace/create a named surface from authoritative ordered add/cut fields."""
        return self.upsert('anatomy', dict(id=key, part=part, elements=elements, resolution=resolution,
            material=material, roughness=roughness, metallic=metallic, replaces=list(replaces),
            **({"skinBlend": skin_blend} if skin_blend is not None else {})))

    @staticmethod
    def anatomical_element(key, region, center, radius, *, primitive='ellipsoid', operation='add',
                           role='surface', end=(0,0,0), rotation=(0,0,0), blend=.02,
                           color=(.6,.6,.6), weights=None, cut_blend=None):
        return dict(id=key, region=region, primitive=primitive, operation=operation, role=role,
                    center=list(center), end=list(end), radius=list(radius), rotation=list(rotation),
                    blend=blend, color=list(color), jointWeights={} if weights is None else weights,
                    **({} if cut_blend is None else dict(cutBlend=cut_blend)))

    def contact_chain(self, key, parent, upper, lower, foot, pole, *, sole=0, contact_offset=None):
        return self.upsert('contactChains', dict(id=key, parent=parent, upper=upper, lower=lower,
                                              foot=foot, pole=list(pole), sole=sole,
                                              **({} if contact_offset is None else {'contactOffset':list(contact_offset)})))

    def anatomy_anchor(self, key, source, point, *, maximum_projection=.05):
        return self.operation('anatomyAnchor', key=key, source=source, point=list(point), maximumProjection=maximum_projection)

    def rebind_anatomy(self, source, *, allow_structural_changes=False, maximum_projection=.05,
                      tolerance=.0002, element_map=None):
        return self.operation('rebindAnatomy', key=source, allowStructuralChanges=allow_structural_changes,
                              maximumProjection=maximum_projection, tolerance=tolerance,
                              elementMap={} if element_map is None else element_map)

    def bind_anatomy(self, key, anchor, kind, target, *, follow_normal=False):
        return self.operation('bindAnatomy', key=key, anchor=anchor, kind=kind, target=target,
                              followNormal=follow_normal)
