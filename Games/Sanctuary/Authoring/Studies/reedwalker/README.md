# Reedwalker: prepared generalization study

Status: source and CPU contact checks prepared. **The native authoring transaction,
mesh compilation, visual review and runtime measurement have not been run.** Apply
this study after Vesper's visual target review; it is not yet completion evidence.

Reedwalker is an original squat marsh creature with six splayed legs, a broad flat
head, webbed pads and a long bending tail. The torso stands about 0.615 m high; its
tail centerline is 1.90 m long. Four alternating tripod transfers carry it 0.56 m
forward during an eight-second phrase. Torso compression, support sway, head
scouting and delayed tail curves are authored kinematics. No force-driven gait or
tail dynamics is claimed.

The recipe uses 15 anatomy sources and 25 joints. Body, head and tail each own one
source. Every leg owns a continuous upper/lower surface and a paw surface. Regions,
internal rods and blend weights are authoritative field elements. All source,
joint, contact-chain, landmark, anchor and phrase edits fit in one 57-operation
revision-checked public `craft` transaction. No new generator or shared species
condition was added.

From the repository root:

```sh
# Produce seed.json, operations.json and preparation.json; no app connection.
python3 Games/Sanctuary/Authoring/author_reedwalker_study.py

# Production CPU motion/contact checks; links already built FieldCore objects.
# Does not invoke SwiftPM, open an app, render or contend for its build lock.
python3 Games/Sanctuary/Authoring/author_reedwalker_study.py --check-cpu

# AFTER Vesper's visual target review: apply and leave the clay study for inspection.
python3 Games/Sanctuary/Authoring/author_reedwalker_study.py --apply --leave-open
```

`--apply` saves the previous complete study, selects Sanctuary, loads the minimal
seed, applies one atomic transaction, checks all six native contact chains,
samples the native motion and three deformation regions, and writes an unreviewed
source/study under `runs/<timestamp>/`. It creates a named unreviewed checkpoint.
Failure restores the previous study even with `--leave-open`; normal execution
also restores it unless `--leave-open` was requested. It never calls `assetSave`
or writes to published `Assets`. The initial seed's body is replaced by anatomy
within the craft transaction. Existing review baselines and player saves are not
inputs to this script.

## CPU evidence and real corrective iteration

[Passing CPU report](../../../../../.build/reedwalker-cpu/20260912-042634-728994/result.json)
and [exact object/source provenance](../../../../../.build/reedwalker-cpu/20260912-042634-728994/provenance.json):
481 samples, 60 Hz over eight seconds, using production `FootstepPlan`, `PartRig`,
`ContactRig` and `BodySupport`. Maximum contact residual was 7.19e-8 m; maximum
movement between consecutive planted samples was 1.33e-7 m; at least three paws
remained planted. Sampled static COM remained in support. These figures check
source and contact math; they do not establish believable movement, collision
safety, deformation appearance or physical realism.

The [first production-math run](../../../../../.build/reedwalker-cpu/20260912-042544-639921/result.json)
exposed a 59.4 mm unreachable target and 4.36 mm planted drift at the second support
transfer. The intended edit was a low tripod crossing with fixed supporting paws.
The original upper/lower segment lengths prevented the requested reach. Moving
the authored knee pivots outward and upward, and rebuilding their field elements
from those same source pivots, supplied reach without changing planted targets or
timing. The existing public rig/contact model removed the obstacle; the new CPU
result confirms its numerical effect. **Visual improvement remains unverified.**

## Required next review

Inspect actual native clay motion from front, quarter, side, back and above,
including the first and second tripod transfer boundaries. Check the knee fold,
limb-to-body joins, paw contact area, torso volume and tail intersections. Then
restore material review and inspect the face and webbed pads closely under the
required indoor/outdoor and wet/dry conditions. Use a matched study, source
fingerprints and native moving capture. Confirm native Motion controls and exact
paused/future replay. Review recorded deformation metrics without treating their
pass as an artistic assessment. Native compilation, actual-engine integration
and runtime timing remain required before making a generalization claim.
