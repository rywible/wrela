# Render speed and quality (deleted when the work is done)

The ask: every idea of the research (the ranked plan and the compiler's levers) implemented, and
each paying off: faster, better, or both, measured.

How each is measured:
- Speed: the native host's serial times per pass over the camera's path (scratchpad `path.py`),
  then Chrome's frames back to back (`sixty_frames_a_second_over_the_path_in_chrome`).
- Quality: the clearing's tests (flicker against plain rendering, upscaled against native,
  levels where they meet, popping, ghosting, coverage), each number before and after.

Where the time went (native serial, path medians by 10 s, ms): grass 0.9–1.8 (2.3 at most), the
trees' prepass 0.6–1.3, cards 0.2–1.6, the look 1.2, temporal AA 0.87, terrain 0.52 + its cook
0.19, far trees 0–0.5, limbs 0–0.57, grass placing 0.3. Found: grass is bound by its vertices'
outputs (21 floats; 10 floats: 1.34 → 0.89 ms), not by shading or the sway's arithmetic.

## Tasks
- [ ] 1 Grass: fewer vertex outputs (a flat index, the rest read where shaded); a blade's tip one
      vertex (a far blade one triangle); one draw per tier; stable fades
- [ ] 2 Hashed alpha in object space: the cards' cut, the levels' cross-fades (`keeps`), the
      impostors' and volumes' dithers, grass and flower fades
- [ ] 3 Fewer cut fragments: cards trimmed to their leaves' outline; the prepass front to back
- [ ] 4 Temporal AA: a 3×3 variance box (statistics at the scene's resolution), the clamp relaxed
      where foliage covers part of a pixel (from the tags), cost no more than now
- [ ] 5 Passes: the hosts fuse consecutive passes on the same targets; stores dropped where
      nothing reads them; transient attachments; one pass for the prepass and shading
- [ ] 6 Compiler: vertex outputs packed and cut (certified f16, recomputed from flat inputs)
- [ ] 7 Compiler: certified f16 in shaders (ranges from intervals), with an f32 fallback
- [ ] 8 Compiler: specialization where a shader branches on a draw's uniform data
- [ ] 9 Compiler: fields band-limited from their gradients (leaf cut, paper grain, dabs)
- [ ] 10 Compiler: interval-pruned kernels for cooking (load, hot reload); certified culling
- [ ] 11 Far trees as voxel bricks cooked from their fields, in place of impostors and volumes
- [ ] 12 4× MSAA on transient attachments (and alpha to coverage), measured; kept if it pays
- [ ] 13 Numbers before and after; vision.md and #26 updated; this file deleted
