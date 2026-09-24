# Performance authoring review — 22 September 2026

The performance editor now supports clip duplication and range trimming, atomic retiming across visual and physical tracks, separate body/facial key selection, collision-safe key moves, editable event timing and typed parameters, interaction target/ramp editing, transition audition through the real runtime, projected motion trails, whole-window contact review and immediate physical playtesting. Clip, event, contact and physical-review components are separate from runtime sampling and authoring transactions.

## Evidence and correction

The Ash Warden walk initially exceeded its existing 25 mm maximum contact-residual threshold at forepaw contact onset (25.086 mm). Lowering the lateral spine roll from 0.013 to 0.008 radians reduced the measured peak to 23.778 mm. No acceptance threshold was relaxed.

An additional playback bug made explicit zero-blend scrubbing use the character's authored idle-to-walk transition. The runtime now gives explicit controller/editor blends precedence; omitting the argument uses authored transitions. A regression test compares every resulting skin-matrix component with the directly sampled clip pose. Removing the unwanted startup crossfade reduced maximum planted slip from 7.509 mm to 1.791 mm across the 145-frame runtime review. Maximum residual remains 23.778 mm. Root motion and physical contacts retain deterministic replay.

## Visual verdict

Reviewed the eight-pose rendered walk contact sheet in `output/authoring-lookdev/1790110916847-18149/source/output/performance-lookdev/v1/walk/contact-sheet.png`. The side silhouette, supported four-beat stride and movement direction are legible. The result is a motion/anatomy blockout, not accepted AAA character output.

Visible deficiencies include thin angular lower legs, abrupt shoulder/thigh mass transitions, a broad torso with little surface variation, stiff head carriage, and dark underside grooming strokes that read as hanging lines. A contact sheet cannot establish timing quality, so motion-video review remains necessary. The captured initial frame also included the accidental idle crossfade fixed above; this capture is not evidence for the corrected startup.

The next bounded visual pass is an identical-camera recapture at 0, 0.05, 0.10 and 0.30 seconds with immediate clip selection, followed by shoulder/hip blending, leg-volume refinement and grooming inspection while preserving planted contacts. Review front/three-quarter views and interaction close-ups before calling the character visually complete. Passing the motion measurements does not substitute for those art decisions.

## Validation

Performance sampling, authoring and preview tests cover exact boundary-pose round trips, cropped and retimed semantic/physical windows, identity-safe duplication, protected key moves, root-loop velocity continuity, unequal-duration blended trail endpoints, short contact intervals, stale preview cancellation, authored transition playback and explicit blend overrides. The real RuntimeSession and Warden physical-review tests pass. Full TypeScript checking and workspace dependency boundaries passed after integration.
