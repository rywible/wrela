# Vesper performance source and interval editing

The production acting recipe is
`Games/Sanctuary/Authoring/author_vesper_performance.py`. It sends the same public
revision-checked `CreatureWorkspace` transactions available for any creature.
The phrase, contact plan and arrangement remain editable source. It retains the
previous phrases as explicit alternatives, then selects `vesper-production-performance`.
Its 12 second arrangement is nonlooping; replaying from time zero restarts it.
The final stance retains its diagonal travel and turn.

## Acting intentions

| Interval | Authored intention |
| --- | --- |
| 0–1.55 s | Elevated thorax, quiet deliberate breath and held gaze |
| 1.55–3.65 s | Lower the root about 0.64 m into four fixed paw anchors |
| 3.65–5.9 s | Thorax initiates the coil; head follows with a 0.22 s local delay |
| 5.9–6.45 s | Fore-left diagonal reach and forceful torso release |
| 6.45–7.3 s | Paw plants before the chest's downward catch; head completes its turn |
| 7.3–8.35 s | Three staggered receiving steps support the turned body |
| 8.35–12 s | Shoulder recovery, body settling and quiet final stance |

There are at least three authored supports throughout. The body is kinematically
posed and contact IK solves the registered two-link limbs. The small positional
support assist is capped at 6 cm and 25% strength. Neither the support count nor
that assist demonstrates force, momentum, frictional stability or physical balance.
The guide simulation supplies secondary movement; the score contains no animated
scale and no independent decorative sine oscillations at its attachments.

## Bounded edits

`Tools/AgentTools/motion.py` exposes the native `editInterval` operation:

```python
from motion import interval
with studio.edit() as edit:
    interval(edit, 'vesper-production-performance', start=1.55, end=5.9,
             fade_in=1.3, fade_out=1,
             adjustments=[dict(joint='body', offset=[0,-.10,0]),
                          dict(joint='mask', delay=.15)],
             bounds=[dict(joint='body', channel='y', minimum=-.8, maximum=.03)],
             protected_times=[5.9])
```

The implementation in `FieldCore/MotionIntervalEdit.swift` uses native phrase
interpolation. Offsets and rotations act in each named joint's authored local
frame; positive delay follows that joint's previous motion later. Contacts retain
their exact source keys and clock. Protected times retain the complete original
pose through a smooth correction notch. No anatomy names are inferred.

Reversing the local clock, exceeding an interpolated channel bound, invalid joint
IDs, duplicate controls and source budgets reject the candidate atomically. Error
paths identify the joint, channel, time and excess where applicable. Bounds are
checked at 60 Hz, including between source controls; this does not certify extrema
between ticks. The correction adds controls at up to 10 Hz inside its interval.
Existing controls outside remain unchanged, but spline tangent neighborhoods can
change near new controls. Exact source samples and contact clocks are retained;
this is not exact preservation of every point on the previous continuous spline.
Contact target preservation does not guarantee reach. Run the final contact audit.

## Reproduction and evidence

```sh
python3 Games/Sanctuary/Authoring/author_vesper_performance.py --write .build/vesper-production-operations.json
swift test --filter 'MotionIntervalEditTests|VesperProductionMotionTests'
# After the native app is rebuilt, reopened and Vesper selected:
python3 Games/Sanctuary/Authoring/author_vesper_performance.py
scripts/stagectl poseReport --start 0 --end 12 --samples 121
scripts/stagectl secondaryReport --samples 61
scripts/stagectl saveStudy .soundstage/vesper-production-performance.json
scripts/performance-film --fps 30 --label vesper-production-performance
```

The first actual clay film confirmed correct timing but showed that the opening
stance already looked low and flexed. The source was refined with 20 cm root
height, 7 degrees of body extension, a level head counterpose, a larger gaze
counterturn during the coil and an elevated final recovery. The exact frozen
`connected-current.json` source was evaluated with the release engine's
`AssetSource.evaluatedPose` and `SecondaryPosePlayer` for all 721 ticks. The
candidate opening mask joint rises from 3.000 to 3.572 m; the deepest crouch is
2.107 m and the final mask joint is 3.427 m. Contact residual remains below
0.00000018 m; knee heights remain above 0.433 m. These position differences
establish pose contrast, but the revised actual moving render must assess acting.

The same full-source CPU run exposes about 0.100 m capsule-span overlap at
`mane-guide-15-0 → mane-guide-15-1` through the entire phrase, despite negligible
particle penetration. The anchored first-span envelope deliberately overbounds
its groom profile. This remains an explicit diagnostic requiring rendered root
inspection; it has not been suppressed or counted as proof of clean hair contact.
Maximum guide stretch in this comparison changed from 0.02855 to 0.02279.

The CPU test executes the actual authoring recipe's emitted operations through
`MotionIntervalEdit`, `FootstepPlan`, `MotionPhrase`, `PartRig` and `ContactRig` for
721 samples. It measures final paw reach, planted speed and knee heights and
checks crouch depth and the final turn. It does not inspect the rendered body or
prove clearance of its surface. Real clay, moving renders and game measurements
are still required before making a visual quality or performance claim.

## Friction log

| Desired edit | Obstacle | Capability | Outcome |
| --- | --- | --- | --- |
| Deepen the crouch without moving the paws | Generator edits were overridden by the active authored phrase; editing only root keys disturbed surrounding timing | Native bounded interval edits with retained contact keys and explicit channel limits | The source now lowers about 0.64 m; native reach audit and actual-render review evaluate the result |
| Make the chest lead and the head follow | Existing phrase turned head first; broad shared keys moved the entire chain together | Per-joint local-clock delay within an interval, with monotonic-clock rejection | Production phrase requests a 0.22 s head delay while retaining footfall times; moving visual review must assess its acting |
| Execute a real diagonal step | Previous forepaws lifted and mostly returned to place while torso swayed | Existing reusable footstep compiler, used with displaced landing anchors | First paw lands 0.5 m sideways and 0.4 m forward; exact planted anchors are audited |
| Retain the landing and settle afterward | Loop closure forced extra cosmetic reset steps and undid displacement | Existing nonlooping arrangement plus native timeline endpoint handling | Final body retains 23° root yaw and its receiving stance through 12 s |


Paw orientation is authored as a separate smooth replacement phrase. Each paw
rotates only between its own lift and land events, then holds its heading.
`ContactRig` projects the incoming authored heading into the ground tangent plane
while preserving the sole normal. The 721-tick test also checks that a planted
paw's heading never changes. Tail counterrotation and delayed recovery are
explicitly authored overlap; the tail is not part of the particle simulation.
