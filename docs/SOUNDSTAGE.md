# Soundstage authoring workshop

The soundstage is the workshop for fields, materials, lighting and simulation.
It uses the game's field compiler, meshlets, surface shaders, atmosphere and wind.
Build with `./scripts/build`, then open `.build/Soundstage.app`. `scripts/soundstage`
combines build/open. Its command directory is `.soundstage`; the game's is
`.sanctuary`. Run one renderer at a time when measuring performance.

## Projects and shared authoring

Select **Sanctuary** or **The Last Survey** in Object → Project, or use
`stagectl project sanctuary` / `stagectl project cave`. The catalog, anatomy
controls and ranges, motion clips, behavior scenarios, material recipes and art
direction all follow the selected project. Changes to a working study are retained
when switching projects; undo/checkpoints stay within their own project.
`catalog` is the authority for available parameters and named views.

Use `shape --set KEY VALUE` and `motion --set KEY VALUE` for project-defined
controls (repeat `--set` for several values). Existing tree/Frostling convenience
flags remain supported. Canonical assets live at `Games/<Project>/Authoring/Assets`.
Studies record the project; loading a study for another project requires switching
first. Older Sanctuary studies remain supported.

The game apps now contain gameplay only. Use this standalone Soundstage for all
object, animation and lighting inspection. Game commands use `gardenctl` or
`gamectl --project cave`; authoring uses `stagectl`.

For enclosed environments, generators can supply interior views. Cave provides
`passage`, `junction` and `chamber`. Pair them with `rig flashlight`: the camera
and light sit inside the authored field. Standard views restore object orbit.
See [Cave workflow](CAVE.md) for anatomy, encounters and environment editing.

## Start here

```sh
./scripts/stagectl catalog
./scripts/stagectl subject tree
./scripts/stagectl rig softbox
./scripts/stagectl view quarter
./scripts/stagectl pause true
./scripts/stagectl saveStudy tree-before.json
./scripts/stagectl shape --height 5.8 --width 6.8 --trunk 0.27 --spread 1.2 --leaves 2200
./scripts/stagectl capture --label tree-shaped
./scripts/stagectl lighting sunset
./scripts/stagectl view crown
```

`catalog` lists subjects, rigs, views, field operations and parameter ranges.
`status` includes `workshop.document`, dimensions in metres, physical camera/light
positions, mesh-build time and source path. Captures include that same exact-frame
state and source/shader fingerprints. A capture acknowledgement means the PNG and
metadata were written, not merely requested.

The native inspector has **Object**, **Parts**, **Motion**, **Behavior**, **Lighting**, **Look** and **Files** sections.
The 16:9 viewport occupies its own space. Camera, fit, history, playback and review
controls remain visible in a fixed dock. Every slider has an editable numeric value
and a reset button; press Return or leave the field to apply a value. Invalid
values leave the last valid state intact. Drag the viewport to orbit, scroll to zoom,
R to fit, P to play/pause, F to capture and review, H to hide/show the inspector, Tab to switch sky
and the current object. Reopening the app restores `.soundstage/last-study.json`.
This preserves an unsaved working study; it does not implicitly publish an asset.

## Subjects and shape authoring

Built-ins: **tree, seed, stone, branch, calibration, sky**. Saved field assets in
`Games/Sanctuary/Authoring/Assets/*.json` join the subject catalog automatically. `stone-vessel`
is a complete editable example. `studioObject` remains an alias of `subject`.

A tree includes the actual trunk, canopy and generated leaf surfaces. Tree and
branch parameters are growth height, crown width, trunk radius, branch spread and
leaf count. Growth height/width control the field recipe; rounded roots and leaves
extend beyond them. The panel reports the actual compiled dimensions separately.
Edits compile a candidate before replacing the visible subject. Invalid edits
preserve the previous subject. The last six compiled recipes are cached. Shape
edits preserve physical camera distance; **Fit subject** deliberately reframes.

```sh
./scripts/stagectl assetLoad Games/Sanctuary/Authoring/Assets/stone-vessel.json
./scripts/stagectl assetReload
./scripts/stagectl assetSave
```

`assetLoad` accepts an absolute path or a workspace-relative path. `assetSave`
writes `Games/Sanctuary/Authoring/Assets/<id>.json`, including current roughness, metallic and tint
overrides. `assetReload` reloads that canonical saved path. Imported paths stay attached to the working subject and are recorded in studies.
Automatic reload watches that exact path. **Save asset** publishes to the canonical
asset catalog and switches the watcher to the published path.

The tree recipe and material overrides are consumed by the garden on startup or
`./scripts/gardenctl reloadAssets`. The status field `gameTreeParameters` confirms
which recipe the garden compiled. Existing instances retain their deterministic
placement and seasonal tint variation. Other custom assets are available through
`AssetSource.load(id).compile()` for game assembly; saving a subject does not
invent placements for it in the garden.

Generic field assets require no Swift rebuild. A minimal source is:

```json
{
  "version": 1,
  "id": "rounded-stone",
  "name": "Rounded stone",
  "generator": "fields",
  "parts": [{
    "name": "Body",
    "field": {"op": "sphere", "values": [0.45]},
    "color": [0.55, 0.51, 0.43],
    "material": 5,
    "roughness": 0.72,
    "metallic": 0
  }]
}
```

Fields are in **metres**. Operations and ordered values:

| Operation | Values | Children |
| --- | --- | --- |
| sphere | radius | none |
| box | half extent x, y, z | none |
| capsule | start x,y,z; end x,y,z; radius | none |
| torus | major radius, minor radius | none |
| move / stretch | x, y, z | one field |
| scale | uniform scale | one field |
| union / subtract | none | two fields |
| blend | blend radius | two fields |

`children` and `values` may be omitted when empty. Parts default to neutral color,
material 7, roughness .7 and metallic 0. Material kinds: 4 bark, 5 stone, 6 seed,
2 canopy, 8 leaves, 7 plain PBR. Generic parts use the same compiler as Swift
fields; new operations or specialized Swift generators still require rebuilding.
Supported bounds span 1 mm...100 m, at most 32 parts, source size under 256 KiB,
at most 256 field nodes in total, and expression depth under 24. Empty surfaces fail before replacing the subject.

## Independent lighting and environment

`rig outdoor` uses the selected procedural sky. `lighting noon|golden|sunset|
afterglow|overcast|rain` selects a sky and switches outdoors. Selecting a new subject
preserves the lighting. Indoor presets are **softbox, overhead, lamp, room**;
`lighting indoor` is a compatibility alias for the room setup.

```sh
./scripts/stagectl rig lamp --x -0.85 --y 0.35 --z 1.1 --intensity 3 --size 0.035 --warmth 0.9 --fill 0.005
./scripts/stagectl rig overhead
./scripts/stagectl rig room
./scripts/stagectl rig outdoor
./scripts/stagectl sky --altitude 4 --coverage 0.55 --haze 0.9
```

Light position and source radius are **multiples of the subject's largest extent**,
relative to its bounds center, independently of the inspection target. Thus a preset works on a 35 mm seed and a tree.
Status also reports light position in metres. Intensity is a relative photometric
control, not lux calibrated. Warmth blends neutral to warm RGB. Fill is explicit
uniform ambient light. The room adds enclosing geometry; lamp/overhead/softbox
use an open stage with a finite key light and inverse-square attenuation.

All rigs share PBR shading and the shadow pass. Finite lights use a perspective
shadow map aimed at the subject. Source size broadens highlights and filters
shadows approximately; it is not an area-light visibility integral. Occlusion
outside that shadow frustum, full room GI and accurate reflected room geometry
are not implemented. Calibration spheres expose these limits. Do not use their
appearance as proof of physically calibrated lighting.

## Inspection and material studies

```sh
./scripts/stagectl view front
./scripts/stagectl view back
./scripts/stagectl view above
./scripts/stagectl view base
./scripts/stagectl view detail
./scripts/stagectl view crown
./scripts/stagectl layout --reference true --arrangement grove
./scripts/stagectl camera --distance 13
./scripts/stagectl layout --roughness 0.35 --metallic 0 --scale 1.2
./scripts/stagectl parameters --wetness 1 --wind 2
```

Views also include quarter, left, right, sunward, away and zenith. Sky views hide
the object; an object view returns to the current asset. The camera orbits the
subject's bounds, with close-up focus at its base, center or crown. Camera distance
accepts `camera --metres 3` for a physical distance. Legacy `--distance` is in
quarter-extents; both stop before the 450 m far plane. Status reports metres.
`layout focus` is the target height fraction, 0...1. Scale is an inspection/instance
scale; it is saved in a study, not baked into asset source. The fixed 1.72 m post
provides a human eye-height reference.

Open object rigs use an analytic infinite ground plane: it receives the same PBR lighting
and shadows as the subject, with filtered grid lines and outdoor aerial perspective.
There is no platform edge. The room retains its enclosed floor and walls. This
is a flat authoring surface, not streamed terrain or a curved Earth simulation.
The ground grid's spacing is the largest power of ten metres no larger than the subject extent. Grove repeats three copies
at different orientations, exposing repetition and neighboring shadows.

Roughness/metallic use **−1 to inherit the asset** in the protocol. The native
**Use asset material** button restores inheritance. Tint is a scalar multiplier.
The calibration subject supplies four dielectric roughness samples and a polished
metal sample. Wetness and exposure remain independent environment controls.

## Save, compare and replay

```sh
./scripts/stagectl saveStudy tree-before.json
./scripts/stagectl loadStudy tree-before.json
./scripts/stagectl layout --turntable true
./scripts/stagectl pause false
./scripts/stagectl pause true
./scripts/stagectl step --frames 60

./scripts/workshop-study --baseline .soundstage/studies/tree-before.json --label tree-review
./scripts/workshop-study --views quarter --lights sunset --rigs '' --motion 8 --step 15 --sweep --label tree-motion
```

Relative study paths go under `.soundstage/studies`; absolute paths are accepted.
Studies contain the complete field source, rig, sky, materials, layout, camera,
clock and vegetation wind state. They restore exact paused rendering and future
fixed-step wind/turntable motion. Cloud weather is deterministically reconstructed
from clock and sky settings. Studies support simulation times through 3600 seconds.

`workshop-study` compares the baseline with the current workshop across matching
views and light conditions. Baseline and current share the baseline clock and
wind state; the comparison preserves physical camera distance and target height
where the subject bounds allow it. It restores the original working state even after a
failed capture. Reports archive their own PNGs, exact-frame metadata and complete replay studies.
They open automatically in a native Soundstage review window, without a web server.
Use `--no-open` for unattended runs. Reports include an HTML wipe viewer with zoom, condition selection,
motion scrubbing/playback and exact metadata. Motion images are **paused exact-time
renders**: useful for shape/wind/camera comparisons, but not evidence of live sky
cache stability. Use `check-sky-motion` for that separate check. Default playback
is four preview frames per second; the report records the actual simulated spacing.

`study --sky-only` and `replay <commands.json>` remain available. Run only one
script that changes the workshop at a time. Capture studies restore saved state;
profiling does not rewind time.

## Verification

```sh
swift test
.build/Soundstage.app/Contents/MacOS/Sanctuary --check-renderer
./scripts/validate-stage
./scripts/validate-workshop
./scripts/validate-authoring
```

The workshop checks cover field edits, camera preservation, atomic rejection,
exact study and future-motion replay, custom field loading, material changes,
lighting changes and calibration subjects. Use actual native controls and inspect
captures as well: a passing protocol test cannot judge a tree's silhouette.

## Short performance and traversal checks

```sh
./scripts/profile --stage --seconds 20 --label sunset
./scripts/profile --stage --seconds 20 --counters --label sunset-passes
./scripts/profile --seconds 20 --label garden
./scripts/stagectl pause true
./scripts/stagectl verifySky
```

The profiler warms live updates for three seconds, clears old timing history,
then records whole-frame GPU time, frame intervals, thermal state and submitted FPS to a JSON
report under `.soundstage/profiles` or `.sanctuary/profiles`. It restores pause
and profiling settings afterward. Keep the scene, camera and lighting fixed;
run one app and no builds/captures during measurement. This is a short workload
check, not a sustained thermal guarantee. Submitted FPS is not display-present
telemetry. Compare matching settings and report normal live update frames.

`--counters` enables Metal stage-boundary timestamps on one frame in five.
These are real per-stage intervals; stages can overlap or wait for dependencies,
so their sum is not frame duration. The GPU clock is correlated with Metal's CPU
nanosecond timestamps before and after a sampled frame. Counters have overhead;
use counter-free runs for final frame-budget claims. `frameGPUBySkyPhase` remains
whole-frame time grouped by sky update phase, not a per-pass measurement.

`verifySky` requires a paused rendered sky. It tests 131,072 candidate rays and
nine points per certified skip against the full GPU density reconstruction.
Check a cloudy preset, other seeds, evolved time and high wind. Zero certified
segments is not meaningful coverage. The conservative max hierarchy and warp
bounds supply the reasoning; the numerical probe catches implementation errors.

`scripts/check-sky-motion` waits for an aged completed snapshot, captures live
motion, pauses for a fresh reference, and saves a comparison report and HTML.
Use `--garden` for the game, or `--age` to override the default age threshold
(18 seconds at low sun, nine seconds in daylight). It preserves pause/profiling
settings; simulation time advances and is not rewound. The report records the
small capture-time difference. Inspect the silhouettes as well as displacement:
a low mean pixel error can hide objectionable outlined edges. Run without other
profilers or capture sequences, and check sunward and zenith at stronger wind.

Latest measured results and acceptance evidence: `SKY_PERFORMANCE.md`.


## Everyday edit history and recovery

**Undo/Redo** and **⌘Z/⇧⌘Z** cover native edits, agent edits, source reloads,
camera gestures, lighting and the scene look. A drag/scroll gesture becomes one
history entry. History stores full study snapshots, including simulation state,
and retains the last 50 edits for the current session. A new edit clears redo.
Undo does not rewrite published asset files or the published project look; it
restores the working preview. Publish again to change the shared source on disk.

The **Files** section saves named checkpoints and restores them without a file
picker. Checkpoints persist across sessions under `.soundstage/checkpoints`.
Restoring a checkpoint is itself undoable. Names are unique to avoid silently
replacing a useful checkpoint. Ordinary Save/Load study remains available for
choosing arbitrary paths.

```sh
./scripts/stagectl checkpoint 'Tree before branching'
./scripts/stagectl shape --spread 1.5
./scripts/stagectl undo
./scripts/stagectl redo
./scripts/stagectl restoreCheckpoint 'Tree before branching'
```

Automatic source reload is on by default. It watches the current JSON field
source, waits about 450 ms after the last detected file change, then validates and
compiles a candidate. It preserves view orientation and physical camera distance.
Invalid/deleted source files retain the last valid object and show an error. Each
stable revision is attempted once. Undoing a reload stays undone until another
file revision arrives. Specialized Swift generators still require a rebuild.

```sh
./scripts/stagectl assetLoad Games/Sanctuary/Authoring/Assets/stone-vessel.json
./scripts/stagectl watchSource false
./scripts/stagectl watchSource true
```

## Frozen render baselines and one-action review

**Capture & review** (or F) captures the current view at an exact paused time,
archives the pixels and replay state, restores the working scene and opens the
native review. No manual server or browser setup is required. **Open review**
returns to the last report. Use the explicit Baseline/Split/Current buttons or the
wipe slider; the buttons are convenient for native computer-use clients.

In **Files**, enter a name and click **Pin review** to make the last review a
baseline. Select it in **Review baseline**; choose **None** to capture without a
comparison. A single-frame baseline compares its frozen pixels with the current
working view, so changes to objects, lighting, sky, camera or global art direction
remain visible. Exact settings on both sides are available in the report.

A multi-frame baseline replays its archived objects/cameras/lighting/wind with the
current renderer and current scene look. It does not re-render the baseline side.
Archived style boards preserve all reference subjects. Older pre-v2 reports lack
full per-frame wind state and cannot be used for exact multi-frame replay; their
stored pictures remain viewable.

```sh
./scripts/stagectl captureReview
# captureReview starts a job; poll status.workshop.authoring.reviewRunning/status.
./scripts/stagectl pinBaseline 'Approved lighting experiment'
./scripts/stagectl selectBaseline 'Approved lighting experiment'
./scripts/stagectl selectBaseline None

./scripts/workshop-study --current --label before
./scripts/workshop-study --current --reference <report.json> --label after
./scripts/workshop-study --reference <report.json> --label renderer-replay
```

`workshop-study` uses an automation scope to keep capture setup out of undo history
and suspend file reloads during a review. It restores the original working study
and ends that scope in its failure path too. Do not run concurrent mutation scripts.
The native launcher rejects overlapping reviews and restores its starting study
if the subprocess fails. `stagectl automation false` clears a scope if an externally
launched script was forcibly killed before its cleanup could execute.

## Shared art direction: objects, lighting and sky

`Games/Sanctuary/Authoring/ArtDirection.json` is the project-wide source of the art brief, working
palette, reference subject list and `renderLook`. The **Look** inspector previews
that shared rendering layer across every object, light and sky in the scene:

- **Surface detail**, **roughness bias** and **material color** act in shared PBR
  shading, before illumination. Individual assets keep their material identity.
- **Sun strength** scales outdoor direct light, atmosphere radiance, sky
  reflections and the solar disk consistently. **Ambient strength** controls
  diffuse sky/room fill. **Sky color** affects atmospheric radiance and its
  contribution to surface lighting, reflection and distant ground.
- **Overall color**, **contrast**, **warmth grade** and **glare** operate on the
  complete HDR image. They therefore apply to objects, lighting and clouds together.

These controls do not rebuild the physical atmosphere lookup or remesh objects.
The Hillaire atmosphere remains the underlying simulation; the profile is a
purposeful art-direction layer above it.

Preview edits are undoable and saved in studies. **Publish project look** writes
`renderLook` into the shared art-direction file while preserving the brief and
references. Both the game and workshop watch that file and apply valid changes.
Malformed profile edits keep the last valid look. A saved study can intentionally
restore an older look in the workshop preview; publishing is a separate action.

```sh
./scripts/stagectl look --detail 0.8 --roughnessBias 0.025 --surfaceSaturation 0.96
./scripts/stagectl look --sunStrength 1 --ambientStrength 1.08 --skySaturation 0.98
./scripts/stagectl look --saturation 1 --contrast 0.98 --warmth 0.015 --bloom 0.25
./scripts/stagectl publishLook
./scripts/stagectl styleBoard
./scripts/workshop-study --style-board --label cohesion
```

**Style board** renders the candidate next to the configured working references,
plus dedicated sky views and a studio lighting calibration subject. Noon, sunset
and softbox groups share the same scene look. Assets are individually framed and
labeled with their real height, so compare shape/detail language rather than
inferring relative scale from tile size. The art brief and palette travel with the
report. Click any tile to inspect it at full size.

The first references are explicitly provisional. Global shading and grading can
unify a scene, but cannot repair incompatible silhouettes, animation, cloud massing
or detail hierarchy. Those remain source-authoring decisions. Use the style board
to critique them and replace the working references as the game's direction matures.

## Creature anatomy, animation and behavior

Use the **Authoring section** menu to choose Object, Parts, Motion, Behavior,
Lighting, Look or Files. These all inspect the same subject and lighting.

Start with `stagectl subject frostling`. Its canonical document is now a short
**anatomy preset**, not an expanded field tree. Object exposes head size, cheeks,
body proportions, ear length/width/splay, eye size/spacing, paws and tail. Eyes are
seated against the head surface when proportions change; facial features and ears
follow the head frame. `Game/FrostlingRecipe.swift` is the readable Swift recipe
that expands these controls into named fields. New species can supply different
recipes; this is one creature family, not a universal anatomy generator.

```sh
./scripts/stagectl subject frostling
./scripts/stagectl shape --headSize 1.1 --cheeks 1.02 --earLength 0.92
./scripts/stagectl creature --mode idle --seconds 0
./scripts/stagectl view front
./scripts/stagectl rig softbox
./scripts/stagectl assetSave
```

**Parts** selects stable IDs, isolates a part together with its attachments, and
frames that selection. Offset, rotation, scale, pivot and parent are editable.
Eyes, ear linings and nose inherit their parent's complete transform. Attachment
cycles, missing parents, duplicate identities and invalid bounds are rejected
before replacing the current object. The generated creature stores exceptional
edits as compact `partOverrides`; ordinary anatomy authoring needs no pivots.
Raw field assets can still provide optional `joint` records with `id`, `parent`,
`pivot`, `offset`, `rotation` (XYZ degrees) and uniform `scale`. All fields share
bind coordinates in metres; joints transform around bind pivots, not mesh centers.

```sh
./scripts/stagectl part head --isolated true
./scripts/stagectl framePart
./scripts/stagectl partEdit --yaw 12
./scripts/stagectl part all --isolated false
```

**Motion** offers bind pose, idle, hop and behavior-driven animation. Play/pause,
slow playback, timeline scrubbing, previous/next frame and restart all work on the
same recipe used in Sanctuary. In hop mode the slider spans one cycle. Tune hop
duration, stride, height, crouch, ear follow-through, breath, blink frequency and
idle attention, then Save motion recipe. Head attention settles between gaze
changes; blinks have variable spacing and occasional double blinks. The eyes
close geometrically, with correct normals under nonuniform transforms. Paw
support phases hold still; movement and turning occur in the flight phase.

```sh
./scripts/stagectl creature --mode hop --seconds 0.4 --rate 0.25
./scripts/stagectl motion --height 0.16 --crouch 0.06 --attention 0.65
./scripts/stagectl motionReview
```

Capture motion strip / Review behavior sequence creates a scrub-friendly native
HTML review with exact per-frame studies and metadata. It samples both front and
side views under the current rig, retaining custom light positions and strength.
A hop strip covers a full cycle regardless of playback speed; idle covers five
seconds, and behavior eleven seconds from the current time. The command-line
equivalent adds `--creature-sequence --motion 12`.
Existing `workshop-study --motion ... --step ...`
works for arbitrary combinations of views, wetness and lighting. A motion or
attachment-only edit reuses the compiled geometry. Anatomy changes compile a
candidate; every successful edit supports undo/redo and exact study replay.

**Behavior** runs the actual shared brain with deterministic scenarios: calm
visitor, startle, recovery, occluded visitor, food and an obstacle. Restart, seek,
step or Run 10 seconds; choose a seed to vary excursions. Frame encounter shows
the whole test area; Follow creature tracks the actor with the camera. Blue/red
marks a calm/running visitor, green food, gray an obstacle, orange the landing
target. The inspector exposes fear, current decision and recent reasons; status
and captured metadata retain the bounded decision trace. Manual visitor position,
running and visibility override the scripted stimulus; seeking replays that
constant override from the beginning. Restart scenario restores scripted inputs.

```sh
./scripts/stagectl creature --scenario recovery --seconds 4 --follow false
./scripts/stagectl stimulus --x 0 --z 3 --running true --visible true
./scripts/stagectl step --frames 120
./scripts/validate-creatures
```

Studies include the complete creature clock, seed, brain, hop state, selection and
stimulus. Loading a study restores both the picture and future decisions. Game
saves preserve the same simulation; older expedition saves initialize the actor
at the correct den/home. `gardenctl reloadAssets` reloads its published anatomy
and motion. The game supplies terrain and solid traversal checks and line of
sight independently of the player's camera direction.

Current limits: rigid articulated parts, not skinning, muscle simulation or
terrain foot IK; one bounded creature behavior with simple steering, not a full
navigation/ecology system. Material 9 is a filtered short-coat surface plus an
approximate grazing sheen, not individual hairs. The visual target remains cute,
soft and painterly. Avoid protruding eyeballs, segmented ears and busy mouth
anatomy when refining this character.
