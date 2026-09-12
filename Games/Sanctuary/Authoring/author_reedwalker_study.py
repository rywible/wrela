#!/usr/bin/env python3
"""Prepare Reedwalker; only --apply contacts the running native Soundstage.

  python3 Games/Sanctuary/Authoring/author_reedwalker_study.py
  python3 Games/Sanctuary/Authoring/author_reedwalker_study.py --apply --leave-open

Run --apply only after Vesper's visual target review. It loads a small fields seed,
then submits anatomy, rig, weights and movement through CreatureWorkspace in one
atomic craft transaction. Native geometry/contact reports are saved separately.
The default current-study restoration is disabled only by --leave-open. Neither
mode publishes to Assets. Preparation is not completed generalization evidence.
"""
import argparse
import datetime
import hashlib
import json
import math
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'Tools' / 'AgentTools'))
from creature import CreatureWorkspace, CraftTransaction

NAME = 'reedwalker'
PHRASE = 'reedwalker-tripod-crossing'
CLIP = 'marsh-crossing'
DURATION = 8.0
DEFAULT_DIRECTORY = ROOT / 'Games/Sanctuary/Authoring/Studies/reedwalker'
SKIN = (.225, .315, .198)
BACK = (.295, .365, .205)
BELLY = (.420, .415, .264)
DARK = (.043, .067, .036)
SOLE = .055


def add(a, b):
    return [a[i] + b[i] for i in range(3)]


def lerp(a, b, t):
    return [a[i] + (b[i] - a[i]) * t for i in range(3)]


def element(key, region, center, radius, weights, **options):
    options.setdefault('color', SKIN)
    return CraftTransaction.anatomical_element(key, region, center, radius,
                                                weights=weights, **options)


def along(key, region, a, b, width, depth, weights, **options):
    d = [b[i] - a[i] for i in range(3)]
    length = math.sqrt(sum(x*x for x in d))
    rotation = [math.degrees(math.asin(d[2]/length)), 0,
                math.degrees(math.atan2(-d[0], d[1]))]
    return element(key, region, lerp(a, b, .5), [width, length*.63, depth],
                   weights, rotation=rotation, **options)


def joint(key, pivot, parent=None):
    return dict(id=key, pivot=list(pivot), **({'parent': parent} if parent else {}))


def pose(offset=(0, 0, 0), rotation=(0, 0, 0), scale=(1, 1, 1)):
    return dict(offset=list(offset), rotation=list(rotation), scale=list(scale))


def legs():
    """Independent authored hexapod proportions; all pivots are bind metres."""
    for row, z in [('front', -.52), ('middle', .04), ('rear', .58)]:
        for sign, side in [(-1, 'left'), (1, 'right')]:
            knee_z = z + {'front': -.03, 'middle': .035, 'rear': .10}[row]
            foot_z = z + {'front': -.14, 'middle': .00, 'rear': .14}[row]
            yield dict(id=row+'-'+side, side=sign, row=row,
                       root=[sign*.285, .43, z], knee=[sign*.735, .335, knee_z],
                       foot=[sign*.785, SOLE, foot_z])


TAIL_POINTS = [[0, .405, .70], [.12, .36, 1.14], [.18, .31, 1.62],
               [.12, .245, 2.10], [-.02, .19, 2.54]]
TAIL_JOINTS = ['tail', 'tail-middle-1', 'tail-middle-2', 'tail-tip']


def rig():
    yield joint('body', [0, .42, 0])
    yield joint('abdomen', [0, .405, .43], 'body')
    yield joint('head', [0, .445, -.57], 'body')
    for i, name in enumerate(TAIL_JOINTS):
        yield joint(name, TAIL_POINTS[i], 'abdomen' if i == 0 else TAIL_JOINTS[i-1])
    for leg in legs():
        key = leg['id']
        yield joint(key+'-upper', leg['root'], 'body')
        yield joint(key+'-lower', leg['knee'], key+'-upper')
        yield joint(key+'-paw', leg['foot'], key+'-lower')


def anatomy():
    body = [element('thoracic-core', 'thorax', [0, .405, -.18], [.365, .205, .62],
                    {'body': 1}, color=SKIN, blend=.035),
            element('soft-abdomen', 'abdomen', [0, .385, .46], [.355, .190, .49],
                    {'body': .25, 'abdomen': .75}, color=BACK, blend=.055),
            element('ventral-keel', 'belly', [0, .275, .03], [.280, .090, .68],
                    {'body': .65, 'abdomen': .35}, color=BELLY, blend=.025),
            element('spinal-rod', 'internal-spine', [0, .41, -.50], [.026]*3,
                    {'body': .6, 'abdomen': .4}, primitive='capsule',
                    end=[0, .405, .69], role='internalStructure')]
    for i, z in enumerate((-.40, -.15, .10, .35, .59)):
        body.append(element('dorsal-ridge-'+str(i), 'dorsal-fold', [0, .572, z],
                            [.295, .047, .065], {'body': 1} if z < .25 else {'abdomen': 1},
                            color=BACK, blend=.015))
    for leg in legs():
        root = leg['root']
        body.append(element('girdle-'+leg['id'], 'limb-girdle', root, [.13, .12, .165],
                            {'body': .75, leg['id']+'-upper': .25}, blend=.04))
    yield 'body', body, 64, .82

    head = [element('flat-cranium', 'cranium', [0, .465, -.805], [.455, .15, .36],
                    {'head': 1}, color=BACK, blend=.025),
            element('broad-snout', 'snout', [0, .412, -1.005], [.40, .103, .24],
                    {'head': 1}, color=SKIN, blend=.030),
            element('lower-lip', 'jaw', [0, .350, -.980], [.385, .056, .235],
                    {'head': 1}, color=BELLY, blend=.02),
            element('mouth-recess', 'mouth', [0, .386, -1.167], [.315, .018, .105],
                    {}, operation='cut', color=DARK, blend=0)]
    for sign in (-1, 1):
        side = 'left' if sign < 0 else 'right'
        head += [element('orbital-ridge-'+side, 'orbit', [sign*.329, .545, -.890],
                         [.102, .080, .117], {'head': 1}, color=BACK, blend=.022),
                 element('eye-'+side, 'eye', [sign*.349, .574, -.924],
                         [.057, .039, .044], {'head': 1}, color=[.49, .39, .14], blend=.001),
                 element('pupil-'+side, 'pupil', [sign*.351, .578, -.960],
                         [.035, .020, .012], {'head': 1}, color=DARK, blend=0),
                 element('nostril-'+side, 'nostril', [sign*.17, .461, -1.178],
                         [.026, .016, .026], {}, operation='cut', color=DARK, blend=0)]
    yield 'head', head, 64, .70

    tail = []
    for i in range(8):
        section = i // 2
        a = lerp(TAIL_POINTS[section], TAIL_POINTS[section+1], (i % 2)*.5)
        b = lerp(TAIL_POINTS[section], TAIL_POINTS[section+1], (i % 2+1)*.5)
        radius = .154*(1-i/9)**.8
        left = TAIL_JOINTS[min(section, 3)]
        right = TAIL_JOINTS[min(section+1, 3)]
        weights = {left: 1} if left == right else {left: .75-(i % 2)*.5,
                                                                right: .25+(i % 2)*.5}
        tail.append(along('tail-segment-'+str(i), 'tail', a, b, radius, radius*.72,
                          weights, color=BACK if i % 2 == 0 else SKIN, blend=.026))
    yield 'tail', tail, 64, .80

    for leg in legs():
        key = leg['id']; upper = key+'-upper'; lower = key+'-lower'; paw = key+'-paw'
        a, b, c = leg['root'], leg['knee'], leg['foot']
        limb = [along('proximal-muscle', 'upper-limb', a, b, .105, .110,
                      {upper: 1}, color=SKIN, blend=.018),
                element('elbow-volume', 'joint', b, [.082, .085, .080],
                        {upper: .5, lower: .5}, blend=.026),
                along('distal-muscle', 'lower-limb', b, c, .067, .074,
                      {lower: 1}, color=BACK, blend=.019),
                element('wrist', 'wrist', add(c, [0, .034, 0]), [.074, .081, .067],
                        {lower: .45, paw: .55}, color=SKIN, blend=.018),
                element('upper-bone', 'internal-bone', a, [.018]*3, {upper: 1},
                        primitive='capsule', end=b, role='internalStructure'),
                element('lower-bone', 'internal-bone', b, [.013]*3, {lower: 1},
                        primitive='capsule', end=c, role='internalStructure')]
        yield upper, limb, 48, .84
        foot = [element('webbed-pad', 'contact-pad', add(c, [0, -.011, -.026]),
                        [.135, .044, .14], {paw: 1}, color=BELLY, blend=.012)]
        for toe, x in enumerate((-.088, 0, .088)):
            foot.append(element('toe-'+str(toe), 'toe', add(c, [x, -.013, -.128]),
                                [.044, .040, .111 if toe == 1 else .085], {paw: 1},
                                rotation=[0, (toe-1)*-12, 0], color=SKIN, blend=.012))
        yield paw, foot, 48, .81


# Chest travel, support sway, compression and head/tail intent are authored keys.
# Swing paths themselves are constructed exclusively by native FootstepPlan.
# t      x       y       z     roll   pitch  headYaw tailYaw  flex
ACTING = [
    (0.00,  .000,  .000,  .000,   0.0,  0.0,   -8,   -3,   0),
    (0.60,  .000,  .006,  .000,   0.0,  0.0,   -5,   -3,   0),
    (1.10,  .012, -.040, -.010,  -0.7, -2.5,    2,    0,  .5),
    (1.45,  .035, -.042, -.045,  -1.4, -2.5,    5,    3,  .5),
    (1.85,  .040, -.018, -.105,  -2.0, -1.0,    3,    8,  .2),
    (2.25,  .025, -.032, -.155,  -1.0, -2.0,    0,   10,  .4),
    (2.50, -.010, -.016, -.180,   0.5, -1.0,   -2,    4,  .2),
    (2.60, -.035, -.018, -.185,   1.4, -1.0,   -3,    1,  .2),
    (3.00, -.040, -.008, -.230,   2.0,  0.0,   -3,   -7,  .1),
    (3.40, -.020, -.031, -.280,   0.9, -1.5,   -1,  -10,  .4),
    (3.70,  .035, -.022, -.310,  -1.3, -1.0,    2,   -2,  .2),
    (4.10,  .040, -.006, -.360,  -2.0,  0.5,    3,    8,  .1),
    (4.50,  .020, -.030, -.425,  -0.8, -1.5,    0,   11,  .4),
    (4.85, -.035, -.020, -.455,   1.3, -1.0,   -2,    2,  .2),
    (5.25, -.040, -.006, -.510,   2.0,  0.0,   -3,   -7,  .1),
    (5.65, -.020, -.038, -.560,   0.8, -2.5,   -1,  -11,  .5),
    (6.15,  .000, -.012, -.560,   0.0,  0.7,    2,   -3,  .1),
    (6.75,  .000,  .002, -.560,   0.0,  0.0,    7,    5,   0),
    (7.35,  .000, -.002, -.560,   0.0,  0.0,    9,    1,   0),
    (8.00,  .000,  .000, -.560,   0.0,  0.0,    9,    0,   0),
]


def motion_samples():
    result = []
    for i, (time, x, y, z, roll, pitch, gaze, tail, flex) in enumerate(ACTING):
        prev_tail = ACTING[max(0, i-1)][8]
        prev_prev = ACTING[max(0, i-2)][8]
        poses = {'body': pose((x, y, z), (pitch, 0, roll),
                              (1+.025*flex, 1-.055*flex, 1+.02*flex)),
                 'abdomen': pose(rotation=(-pitch*.35, -tail*.13, -roll*.2)),
                 'head': pose(rotation=(-pitch*.7, gaze, -roll*.4)),
                 'tail': pose(rotation=(0, tail*.45, roll*.25)),
                 'tail-middle-1': pose(rotation=(tail*.08, prev_tail*.55, 0)),
                 'tail-middle-2': pose(rotation=(prev_tail*.09, prev_prev*.65, 0)),
                 'tail-tip': pose(rotation=(prev_prev*.10, prev_prev*.50, 0))}
        # Cancel inherited torso tilt/yaw on each paw before contact alignment.
        for leg in legs():
            poses[leg['id']+'-paw'] = pose(rotation=(-pitch, 0, -roll))
        result.append(dict(time=time, poses=poses))
    return result


def footsteps():
    groups = [('front-left', 'middle-right', 'rear-left'),
              ('front-right', 'middle-left', 'rear-right')]
    anchor = {leg['id']: leg['foot'] for leg in legs()}
    for group, lift, land, distance in [(0, 1.45, 2.25, .28), (1, 2.60, 3.40, .28),
                                       (0, 3.70, 4.50, .56), (1, 4.85, 5.65, .56)]:
        for key in groups[group]:
            yield dict(chain=key, lift=lift, land=land,
                       target=add(anchor[key], [0, 0, -distance]), height=.125)


def author(transaction):
    for value in rig():
        transaction.upsert('joints', value)
    for part, elements, resolution, roughness in anatomy():
        # This study creates exact named outputs; it does not merge or replace any
        # sibling outputs. Omit that optional collection for source compatibility.
        transaction.upsert('anatomy', dict(id=NAME+'-'+part, part=part, elements=elements,
            resolution=resolution, material=7, roughness=roughness, metallic=0))
    for leg in legs():
        key = leg['id']
        transaction.contact_chain(key, 'body', key+'-upper', key+'-lower', key+'-paw',
                                  [leg['side']*.6, .06, .02], sole=SOLE)
    contacts = [dict(chain=leg['id'], keys=[dict(time=t, position=leg['foot'], planted=True)
                                           for t in (0, DURATION)]) for leg in legs()]
    references = [dict(id='authored-tripod-intent', locator='authored://reedwalker/marsh-crossing',
                       sourceTime=0, phraseTime=0,
                       note='Six-legged low-body crossing. Two alternating tripods; head scouts, '
                            'the body transfers support, and the long tail follows. Authored acting, '
                            'not force-based gait simulation.', landmarks={})]
    transaction.phrase(PHRASE, DURATION, motion_samples(), contacts=contacts, references=references)
    transaction.footsteps(PHRASE, list(footsteps()), minimum_support=3)
    transaction.set('arrangement', dict(clip=CLIP, duration=DURATION, looping=False,
        layers=[dict(id='reedwalker-acting', phrase=PHRASE, start=0, end=DURATION,
                     sourceStart=0, sourceEnd=DURATION, fadeIn=0, fadeOut=0, weight=1, additive=False)]))
    # Mass data enables inspection; no static balance assistance moves the body.
    transaction.mass('body', [0, .405, -.18], 11)
    transaction.mass('abdomen', [0, .385, .46], 7)
    transaction.mass('head', [0, .445, -.88], 4)
    transaction.mass('tail', [.08, .34, 1.12], 2)
    transaction.landmark('nose-tip', 'head', [0, .43, -1.23], 'Flat, broad sensing head')
    transaction.landmark('tail-end', 'tail', [-.02, .19, 2.54], 'Long taper and distal overlap')
    transaction.landmark('thoracic-center', 'body', [0, .61, -.18], 'Low body silhouette')
    # This exact isolated top surface point has stable source correspondence.
    transaction.anatomy_anchor('head-crown', NAME+'-head', [0, .615, -.805])


def operations():
    transaction = CraftTransaction(None, {'revision': 'offline-preparation'})
    author(transaction)
    return transaction.operations


def seed():
    # The only initial surface is replaced by craft.anatomy in the transaction.
    return dict(version=1, id=NAME, name='Reedwalker · Marsh Study', generator='fields',
                parts=[dict(name='body', joint=joint('body', [0, .42, 0]),
                            field=dict(op='move', values=[0, .42, 0], children=[
                                dict(op='sphere', values=[.1])]),
                            color=list(SKIN), material=7, roughness=.82, metallic=0)])


def inspect_plan(ops):
    """Input diagnostics only. Contact solving remains the native production code."""
    joints = list(rig()); sources = list(anatomy()); steps = list(footsteps())
    assert len(ops) <= 128 and len(sources) == 15 and len(joints) <= 64
    ids = {j['id'] for j in joints}
    assert len(ids) == len(joints)
    assert {part for part, _, _, _ in sources}.issubset(ids)
    assert len({s['chain'] for s in steps}) == 6
    assert motion_samples()[0]['time'] == 0 and motion_samples()[-1]['time'] == DURATION
    for _, elements, _, _ in sources:
        assert len(elements) <= 128
        for e in elements:
            assert set(e['jointWeights']).issubset(ids)
            assert not e['jointWeights'] or abs(sum(e['jointWeights'].values())-1) < 1e-6
    boundaries = sorted({0, DURATION} | {s[k] for s in steps for k in ('lift', 'land')})
    support_intervals = []
    for start, end in zip(boundaries, boundaries[1:]):
        midpoint = (start+end)/2
        swinging = [s['chain'] for s in steps if s['lift'] <= midpoint < s['land']]
        assert len(set(swinging)) == len(swinging)
        supports = sorted(ids for ids in {l['id'] for l in legs()} if ids not in swinging)
        assert len(supports) >= 3
        support_intervals.append(dict(start=start, end=end, supporting=supports))
    return dict(status='prepared-only', nativeCompilationRun=False, nativeContactSolverRun=False,
                visualReviewRun=False, anatomySources=len(sources), joints=len(joints),
                operations=len(ops), durationSeconds=DURATION,
                sourceBodyHeightMetres=.615, tailCenterlineMetres=sum(
                    math.dist(a, b) for a, b in zip(TAIL_POINTS, TAIL_POINTS[1:])),
                plannedSupportIntervals=support_intervals,
                limitation='These are authored-input checks, not a substitute for native contact, '
                           'deformation, collision, motion or runtime review.')


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, allow_nan=False)+'\n')


def prepare(directory):
    ops = operations()
    report = inspect_plan(ops)
    envelope = dict(operations=ops)
    report['operationsSHA256'] = hashlib.sha256(json.dumps(envelope, sort_keys=True).encode()).hexdigest()
    write_json(directory/'seed.json', seed())
    write_json(directory/'operations.json', envelope)
    write_json(directory/'preparation.json', report)
    return report


def apply(directory, leave_open):
    workspace = CreatureWorkspace(); command = workspace.command
    run_id = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%d-%H%M%S-%f')
    run = directory/'runs'/run_id
    run.mkdir(parents=True, exist_ok=False)
    original = command('saveStudy', path=str(run/'previous-study.json'))['path']
    completed = False
    try:
        command('project', value='sanctuary')
        command('assetLoad', path=str((directory/'seed.json').resolve()))
        command('pause', value=True)
        with workspace.edit() as transaction:
            author(transaction)
        snapshot = workspace.inspect()
        write_json(run/'craft-inspection.json', snapshot)
        assert set(snapshot['contactChains']) == {leg['id'] for leg in legs()}, 'Six native contact chains required'
        assert CLIP in snapshot['clips'], 'Source-authored clip was not registered'
        assert all(part['vertices'] > 0 for part in snapshot['parts']), 'A generated surface is empty'
        command('rig', value='softbox')
        command('view', value='quarter')
        command('rehearsal', slopeX=0, slopeZ=0, stepHeight=0, stepZ=0, solving=True)
        command('surfaceReview', mode='clay', hiddenParts=[])
        command('creature', mode=CLIP, seconds=0)
        report = workspace.report(samples=61)
        write_json(run/'native-motion-report.json', report)
        rows = report.get('samples', [])
        assert rows and all(len(row.get('contacts', [])) == 6 for row in rows), 'Native report must evaluate every chain'
        residual = max(c['residual'] for row in rows for c in row['contacts'])
        supports = min(sum(c['planted'] for c in row['contacts']) for row in rows)
        assert residual < .004, 'Native reach residual exceeds 4 mm: '+str(residual)
        assert supports >= 3, 'Native support scheduling lost the required tripod'
        for part in ('body', 'tail', 'front-left-upper'):
            write_json(run/('native-deformation-'+part+'.json'), workspace.report(samples=25, part=part))
        source = command('authoring')['authoring']['source']
        write_json(run/'asset.json', source)
        study = command('saveStudy', path=str(run/'clay-study.json'))['path']
        checkpoint = 'Reedwalker unreviewed '+run_id
        command('checkpoint', name=checkpoint)
        write_json(run/'result.json', dict(status='native-source-authored-awaiting-visual-review',
            study=study, checkpoint=checkpoint, source=str(run/'asset.json'),
            maximumContactResidualMetres=residual, minimumPlantedContacts=supports,
            published=False, visualReviewRun=False, runtimePerformanceMeasured=False))
        completed = True
        return run
    finally:
        # Failure restores the prior study even with --leave-open.
        if not completed or not leave_open:
            command('loadStudy', path=original)


def check_cpu(directory):
    """Link existing production objects, without SwiftPM, an app, or GPU work."""
    candidates = [ROOT/'.build/release', ROOT/'.build/debug']
    build = next((p for p in candidates if (p/'FieldCore.build/CreatureCraft.swift.o').exists()
                  and (p/'Modules/FieldCore.swiftmodule').exists()), None)
    if build is None:
        raise RuntimeError('Existing complete FieldCore objects are required; finish the normal build first')
    objects = sorted((build/'FieldCore.build').glob('*.swift.o'))
    run_id = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%d-%H%M%S-%f')
    run = ROOT/'.build/reedwalker-cpu'/run_id
    run.mkdir(parents=True, exist_ok=False)
    binary = run/'contact-check'
    checker = DEFAULT_DIRECTORY/'ContactCheck.swift'
    command = ['xcrun', 'swiftc', '-I', str(build/'Modules'), str(checker),
               *map(str, objects), '-o', str(binary)]
    subprocess.run(command, check=True, cwd=ROOT)
    result = subprocess.run([str(binary), str(directory/'operations.json')],
                            capture_output=True, text=True, cwd=ROOT)
    (run/'result.json').write_text(result.stdout)
    if result.stderr:
        (run/'stderr.txt').write_text(result.stderr)
    # Record the exact binary inputs rather than assuming they match a live build.
    write_json(run/'provenance.json', dict(buildDirectory=str(build.resolve()), operationsSHA256=hashlib.sha256(
        (directory/'operations.json').read_bytes()).hexdigest(),
        checkerSHA256=hashlib.sha256(checker.read_bytes()).hexdigest(),
        coreObjectSHA256={p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in objects}))
    if result.returncode:
        raise RuntimeError('Production CPU contact check failed; inspect '+str(run/'result.json'))
    return run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, default=DEFAULT_DIRECTORY)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--apply', action='store_true', help='Apply to Soundstage after Vesper visual target review')
    mode.add_argument('--check-cpu', action='store_true', help='Run production FieldCore contact math using existing compiled objects')
    parser.add_argument('--leave-open', action='store_true', help='Leave accepted unreviewed clay study open with --apply')
    args = parser.parse_args()
    if args.leave_open and not args.apply:
        parser.error('--leave-open requires --apply')
    report = prepare(args.directory.resolve())
    if args.check_cpu:
        print(check_cpu(args.directory.resolve()))
    elif args.apply:
        print(apply(args.directory.resolve(), args.leave_open))
    else:
        print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
