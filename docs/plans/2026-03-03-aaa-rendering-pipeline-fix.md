# AAA Rendering Pipeline Fix — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the forest demo's broken rendering pipeline so that the existing shader infrastructure (CSM shadows, SSAO, bloom, god rays, atmospheric sky, fog, PBR materials) actually produces visible AAA-quality output.

**Architecture:** Sequential bottom-up fix of 7 rendering layers, each verified with a Playwright screenshot before moving to the next. The rendering pipeline code exists but is broken at the integration level — objects float in a flat blue void despite sophisticated shader code being present.

**Tech Stack:** Rust, WebGPU/WGSL, wasm-bindgen, Playwright (via MCP), Wrela compiler CLI

---

## Build/Verify Loop (Used After Every Fix)

Every task ends with this verification sequence:

```bash
# 1. Compile check (fast, catches type errors)
cargo check -p wrela_client --target wasm32-unknown-unknown

# 2. Build the game (compiles WASM, generates manifests, copies assets)
cd /Users/ryanwible/projects/wrela && cargo run -p wrela -- game dev apps/wrela-forest
# This starts the dev server on 127.0.0.1:8091

# 3. Playwright screenshot capture (in a separate terminal/process)
# Use Playwright MCP to navigate to http://127.0.0.1:8091
# Wait for boot overlay to clear (poll window.render_game_to_text())
# Take screenshot and save to .artifacts/aaa-fix/<LAYER>/<iter>/shot.png

# 4. Visual verification: compare screenshot against expected improvement
# 5. Kill dev server, iterate if needed
```

**Playwright verification script pattern** (use Playwright MCP tools):
1. `browser_navigate` to `http://127.0.0.1:8091`
2. Wait 8-10 seconds for WASM boot + asset load + shader compilation
3. `browser_evaluate`: `window.render_game_to_text()` — check status is not "Loading..."
4. `browser_take_screenshot` — save to artifacts directory
5. `browser_evaluate`: `JSON.stringify(window.__wrelaRuntime)` — capture runtime telemetry
6. Compare screenshot with previous iteration

---

## Task 1: Diagnose Ground Plane Visibility

**Files:**
- Read: `client/src/web.rs` — `generate_ground_plane()`, `load_forest_procedural_assets()`, `load_scene_from_manifest()`
- Read: `client/src/web.rs` — `build_render_scene_snapshot()` or equivalent that populates instances

**Step 1: Read the ground plane generation code**

Read `client/src/web.rs` and find:
- `generate_ground_plane()` — what size, what Y position, what material
- Where the ground mesh is added to the scene graph
- Whether the ground mesh gets included in the render instance list
- The ORM texture generation for grass — what are the R (AO), G (roughness), B (metallic) channel values

**Step 2: Read the scene instance builder**

Find the function that builds `RenderSceneSnapshot3D` and trace which instances make it into the render list. Check:
- Is the ground mesh index valid?
- Is the ground transform placing it at Y=0?
- Is the camera positioned above Y=0 looking down?
- Is there a culling issue (BVH AABB rejecting the ground)?

**Step 3: Read the material uniforms for ground**

Check the `MaterialUniforms` struct values for the ground mesh:
- `base_color_factor` — should be [1,1,1,1]
- `metallic_factor` — should be ~0.0 for grass (NOT 1.0!)
- `roughness_factor` — should be ~0.7 for grass

**CRITICAL CHECK:** If `metallic_factor = 1.0` on the ground (as suggested by code exploration), this is a primary bug. Metallic=1.0 means zero diffuse contribution → the ground appears black/invisible. The fix is to set metallic_factor to 0.0 for all non-metal surfaces (grass, bark, rock, leaf, enemy skin).

**Step 4: Document findings**

Write a diagnostic summary listing:
- Ground mesh: present/absent, size, Y position
- Ground material: metallic_factor, roughness_factor, base_color_factor
- Instance inclusion: yes/no, why
- Camera position relative to ground

**Expected outcome:** Identify exactly why the ground isn't visible.

---

## Task 2: Fix Ground Plane Visibility

**Files:**
- Modify: `client/src/web.rs` — ground generation, material setup, instance building

**Step 1: Fix material metallic/roughness values**

For every procedural material in `load_forest_procedural_assets()`, ensure correct PBR values:

| Material | metallic_factor | roughness_factor |
|----------|----------------|-----------------|
| Grass/Ground | 0.0 | 0.7 |
| Bark/Trunk | 0.0 | 0.85 |
| Leaf/Foliage | 0.0 | 0.6 |
| Rock | 0.0 | 0.8 |
| Enemy Skin | 0.0 | 0.5 |

These are all non-metallic natural surfaces. `metallic_factor` must be 0.0.

Also check the ORM texture generation — the B channel (metallic) should be near 0 for all natural surfaces. If the procedural texture code is generating wrong channel values, fix the generation.

**Step 2: Ensure ground mesh is in the instance list**

If the ground is being generated but not rendered, trace the instance pipeline and ensure it's included. The ground should be a single large quad at Y=0 spanning the arena.

**Step 3: Fix ground mesh size if needed**

The combat arena extends from [-10.5, -10.5] to [10.5, 10.5]. The ground should cover at least this area. If `generate_ground_plane()` creates a 20x20 quad (±10), extend it to ±15 or ±20 for visual margin.

**Step 4: Build and verify**

```bash
cargo check -p wrela_client --target wasm32-unknown-unknown
```

Then build, serve, Playwright screenshot. Expected: Ground plane visible under objects, textured with grass.

**Step 5: Commit**

```bash
git add client/src/web.rs
git commit -m "fix: correct PBR material metallic/roughness values for natural surfaces, ensure ground plane renders"
```

---

## Task 3: Fix Sky Atmosphere Rendering

**Files:**
- Read: `client/src/sky.rs` — sky shader, SkyUniforms, sky render pass
- Read: `client/src/web.rs` — sky pass invocation in `render_3d`
- Modify: `client/src/web.rs` and/or `client/src/sky.rs`

**Step 1: Diagnose sky pass execution**

Check in `render_3d()`:
- Is the sky pass being called?
- Is it writing to the correct render target (hdr_target_view)?
- Is the `inv_view_proj` matrix correct? (An incorrect inverse would produce a broken sky)
- Are the sky uniforms reaching the shader?

**Step 2: Verify sky shader output**

The sky shader reconstructs world-space ray direction from clip coords. Key issues could be:
- Reverse-Z depth convention affecting clip-space reconstruction
- The `inv_view_proj` not matching the actual view-projection used for rendering
- Sky colors being too similar to the clear color (so sky appears as a flat color)

Check the clear color of the HDR target — if it's the same blue as what we see, the sky pass might not be executing at all.

**Step 3: Fix sky rendering**

Likely fixes:
- Ensure sky pass renders BEFORE the main PBR pass (it should clear the HDR target)
- Ensure inv_view_proj is computed from the UNJITTERED projection (TAA jitter would break sky)
- If sky colors are too desaturated, increase contrast between zenith/horizon/ground:
  - zenith: [0.02, 0.03, 0.08] (darker blue)
  - horizon: [0.15, 0.12, 0.10] (warm amber)
  - ground: [0.04, 0.035, 0.03] (dark earth)
- Increase sun intensity from 16.0 to 24.0 for more dramatic sky

**Step 4: Build, serve, Playwright screenshot**

Expected: Visible sky gradient — darker zenith, lighter horizon, warm sun area. Not flat blue.

**Step 5: Commit**

```bash
git add client/src/web.rs client/src/sky.rs
git commit -m "fix: sky atmosphere rendering - correct inv_view_proj and sky color profile"
```

---

## Task 4: Fix PBR Lighting Intensity

**Files:**
- Read: `client/src/web.rs` — frame uniform setup, light direction, light color, ambient color
- Modify: `client/src/web.rs`

**Step 1: Diagnose lighting parameters**

In `render_3d()`, find where `FrameUniform3D` is populated. Check:
- `light_dir`: Should be a normalized direction vector pointing FROM the sun (e.g., [0.3, -0.8, 0.4])
- `light_color`: Should be HDR (e.g., [3.0, 2.7, 2.2, 1.0] for warm sunlight)
- `ambient`: Should be visible but subtle (e.g., [0.15, 0.18, 0.25, 1.0] for cool fill)
- Camera position: Should be above the scene looking at the arena

**Step 2: Fix lighting values**

Target a dramatic dusk-gothic look per the art direction:
- **Sun direction**: ~30-40 degrees above horizon, slightly behind camera → [0.3, -0.6, 0.5] normalized
- **Sun color (HDR)**: Warm amber [3.5, 2.5, 1.8, 1.0]
- **Ambient color**: Cool blue fill [0.12, 0.15, 0.22, 1.0]
- **Exposure** (in post-process): 1.0-1.2 (check PostProcessUniforms)

The key insight: if light_color intensity is [1.0, 1.0, 1.0], that's LDR lighting in an HDR pipeline → everything looks dim after tonemapping. HDR light colors should be 2-5x for outdoor scenes.

**Step 3: Verify material response**

After fixing lighting, all surfaces should show:
- Visible diffuse contribution (non-metallic surfaces lit by directional light)
- Specular highlights on appropriate surfaces
- Color variation between different materials

**Step 4: Build, serve, Playwright screenshot**

Expected: Lit scene with visible color on all surfaces. Trees should be brown, ground green, rocks grey.

**Step 5: Commit**

```bash
git add client/src/web.rs
git commit -m "fix: increase PBR lighting intensity to HDR levels for proper tonemapped output"
```

---

## Task 5: Fix Cascaded Shadow Maps

**Files:**
- Read: `client/src/shadows.rs` — cascade computation, shadow pass encoding, shadow sampling
- Read: `client/src/web.rs` — shadow system initialization, shadow bind group
- Modify: `client/src/web.rs` and/or `client/src/shadows.rs`

**Step 1: Diagnose shadow system**

Check:
- Is `self.shadow_system` initialized with valid pipeline/textures?
- Is `encode_shadow_passes()` being called in `render_3d`?
- Is the shadow bind group (group 3) being set on the main PBR render pass?
- Is the shadow atlas texture being created at correct resolution (6144x2048)?

**Step 2: Verify shadow sampling in shader**

In the PBR fragment shader, check:
- `compute_shadow()` function — does it return values in [0, 1]?
- Cascade selection — does the view-depth calculation match the cascade splits?
- PCF sampling — are the UV coordinates correct for the atlas layout?
- Is the shadow result actually multiplied into the final color? (look for `* shadow` in the fragment shader)

**Step 3: Fix shadow issues**

Common shadow bugs:
- Shadow atlas not rendered (check encode_shadow_passes is called)
- Shadow matrices not uploaded (check update_cascades is called with correct light direction)
- Shadow UV mapping incorrect (cascade offset in atlas)
- Shadow bias too large (everything unshadowed) or too small (shadow acne)
- Depth comparison reversed (reverse-Z needs special handling)

**Step 4: Build, serve, Playwright screenshot**

Expected: Visible character and tree shadows on the ground plane. Shadow direction matches light direction.

**Step 5: Commit**

```bash
git add client/src/web.rs client/src/shadows.rs
git commit -m "fix: cascaded shadow maps producing visible shadows on scene geometry"
```

---

## Task 6: Fix Fog and Atmospheric Depth

**Files:**
- Read: `client/src/web.rs` — SceneVisualProfile, resolve_scene_visual_profile, fog uniform binding
- Modify: `client/src/web.rs`

**Step 1: Diagnose fog parameters**

In `render_3d()`, check how fog parameters reach `FrameUniform3D`:
- `fog_color_and_start`: [r, g, b, start_distance]
- `fog_params`: [end_distance, density, height_falloff, 0]

The scene layout specifies fog volumes. Check:
- Are the fog parameters from `SceneVisualProfile` reaching the frame uniform?
- What are the actual values? (e.g., fog_start=10, fog_end=50, density=0.05)
- Is the fog color close to the sky horizon color? (It should be for seamless blending)

**Step 2: Fix fog parameters**

Target values for a gothic forest atmosphere:
- `fog_color`: Match sky horizon [0.11, 0.12, 0.14] in linear space
- `fog_start`: 8.0 (fog begins this far from camera)
- `fog_end`: 40.0 (fully fogged at this distance)
- `fog_density`: 0.04 (exponential fog density)
- `fog_height_falloff`: 0.15 (fog is thicker near ground)

**Step 3: Verify fog shader**

The `apply_fog()` function in the PBR shader should:
- Compute distance from camera
- Apply height-based falloff
- Blend between scene color and fog color
- Add sun in-scattering for volumetric look

**Step 4: Build, serve, Playwright screenshot**

Expected: Background objects fade into atmospheric haze. Depth layering is visible. Far trees are hazier than near ones.

**Step 5: Commit**

```bash
git add client/src/web.rs
git commit -m "fix: atmospheric fog with distance/height falloff creating depth layering"
```

---

## Task 7: Fix Post-Processing Chain (Bloom, SSAO, Tonemapping)

**Files:**
- Read: `client/src/postprocess.rs` — bloom, tonemap, FXAA, god rays
- Read: `client/src/ssao.rs` — SSAO system
- Read: `client/src/web.rs` — post-process invocation
- Modify: `client/src/web.rs` and/or `client/src/postprocess.rs`

**Step 1: Diagnose post-processing**

In `render_3d()`, check:
- Is `self.post_process` initialized?
- What are the PostProcessUniforms values? (bloom_intensity, exposure)
- Is bloom downsample/upsample chain executing? (check bloom_mip textures created)
- Is the tonemap pass reading HDR and writing to FXAA intermediate?
- Is FXAA writing to the final surface?
- Is SSAO enabled? (`self.ssao_system.enabled()`)

**Step 2: Fix bloom parameters**

- `bloom_intensity`: 0.04-0.08 (subtle, just softens bright areas)
- `bloom_threshold`: 1.0 (only truly bright HDR pixels bloom)
- `exposure`: 1.0-1.2 (this is critical — too low and everything is dark, too high and it's washed out)

**Step 3: Fix SSAO**

- Ensure SSAO is enabled (`ssao_system.enabled()` returns true)
- If disabled by default, enable it
- SSAO should darken crevices (tree base, rock bottoms, character joints)
- AO intensity: 0.5-0.7 (visible but not overdone)

**Step 4: Fix god rays**

- God rays require `sun_screen_pos` to be computed (sun must be in view or near edge)
- If the sun is behind the camera, god rays won't trigger — this is OK for some camera angles
- Intensity: 0.15-0.25 (subtle volumetric effect)

**Step 5: Build, serve, Playwright screenshot**

Expected: Soft bloom on bright areas (sky near sun, specular highlights). Ambient occlusion darkening crevices. Proper tonemapped output with good contrast.

**Step 6: Commit**

```bash
git add client/src/web.rs client/src/postprocess.rs client/src/ssao.rs
git commit -m "fix: post-processing chain - bloom, SSAO, tonemapping producing visible quality improvement"
```

---

## Task 8: Content Polish — Tree Canopies, Scene Density, Materials

**Files:**
- Modify: `client/src/web.rs` — procedural mesh generation, material quality
- Modify: `apps/wrela-forest/assets/generated/environment/forest-scene-layout-v1.json`

**Step 1: Fix tree canopy foliage**

The trees currently render as bare trunk cylinders. Check:
- Is foliage mesh generated? (Look for the tree generation code)
- Is it a separate mesh from the trunk?
- Does the scene layout JSON include canopy instances? (Yes — `redwood_canopy.glb` entries exist)
- Are these GLB references being resolved to procedural meshes?

Fix: Ensure the procedural tree generation creates both trunk AND foliage meshes, and that the scene builder instantiates canopy meshes at the positions from the layout JSON.

**Step 2: Improve procedural texture quality**

The 512x512 procedural textures need improvement:
- **Grass albedo**: Darker, more saturated green with earth-tone variation
- **Bark albedo**: Rich brown with vertical streaking
- **Leaf albedo**: Deep green with translucency variation
- **Rock albedo**: Grey with lichen/moss tinting

For each, the ORM texture is critical:
- R (AO): Pre-baked ambient occlusion (0.3-1.0 range, not flat 1.0)
- G (roughness): Surface-appropriate roughness
- B (metallic): 0 for all natural materials

**Step 3: Increase scene density**

The forest should feel enclosed. Add to the scene layout:
- More background trees at the arena edges (12-16 total, not just 7 trunk instances)
- Small understory props (more ferns, mushrooms, fallen leaves)
- Ensure canopy coverage provides dappled shade feel

**Step 4: Build, serve, Playwright screenshot**

Expected: Forest that reads as a forest. Visible canopies overhead. Rich material colors. Dense foliage.

**Step 5: Commit**

```bash
git add client/src/web.rs apps/wrela-forest/assets/generated/environment/forest-scene-layout-v1.json
git commit -m "feat: tree canopies, improved procedural materials, denser scene composition"
```

---

## Task 9: Camera and Composition Polish

**Files:**
- Modify: `client/src/web.rs` — camera defaults, orbit parameters
- Modify: `apps/wrela-forest/assets/generated/environment/forest-scene-layout-v1.json`

**Step 1: Set default camera for best composition**

The default camera should frame:
- Player character in lower third
- Ground plane visible
- Canopy visible at top
- Enemy visible in middle distance
- Slightly elevated angle (~15-20 degrees above horizontal)

Target default camera:
- Position: [0, 3.5, 8] (behind and above player)
- Target: [0, 1.2, 0] (player chest height)
- FOV: 50 degrees

**Step 2: Tune combat camera behavior**

- Lock-on should smoothly track targeted enemy
- Camera shouldn't clip through trees
- Orbit should respect elevation limits (10-60 degrees above ground)

**Step 3: Build, serve, Playwright screenshot from multiple angles**

Capture:
1. Default idle composition
2. Camera orbit left
3. Camera orbit right
4. Lock-on to enemy

**Step 4: Commit**

```bash
git add client/src/web.rs apps/wrela-forest/assets/generated/environment/forest-scene-layout-v1.json
git commit -m "fix: camera defaults and composition for dramatic forest clearing framing"
```

---

## Task 10: Full Scenario Playwright Verification Matrix

**Step 1: Run full scenario matrix**

Capture screenshots for each scenario:

| Scenario | Actions | Expected |
|----------|---------|----------|
| idle_composition | Wait 3s | Lit forest clearing, player idle, enemy visible, shadows, fog |
| camera_orbit | Mouse drag 180 degrees | Scene visible from multiple angles, no visual artifacts |
| lock_toggle | Press Enter | Camera smoothly locks to enemy |
| attack_combo | Press J three times | Attack animation, hit sparks on enemy |
| dodge_parry | Press Space, then L | Dodge animation, parry stance |
| death_restart | Let enemy kill player | Death screen, restart works |

**Step 2: Capture and review all screenshots**

Save to `.artifacts/aaa-fix/FINAL/`:
- `idle_composition/shot-0.png`, `shot-1.png`
- `camera_orbit/shot-0.png`, `shot-1.png`
- `lock_toggle/shot-0.png`, `shot-1.png`
- `attack_combo/shot-0.png`, `shot-1.png`
- `dodge_parry/shot-0.png`, `shot-1.png`
- `death_restart/shot-0.png`, `shot-1.png`

**Step 3: File any remaining visual issues as follow-up tasks**

---

## Task 11: Independent Code Review

**Step 1: Launch review subagent**

Review all changes made across Tasks 1-10 for:
- Correctness: shader math, PBR parameter ranges, matrix operations
- Performance: no redundant texture uploads, no unnecessary passes
- Architecture: changes follow existing patterns, no dead code introduced
- Completeness: all 7 rendering layers addressed, Playwright evidence for each

**Step 2: Address review findings**

Fix any issues identified by the reviewer.

**Step 3: Final commit**

```bash
git add -A
git commit -m "fix: address code review findings from AAA rendering pipeline fix"
```
