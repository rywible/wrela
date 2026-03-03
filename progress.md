Update: February 27, 2026 (AAA compiler-first implementation lane)

- Hard-cut agent orchestration contracts to v2 in `agent_control`:
  - `AgentRunIntentV2`, `AgentExecutionPlanV2`, `AgentTaskSpecV2`, `AgentRunSummaryV2`.
  - deterministic plan graph now models compiler-first one-shot stages.
- Hard-cut CLI evolution:
  - added `mmo` and `studio` command families.
  - `wrela agent-run` now requires `--intent-v2`.
  - help and parser tests updated for strict behavior.
- Compiler-first architecture correction:
  - render/shader IR work is now integrated inside the compiler crate modules:
    - `compiler/render_ir/*`
    - `compiler/shader_compiler/*`
  - rejected standalone crate direction; removed temporary top-level `render_ir` / `shader_compiler` directories.
- `agent-run` foundation artifacts expanded with compiler-owned render/shader outputs:
  - `render-graph-ir-v1.json`
  - `render-graph-ir-v1-fingerprint.json`
  - `shader-program-ir-v1.json`
  - `shader-program-ir-v1-fingerprint.json`
  - `shader-program-v1.wgsl`
- Verification snapshots (current lane):
  - `cargo check -p wrela` pass
  - `cargo test -p wrela --lib render_ir:: -- --nocapture` pass
  - `cargo test -p wrela --lib shader_compiler:: -- --nocapture` pass
  - `cargo test -p wrela --test cli cli_help -- --nocapture` pass
  - `cargo test -p wrela --test cli cli_agent_run_requires_intent_v2 -- --nocapture` pass

Original prompt: PLEASE IMPLEMENT THIS PLAN: Agent-First Frontend Pipeline V1 (Hard Cut) with Browser-Playable Demo AC, including new frontend language/pipeline/layers/CLI, strict hard-cut checks, and AC that a browser game is playable through the new pipeline with artifacted verification.

- Started orchestration with parallel workers per AGENTS.md.
- Completed G1 workers:
  - WP-01 parser/AST/HIR surface for `render` + `gpu fn` scaffolding and legacy annotation parse rejection.
  - WP-02 architecture layer split (`Presentation`, `FrontendIntegrations`) + import rules/tests.
  - WP-03 `wrela frontend` command family routing + JSON-mode placeholders.
- G2 workers launched for type semantics, render compiler manifests, and diagnostics/fix.
- Loaded `develop-web-game` skill and prepared for Playwright-driven browser verification.

TODO next:
- Integrate G2 outputs and resolve compile/test fallout.
- Implement runtime + preview + demo app (`apps/wrela-browser-demo`) and hard-cut gates/artifacts.
- Run end-to-end browser preview evidence and final independent reviewer subagent.
- WP-10 started: scaffolded `apps/wrela-browser-demo` with lane split across `domain/**`, `presentation/**`, `frontend/integrations/**`, `application/**`, and composition root under `application/composition/**`.
- Added new-pipeline render source in presentation lane (`render ... preset game_arcade_2d` + `gpu fn ... -> String`), with no app-authored `.wgsl` files.
- Implemented deterministic gameplay flow covering movement, pickup collection, score increase, win state, and restart path.
- WP-07 + client WP-10 in progress: cut client manifest/runtime to v3-only (`render-schema-v3`/`shader-bundle-v3`) and `frame_graph`-only resolution; removed first-pass fallback behavior.
- Added WebGPU frame-graph execution path in wasm client: per-pipeline pipeline creation, multi-pass render submission, and compute-pass readiness tracking (no-op compute passes).
- Added deterministic test hooks on `window`: `render_game_to_text()` and `advanceTime(ms)` with deterministic stepping mode.
- Added runtime evidence fields on `window.__wrelaRuntime` for frame-graph execution proof (schemas, declared passes, executed frames, last render/compute pass counts, compute-pass-ready flag).
- Added win + restart loop wiring: client sends restart intent via existing `collect_pressed` field once won; server accepts optional `collect_pressed` compatibly and restarts authoritative state when requested.
- Tests run:
  - `cargo test -p wrela_client render_manifest` (pass)
  - `cargo test -p wrela_runtime game_slice` (pass)
- Playwright run executed via skill client against static dist host (`python3 -m http.server 8091 --directory apps/wrela-game-slice/target/wrela-game-slice`).
- Playwright blocker/findings: dist uses stale prebuilt `client-runtime.js/wasm` so `window.__wrelaRuntime` evidence fields from source changes are not reflected in runtime capture; `/ws` endpoint is 404 on static host so authority-driven win/restart path could not be exercised end-to-end in browser.
- Fixed parser validation gap so `return` inside `gpu fn` is accepted (`compiler/parser/validate.rs`).
- `wrela frontend build/check/run` now pass for `apps/wrela-browser-demo` using local CLI (`cargo run -p wrela -- ...`).
- Captured frontend pipeline artifacts under `apps/wrela-browser-demo/.artifacts/frontend-pipeline/**`, including build/check summaries and Playwright run reports/screenshots.
- Completed Playwright validation loop against live `wrela game dev` host (WebGPU + WS authority) on `127.0.0.1:4173` using skill client.
- Evidence artifacts:
  - report: `.artifacts/frontend-pipeline/playwright/latest-report.json`
  - screenshots: `output/web-game/shot-{0,1,2}.png`
  - state captures: `output/web-game/state-{0,1,2}.json`
- Validation passed in strict mode (`status=ok`, `strictExitCode=0`), with runtime proving manifest validation and frame-graph execution (`render-schema-v3` + `shader-bundle-v3`, `frame_graph_path_used=true`).
- Observed non-fatal browser warning: deprecated WebGPU init parameters reported from generated `client-runtime.js`.
- Re-ran Playwright strict smoke after manifest-validation refactor; pass confirmed.
- Latest evidence:
  - report: `.artifacts/frontend-pipeline/playwright/latest-report.json`
  - screenshots: `output/web-game/shot-{0,1,2}.png`
  - state: `output/web-game/state-{0,1,2}.json`
- Assertions: 21 total / 0 failed, strictExitCode=0, diagnostics error count=0 (one non-fatal warning only).
- Post-review hardening complete: execution-report failure reconciliation, stronger CLI manifest contracts, parallel client manifest fetch, and expanded negative tests.
- Final validation evidence:
  - `cargo check --workspace` pass
  - `cargo test -p wrela --bin wrela shared_execution_report_tests` pass
  - targeted CLI contract tests pass (agent-run, world-chunk build, streaming check)
  - `cargo test -p wrela_client` pass
  - `cargo test -p wrela_client --target wasm32-unknown-unknown --no-run` pass
  - `cargo run -p wrela -- agent-run ... --error-format=json` pass with existing `execution_report`
  - Playwright strict pass (`status=ok`, `strictExitCode=0`) with updated screenshots/state
- Closed reviewer coverage gap by extracting manifest parse/validation to always-compiled `client/src/manifest_validation.rs` and wiring `web.rs` to use it.
- Host test coverage now executes manifest validation suite (`cargo test -p wrela_client` shows manifest_validation tests).
- Added explicit world malformed-JSON negative test.
- Removed duplicated host-only asset-pack shim by promoting `wrla_asset_pack` to common client dependency.
- Stabilized strict Playwright loop by changing restart key mapping in wasm client from `Space` to explicit restart keys (`R`/`Enter`) so warmup action bursts do not accidentally reset runtime ticks.
- Final evidence: strict Playwright report now `status=ok`, `strictExitCode=0` with 0 failed assertions.
- Added host-runnable restart-key policy module (`client/src/restart_key_policy.rs`) with direct tests ensuring only `R`/`Enter` trigger restart and Space is rejected.
- Added CLI integration negative test for malformed `world-chunks.json` (`cli_game_check_rejects_malformed_world_chunk_manifest_for_wrela_game_slice`).
- Re-ran consolidated regression matrix: all targeted checks/tests pass.
- Final proof runs:
  - `wrela agent-run ... --error-format=json` pass with existing execution report path.
  - strict Playwright pass (`status=ok`, `strictExitCode=0`, failedAssertions=0) with latest screenshot/state evidence.
- Added deterministic fault-injection hook for agent-run execution-report write failures (`WRELA_AGENT_RUN_FORCE_REPORT_WRITE_FAILURE=1`) and CLI integration test verifying summary rewrites to `failed-report` without `execution-report.json` artifact reference.
- Added host-runnable restart-latch behavior module/tests (`client/src/restart_latch.rs`) and wired `web.rs` through helper.
- Consolidated regression suite re-run: all targeted checks and new tests pass.
- Final strict browser proof re-run with movement-only action burst (no restart buttons) succeeded: report `status=ok`, `strictExitCode=0`, `assertions.failed=0`.
- Closed residual risks:
  - execution-report fault injection now supports partial/corrupt write failure modes with integration coverage.
  - malformed world-chunk test now restores original manifest via guard/Drop.
  - added host-runnable restart-latch tests and wasm-target key input wiring coverage in `web.rs`.
- Final strict Playwright proof passed (`status=ok`, `assertions.failed=0`) using stable actions-json profile.

Update: March 3, 2026 (hero GLB runtime integration + walk validation)

- Hard-cut player rendering path to required hero GLB in `client/src/web.rs`:
  - Added `PLAYER_HERO_GLB_FILE` and async GLB fetch/parse in `load_forest_procedural_assets`.
  - Runtime now fail-closes on missing mesh/skeleton/clip data for player hero asset.
  - Player animation state mappings now derive from GLB clip names via new `hero_clip_mapping` module.
- Added `client/src/hero_clip_mapping.rs` with tested clip resolution behavior:
  - case-insensitive exact+partial name priority matching,
  - idle/walk/run mapping with actionable errors,
  - single-clip fallback behavior.
- Updated scene instance handling in `web.rs`:
  - scene builder now returns explicit `player_instance_index` + `enemy_instance_index`;
  - runtime stores these indices and uses them for transform updates (removed last-two-instance assumption).
- Removed enemy skeleton/clip registration from runtime asset load path so the global joint palette is not overwritten by a second rig in the same frame.
- Added `player_state` into `render_game_to_text()` payload for deterministic browser-side animation-state verification.

Validation runs completed:
- `cargo fmt -- client/src/web.rs client/src/hero_clip_mapping.rs client/src/lib.rs` (pass)
- `cargo test -p wrela_client hero_clip_mapping` (pass)
- `cargo check -p wrela_client --target wasm32-unknown-unknown` (pass)
- `cargo test -p wrela_client` (pass; 411 tests)

Browser/WebGPU evidence (Playwright skill loop):
- Focused static proof (hero visible + walking state):
  - `.artifacts/webgpu-engine-pass/hero-walk-static-final/playwright/shot-0.png`
  - `.artifacts/webgpu-engine-pass/hero-walk-static-final/playwright/state-0.json` (`player_state: 1`)
- Connected strict proof (authority online, strict pass):
  - baseline: `.artifacts/webgpu-engine-pass/hero-walk-connected-proof/baseline/report.json` (`status=ok`, `strictExitCode=0`)
  - walk: `.artifacts/webgpu-engine-pass/hero-walk-connected-proof/walk/report.json` (`status=ok`, `strictExitCode=0`)
- Connected left-move strict proof (authority online, strict pass, walk state active with moved X):
  - `.artifacts/webgpu-engine-pass/hero-walk-connected-left/report.json` (`status=ok`, `strictExitCode=0`)
  - `.artifacts/webgpu-engine-pass/hero-walk-connected-left/state-0.json` (`player_state: 1`, `player.x: 642.5`)
  - `.artifacts/webgpu-engine-pass/hero-walk-connected-left/shot-0.png`

Independent review subagent rerun after these artifacts reported no blocking findings for user goal (render hero GLB + walk around); remaining risks are medium/low evidence rigor and single-clip semantics.

Update: March 3, 2026 (AAA forest hard-cut iteration resume)

- Root-caused strict Playwright `idle_composition` failure to stale/missing dist scene layout asset:
  - runtime was serving `target/.../assets/generated/environment/forest-scene-layout-v1.json` at schema v1 (or absent), causing WebGPU bootstrap failure and draw-call assertions to fail.
- Hard-cut build pipeline fix in `compiler/bin/wrela/commands/game.rs`:
  - added required forest scene-layout contract constants (`FOREST_SCENE_LAYOUT_RELATIVE_PATH`, required keys).
  - added `validate_forest_scene_layout_asset_contract(app_root)` to fail build if scene layout is missing/invalid or below schema v2.
  - added `sync_authored_assets_to_dist(app_root, dist_dir)` to mirror app-authored assets into dist every build.
  - wired both steps into `game_build_project` before loader/protocol emission.
- Added unit tests in `compiler/bin/wrela/commands/game.rs`:
  - `validate_forest_scene_layout_asset_contract_requires_v2_fields` (pass)
  - `validate_forest_scene_layout_asset_contract_rejects_missing_required_fields` (pass)
  - `sync_authored_assets_to_dist_copies_nested_assets` (pass)

TODO next:
- Rebuild forest app and verify served scene layout now reflects schema v2.
- Re-run `scripts/webgpu_engine_pass/browser_smoke.sh` under AAA artifact lane iteration.
- Inspect Playwright screenshots + runtime state + diagnostics for next failing strict assertion (if any) and iterate.
- Continue through full scenario matrix and final gate scripts.
- Added monotonic runtime tick telemetry in `client/src/web.rs` for strict Playwright progression checks across restart flows:
  - runtime now tracks `runtime_tick_epoch_offset`, `runtime_tick_monotonic`, and `runtime_tick_last_source`.
  - exported `window.__wrelaRuntime.tick` now uses monotonic timeline across authority reset events.
  - exported `window.__wrelaRuntime.state_tick` preserves raw authoritative tick for debugging.
- Verified compile after telemetry change: `cargo check -p wrela_client` (pass).
- Added deterministic combat-floor overlay mesh in `load_scene_from_manifest` and scene builder integration:
  - new arena-sized procedural floor is now instantiated from combat extents for stable readability.
  - floor uses deterministic grass PBR textures and is anchored at arena min-Y.
- Calibrated lighting/post profile in `client/src/web.rs`:
  - lower exposure and bloom, darker sky gradient, cooler denser fog.
- Iterated scene layout ground authoring in `apps/wrela-forest/assets/generated/environment/forest-scene-layout-v1.json` and validated via Playwright visual loops.
- Focused Playwright loops run and inspected manually:
  - `.artifacts/aaa-forest-demo/LIGHT-04/iter-004/playwright/idle_composition/shot-1.png`
  - `.artifacts/aaa-forest-demo/ENV-03/iter-005/playwright/idle_composition/shot-1.png`
  - `.artifacts/aaa-forest-demo/ENV-03/iter-006/playwright/idle_composition/shot-1.png`
  - `.artifacts/aaa-forest-demo/ENV-03/iter-007/playwright/idle_composition/shot-1.png`
- Full strict matrix rerun after latest environment/camera changes:
  - `WRELA_AAA_LANE=ORCH-00 WRELA_AAA_ITERATION=iter-005 scripts/webgpu_engine_pass/browser_smoke.sh apps/wrela-forest` (pass).

Update: March 3, 2026 (BLACKFIX-00 startup black-screen iteration)

- Root cause found via focused Playwright loop: startup shell hid boot overlay too early (fixed 400ms timeout) while runtime still reported `status: Loading render/shader artifacts...` and `combat_camera.rendered_enemy_instance_count: 0`, producing a dark/black-looking screen.
- Hard-cut startup UX fix in compiler game shell templates:
  - `compiler/bin/wrela/game_assets/index.html`
    - added full-screen boot overlay (`#boot-overlay`) with `#boot-status` + `#boot-detail` labels.
    - switched body background from near-black to readable blue-black gradient base.
  - `compiler/bin/wrela/game_assets/loader.js`
    - added explicit boot status helpers and error UI path.
    - removed fixed `setTimeout(... hide overlay ...)` behavior.
    - added runtime readiness watcher that polls `window.render_game_to_text()`, updates overlay detail from runtime `status`, and hides overlay only when runtime is non-loading and scene instances are rendering.
- Focused Playwright artifacts:
  - Repro baseline (dark frame while loading): `.artifacts/aaa-forest-demo/BLACKFIX-00/iter-003/playwright/shot-0.png`
    - state showed `status="Loading render/shader artifacts. root='.'"` and `rendered_enemy_instance_count=0`.
  - Post-fix startup proof (overlay visible with loading text): `.artifacts/aaa-forest-demo/BLACKFIX-00/iter-004/playwright/shot-0.png`
  - Post-fix gameplay proof (overlay cleared, scene rendering): `.artifacts/aaa-forest-demo/BLACKFIX-00/iter-005/playwright/shot-1.png`
  - Iteration report pass: `.artifacts/aaa-forest-demo/BLACKFIX-00/iter-005/playwright/report.json` (`status=ok`, `strictExitCode=0`, failed assertions=0).

TODO next:
- If users still report black screen on specific machines, add explicit WebGPU capability check in loader and display a non-WebGPU fallback/error panel with browser/GPU guidance.

Update: March 3, 2026 (BLACKFIX-01 wiring cutover: camera + scene profile)

- Camera wiring fixes in `client/src/web.rs`:
  - Added runtime mouse orbit controls in game mode (`mousedown`/`mousemove`/`mouseup` + wheel zoom) in `install_input_handlers`.
  - Added runtime drag state fields (`camera_orbit_dragging`, `camera_last_pointer_pos`) and stored handler closures (`on_pointermove`, `on_wheel`) to prevent drop.
- Telemetry wiring fix:
  - `render_game_to_text().combat_camera.camera_*` now reports the live renderer orbit camera (`self.orbit_camera`) instead of stale game-logic camera fields.
  - Added `camera_azimuth`, `camera_elevation`, `camera_distance`, and `camera_fov_y` to combat camera payload.
- Scene profile wiring fix:
  - Added `SceneVisualProfile` + `resolve_scene_visual_profile()` and routed fog/light profile through scene runtime state (`scene_lut_profile_id`, `scene_fog_volume_count`).
  - `RenderSceneSnapshot3D` now carries fog parameters (`fog_color/start/end/density/height_falloff`) and `render_3d` frame uniform now consumes scene-provided fog values instead of hardcoded constants.

Focused Playwright/Chromium evidence:
- Camera drag proof (before/after state delta):
  - `.artifacts/aaa-forest-demo/BLACKFIX-00/iter-009/camera-delta.json`
  - camera changed from eye `(5.5, 3.4, 8.6)` to `(-8.659, 5.475, -3.869)`.
- Additional camera state check after telemetry fix:
  - direct run output showed azimuth changing `0.569 -> -2.335` with eye change.
- Latest visual smoke snapshot after profile wiring:
  - `.artifacts/aaa-forest-demo/BLACKFIX-00/iter-011/playwright/shot-1.png`

Open issues still visible:
- Scene remains visually low-fidelity/flat due authored asset/content constraints (ground + sparse composition + weak material response), not because camera input/telemetry are disconnected anymore.
- Next hard fix should be authored ground/terrain contract and richer environment composition, not more camera plumbing.
