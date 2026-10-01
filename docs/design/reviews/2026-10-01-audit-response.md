# Response to the 2026-10-01 audit

*Responding to [2026-10-01-audit.md](2026-10-01-audit.md). The owner delegated the choices, and the resulting decisions are D-067 to D-088 in [../decisions.md](../decisions.md).*

## Overall

The audit is accurate. Every piece of evidence it cites checked out against the files. Of 51 findings, 48 are accepted as written, 2 are accepted with a nuance, and 1 is answered differently from the suggested fix. It sharpened the record in three ways:

1. **Measure before designing further.** This is the most important finding. 66 decisions were accepted and none was validated. D-067 makes a hand-written measurement spike, with kill criteria, the next piece of work.
2. **U1, F8 and T2 turn out to be one finding.** Putting a field's structure in its *type* (D-070) answers the staging question (U1). It draws the structure/data line where the target needs it (F8). It also shows that every sketch so far can be served by ahead-of-time monomorphization, which is the evidence behind T2. Together they remove `@specialize`, the browser compiler tier, and the versioned IR. That's the largest simplification so far.
3. **The memory model's soundness gaps were real.** M1 (region-bound values escaping) and M3 (handles pointing outside the region) each could have broken rollback silently.

## Where this response differs from the audit

- **T1, nuance.** The audit is right that a hand-written BVH gets the same culling, so "100× vs a naive grid" isn't a performance claim. The compiler's real contribution is *generality*. Pruning, intervals and gradients apply to every field anyone writes, including fields written by agents, with nobody writing a per-asset acceleration structure. That's an ergonomics claim at scale, and it's part of thesis 1. Thesis 4's *performance* claim still has to be measured against baked assets, exactly as T1 says.
- **T2, nuance.** Deferring the browser tier softens the user's original thesis 4 wording, "access to the semantics of the game *at runtime*." vision.md now says so explicitly, rather than quietly reinterpreting it.
- **F7, a different fix.** The audit asks how library code names choice points. The answer here is that it doesn't. Masks are opaque (`LiveMask<F>`), and creature parts become an engine concept with their own `PartMask`, culled from derived per-part intervals. That keeps D-050's compiler/engine line intact.

## Disposition of every finding

| Finding | Outcome | Decision |
|---|---|---|
| T1 · Thesis 4 unmeasured | Accepted, with nuance (above): measurement spike with kill criteria | D-067 |
| T2 · Browser compiler tier | Accepted: deferred | D-069 |
| T3 · CPU field evaluation budget | Accepted: sim budget of ≤ 4 ms per tick; measured in the spike | D-067, D-068 |
| T4 · Thesis 1 untested | Accepted: authoring experiment plus agent syntax test | D-067 |
| T5 · No filtering story | Accepted: bandlimits promoted; footprint-aware noise | D-077 |
| T6 · Feature interactions and scope | Accepted: three feature tiers | D-088 |
| F1 · Unit names shadowed by locals | Accepted: separate unit namespace | D-076 |
| F2 · `@lipschitz` means two things | Accepted: `@assert` vs `@assume` | D-077 |
| F3 · `SdfBound` after `displace` | Accepted: `Exact` / `Bound` / `Lipschitz` kinds, `facts::` query | D-077 |
| F4 · `SimState` as a blocklist | Accepted: declared trait with a structural check | D-078 |
| F5 · NaN determinism hole | Accepted: canonicalize at observation points; trap on NaN creation in debug | D-074 |
| F6 · Leading `-` and `\|` | Accepted: only a leading `.` continues | D-079 |
| F7 · Masks mean parts | Answered differently (above): opaque masks plus an engine `PartMask` | D-080 |
| F8 · Structure/data line | Accepted: structure is types | D-070 |
| F9 · Implicit hoisting in `@audio` | Accepted: staging is guaranteed or rejected | D-072 |
| F10 · Special treatment moved to the stdlib | Accepted: closed list of lang items; admission test; acoustics to the engine | D-081 |
| F11 · Arithmetic differs by target | Accepted: numeric semantics table | D-074 |
| F12 · GPU intervals not sound | Accepted: widen outward | D-075 |
| F13 · One origin for every game | Accepted: one origin per game | D-082 |
| F14 · `^`, `Copy`/`.copy()`, music budget, cache keys | Accepted: `**`, `.clone()`, separate music budget, per-artifact cache keys | D-076, D-083 |
| M1 · Region-bound values escape | Accepted | D-084 |
| M2 · D-063's premise gone | Accepted: double-buffer only what crosses entities | D-085 |
| M3 · Handles to outside data | Accepted: deterministic keys | D-084 |
| M4 · Checksum cost | Accepted: incremental per-chunk hashes | D-084 |
| M5 · Exclusivity inside a dispatch | Accepted: invocation-safe `mut` kernel parameters | D-084 |
| M6 · Unstated receiver exception | Accepted: consuming methods need `take` | D-084 |
| M7 · Projection types; borrow extension | Accepted | D-084 |
| M8 · A save is a memory image | Accepted: keyframes are same-build only; saves serialize structurally | D-084 |
| M9 · Smaller points | Accepted: marking pre-pass, explicit container creation, `GpuData` layout, honest cost figures | D-084 |
| U1 · Staging model | Accepted: structure is types | D-070 |
| U2 · Generics and traits | Accepted: defaults chosen | D-071 |
| U3 · Effect lattice | Accepted: named effects and a per-context table | D-072 |
| U4 · Compile-time evaluation | Accepted: defaults chosen | D-073 |
| U5 · Fast-float semantics | Accepted: no fast CPU mode | D-074 |
| U6 · Units | Accepted | D-076 |
| U7 · Performance targets | Accepted: reference devices and budgets | D-068 |
| U8 · Engine in firmware or game | Accepted: compiled into each game | D-069 |
| U9 · Deforming creatures and distance fields | Accepted: shadow maps and per-bone collision | D-086 |
| U10 · The rest | Accepted: integer overflow, strings, destructors, async, after-trap; modules deferred | D-074, D-087 |
| S1 · The herd can't herd | Accepted: fixed in sketch 03 | D-085 |
| S2 · Move without `take` | Accepted: fixed in sketch 02 | n/a |
| S3 · Readback contradiction | Accepted: fixed in sketch 02 | n/a |
| S4 · Mask size | Accepted: fixed by opaque, typed masks | D-080 |
| S5 · Stdlib naming | Accepted: `std::`, written in wrela | D-081 |
| S6 · Stale text | Accepted: fixed in sketch 01 and D-063 | n/a |
| P1 · Accepted ≠ validated | Accepted: evidence table and revisit triggers | D-088 |
| P2 · No current-state language doc | Accepted: `language.md` required before compiler code | D-088 |
| P3 · Status drift | Accepted: statuses defined in the log header; CLAUDE.md updated | D-088 |
| P4 · Stale references | Accepted: fixed | D-081 |
| P5 · Lopsided sketch coverage | Accepted: the next sketches cover what ships, the agent loop and world-scale terrain | D-067, D-088 |
| P6 · Unqualified claims | Accepted: relabelled as hypotheses | D-088 |

## What's next

The audit's ordering stands, adjusted for the decisions above:
1. **The measurement spike** (D-067), with kill criteria.
2. **The agent-authoring experiment** (D-067).
3. **`language.md`** (D-088), so there's one current description of the language before any compiler code.
4. **Sketch 04: what ships.** Now that structure is types, this means the build's specializer output for sketches 01 and 02.
