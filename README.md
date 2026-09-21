# Wrela Studio

A browser-native studio for field-authored objects, creatures, vegetation, materials, lighting, water, and streamed heightfield worlds. Source is versioned JSON. The Studio and standalone player share the compiler, world planner, simulation, and raw WebGPU renderer.

## Run

Install the [Bun](https://bun.com) version in `.bun-version`, then:

```sh
bun install --frozen-lockfile
bun run dev
```

Open the local address printed by the server. WebGPU and hardware acceleration are required. The reference project opens a winter valley with a field-generated polar bunny, seeded pines, terrain, sky, and analytic water.

Select a definition in the project browser. Use its inspector to edit it. Switch to **Neutral studio** to isolate a subject; **World stage** tests it in the environment. Drag the viewport to orbit, Shift-drag to pan, and scroll to zoom. **Handles** exposes shape or skeleton controls. Character workspaces separate anatomy, rig, pose, motion, and physical behavior. Biped, quadruped, and chain templates can replace a rig in one undoable edit. Reference stages select reusable lighting, environment, and subjects; worlds select their own definitions, authored instances, and population rules. Preview exposure and wind do not edit source.

`⌘/Ctrl-S` saves. `⌘/Ctrl-Z` undoes a transaction. `⌘/Ctrl-Shift-Z` redoes it. `Space` toggles playback. `F` resets the view. `⌘/Ctrl-K` opens commands, including JSON import, project backup, and reopening saved work.

**Play** opens **Winter valley exploration**: guide the bunny to three trail lanterns, hold E to restore them, then return home before warmth runs out. The following camera, map, keyboard/touch controls, sound, pause and saves are part of the same Player used for exports. WASD/arrows move; Shift hurries. The game module consumes semantic input and advances through the shared fixed-step physics runtime.

**Export** creates a self-contained HTML player containing the source snapshot, verified cooked geometry, compiler worker and runtime. It opens without Studio or a network connection in a compatible browser. Static releases load matching cooked products; terrain remains streamed. Runtime saves preserve live character motion and occurrence changes separately from the published project. Unknown content changes are rejected; trusted runtime code can supply explicit artifact and motion migrations. Back up projects with JSON export; browser storage is not a backup. Recovery drafts remain separate for every writer and are available from the **Recovery drafts** menu.

## Local workspace

```sh
bun run dev --project /absolute/path/to/project
```

The bridge binds to loopback, validates the configured origin, and uses an authenticated same-origin session. Files are ordinary JSON, one per definition. Crash-safe generations are stored under `.wrela/generations`; `.wrela/CURRENT` identifies the published generation. A cross-process publication lock covers source comparison and pointer replacement, so competing publishers cannot both succeed from the same base. Source edits within the current generation are detected before the next save. Internal symlinks are refused. The local bridge supports macOS and glibc Linux; direct file editors do not participate in its advisory lock.

## Agent operations

Studio exposes `window.wrela`. It does not embed a chatbot. `discover()` returns operations, schemas, conventions, and limits; `inspect(id)` returns a focused copy. The same typed `AuthoringSession` and command types are available from `@wrela/authoring` to trusted TypeScript tools.

```ts
const { revision } = wrela.inspect();
const edit = wrela.apply({
  expectedRevision: revision,
  label: "Widen the valley",
  operations: [{
    kind: "terrain.widenValley",
    target: "valley-terrain",
    intervention: "river-valley",
    width: 80,
  }],
});
await wrela.preview.prepare({ revision: edit.revision, quality: "review" });
const capture = await wrela.preview.capture({
  revision: edit.revision,
  tick: 120,
  channels: ["beauty", "depth", "lod"],
});
```

Capture accepts named cameras (`front`, `back`, `left`, `right`, `top`, `three-quarter`, `valley-overlook`), diagnostic channels including silhouette, identity and material channels, and `overlays: ["rig", "colliders"]`. Identity captures include their RGB-to-instance mapping. `discover()` describes the schemas for preview and capture as well as source operations.

Related edits commit together and create one undo step. Revisions increase on undo and redo. Imported projects are validated data, never executable programs. Generated geometry carries source identities for picking and diagnostics. Capture waits for a bounded requested revision and records camera, quality, tick, adapter, and source identity. `await wrela.preview.seek(seconds)` replays physics in bounded batches; choosing a newer time supersedes the preceding replay.

Independent proposals can use `preconditions: { reads, writes }` with document revisions instead of a global `expectedRevision`. `transactionId` makes retries idempotent; `actor` and `intent` retain authorship. `wrela.transactions` exposes preview, list, inspect, targeted revert and history usage. Unrelated documents share storage across transactions; history is bounded by bytes as well as count. Captures reject omitted GPU products and report actual rendered identities and completeness per channel.

Headless tools use the same contracts: `bun run author /path/to/workspace inspect`, `discover`, `preview proposal.json`, or `apply proposal.json`. A proposal carries `{ workspaceKey, batch }`; publication verifies that base again under the lock. Parameterized recipes retain provenance and local overrides while producing ordinary editable definitions; refreshing generated instances is an explicit operation.

## Verify and build

```sh
bun run check   # Strict browser/tool types, import boundaries, Biome
bun test        # Numerical, transactional, compilation, world, physics tests
bun run verify # Isolated hardware browser, GPU parity, recovery, UI and player
bun run perf   # Repeated 1080p reference-scene and travel measurements
bun run perf:low # Lower-power render profile at a 720p viewport
bun run perf:author # Accepted-edit latency gate on a 200-definition project
bun run cook project.json products.json review # Versioned immutable geometry products
bun run build  # Static Studio, player, worker, and offline snapshot in dist/
```

Verification uses Bun.WebView with an isolated Chrome process. Its launcher deliberately avoids Bun’s default `--disable-gpu` flag, rejects software adapters as hardware evidence, and cooperatively leases the GPU across worktrees. Generated reports, exact source/environment manifests and captures live in ignored `output/` directories. CPU submission time, attributed GPU timestamps and browser frame-callback pacing are reported separately; callback intervals do not establish display presentation times. Tests include explicit GPU allocation rejection, material stability across origin changes, multiple writers, storage migration, offline export and the complete physical trail.

Performance gates cover repeated stationary views, 288 metres of travel and thirteen characters, with frame-time, hitch, live-artifact memory and completeness limits. Render profiles coordinate resolution, shadows, anti-aliasing, vegetation detail and upload budgets. CI runs source checks, CPU tests, the authoring benchmark and a production build. A manual hardware job targets a separately provisioned `wrela-gpu` runner and retains pass/failure evidence.

Run separate worktrees on different `PORT` values and use separate project directories. Browser profiles, output, and test ports are isolated. The GPU lease coordinates Wrela tools; it cannot reserve hardware against unrelated applications.

`dist/` can be served by any static HTTP server. HTTPS or loopback is required for WebGPU. The production service worker caches a compatible snapshot; an update waits for the old client to close before activation.

## Scope and conventions

Metres, seconds, radians; right handed, +Y up; column-major matrices and `[x,y,z,w]` quaternions. Authored colors are linear RGB. Finite field surfaces use derived meshes; terrain uses bounded regular-grid quadtree patches with explicit 2:1 edge stitching. Character animation and Rapier physics use fixed steps. Water rendering and queries share analytic components.

Heightfields do not support carved caves or overhangs. Water is an analytic surface, not a fluid solver. Physics supports authored primitives, compounds and static triangle collision, filtered queries and contact events through a narrow Rapier adapter. Finite field extraction reports unresolved semantic features; its general surface error is not certified. Vegetation has projected-size detail selection with hysteresis; hero character geometry stays at authored quality. Browser coverage and hardware performance must be verified on the actual release targets; a successful Metal run is not evidence for every GPU or shipping Safari version. Cloud accounts, marketplaces, downloaded plugins, and an arbitrary source language are outside this release’s design.

Licensed under MIT. Distributed third-party licenses are included in `THIRD_PARTY_NOTICES.txt` and standalone exports; visual content in the reference project is generated from authored definitions.
