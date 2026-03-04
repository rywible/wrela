# AAA Rendering Pipeline Fix — Surgical Playwright Debug Loop

## Problem

The forest demo renderer has sophisticated shader code for CSM shadows, SSAO, bloom, god rays, atmospheric sky, fog, and PBR materials — but the visual output shows objects floating in a flat blue void with nearly-black silhouette characters. The rendering infrastructure exists but is broken at the integration level.

## Approach

Sequential Playwright-verified iteration loop. Fix each rendering layer bottom-up, capture screenshots to verify each fix before moving to the next. Subagents do the code work within each layer; orchestrator verifies with Playwright between layers.

## Fix Layers (Priority Order)

### Layer 1: Ground Plane Visibility
- **Symptom**: No ground visible, objects float in void
- **Root cause candidates**: Ground mesh not generated, camera clipping, depth buffer issue, ground material transparent
- **Fix**: Ensure ground mesh renders with visible material, establishes spatial anchor for scene
- **Verify**: Playwright screenshot shows textured ground surface under objects

### Layer 2: Sky Atmosphere
- **Symptom**: Flat light-blue background instead of atmospheric gradient
- **Root cause candidates**: Sky pass not executing, sky uniforms not bound, fullscreen quad not covering viewport
- **Fix**: Get atmospheric scattering shader producing zenith-to-horizon gradient with sun disc
- **Verify**: Screenshot shows visible sky gradient with color variation

### Layer 3: PBR Lighting
- **Symptom**: Materials appear nearly black/dark silhouettes
- **Root cause candidates**: Light direction not reaching surfaces, normal matrix incorrect, albedo textures too dark, exposure/intensity too low
- **Fix**: Ensure directional light illuminates surfaces, materials respond to light, proper HDR intensity
- **Verify**: Screenshot shows lit surfaces with visible material detail and color

### Layer 4: Cascaded Shadows
- **Symptom**: No shadow contribution despite CSM code
- **Root cause candidates**: Shadow atlas not rendering, shadow sampling returning 1.0, cascade splits miscalculated
- **Fix**: Shadow maps render correctly, PBR shader samples them to produce visible darkening
- **Verify**: Screenshot shows character/tree shadows on ground

### Layer 5: Fog and Atmosphere
- **Symptom**: No fog despite scene layout configuring fog volumes
- **Root cause candidates**: Fog parameters not reaching fragment shader, fog color same as background, density too low
- **Fix**: Distance/height fog blends with scene, creates depth layering
- **Verify**: Screenshot shows depth-faded background objects with atmospheric haze

### Layer 6: Post-Processing Chain
- **Symptom**: No bloom, SSAO, god rays, or tonemapping visible
- **Root cause candidates**: Post-processing passes executing but writing to wrong target, bloom threshold too high, SSAO intensity zero
- **Fix**: Enable and tune bloom, SSAO, god rays, ACES tonemap, FXAA
- **Verify**: Screenshot shows soft bloom on bright areas, ambient occlusion in crevices, volumetric light

### Layer 7: Content Polish
- **Symptom**: Bare trunk cylinders, sparse scene, developer-grade HUD
- **Fix**: Improve tree canopy foliage, material quality, scene composition density, HUD visual polish
- **Verify**: Screenshot shows forest that reads as a forest, not floating cylinders

## Success Criteria

Final Playwright screenshot shows:
1. Textured ground plane anchoring the scene
2. Atmospheric sky with gradient and sun
3. Lit, colored PBR surfaces on all objects
4. Visible shadows from characters and trees
5. Atmospheric fog creating depth
6. Post-processing (bloom, AO, tonemapping) visibly improving image quality
7. Scene reads as a dark gothic forest clearing, not a debug scene

## Build/Verify Loop

```
for each layer:
  1. Diagnose root cause (read code, trace data flow)
  2. Implement fix (subagent or direct edit)
  3. cargo check -p wrela_client --target wasm32-unknown-unknown
  4. Build: wrela game build apps/wrela-forest
  5. Serve: wrela game dev apps/wrela-forest
  6. Playwright screenshot capture
  7. Visual verify: does the fix produce expected improvement?
  8. If no: iterate on fix
  9. If yes: move to next layer
```
