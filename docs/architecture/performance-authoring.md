# Performance authoring

Character motion remains the existing joint-key clip format. The optional `character.performance` document adds per-clip semantic events, additive facial joint keys, model-space interaction targets, expected ground contacts, explicit transitions, and a speed-based locomotion blend. Characters without this field retain existing playback.

## Ownership and data flow

- `packages/model/src/performance.ts` owns bounded schemas and cross-reference validation. Performance references existing joints and motion IDs; it does not create another skeleton or clip store.
- `packages/authoring/src/performance.ts` owns immutable key editing, validated performance edits, and atomic retiming. Retiming changes body keys, face keys, events, alignment ramps and contact intervals together.
- `packages/runtime/src/performance.ts` owns deterministic pose evaluation and diagnostics. RuntimeSession uses this evaluator for current and previous clips before its existing creature constraints, articulation, cloth, grooming and physics paths.
- `apps/studio/src/performance-authoring-panel.tsx` edits the same document through standard undoable transactions and existing preview callbacks. A front-projected SVG motion trail and contact measurements use the runtime evaluator.

Compiled characters carry the performance document. Authored event markers feed the existing fixed-step event queue; explicit runtime event registration takes precedence. Transition durations feed existing pose crossfades. Existing snapshots and seek replay remain responsible for runtime state.

## Evaluation conventions

Locomotion samples are sorted by metres per second. The configured speed selects adjacent samples and interpolates their poses; normalized clip phase synchronizes clips with unequal durations. Root accumulation uses each source clip's displacement. Only clips participating in the locomotion blend are affected. The authored speed is the preview default. Controllers can call `RuntimeSession.setLocomotionSpeed(id, speed)` to override it through the recorded input stream. Changes preserve physical root position while changing subsequent stride displacement. Overrides survive replay, saves and dormant actor restoration; they are not automatically inferred from velocity. Sample speed labels should match the authored stride displacement and duration.

Facial keys are additive local joint rotations and translations applied after body sampling. They support a facial skeleton (jaw, lids, eyes, brows), not an imported morph-target format. Existing creature expression controls continue to operate afterward.

Alignment targets use character model space. Each target ramps a root translation correction smoothly before and after the contact time. This aligns a joint without replacing existing creature IK. Multiple targets are applied in authored order. For simultaneous competing constraints, use the creature constraint solver; these root corrections do not solve a full-body interaction system. Runtime actors using a visual root-motion policy intentionally discard root translation and therefore should use their controller/IK path for positional interactions.

Contacts are review annotations with time windows, ground height and tolerance. Diagnostics report vertical error and horizontal slide speed; they do not alter the pose or claim to check terrain collision. They measure the authored performance before procedural IK and physics. Trails show model-space joint trajectories, including the end pose of looping clips.

## Studio workflow

Select a clip and joint, scrub the existing preview, and set body or additive facial keys. Edit duration to retime all tracks as one transaction. Add semantic event cues at the cursor, edit transition durations, and build a locomotion blend from looping clips. Interaction targets reuse the translation fields; mark and adjust ground-contact intervals to inspect sliding and height errors. Stored event payloads support primitive typed values; the compact UI emits a `cue` payload.

The current editor is a working numeric timeline and review panel. Curve tangents, an interactive 3D trajectory overlay, full-body multi-actor alignment, audio waveform editing and facial capture/import remain future work. No visual or gameplay acceptance claim follows merely from passing tests.

## Verification

Focused tests cover phase synchronization with unequal durations, accumulated roots, facial layering, target reach/release, contact stability/sliding, loop-end trails, event boundaries, atomic retiming, key replacement and invalid references. Controller input tests cover root continuity across a speed change, loop boundaries, checkpoint replay, save restoration, malformed speeds and dormant activation. Runtime integration tests cover compiled authored markers, transition duration and evaluated poses through RuntimeSession.

## Coherent timeline editing

Duplicate copies body/face keys, events, contact annotations, interaction targets, physical creature contacts and review scenarios to fresh identifiers. Trim evaluates the retained boundary poses through the runtime quaternion sampler, keeps interior keys, crops contact windows, shifts event/alignment times, and updates physical review duration in one undo transaction. A trimmed clip becomes non-looping and leaves its locomotion blend until the artist reviews its new seam. Events and alignment peaks outside the retained range are removed. Retiming also scales physical creature contact windows and their approach/release ramps; changing only the visual keys would otherwise leave the physical solver on the old timeline.

The body and additive face track selectors keep their values separate when keys share a joint and time. Timeline keys can move without silently replacing another key. Event identifiers, timing and typed parameters are editable; interaction targets expose their contact time, approach, release, strength and all three coordinates. These controls remain split into focused clip, event, blend, contact and physical review components.

The transition audition scrubs or plays the actual recorded fixed-step runtime transition starting at the selected source cursor. Requests are serialized and obsolete requests stop before they can queue another motion. Explicit playback blend values override authored transitions, so a zero-blend clip scrub evaluates the requested clip immediately; omitted values use the authored transition. Authored crossfade durations and the existing physical/contact solver are used. Two control ticks count toward the bounded 120-second preview budget.

Whole-window contact review includes the interval endpoints and even contact windows shorter than one frame, with at most 241 samples per window. It reports maximum height/slide errors and links back to the worst time. Root accumulation makes contact velocity continuous at loop seams, and the first frame uses a forward difference rather than reporting an artificial zero velocity. Blended locomotion trails retain the end displacement of every source clip.

## Lookdev evidence

`bun tools/performance-lookdev.ts --motion=walk --frames=24 --revision=<name>` writes rendered poses, a contact sheet and a movie together with `motion-review.json`: actual 60 Hz runtime foot trajectories, planted slip, contact residuals and solver conflicts. Run `interact` for the facial/interaction performance and `turn` for support changes. Keep the image sequence and runtime measurements together; neither is an artistic approval by itself. The Warden walk's lateral spine sway was reduced from 0.013 to 0.008 radians after the runtime review measured a 25.086 mm contact onset residual. The updated source measured 23.778 mm maximum residual and 1.791 mm planted slip after fixing explicit zero-blend playback, without weakening its 25 mm acceptance thresholds.
