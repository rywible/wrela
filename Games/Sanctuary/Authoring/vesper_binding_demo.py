#!/usr/bin/env python3
"""Inspect or exercise Vesper's persistent mane/body correspondence via public APIs.

Default `plan` is read-only. `install` adds bindings at their current position,
without changing the design or publishing source. `exercise` briefly translates
only the authoritative body field by a bounded amount, records actual renders
and consumer displacement, then restores the exact original study in finally.

This proves correspondence across source edits and runtime recompilation.
The guide's existing posed joint attachment is retained; body skin weights are
not copied and exact tracking of a skinned body point is not claimed.
Run only against the Soundstage session you own; this script does not start apps.
"""
import argparse
import copy
import hashlib
import json
import math
import sys
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'Tools/AgentTools'))
from creature import CreatureWorkspace, CraftTransaction


def delta(a, b):
    return [a[k]-b[k] for k in range(3)]


def length(v):
    return math.sqrt(sum(x*x for x in v))


def capture(studio, label):
    result=studio.command('capture', label=label)
    path=Path(result['path'])
    return dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())


def prepare(studio, guide_index=None):
    authored=studio.command('authoring')['authoring']
    source=authored['source']
    if source.get('id') != 'vesper':
        raise RuntimeError('Select Sanctuary / Vesper before inspecting its binding demonstration')
    snapshot=studio.inspect()
    if snapshot['revision'] != authored['revision']:
        raise RuntimeError('Source changed during inspection; inspect again before editing')
    body=next((a for a in snapshot['source'].get('anatomy',[]) if a['part']=='body'),None)
    groom=next((g for g in snapshot['source'].get('grooms',[]) if g['part']=='mane'),None)
    if body is None or groom is None:
        raise RuntimeError('Apply the authored Vesper anatomy and mane groom before this demonstration')
    node_records={n['id']:n for n in snapshot['guideNodes']}
    nodes={key:node['position'] for key,node in node_records.items()}
    bound={b['target'] for b in snapshot['source'].get('anatomyBindings',[]) if b['kind']=='guideNode'}
    candidates=[]
    for index, guide in enumerate(groom['guides']):
        if guide_index is not None and index != guide_index:
            continue
        if any(node in bound for node in guide):
            continue
        point=nodes[guide[0]]
        probe=studio.probe('body',point,radius=.6)
        candidates.append((probe['distance'],index,guide,probe))
    if not candidates:
        raise RuntimeError('The requested mane guide is absent or already bound; select an unbound guide')
    distance,index,guide,probe=min(candidates,key=lambda item:(item[0],item[1]))
    if distance>.6:
        raise RuntimeError(f'Nearest unbound mane root is {distance:.3f} m from the body; inspect the intended attachment before binding')
    layers=[layer for layer in source.get('surfaceLayers',[]) if layer['part']=='mane']
    target_bindings={b['target'] for b in snapshot['source'].get('anatomyBindings',[]) if b['kind']=='surfaceLayer'}
    layer=next((x for x in layers if x['id'] not in target_bindings),None)
    prefix=f'vesper-mane-correspondence-{index}'
    return dict(snapshot=snapshot,body=body,guide=guide,index=index,probe=probe,prefix=prefix,
                layer=layer,rootDistance=distance,
                selectedRootPosition=nodes[guide[0]],sourceID=source['id'],
                guidePoseJoints={node:node_records[node]['joint'] for node in guide})


def install(studio, plan):
    # Decisions use the same revision observed while choosing the surface/guide.
    transaction=CraftTransaction(studio,plan['snapshot'])
    anchor=plan['prefix']+'-anchor'
    transaction.anatomy_anchor(anchor,plan['body']['id'],plan['probe']['point'],maximum_projection=.02)
    for node in plan['guide']:
        transaction.bind_anatomy(plan['prefix']+'-'+node,anchor,'guideNode',node,follow_normal=True)
    if plan['layer']:
        transaction.bind_anatomy(plan['prefix']+'-finish',anchor,'surfaceLayer',plan['layer']['id'])
    transaction.commit()
    return anchor


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode',choices=['plan','install','exercise'],default='plan')
    parser.add_argument('--guide-index',type=int)
    parser.add_argument('--shift',type=float,default=.02,help='Temporary vertical body translation in metres; exercise only, 0.005…0.05')
    parser.add_argument('--output',type=Path,help='Optional report JSON; requires a new path')
    args=parser.parse_args()
    if not .005<=args.shift<=.05:
        parser.error('--shift must be between 0.005 and 0.05 metres')
    if args.output and args.output.exists():
        parser.error('--output must name a new report path')
    studio=CreatureWorkspace();plan=prepare(studio,args.guide_index)
    report=dict(mode=args.mode,source='vesper',anatomy=plan['body']['id'],guide=plan['guide'],
                rootDistanceMetres=plan['rootDistance'],surfacePoint=plan['probe']['point'],
                surfaceLayer=plan['layer']['id'] if plan['layer'] else None,
                intendedDesignChange=False,published=False,
                guidePoseJoints=plan['guidePoseJoints'],
                scope='Persistent bind-space correspondence through source edits and recompilation; existing guide pose joints remain authoritative',
                copiesSurfaceSkinWeights=False,guaranteesPosedSurfaceTracking=False)
    if args.mode=='install':
        report['anchor']=install(studio,plan)
        installed=studio.inspect()
        report['revision']=installed['revision']
        record=next(a for a in installed['source']['anatomyAnchors'] if a['id']==report['anchor'])
        element=next(e for e in plan['body']['elements'] if e['id']==record['element'])
        report['anchorElement']=record['element'];report['anchorRegion']=record['region']
        report['anchorElementSkinWeights']=element['jointWeights']
    elif args.mode=='exercise':
        label='vesper-binding-'+uuid.uuid4().hex[:8]
        original=studio.command('saveStudy',path=label+'-original.json')['path']
        try:
            studio.command('pause',value=True)
            frozen=studio.command('saveStudy',path=label+'-frozen.json')['path']
            report['before']=capture(studio,label+'-before')
            report['anchor']=install(studio,plan)
            bound=studio.inspect()
            record=next(a for a in bound['source']['anatomyAnchors'] if a['id']==report['anchor'])
            element=next(e for e in plan['body']['elements'] if e['id']==record['element'])
            report['anchorElement']=record['element'];report['anchorRegion']=record['region']
            report['anchorElementSkinWeights']=element['jointWeights']
            report['installed']=capture(studio,label+'-installed')
            report['installationPixelsEqual']=report['before']['sha256']==report['installed']['sha256']
            points={n['id']:n['position'] for n in bound['guideNodes']}
            layers={x['id']:x['center'] for x in bound['resolvedSurfaceLayers']}
            anchors={a['id']:a for a in bound['source']['anatomyAnchors']}
            moved=copy.deepcopy(plan['body'])
            for element in moved['elements']:
                element['center'][1]+=args.shift
                if element['primitive']=='capsule':
                    element['end'][1]+=args.shift
            edit=CraftTransaction(studio,bound)
            edit.upsert('anatomy',moved)
            edit.commit()
            updated=studio.inspect()
            updated_points={n['id']:n['position'] for n in updated['guideNodes']}
            expected=[0,args.shift,0]
            report['guideDeltas']={node:delta(updated_points[node],points[node]) for node in plan['guide']}
            report['maximumGuideDisplacementError']=max(length(delta(value,expected)) for value in report['guideDeltas'].values())
            report['anchorDelta']=delta(next(a for a in updated['source']['anatomyAnchors'] if a['id']==report['anchor'])['sourcePosition'],anchors[report['anchor']]['sourcePosition'])
            if plan['layer']:
                field=plan['layer']['id']
                now=next(x for x in updated['resolvedSurfaceLayers'] if x['id']==field)['center']
                report['finishFieldDelta']=delta(now,layers[field])
            report['moved']=capture(studio,label+'-moved')
            report['resolvedConsumersFollow']=report['maximumGuideDisplacementError']<.001
            if not report['resolvedConsumersFollow']:
                raise AssertionError(f"Bound guide displacement residual {report['maximumGuideDisplacementError']:.6f} m")
            studio.command('loadStudy',path=frozen)
            report['restored']=capture(studio,label+'-restored')
            report['restoredPixelsEqual']=report['before']['sha256']==report['restored']['sha256']
            if not report['restoredPixelsEqual']:
                raise AssertionError('Restoring the frozen original did not reproduce its exact pixels')
        except Exception as error:
            report['error']=str(error)
            raise
        finally:
            studio.command('loadStudy',path=original)
            report['originalStudyRestored']=True
            if args.output:
                args.output.parent.mkdir(parents=True,exist_ok=True)
                args.output.write_text(json.dumps(report,indent=2)+'\n')
    if args.output:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))

if __name__=='__main__':
    main()
