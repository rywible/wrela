# wrela: the platform runtime

*Status: accepted direction, 2026-10-01. This is the one place that describes how a compiled game runs in the browser: what the compiler emits, the standard runtime that ships with every game, the threads, the CPU–GPU boundary, and console play. Decisions: D-016, D-017, D-018, D-069, D-082, and D-097 to D-102, in [decisions.md](decisions.md). If this document and the log disagree, the log wins. **Nothing here has been built.** The browser facts it rests on that haven't been tested are marked, and they're listed in the log's evidence table.*

## Goals

- **Keep the compiler ignorant of JS and the engine** (D-050). It emits WASM, WGSL and data; nothing else.
- **Keep bulk data where it's used.** The CPU–GPU boundary carries recipes and changes, not results (D-097).
- **Keep the host thin.** Mechanism lives in the host, policy in wrela (D-016).
- **Let old games keep working.** A runtime update must never break a game that has shipped (D-100).

## 1. What ships

```
wrela build ──► game.wasm     engine + game, CPU          ┐
            ──► *.wgsl        GPU entry points            ├ from the compiler (D-099)
            ──► manifest      pipelines, layouts          │
            ──► data/         compile-time constants      ┘
packaging   ──► runtime/      host JS, bootstrap HTML, service worker   ← the standard runtime, pinned version (D-100)
```

- **The compiler never emits JS, HTML or CSS.** Game code reaches the host through WASM imports declared in the stdlib's unsafe core. That's ordinary FFI (D-099).
- **Everything game-specific is data.** The manifest lists pipelines, bind group layouts and every `GpuData` layout (sketch 04 §1). The runtime reads it at load.
- **Packaging copies in the runtime version the game was built with.** A new runtime can't break an old game.

## 2. Who owns what

| | Owns | Does | Never does |
|---|---|---|---|
| **Runtime (JS)** | Tables from u32 handles to WebGPU objects | Decodes the command stream into WebGPU calls; wraps Web APIs as host functions | Hold game data, or do per-entity work |
| **WASM** | Sim state (regions), engine control state, command streams, upload staging | The sim, CPU field queries, deciding what the GPU runs, recording commands | Bulk presentation work |
| **WGSL** | Realized meshes, posed instances, tile lists, indirect-draw arguments, presentation-only state (particles, secondary motion) | Realization, posing, culling and LOD, drawing | Anything the sim reads (D-015) |

**Where work runs (D-097):**

| Work | Where | Why |
|---|---|---|
| Sim: gameplay, physics, AI | CPU, sim worker | Determinism (D-015) |
| Field queries the sim makes (raycasts, mass) | CPU | Strict floats; 3.2 µs per raycast (spike 01) |
| Spawn-time physique | CPU, helper workers | 39 ms per individual (D-091) |
| Posing for presentation | GPU compute | Spike 02's JS posing took 0.6–1.7 ms of main thread per frame |
| Sim-tier poses (D-034) | CPU | They're sim state |
| Realization, culling, LOD, binning | GPU | Spikes 01 and 02; nothing is read back |
| Drawing | GPU | |
| Audio | Audio worklet (WASM) | D-017 |

## 3. Threads

```
main thread ──input──────────────────────────► render worker
                                                 owns GPUDevice + OffscreenCanvas
sim worker ──presentation snapshot (per tick,──►  reads the latest snapshot, interpolates,
             double-buffered)                    runs presentation, records commands
helper workers: parallel combinators, physique, storage
audio worklet ◄──ring buffer──
        all share one WASM memory (a SharedArrayBuffer)
```

- **One worker makes every GPU call,** because WebGPU objects belong to the worker that created them (D-098).
- **The sim never waits on the frame, and the frame never waits on the sim.** Presentation reads the most recent finished snapshot and never writes sim state (D-015).
- **The shared memory's maximum is reserved at startup.** Growing a shared memory replaces its buffer object, and every view the host holds would need refreshing.
- **Cross-origin isolation (COOP/COEP) is required** for the shared memory (D-017).

## 4. The CPU–GPU boundary

**What crosses, per frame:** field values (a grazer is 1,312 bytes), the presentation snapshot, field edits, the camera. A few KB to a few hundred KB, so the copy shouldn't matter (an estimate). Results, such as meshes (about 0.9 MB per grazer at 3cm), never cross.

**The browser's constraints:**

| Constraint | Consequence |
|---|---|
| No zero-copy path from WASM memory to a GPU buffer; unified memory doesn't help, because the GPU process sits in between | Every upload is an explicit copy, kept small |
| Readback is asynchronous; probably 1–3 frames late (*hypothesis*) | Only for consumers that can wait: studio probes, picking, debug hashes. Never the sim. |
| One queue, no async compute | GPU realization competes with drawing; it's time-sliced (D-091) |
| WebGPU calls cost more than the JS↔WASM crossing (*hypothesis*) | Few large pooled buffers, few bind groups, indirect draws |
| No bindless; by default 8 storage buffers per stage and 128 MiB per storage binding | Pools are sub-allocated and GPU data is named by u32 offsets. The runtime requests the adapter's real limits. |
| No multi-draw-indirect in core WebGPU | One indirect draw per pipeline, with vertex pulling |
| `writeBuffer` from shared WASM memory: allowed by the spec, *untested* in Chrome, Safari and Firefox | If a browser refuses, the runtime stages through a non-shared buffer: one more copy |

**In the language (D-102, names are placeholders):**

```wrela
// `sample` is the tier-0 kernel in language.md §19.
let samples = gpu.buffer::<f32>(count: n)               // a GpuBuffer<f32>: a handle, unreadable on the CPU
gpu.write(lights_buf, lights)                           // an explicit copy
dispatch(sample, groups: n / 64, field: blob(0.5), grid: g, out: samples)   // records a dispatch; `g` travels as a uniform
let probe = gpu.read(samples.span(0, 16))               // a future, with the `nondet` effect (T2)
```

- **Writes and dispatches take effect in recorded order.** WebGPU orders dispatches itself, so there are no barriers between them, only inside kernels (D-093).
- **GPU calls carry the `host` effect,** so `@deterministic` code can't make them.

**The command stream (D-017, D-099):** WASM records commands into its own memory and the render worker's runtime decodes them in bulk. The vocabulary is generic: create a buffer, write a range of WASM memory into it, dispatch pipeline N with bind group M, draw indirect. Batching keeps WebGPU calls down. The same stream drives the native host, and a recorded stream can be replayed by agent tooling (D-021).

## 5. The runtime

Hand-written TypeScript, the same for every game (D-100):

| Part | Runs on | Does |
|---|---|---|
| Bootstrap | Page | The HTML template and headers; starts the workers |
| Main-thread shim | Main thread | Input, gamepad, fullscreen, pointer lock, a hidden `<input>` for IME text and accessibility, visibility; hands the canvas to the render worker |
| GPU decoder | Render worker | Command stream to WebGPU, using the manifest |
| Storage | A worker | Read and write bytes at a path, over OPFS. Synchronous access handles only exist in dedicated workers. |
| Fetch and streaming | A worker | Content beyond the time-to-play payload (D-020) |
| Audio | Audio worklet | The ring buffer over shared memory |
| Service worker | Per origin | Offline play and caching |
| Console bridge | Main thread | The versioned `postMessage` protocol, only when embedded in the shell |

- **Policy is wrela code.** Save slots, versioning and migration are engine and game code on top of "bytes at a path" (D-016).
- **A missing Web API is added to the runtime for every game.** A game never ships its own JS (D-099).
- **Each game origin caches its own copy** of the runtime, because caches are partitioned by origin (D-082). The runtime's budget is ≤ 1 MB (D-069).
- **The native host** (wasmtime + wgpu) implements the same interface, with files for storage and no console bridge. Headless agent runs, CI and dedicated servers run the same game WASM.

## 6. Console play

- **The shell (games.wrela.dev) is a wrela program** with its own copy of the runtime (D-100).
- **It embeds each game's origin in an iframe** with `allow="cross-origin-isolated; fullscreen; gamepad"`. Without `cross-origin-isolated`, the embedded game loses SharedArrayBuffer and threads (D-101).
- **A game's local saves live in its own origin's OPFS,** standalone or embedded. Games are served from subdomains of the shell's site, so the embedded game is same-site and keeps its first-party storage: the same saves both ways. A game on another site would get separate saves inside the console. *Untested in all three browsers.*
- **The shell brokers only what spans games:** sign-in, cloud-save sync, the library, save export and import, and returning to the console.
- **The bridge protocol is the one compatibility contract.** A new shell must embed old games, which carry old runtimes, so the protocol is versioned from the first release.
- **OPFS is best-effort** (D-018): call `navigator.storage.persist()`, encourage install, ship save export early.

## Open

- **The host interface definition:** its format, and how both hosts' bindings are generated from it (D-016).
- **The command encoding:** its binary format, and how the manifest is versioned within a build (sketch 04).
- **Kernels whose parameters exceed the target's binding limits** (D-102).
- **The bridge protocol's messages** and its versioning rule.
- **The names and syntax** of `GpuBuffer`, `gpu.write`, `gpu.read` and `dispatch` (D-102).
