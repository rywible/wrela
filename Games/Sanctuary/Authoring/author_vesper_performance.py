#!/usr/bin/env python3
"""Install Vesper's 12 second acting source through the native public interfaces.

Run from the repository root after selecting Vesper in Soundstage:
  python3 Games/Sanctuary/Authoring/author_vesper_performance.py
  scripts/stagectl poseReport --start 0 --end 12 --samples 121

--write PATH exports the exact public operations for review or CPU validation
without contacting Soundstage. Publication is explicit with --publish.
"""
import argparse
import copy
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'Tools' / 'AgentTools'))
from creature import CreatureWorkspace
from motion import interval

PHRASE = 'vesper-production-performance'
PAW_PHRASE = 'vesper-planted-paw-heading'
DURATION = 12

# Position is root travel in metres. Chest angles distribute along the authored
# serial thoracic spine. The head has its own score and a native delayed pass.
#             time    x      y      z    bodyP bodyY bodyR chestP chestY neckP headP headY jaw
ACTING = [
    (0.00,    0,   .20,     0,     7,    0,    0,      4,    0,    4,  -11,    3,   -1),
    (0.85,    0,  .208,     0,     7,    0,    0,    4.5,    0,  4.5,-11.5,    3,   -1),
    (1.55,  .025,  .20,   .015,    7,    0,   -.2,     4,    0,    4,  -11,    1,   -1),
    (2.25,  .065,  -.12,  .075,     2,  -1,    .3,     0,   -1,    4,   -6,    0,   -2),
    (3.15,  .025,  -.49,  .125,    -3,  -3,     1,    -8,   -3,    6,    2,    0,   -3),
    (3.65,     0,  -.50,   .11,    -3,  -4,   1.3,    -9,   -5,    7,    3,    2,   -3),
    (4.25,   .04,  -.48,   .12,    -2,  -7,   1.8,    -7,  -12,    5,    4,   11,   -4),
    (5.10,   .11,  -.43,   .13,    -1, -10,     3,    -3,  -18,    3,    3,    0,   -6),
    (5.65,  .075,  -.39,   .09,     0,  -9,   2.2,     0,  -16,    0,    2,  -13,   -9),
    (5.95, -.015,  -.28,  -.02,     1,  -4,   -.5,     4,   -4,   -1,    0,  -10,  -13),
    (6.25,  -.21,  -.12,  -.12,    .5,   5,  -3.5,     8,   15,   -3,    0,    0,  -23),
    (6.45,  -.34,  -.17,  -.19,  -1.5,  11,  -4.5,     3,   21,   -6,   -1,    9,  -25),
    (6.70,  -.35,  -.33,  -.22,    -4,  15,  -2.8,    -8,   22,   -4,    2,   16,  -19),
    (7.08,  -.29,  -.29,  -.24,    -2,  18,  -1.1,    -6,   14,    2,    4,   10,  -12),
    (7.65,  -.23,  -.20,  -.25,    -1,  20,   .6,     -2,    5,    5,    2,    1,   -7),
    (8.35,  -.24,   .10,  -.26,     3,  22,    1,      4,   -1,    3,   -5,   -5,   -3),
    (9.10,  -.25,   .24,  -.26,     6,  23,   .3,      4,   -1,    3,   -9,   -3,   -1),
    (9.85,  -.25,   .15,  -.26,     5,  23,    0,      2,    0,    3,   -8,   -1,   -1),
    (10.6,  -.25,   .18,  -.26,     5,  23,    0,      2,    0,    2,   -7,   -1,   -1),
    (11.2,  -.25,  .184,  -.26,     5,  23,    0,    2.2,    0,  2.2, -7.2,   -1,   -1),
    (12.0,  -.25,   .18,  -.26,     5,  23,    0,      2,    0,    2,   -7,   -1,   -1),
]

ANCHORS = {'fore-left': [-.93, .18, -1.3], 'fore-right': [.93, .18, -1.3],
           'hind-left': [-.87, .18, 1.45], 'hind-right': [.87, .18, 1.45]}
# The first paw anticipates the torso impulse and accepts it at 6.45 s. The
# other paws step once to receive the turned body; no grounded anchor slides.
STEPS = [
    dict(chain='fore-left', lift=5.90, land=6.45, target=[-1.43, .18, -1.70], height=.36),
    dict(chain='fore-right', lift=6.64, land=7.08, target=[.43, .18, -1.35], height=.26),
    dict(chain='hind-left', lift=7.30, land=7.73, target=[-.45, .18, 1.14], height=.20),
    dict(chain='hind-right', lift=7.92, land=8.35, target=[1.02, .18, .60], height=.22),
]
BEATS = [
    dict(name='Attend', time=0, intent='A quiet breath and a held gaze establish deliberation.'),
    dict(name='Compress', time=1.55, intent='The body sinks deeply into four fixed paw anchors.'),
    dict(name='Coil', time=3.65, intent='The thorax turns first; the head follows the chest and fixes its intent.'),
    dict(name='Drive', time=5.90, intent='The near forepaw reaches diagonally as the torso releases.'),
    dict(name='Catch', time=6.45, intent='The planted diagonal paw accepts the body; the chest compresses after impact.'),
    dict(name='Recover', time=7.30, intent='The remaining paws receive the turning body with staggered support.'),
    dict(name='Settle', time=8.35, intent='The shoulders rise, then settle while mane and cloth continue their motion.'),
    dict(name='Still', time=10.60, intent='The changed stance remains quiet; the character holds the ground it claimed.'),
]


def pose(offset=(0, 0, 0), rotation=(0, 0, 0)):
    return dict(offset=list(offset), rotation=list(rotation), scale=[1, 1, 1])


def samples():
    result = []
    for t, x, y, z, bp, by, br, cp, cy, np, hp, hy, jaw in ACTING:
        values = {
            'body': pose((x, y, z), (bp, by, br)),
            'haunch': pose(rotation=(-bp*.55, -by*.18, -br*.3)),
            'spine-0': pose(rotation=(cp*.22, cy*.24, -br*.12)),
            'spine-1': pose(rotation=(cp*.34, cy*.34, -br*.16)),
            'spine-2': pose(rotation=(cp*.44, cy*.42, -br*.20)),
            'neck': pose(rotation=(np, cy*.12, br*.2)),
            'mask': pose(rotation=(hp, hy, -hy*.07)),
            'jaw': pose(rotation=(jaw, 0, 0)),
            # Simulation drives the fine overlap. These attachment poses avoid
            # inheriting the older generator's unrelated continuous swaying.
            'mane': pose(), 'beard': pose(), 'tail': pose(rotation=(0, -by*.35-cy*.6, 0)),
            'mantle': pose(), 'trim': pose(), 'horns': pose(), 'eyes': pose(),
        }
        result.append(dict(time=t, poses=values))
    return result


def author(transaction):
    contacts = [dict(chain=name, keys=[dict(time=t, position=p, planted=True) for t in (0, DURATION)])
                for name, p in ANCHORS.items()]
    references = [dict(id='intent-'+b['name'].lower(), locator='authored://vesper/production-performance',
                       sourceTime=b['time'], phraseTime=b['time'], note=b['intent'], landmarks={}) for b in BEATS]
    transaction.phrase(PHRASE, DURATION, samples(), contacts=contacts, references=references)
    transaction.footsteps(PHRASE, STEPS, minimum_support=3)
    heading_times = sorted({0, DURATION} | {s[k] for s in STEPS for k in ('lift', 'land')})
    headings = []
    for t in heading_times:
        values = {}
        for step in STEPS:
            u = max(0, min(1, (t-step['lift'])/(step['land']-step['lift'])))
            # Smooth interpolation of this sparse layer keeps the heading exact
            # throughout every planted interval and only turns during the swing.
            yaw = 23 * u*u*u*(10+u*(-15+6*u))
            values[step['chain']+'-paw'] = pose(rotation=(0, yaw, 0))
        headings.append(dict(time=t, poses=values))
    transaction.phrase(PAW_PHRASE, DURATION, headings, contacts=[], references=[], interpolation='smooth')
    # These are the same public interval requests an agent can make later after
    # seeing a posed defect. The landing clock and all paw paths are preserved.
    interval(transaction, PHRASE, start=1.55, end=5.90, fade_in=1.3, fade_out=1.0,
             adjustments=[dict(joint='body', offset=[0, -.14, 0])],
             bounds=[dict(joint='body', channel='y', minimum=-.70, maximum=.30)],
             protected_times=[5.90])
    interval(transaction, PHRASE, start=3.65, end=8.35, fade_in=.80, fade_out=1.0,
             adjustments=[dict(joint='mask', delay=.22)],
             bounds=[dict(joint='mask', channel='yaw', minimum=-18, maximum=19)],
             protected_times=[8.35])
    interval(transaction, PHRASE, start=3.65, end=10.6, fade_in=1.0, fade_out=1.2,
             adjustments=[dict(joint='tail', delay=.30)],
             protected_times=[10.6])
    transaction.set('arrangement', dict(clip='procession', duration=DURATION, looping=False,
        layers=[dict(id='vesper-finished-acting', phrase=PHRASE, start=0, end=DURATION,
                     sourceStart=0, sourceEnd=DURATION, fadeIn=0, fadeOut=0, weight=1, additive=False),
                dict(id='vesper-paw-heading', phrase=PAW_PHRASE, start=0, end=DURATION,
                     sourceStart=0, sourceEnd=DURATION, fadeIn=0, fadeOut=0, weight=1, additive=False)]))
    # A bounded positional support assist remains subordinate to authored travel.
    # It is kinematic adjustment, with no mass/force realism claim.
    transaction.balance('body', strength=.25, maximum_shift=.06, transition_seconds=.18)


def operations():
    class Recorder:
        def __init__(self): self.operations = []
        def operation(self, op, **values): self.operations.append(dict(op=op, **values)); return self
        def phrase(self, key, duration, values, contacts, references, interpolation='spline'):
            return self.operation('upsert', collection='phrases', value=dict(id=key, duration=duration,
                samples=values, contacts=contacts, references=references, interpolation=interpolation))
        def footsteps(self, key, steps, minimum_support):
            return self.operation('footsteps', key=key, steps=steps, minimumSupport=minimum_support)
        def set(self, collection, value): return self.operation('set', collection=collection, value=value)
        def balance(self, joint, strength, maximum_shift, transition_seconds):
            return self.set('balance', dict(joint=joint, strength=strength, maximumShift=maximum_shift,
                                           transitionSeconds=transition_seconds))
    recording = Recorder(); author(recording)
    return copy.deepcopy(recording.operations)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', type=Path)
    parser.add_argument('--publish', action='store_true')
    args = parser.parse_args()
    if args.write:
        args.write.parent.mkdir(parents=True, exist_ok=True)
        args.write.write_text(json.dumps(dict(operations=operations(), performance=dict(
            clip='procession', duration=DURATION, beats=BEATS, tracks=[])), indent=2)+'\n')
        print(args.write.resolve()); return
    workspace = CreatureWorkspace()
    with workspace.edit() as transaction:
        expected = {'body', 'neck', 'mask', 'spine-0', 'spine-1', 'spine-2'}
        actual = {j['id'] for j in transaction.snapshot['joints']}
        if not expected <= actual:
            raise ValueError('Select Vesper with the authored thoracic chain before installing its performance')
        author(transaction)
    print(json.dumps(transaction.result, indent=2))
    if args.publish:
        workspace.save()


if __name__ == '__main__': main()
