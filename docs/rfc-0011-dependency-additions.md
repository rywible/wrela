# RFC 0011 — Pinned Dependency Additions

This file documents the third-party crates added (or feature-bumped) by the
RFC 0011 "Interactive Runtime" implementation. Pinning these versions keeps
the audit reproducible and makes the layering doctrine (`runtime/` does not
depend on `compiler/`) auditable from the manifest alone.

> Layering doctrine: every dep listed under `runtime/` MUST be a pure
> runtime/platform crate. The `compiler/` crate may use compiler-only deps
> like `cranelift-*`. New deps that cross the layer must justify themselves
> in the RFC and update this file.

## `runtime/Cargo.toml` (latency-first execution layer)

| Crate              | Pinned version                | RFC 0011 use |
|--------------------|------------------------------|--------------|
| `wgpu`             | `29.0.1`                     | C3 swapchain + GPU runtime contract |
| `winit`            | `0.30.13`                    | C3 surface/window/event loop and raw input pump |
| `cpal`             | `0.17.3`                     | C5/C6 audio device callback and underrun counters |
| `smol_str`         | `0.3.5` + `serde`            | C7 unified `TickInputEvent` / `TimestampedRawEvent` strings without per-event heap allocations |
| `rtrb`             | `0.3.2`                      | C4/C5 single-producer/single-consumer ring buffers for raw input and audio (no MPMC, no allocation) |
| `arc-swap`         | `1.7`                        | C5/M1 lock-free `VoiceLedger` published from the engine to the audio worker |
| `crossbeam-queue`  | `0.3.12`                     | bounded MPMC queue retained for non-real-time bridges |
| `crossbeam-utils`  | `0.8.20`                     | atomics + cache padding for the worker handshakes |
| `mio`              | `0.8`                        | platform polling for the input pump on Linux |

## `compiler/Cargo.toml` (closure / planning layer)

| Crate              | Pinned version | RFC 0011 use |
|--------------------|---------------|--------------|
| `wgpu`             | `29.0.1`      | shared engine-frame and presentation-exec resources (matches runtime pin) |
| `naga`             | `29.0.1` + `wgsl-in` | WGSL parsing for presentation pipelines |
| `ciborium`         | `0.2.2`       | H6 persistence record encode/decode (CBOR) |
| `zstd`             | `0.13`        | H6 persistence body compression |
| `pollster`         | `0.4`         | drives `wgpu` futures synchronously inside the closure executor |
| `cranelift-*`      | `0.115`       | compiler back end, never used from `runtime/` |

## `apps/reference_host/Cargo.toml`

| Crate         | Pinned version | RFC 0011 use |
|---------------|---------------|--------------|
| `wrela`       | path dep      | C3 host wires the compiler-side engine frame |
| `wrela_runtime` | path dep    | C3 host wires the runtime-side platform/audio/input |
| `winit`       | `0.30.13`     | C3 reference host event loop |
| `smol_str`    | `0.3.5`       | C3 inspector labels |

## Bumping policy

- Patch bumps inside the major series listed here MUST be done in a single
  commit that updates **all** crates with the same major (e.g. `wgpu 29.x`).
- `winit 0.30.13` is paired with `wgpu 29.0.1` because the surface contract
  matches; do not bump one without re-validating the other.
- `rtrb` and `arc-swap` are real-time-critical; do not introduce alternative
  SPSC/`Arc`-swap crates without revisiting C4/C5.
- `cpal` is the audio-thread boundary; new audio backends must keep the same
  callback semantics (no allocation, no `Mutex` in the callback, see C5/C6).
