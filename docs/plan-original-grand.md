# An original Wrela grand piano, and a path to an orchestra

*Execution plan, 2026-10-29. Requested by Ryan. Status: implementation in progress; see section 13 for scoped evidence and unmet gates. Baseline inspected on the ThinkPad at Wrela commit `807f9c080727c3ecae29d8e6bf43d0eb4b2a7c67`, toolchain pin 0.10.0.*

## 1. The goal and the authority to act

Build an original, entirely physically modelled concert grand: beautiful in an exposed solo, responsive across its full dynamic range, convincing under pedalling and repeated notes, and practical in a browser while a game runs. The artistic target is deep, clear bass, a singing middle, luminous treble, an intimate pianissimo, and a powerful forte that retains detail. The Fazioli F308 is an artistic reference [R1], not a blueprint, a brand for our instrument, or a claim of acoustic identity.

Ryan approved the physical approach and asked for a plan that an agent can hill climb autonomously. During implementation, the agent chooses algorithms, physical parameters, reversible experiments, test material and implementation order within this goal. Routine results do not require Ryan to listen, choose presets, tune coefficients or approve the next experiment. This document is the plan, not an instruction to start a background job when it is merely read.

All checkout work, builds, simulations, captures and benchmarks run on Ryan's ThinkPad through Tailscale, currently `wrela-t480s`, checkout `/home/ryan-wible/projects/wrela`. The local Mac is only a connection endpoint. Verify the remote hostname before work; if access fails, retain progress and request the required authentication rather than quietly moving work to another machine. Do not infer permission to use the Pop!_OS machine or benchmark the Mac from older plans.

Preserve unrelated work. The planning baseline had an untracked `runtime/` directory; it is not this task's output. Follow the current AGENTS.md. One PR holds the requested piano implementation scope; the orchestra roadmap below is later scope. Compiler changes, if needed, belong in the compiler repository and its own PR, not a second language inside this repository. This plan does not authorize spending money, contacting instrument makers, merging, publishing a release, or scheduling recurring work.

### Non-negotiable boundaries

- All shipped sound comes from Wrela code and physical state. No sample playback, sampled attack residuals, commercial preset extraction, fitted sample-library coefficients, neural audio generators or learned audio surrogates.
- The original instrument definition uses our authored geometry and material parameters, with units, bounds, uncertainty and provenance. Numerical optimization of those parameters against our own physical solver is allowed.
- Physics-derived mode tables, coupling matrices and radiation filters are allowed. They must be reproducible from our definitions and solvers, with no hidden recording input.
- Published principles can inform our original implementation. Cite sources; do not copy an implementation or a manufacturer's parameter set. Reading a paper is not a blanket patent clearance. Avoid a method with unresolved material rights questions by choosing an independently implemented alternative; record a genuine unavoidable issue rather than declaring it cleared.
- Preserve attribution for any older derived work while it remains. The new instrument must have no dependency on Salamander-derived data. Do not relabel existing derived tables as original.
- Keep the instrument, the player and the room separately audible and measurable. Reverb, louder playback, compression and impressive repertoire must not conceal an inferior dry instrument.
- This is a reduced physical simulation, not a claim to simulate every molecule. Identify every approximation and measure what it loses.

## 2. What exists, and what changes

The existing code is useful infrastructure, not a blank slate.

| Existing part | Evidence at the baseline | Decision |
|---|---|---|
| `engine/piano.wrela` | SIMD modal banks, inharmonicity, prompt/aftersound pairs, phantom partials, pedal damping, board and room | Reuse sound scheduling and proven numerical utilities where suitable; replace recording-dependent excitation and approximated coupling |
| `engine/piano_grand.wrela` | Salamander Yamaha C5 measurements: 16 keys × 5 velocities and frequency-band tables | Replace with an original, reproducible physical instrument definition |
| Sympathetic resonance | Two partials per key in a 44-group bank, driven by the mean string signal times pedal lift; current board filtering is feed-forward | Make resonance depend on each string's state and damper, including held keys with the sustain pedal up |
| Sounding-key management | `MAX_SOUNDING = 32`; quietest other key abruptly silenced at the cap | Preserve all 88 keys' physical state; use bounded, measured reductions instead of abrupt deletion |
| `engine/perform.wrela`, score code | Expressive scheduled performances; sustain and soft pedal | Preserve score behavior; add required controls without making the instrument depend on a particular piece |
| `examples/piano` | Single-note probe | Retain a probe and add physical-interaction fixtures |
| `studio/audio` | Rendering, spectral and decay analysis, model fitting, click checks | Reuse rendering and analysis; add physical-reference, convergence and provenance workflows |
| `studio/listening` | Shuffled, level-matched comparisons tied to Salamander and selected pieces | Generalize to candidate/champion pairs of our audio; separate timbre from performance and spatial presentation |

Do not carry old assumptions into the new core merely because the old tests assert them. Replace an obsolete 32-key-cap test with state continuity and bounded-cost tests. Keep the old behavior available only as long as needed for migration and regression; remove unused experimental paths before completion.

## 3. The instrument we are building

Working name: **Wrela Grand**. It is a name for our design, not an endorsement or registered-brand claim.

Start with a concert-grand scale of about three metres. Design the string scale, bridge locations, board and hammer progression together. The agent owns voicing inside the agreed character. All numbers below are engineering targets, not measured results or facts about an F308.

The definition includes:

- All 88 keys, A0 through C8; an authored transition from wound bass strings to two- and three-string unisons. The actual string count follows the design, not an assumed universal count.
- Per-string length, diameter/core/winding geometry, mass per length, tension, stiffness, loss mechanisms, strike position and bridge attachment. Check plausible stress and geometry before optimizing tone.
- Hammer mass, felt stiffness and dissipation, contact width, initial motion and interaction with each string. Soft playing must arise continuously from contact physics.
- A ribbed orthotropic soundboard: thickness field, grain direction, material properties, ribs, bridges, rim boundary and damping. Its structural modes belong to the whole instrument.
- Per-key dampers, their contact motion and losses; no dampers where the design specifies none. Sustain, sostenuto and una corda operate on mechanical state. Sostenuto latches the appropriate held keys when pressed.
- An original optional short-blow control can be explored after the three standard pedals pass. It must change action geometry/hammer motion, not merely scale volume or claim to reproduce a manufacturer's mechanism.
- Listener/microphone position, lid opening and an independent room definition. The dry board radiation must already sound compelling.

Before voicing, check static equilibrium: string stress, frame/bridge loads, downbearing, board deflection and rim constraints. Linearize structural modes about that equilibrium; a louder but mechanically impossible design is rejected. Tune unisons and stretched octaves using a declared weighted coincident-partial/beat-rate objective by register, then voice the instrument across registers. Document chosen deviations and their listening rationale. A defect gets a physical explanation or an explicit approximation; do not bury it in an unexplained per-note EQ table.

### 3.1 Parameter provenance contract

Every scalar/table has units, physical meaning, bounds and machine-checkable provenance. Generated/interpolated values inherit all input provenance and record generator/output hashes.

| Class | Allowed origin |
|---|---|
| `constant` | Cited general physical constants, independently encoded |
| `material` | Published material measurements with permitted use, as ranges/priors; identify conditions and uncertainty |
| `design` | Our geometric/mechanical rule with feasibility checks |
| `solve` | Our solver, inputs, version, refinement and output hash |
| `voice` | Our bounded parameter choice with experiment ID |
| `forbidden` | Recording/library fits, copied presets, named-instrument scale tables or manufacturer-specific parameter sets |

Public artistic descriptions can inform the brief but cannot silently supply parameters. Bulk third-party material data still needs suitable permission; citation alone is not a redistribution licence. Unknown provenance fails cooking. Audit numerical inputs, tests, tuning objectives, generated constants and evaluation defaults, not only import paths.

## 4. Architecture: detailed reference and real-time reduction

The research basis is a coupled mechanical/acoustic system [R2]. The following division is our engineering proposal.

### 4.1 Shared physical definition, independent reference checks

Create a plain instrument description that both solvers consume. Put mechanism in engine modules and the original instrument's authored data in content. Suggested locations, adjustable to repository conventions:

- `engine/acoustics/`: units/parameters, passive couplings, strings, contact, modes and radiation; introduce only the modules the piano actually needs.
- `engine/piano_physical.wrela`: assembled piano, action, dampers, scheduling and runtime state.
- `content/grand/`: instrument definition and generated data with their generation manifest.
- `studio/acoustics/`: detailed reference solves, reductions, parameter search and reports.
- `examples/grand/`: playable showcase and diagnostic controls.
- `engine/tests/grand/`: small focused fixtures.

These are proposed paths, not existing APIs. Add packages and allowlist entries as they become real. Keep generation tools in Wrela. Check the pinned toolchain's actual facilities before promising an eigensolver, audio telemetry API or MIDI endpoint.

P0-N is a numerical prerequisite milestone before P2, with primitive checks before P1 selects a runtime representation. Start with a Rayleigh–Ritz orthotropic plate and documented equivalent-rib approximation using small dense generalized eigenproblems. Verify mass positivity, mode orthogonality, eigenpair residuals and refinement. Full finite-element, sparse eigensolver and boundary-element infrastructure are optional escalations justified by a measured error. Original domain numerical algorithms written in Wrela belong in studio/engine; missing language, std or host capabilities belong in the compiler. A missing std matrix inverse does not itself prohibit implementing numerical methods in Wrela.

Use energy-conserving or dissipative discretizations with an explicit energy budget. A moving hammer, bow or boundary can supply energy; record that input rather than applying an incorrect blanket “energy always decreases” rule. Track stored energy, external work, dissipated energy and the residual.

The detailed solver initially covers a few strings and a modest board, with high precision and tighter time/space resolution. Grow resolution only when convergence studies show it matters. A larger solver is not automatically ground truth: check analytic limits, independent small-system solutions and refinement before treating its output as a reference.

### 4.2 Strings and nonlinear contact

For the reference, resolve transverse motion, stiffness, frequency-dependent losses and hammer contact explicitly. Add both transverse polarizations and longitudinal/stretch coupling where their contribution is significant; the low register is an important probe [R3]. Wound strings need a documented effective model and its limitations.

Benchmark reduced modal and dispersive waveguide representations against that reference. Default to a modal implementation to exploit existing SIMD work, but retain it only if it meets accuracy, stability and cost gates. Do not mix two full solvers for the same degree of freedom or count their energy twice.

Derive retained nonlinear forces consistently from the reduced potential and coupling law; do not add arbitrary modal forcing for tension modulation. Measure compliance lost by truncation at each contact point. Use a justified static-compliance correction or more modes, then test convergence of contact duration, peak force and hammer rebound. Initial targets on identifiable probes are <= 5% duration/peak-force error and <= 2% rebound-speed error against the converged reference. Compare a bounded dissipative or non-iterative runtime contact scheme with an independently solved reference contact problem. No particular SAV/quadratization method is mandated without checking its derivation and modified-energy interpretation.

Contact must see the current string motion: a repeated strike acts on a ringing string. Solve contact without attractive felt forces, implausible penetration or unbounded iteration on the audio thread. Prefer a stable bounded solve; if it fails, report a numerical failure and preserve finite output rather than concealing it in a limiter.

Use oversampling only where nonlinear contact or mode generation requires it. Include anti-alias filtering, its delay and cost. A permanently larger sample rate is not the first optimization.

### 4.3 Coupled unisons, bridge and body

Connect every string to shared structural degrees of freedom. The strings drive the board; the board changes their boundary motion. Derive a reduced positive-energy model from the detailed structure, retaining the coupling needed for decay, beating and sympathetic resonance.

At P2, select the actual coupling update. Prototype power-balanced low-rank bridge coupling with fixed structural factors and separately accounted dissipative damper/contact updates. Document whether it needs a solve, what changes the factors, and per-sample operations/storage. Target O(string modes + string contacts × retained board modes), not a dense all-termination solve. Demonstrate pedal/damper sweeps without stale factors; account for splitting error and stability. Alternative scattering/implicit formulations remain eligible if measured gates favor them. Before P2 exits, benchmark an 88-key-size synthetic system with varying dampers against the P6 consumer budgets.

Use low-rank shared modes or another justified reduction to avoid dense all-to-all string work. Passivity must survive reduction and interpolation. A quiet unstruck string can still receive energy; its state cannot be discarded because no note-on event occurred.

Generate attack, two-stage decay, sympathetic response and body motion through this system. Compare with intentionally decoupled ablations to show that the coupling is working for the right reason.

### 4.4 Acoustic output

Keep structural feedback separate from observation: moving a virtual microphone must not change how the strings decay. Include radiation loading in the physical model if required; count that damping only once.

Precompute directional radiation from our structure, then reduce it for runtime. Treat bass wavelength, body size, lid position and stereo coherence explicitly. A key's pitch alone must not determine its stereo placement.

Bound P5 explicitly: Rayleigh-integral radiation from our board modes under declared baffle assumptions, simple lid reflections, shoebox image-source early paths and a passive FDN late field. Check delays, spreading, absorption and band decay against the corresponding simplified analytic models; verify each formula's assumptions and cite its primary source when implemented. Full lid diffraction and an arbitrary-room wave solver are later work, not hidden prerequisites. Make no full-room accuracy claim before an independent reference exists. Separate internal damping from radiation loss so neither is counted twice. No downloaded impulse responses; kernels generated by our solver are allowed with provenance.

### 4.5 Runtime boundaries

Allocate and cook before playback; keep audio work bounded, preallocated and independent of rendering-frame timing. Bound event work too: define queue capacity, equal-time ordering, overflow recovery and per-callback limits. Never silently lose key-up/pedal-up; overflow must report an error and use a declared safe release path. Use sample-timed controls where the host supports them, handle the actual host sample rate, and explicitly adapt to the pinned host's block contract. Never assume a Web Audio block duration is end-to-end latency [R8].

All 88 keys remain represented and excitable, including retriggers and sustained tails. Negligible numerical residue need not persist forever. Cook an energy-to-output error bound for dormant modes, including future bridge excitation, and aggregate discarded error across voices. Flush to exact zero only below that bound, keep receiver coupling available, and wake before predicted output crosses it. Without a valid bound, keep modes active. Test near-threshold resonant drive and a 120-second pedal-held tail after a fortissimo cluster. Measure subnormal-state cost on the ThinkPad rather than assuming a CPU penalty or flush-to-zero behavior. Quality reductions can merge or truncate demonstrably negligible modes, with smooth state transfer and error bounds. They cannot erase a held note, a perceptible tail, or an unstruck string's ability to resonate.

CPU/WASM SIMD is the default runtime path. GPU work is suitable for offline generation experiments on the ThinkPad; putting it in the real-time path requires evidence that it meets deadlines during graphics contention and a viable fallback. No GPU promise precedes those measurements.

### 4.6 Gesture and clock contract

Define instrument events independently of the old MIDI/Played shape: stable event/note ID, key ID, audio-sample timestamp, hammer velocity in m/s at let-off, key-down/up or displacement trajectory, release velocity, repetition/rearm depth, and sample-timed pedal breakpoints. Document normalized-control/action mappings. MIDI is an adapter: zero-velocity note-on means note-off, not silent key holding; provide an explicit silent-key gesture.

Scores can schedule exact sample times immediately. Live controls need verified host-clock/audio-clock mapping, fixed declared delay D, lateness policy and jitter statistics. The current frame-ring/next-quantum path cannot claim capture-time accuracy. If capabilities are missing, record a toolchain gap and retain a labelled quantized adapter until fixed. A delay cannot reconstruct a timestamp never captured.

P4 tests the internal kernel with identical absolute event streams partitioned into 64, 128 and 256 sample chunks. The pinned browser adapter may still use 128-frame blocks. Compare event times and output metrics independently of chunking; do not claim arbitrary browser quanta. Test equal-time key/pedal ordering, sustain catch, sostenuto latch/release, partial repetition, soft-pedal changes during ringing, and queue overflow.

## 5. Evidence and acceptance contract

### 5.1 Three independent verdicts

Every candidate reports:

1. **Physical/numerical:** does the implementation follow its model, stay stable and converge?
2. **Musical/perceptual:** does the rendered instrument satisfy the listening rubric?
3. **Runtime/product:** does the actual browser stay responsive within the budgets?

Passing one never substitutes for the others. “Original” and “entirely physical” are provenance/architecture claims; “beautiful” is a listening judgment; “competitive with a concert grand” requires suitable external listening evidence. Do not infer any of these from a spectrum alone.

### 5.2 Fixed corpus and held-out evaluation

Before tuning, freeze and hash the generator, parameter distributions, score grammar and split rules in P0. Keep validation/final seeds sealed in the evaluator harness until the specified gate. Numerical and musical holdouts are separate. Generate and hash an original fixture corpus. Store the generator, seeds, exact events, control trajectories and expected causal effects. Retain recordings outside tracked source under normal build output.

Cover all 88 keys and at least 12 hammer-velocity levels in metres per second over a P0-frozen plausible interval, plus a normalized-controller-to-hammer mapping test; include long bass decays, exposed treble, adjacent-key transitions, dense chords, rapid repetitions, trills, silent keys held against a struck harmonic relative, pedal flutter, half-pedal sweeps, late pedal catch, sostenuto, una corda, overlapping same-key events, transport reset and control discontinuities.

Keep three sets:

- **Development:** cheap, diagnostic probes used for tuning.
- **Validation:** different notes/velocities/gestures used for milestone promotion; return aggregate scores/pass-fail, not critique text or per-fixture diagnoses. Once inspected for debugging, retire that set to development.
- **Final holdout:** a bounded sealed seed set drawn by the harness from ThinkPad system entropy under the frozen generator; reveal only after recording the candidate commit/protocol. A commit hash alone is gameable and not an independent seed. Persist commitments and reveal records.

Hold out physical regimes and combinations, not just identical notes with different filenames. If a holdout fails and informs a repair, it becomes development evidence. Allow at most two final submissions under one frozen protocol, with no selection among discarded seeds. After the second failure, freeze the champion and report failed qualification. Development can continue under a recorded new hypothesis/protocol epoch retaining the complete history; a new epoch is not another draw supporting the old success claim.

Musical phrases must expose independent melody/accompaniment dynamics, legato, detached playing, bass clarity, high-register attacks and pedalled harmony. Write original test material. Historical pieces already in the repo can be supplemental with their existing attribution; no third-party performance recordings are required.

### 5.3 Initial numerical targets

Freeze definitions and metrology in P0. These are starting engineering targets, not current results.

| Gate | Initial acceptance target |
|---|---|
| Finite behavior | No NaN/Inf, failed contact solve or growing unforced oscillation in adversarial fixtures |
| Rest | Exact zero from unexcited rest; no unsolicited noise/dither in the physical core |
| Analytic modes/decay | Pinned Euler–Bernoulli stiff-string modes within 1 cent over the declared resolved band; simple identifiable exponential-loss fixtures within 2% decay-rate error |
| E1: own discrete balance | f64 canonical fixtures: cumulative normalized residual <= 1e-6 and absolute per-step residual <= 1e-8 of local available energy; test all window sizes declared below |
| E2: unforced stability | f32/f64: total stored energy plus accounted loss obeys the unforced bound; no systematic positive residual, unstable cooked poles or growing quiet tails |
| E3: reference agreement | Same-definition runtime vs converged reference: total mechanical energy trajectory within 0.1 dB on salient windows, subsystem trajectories within 0.5 dB; report all mismatches |
| Reduced sound | Identifiable salient-mode frequency error median <= 1 cent/p95 <= 3 cents; band-energy error median <= 1 dB/p95 <= 3 dB; identified decay rates within 10%, with separate beat/knee/contact reports |
| Aliasing | Alias-isolated probe error below -70 dB relative to fixture peak using the specified oversampled comparison; other model differences reported separately |
| Lifecycle | Reset and queue recovery release resources; retriggers, pedals, tier changes and dormancy preserve state within their declared error bounds |

**E1/E2 definitions.** Name the scheme's actual discrete energy and whether it is physical or modified. For a step, residual = change in stored energy + accounted losses - external work. Include moving controls, boundaries, contact, radiation, state projection/tier changes and any external source. Never manufacture an unexplained “work” term to cancel a residual.

Report per-step residuals and signed/absolute sums over sliding 128-sample, 0.1-second and 1-second windows, plus the whole run. Normalize each window by energy at its start plus positive supplied work inside it, with a fixed per-fixture floor of 1e-12 times that fixture's declared maximum initial energy. Exact rest uses its separate zero test. The floor and units are fixed before candidates; cumulative historical forte strikes cannot hide later quiet-tail growth.

For f64 E1, each window's absolute residual sum must stay within 1e-6 of that local denominator. For f32 E2 use a starting 1e-3 window budget, no positive residual trend under longer unforced runs, and refinement/precision comparisons. Individual subsystem energy may rise from coupling: the no-input decay rule applies to the total closed system. Test exact undamped canonical systems separately from dissipative ones. Cook-time f32 checks must preserve passive/stable operators: check poles for isolated linear filters and an energy/passivity certificate for the coupled system. Independent poles inside the unit circle are not a coupled-system proof. Bias rounding toward dissipation where justified, then recheck acoustic error.

**E3 definitions.** Different schemes can conserve different discrete energies. Compare reconstructed physical observables with consistent units, not one scheme's modified-energy number against another's physical number. Use predeclared time windows and positive-energy floors; report near-zero tails with absolute/error bounds. A modified-energy method needs a refinement study relating its invariant to physical energy. E3 is a model-agreement measure, not a replacement for E1/E2.

**Acoustic metrology.** Define salience from the reference, at -60 dB relative to its note peak; a candidate cannot remove a missing mode from the denominator. Report missing/extra energy, register, velocity and attack/sustain/tail separately. Use one declared onset/delay alignment, never time-warp the attack away. Global medians cannot hide a failed register or salient missing mode.

For alias tests, compare a canonical nonlinear contact/mode-generation probe at runtime rate with the same equations solved at successively refined >= 4× rate and properly low-passed/downsampled. Establish high-rate convergence and time/discretization error first; use known probe fundamentals and reference difference bands to isolate foldback. Broadband attack or a different modal truncation is not automatically alias energy. If contributions cannot be separated within uncertainty, mark the alias result unresolved. Keep ordinary reduction error as a separate report.

**Independent anchors.** Verify a pinned stiff string, small two-string/bridge linear system against independently assembled eigenvalues, undamped power-law contact with a rigid wall against a direct energy-integral solution, a simply supported orthotropic plate, and a baffled-piston radiation fixture. Do not assume every coupled pair has a universal closed-form two-stage decay. Check physical dimensions and unit conversions independently of shared parameter loading; use different discretizations/coupling implementations where practical.

The detailed reference must change by less than one third of the runtime error budget across two successive refinements before serving as an acoustic reference. Analytic anchors and independent derivations catch shared mistakes that mesh refinement cannot. Native/browser exact sample equality is required only where promised by the host; disclose arithmetic differences.

### 5.4 Listening rubric and autonomous review

**P0 capability gate.** Actual audio comprehension is unverified. Test an already available, authorized evaluation path with generated controls; record model/version, location, accepted format, data flow, cost basis and reproducible invocation. Do not infer capability from a text model's name or from its ability to read a WAV header. Reviewers critique only; no model generates replacement sound.

If no qualified path exists, label the musical gate **blocked**, skip P7 promotion and continue independent physics/runtime work. The resulting state can be **physically verified, musically unevaluated**, never the completed piano goal. A new paid service or off-ThinkPad processing requires a concrete separate authorization; no permission request is needed merely to inspect existing capabilities. A local model is eligible only if its actual capability, licence and cost are verified.

Score 1 (broken), 3 (usable with a clear defect), and 5 (convincing/compelling) on: bass clarity/depth; midrange sustain and voice separation; treble brilliance without artifacts; dynamic/timbral continuity; attack/release/repetition; pedal/resonance; register consistency; and spatial solidity. Freeze examples of score meanings using our original neutral physical prototype and deliberately degraded versions. The old sample-derived piano is not the voicing target or fixed “3/5” anchor.

Render directly from the instrument at dry, player-position and room settings; bypass game mixers and soft-clipping. Use float WAV or verified lossless 24-bit input, not the existing undithered 16-bit listening export. Preserve hashes for the exact submitted audio. Match whole musical excerpts with one constant gain each using an independently checked ITU-R BS.1770-5 implementation [R9], with true-peak headroom and no compression. Very short or gated-silent probes use a declared fixed calibration procedure; never normalize notes separately. Retain calibrated unnormalized versions for dynamics tests.

**Fixed comparison batch.** For each promotion compare 24 preselected musical excerpts against the frozen champion, in both A/B orders: 48 judgments, with category coverage fixed in P0. Add 10 duplicate/order-control trials and four degraded anchors, all blinded. Require >= 9/10 consistent controls and all four known major defects recognized; otherwise void the batch and investigate evaluator reliability. One repeat is allowed only after a recorded presentation/evaluator correction; no repeating until a favorable vote.

Collapse the two orders to one per-excerpt win/tie/loss; disagreement is a tie and retained as such. Promote quality only with >= 16/24 wins, <= 2/24 losses, no severe defect or repeatable category regression, and all hard gates passed. Equal-quality cost promotion requires >= 22/24 non-losses plus a predeclared measured resource improvement beyond timing noise. Ties otherwise keep the simpler/lower-cost champion. These are conservative engineering selection rules, not statistically independent human votes or a population-preference probability. Report the full paired result, not a cherry-picked p-value.

Absolute rubric scores are milestone checks, not the optimizer's per-parameter reward: require median >= 4/5 in every category with no severe defect. Validation returns aggregate score/pass-fail only; development returns critique. Freeze evaluator settings and label assignments, and log every challenger to make sequential selection bias visible. Final qualification uses the sealed protocol and capped submissions in §5.2.

A later real-acoustic comparison may use authorized recordings, with documented rights, solely as evaluation and never as optimizer input. It is optional for this original-instrument goal. Without it, say “agent-evaluated original physical grand,” not “indistinguishable from an acoustic piano.” A local defect suite, spectra and text descriptions never substitute for actual audio review.

### 5.5 ThinkPad runtime targets

First inventory CPU, RAM, OS, browser, power mode, sample rate and audio backend on the actual ThinkPad. Use the installed supported browser and record its exact version. Do not extrapolate an offline render speed to browser stability.

Initial **Concert** target, with all 88 keys addressable and 88-key resonance state:

- Establish 48 kHz/128-frame synthesis against the verified pin first: existing studio code hard-codes these values. Record AudioContext and device/backend rates and browser/OS resampling. Arbitrary-rate engine support is a separate capability.
- At a 128-frame/48 kHz callback, p99 instrument-plus-spatializer compute <= 1.0 ms, p99.9 <= 1.5 ms and no callback over its 2.667 ms period in three 10-minute representative runs. At a different supported block/rate, use the same period fractions and record them.
- Zero observed audio underruns in those runs, alone and with the capybara game/keys scenarios whose input scripts, resolution and graphics settings are frozen in P0. Record graphics frame times, AC/battery state, CPU governor, warmup and thermal state. Use AC for the primary contract and separate battery results. Report instrumentation coverage; callback timing does not prove absence of device/backend underruns.
- Instrument-owned working memory <= 64 MiB and compressed generated instrument data <= 8 MiB. Report application/compiler overhead separately; do not omit it from total memory/download figures.
- No playback-time allocation, blocking I/O, synchronous GPU wait or compilation. Report events-to-output timing separately from compute. Physical keyboard-to-speaker latency remains unmeasured unless a proper loopback setup is available.
- A 30-minute final mixed-interaction soak with bounded memory and no state leak; include the 120-second near-silent tail in the timing distribution.

Provide **Studio**, **Concert**, and **Ensemble** for existing game consumers within P6/P8. All derive from the same original definition; Ensemble uses validated reductions and its own listening comparison, without relaxing Concert's target. The full capybara callback (piano plus effects) must retain its existing p99 <= 1.3 ms contract in its native test and the new browser test. The keys callback must meet the same aggregate target with both pianos, click and mixing; p99.9 <= 1.5 ms and deadline/underrun gates also apply. Share immutable cooked data, not independent dynamic states. A 1 ms solo result does not qualify two instances. If no original tier passes, that migration remains incomplete; do not retain a Salamander default silently. Change quality smoothly at safe points, never by dropping a ringing instrument.

These are ambitious initial budgets. If infeasible, keep optimizing and report the precise tradeoff. A formally revised budget must appear as a dated target revision with old/new values and rationale; it cannot count as passing the original budget. Do not stall useful work just because a target is missed.

### 5.6 Local defect suite and estimator calibration

Before listening, check unexpected discontinuities, attack/release artifacts, velocity/neighboring-key loudness and timbre discontinuities, scale inharmonicity, beat rates, prompt/aftersound/knee behavior, aliasing and damper decay. Respect intentional register transitions; do not enforce universally monotonic brightness or two-stage decay on every note. A click detector can flag a legitimate hammer transient: compare contact/reference behavior rather than suppressing all flagged attacks. These checks reject defects; they cannot award beauty.

Existing `studio/audio/measure.wrela` uses a typical-piano partial search and fixed spectral windows. Reuse infrastructure, not unverified precision. Compare eigenvalues directly where appropriate. For audio, select/cite an estimator after synthetic damped-single/doublet tests across A0–C8 and representative durations establish bias and identifiability. FFT bin width alone does not prove a frequency estimator incapable, and a high-resolution method cannot magically resolve an unidentifiable trace.

Measurement uncertainty must be below one third of the relevant gate budget; target <= 0.1 cent bias on identifiable probes and <= 2% decay-rate error. Mark unresolved modes as unresolved. Report prompt/aftersound rates, relative level/knee and beat rate rather than one T60. Hold bass probes long enough to identify their rate or report a bound. Measurement uncertainty and reference convergence are separate checks.

## 6. Milestones and exit evidence

Each stage leaves working code, a minimal focused regression set and an evidence record. Track its physical, musical and runtime subgates separately. If audio evaluation is unavailable, later independent physical/runtime work may proceed with musical subgates explicitly pending; neither the stage's full acceptance nor P8 is passed. Early listening corpora use the keys and gestures implemented at that stage; the full-key corpus becomes mandatory at P3. Run cheap local checks during experiments; broaden only for changed dependencies, integration or final readiness.

| Stage | Deliverable | Exit evidence |
|---|---|---|
| **P0: establish the experiment** | ThinkPad inventory; baseline capture; original corpus; candidate/champion harness; rights/data inventory; verify available toolchain and audio-evaluation capabilities | Reproducible baseline hashes; calibration and measurement self-tests; fixed budgets, manifests and known gaps |
| **P0-N: numerical prerequisites** | Verified dense modal/plate tools, units and coupling prototypes; mode/contact/coupling cost model | Analytic/eigenpair checks pass; measured throughput supports the representation before P2 |
| **P1: one struck string** | Original string and felt model; high-precision reference; bounded runtime contact; analytic and refinement checks | Physical gates pass on low/mid/high probes; audible continuous dynamic response; no sample-derived parameters |
| **P2: a three-note instrument** | A bass note, midrange multi-string unison and treble note on one coupled board | Repeated strikes and damping pass; coupling ablation behaves as predicted; listening identifies no dominant synthetic artifact before expansion |
| **P3: the full grand** | All 88 keys, authored scale and hammer progression, persistent string states and selective dampers | Full-key sweeps, held-key sympathetic tests, register continuity and dense sustained chords pass; no hard 32-key cap |
| **P4: pianist control** | Continuous sustain/half-pedal, late catch, sostenuto, una corda, repeat/release behavior, timestamped live and score controls | Gesture fixtures pass in native and browser paths; no stuck notes or block-boundary-dependent control result beyond declared numerical tolerance |
| **P5: instrument radiation and room** | Position/lid-dependent output, separate physical room approximation and common calibrated gain | Dry gate remains passed; spatial ablations, mono compatibility, no doubled body damping, and independent room review |
| **P6: runtime tiers and consumers** | Verified Concert/Ensemble reductions, state-preserving changes and bounded scheduling | Solo and aggregate budgets pass with graphics; reference/tier listening comparisons pass validation |
| **P7: autonomous voicing** | Parameter search inside plausible design bounds, musical corpus, champion archive and defect-driven iterations | Listening targets pass without numerical or runtime regressions; repeated baseline/champion comparisons stored |
| **P8: release candidate** | Integrate showcase and real consumers; retire old default data path; expose useful controls; documentation and provenance audit | Fresh holdout, soak, integration checks and no sample-derived production dependency; reviewable PR with all claims scoped to evidence |

Dependencies: P0 -> P1 -> P2 -> P3 -> P4, with P0-N primitive checks before P1 chooses its representation and the full numerical/cost contract before P2. Radiation experiments can begin after P2, but P5 integrates after P4. Measure cost at every stage; P6 is the final optimization gate, not the first timing measurement. Begin listening at P1 rather than waiting for P7. P7 refines the instrument through the whole loop; P8 freezes it.

Do not make a full high-resolution 3D piano simulation a prerequisite for P1. Begin with an independently verified reduced physical problem and add complexity when a specific error or missing phenomenon demands it.

### P0 must resolve these actual unknowns

1. Is the pinned Wrela command installed and usable on the ThinkPad? The planning inspection did not establish that. Follow the documented install/pin process if needed; do not compile on the Mac.
2. Can the current tool host generate matrices and perform the required bounded numerical solves? Prototype the smallest case before selecting a large eigenproblem.
3. Does the browser host expose callback durations, underruns and sample-timed event injection? Inspect current compiler/std/host contracts; record demonstrated gaps in `docs/toolchain-gaps.md`.
4. Can an already available path assess actual audio within current ThinkPad-only and spending/processing permissions? Register model/version, execution location, data flow and cost basis; test generated controls. The reviewer harness is text/image oriented and studio expects human listening, so capability is unverified. Do not infer no other approved capability exists. If none qualifies, mark musical review blocked immediately and skip P7 attempts; never substitute spectrogram ratings. New paid or off-ThinkPad processing requires a concrete separate request, not an assumed exception.
5. Which existing consumers import `GRAND` or call the piano APIs? Inventory them before migration; preserve score and interactive behavior.
6. Existing studio constants/generated worklet indicate 48 kHz/128 frames; verify the pinned source rather than assuming a newer web API.
7. Microbenchmark native and browser SIMD throughput in resonator-samples/ms, including memory traffic. Cost modes by register, board/coupling work, oversampled contact and multiple instances before P1; update against P2 measurements. Driven-build speed is a separate relative measurement, not browser throughput.
8. Audit timestamp mapping, per-quantum timing and continuity instrumentation; document demonstrated gaps. If available, capture a deterministic pilot at the PipeWire/Pulse sink monitor, self-test with inserted gaps, and account for clock/resampling alignment. This measures monitored software output, not the DAC/speakers. Freeze instrumentation-overhead limits and disclose coverage.

For a toolchain gap, produce a minimal reproduction and a precise general capability request. Continue independent physical work while it is unresolved. If compiler implementation becomes necessary, use the ThinkPad's compiler checkout and its instructions; engine policy stays here. A missing host measurement is “unmeasured,” not zero.

## 7. The autonomous hill-climbing procedure

### 7.1 Durable experiment record

Keep the authoritative ledger at `/home/ryan-wible/wrela-evidence/grand/<campaign-id>/`, outside disposable build output, with a concise committed summary in this plan or a dedicated allowlisted report. Use atomic writes/checksums and verify recovery after clearing scratch builds. Retain champion inputs/data, promoted renders, seeds and judgments there; only reconstructible challenger caches belong under build/. Each experiment records:

- ID, parent champion, source commit and dirty-diff hash, toolchain version.
- Physical-definition hash, generated-data hash, units and provenance.
- Hypothesis; one intended change; predicted audible and physical effects.
- Corpus/seed IDs, solver resolutions, evaluation version and level-matching settings.
- Hardware/browser/rate/block/graphics-load conditions.
- All gate results, timing distributions, memory/data sizes and raw audio/report paths.
- Listening judgments including contradictions, decision and explanation.
- Best next experiment and any externally blocked gate.

Keep candidate and champion outputs separate. A journal entry or a source constant alone is not a reproducible instrument: generation inputs, tool version and output hashes must agree. Store promoted evidence in the durable ThinkPad directory; commit compact summaries and regeneration code. Never commit someone else's reference audio.

### 7.2 One iteration

1. Select the largest remaining audible defect or failed gate. Give it a specific hypothesis: for example, weak upper-bass sustain caused by excessive bridge loss.
2. Determine whether it is a solver error, an inadequate physical model, poor voicing, performance control or presentation. Do not use voicing to compensate for a solver bug.
3. Make one small structural experiment or a bounded batch of physical parameter variants. Start with coordinate/log-space sweeps or derivative-free search; no need for a general optimizer framework before it pays.
4. Reject implausible geometry/materials, unstable couplings, energy failures and numerical errors before expensive audio review.
5. Render development probes; compare against the verified detailed solver. If improved, run the relevant musical excerpts.
6. Evaluate blinded audio and measured resources. Keep a Pareto set while exploring, but designate one deployable champion.
7. Promote only when hard gates pass and musical quality improves without a material regression, or equal musical quality costs less. Commit the rationale and focused regression for any actual bug fixed.
8. Run milestone validation when the change affects a new subsystem or completes a stage. Run the final holdout only on a frozen release candidate.
9. Record the next action before ending a work session. Resume from that action rather than replaying every benchmark.

A louder result, more reverb, a brighter centroid, a longer tail or a lower spectral error is not automatically better. No single weighted score may trade away a hard failure. Keep instrument physics, voicing, performance and spatial settings in separately labelled experiments.

### 7.3 Search bounds, plateaus and stop rules

- Freeze physical search bounds before optimization. Add a key-specific exception only for an explicit mechanical reason.
- Start each parameter batch with <= 24 variants and short probes. At most three candidates reach the fixed listening batch; log all candidates and selection decisions.
- After three batches without reproducible improvement, change hypothesis class: solver/discretization, physical model, voicing, control or presentation. A new numeric step size alone is not a new hypothesis.
- Bound one development job to 30 minutes, one offline reference job to two hours, and one work session to four hours, with checkpoints between jobs. A proven reference computation may be split into resumable chunks; review any escalation in the ledger before running it.
- Cap a milestone search campaign at 12 parameter batches or 16 compute-hours, whichever comes first. On exhaustion, freeze evidence and decide whether a specific new physical/model hypothesis justifies a new recorded campaign; do not silently reset counters. Session/campaign checkpointing is not permission to schedule unattended future work.
- If all required gates pass and three challengers from genuinely different hypothesis classes cannot improve validation, freeze the candidate and run P8. This is search convergence, not a global optimum.
- Final qualification has at most two submissions per protocol (§5.2). A new development epoch preserves failures; no repeated fresh draws to obtain a lucky pass.
- A rubric/budget change is dated, retains old targets/results and requires fresh validation. It never turns the historical failure into a pass.

Use explicit execution outcomes: **complete** (P8 passed); **converged below target** (champion frozen, target misses and next hypothesis reported); **physically verified, musically unevaluated** (no qualified evaluator); or **blocked** (specific external dependency). These describe the implementation report, not automatic changes to an app goal's state. Convergence below target is not completion: an agent may continue a documented new campaign if there is a materially new hypothesis and authorized resources, but must not churn indefinitely on the same one. Independent stages continue during a genuine blocker.

Keep the champion, remove losing code, preserve useful failure evidence and record the next action before every checkpoint. Do not ask Ryan to rank routine candidates. Ask only for an actual external permission/capability decision that cannot be resolved within the approved scope.

## 8. Product and delivery

The showcase should let a user play, hear a short original performance, change touch response, use the standard pedals, move from the bench to an audience position, and adjust the lid/room. Put scientific diagnostics in a separate developer view. No implementation jargon in the musical controls.

The default preset is one finished concert grand, not a shelf of experiments. Additional voicings come later if they preserve a coherent original instrument and have their own evidence. Good headphones may reveal details, but the mix must also be checked in mono and through a declared small-speaker simulation; no claim of actual speaker listening without doing it.

Before the implementation PR:

- Move every intended consumer to the new default and run the smallest relevant integration tests.
- Trace the shipped dependency/data closure; confirm no Salamander tables or library-derived coefficients enter it.
- Remove obsolete runtime defaults and dead experiments; retain historical attribution where required.
- Audit inherited constants/tests too: old bloom/crest/knock targets, tuning, `studio/audio/model` outputs and sfizz defaults must not enter the new physical or voicing path. Old-piano captures are for migration/cost comparisons only, not acoustic optimization anchors.
- Record source references in `docs/citations.md`, real toolchain gaps in `docs/toolchain-gaps.md`, and measured performance in `docs/bench.md`.
- Include baseline/champion/reference comparisons, generated-data provenance, ThinkPad environment, passed/missed targets and exact replay steps.
- Run repository-required readiness checks once at the end. Report unrelated pre-existing failures separately, without deleting another session's files or weakening checks.
- Push the single piano-scope PR for review. Do not infer merge/release authorization from other plans.

## 9. Extending this to an orchestra

Yes: a physical orchestra is a credible long-term direction. Published work covers bowed-string friction [R4], reed/bore synthesis [R5], valved brass [R6] and coupled percussion/air systems [R7]. Those sources establish mechanisms, not a promise that our browser implementation or sound quality already exists.

### 9.1 What should be shared

Share verified numerical primitives, energy-accounting conventions, contact solvers, structural reductions, acoustic propagation, sample-timed controls, rendering/evaluation tools and provenance. Share the authored scene's listener and room so instruments inhabit one space.

Expose each instrument's physically meaningful controls. A piano wants hammer/action and pedal trajectories; a violin wants bow speed/force/contact position and fingering; a trumpet wants breath pressure, lip state and valves. A note-on/velocity interface can be an adapter, but cannot be the complete orchestra model.

Keep the performance layer distinct from sound production. It translates score intent into continuous, feasible player gestures: breath, bow changes, tonguing, legato, fingering, vibrato and phrase direction. Mechanical fidelity cannot make a rigid performance expressive by itself.

Extract a general instrument interface only when the piano and a second instrument establish its actual common needs. Do not hold the piano hostage to a universal graph editor or orchestra framework.

### 9.2 Family roadmap and gates

| Order | Family | Reuse | New difficult work | Demonstration gate |
|---|---|---|---|---|
| O1 | Harp and simple struck bars, then celesta | Strings/contact/body modes, damping and radiation | Pluck/finger release; bar geometry; mallet contact; harp pedal tuning | Exposed arpeggios and repeated strikes with natural decay and full parameter provenance |
| O2 | Solo violin/cello, then viola and bass | String/body coupling and acoustic output | Bow stick-slip, torsion/contact width, fingering, bow changes, bridge/body differences | Stable playable range; legato, detached strokes, dynamics and bow changes without synthetic chatter |
| O3 | Clarinet, then flute, oboe and bassoon | Passive networks, controls, radiation and evaluation | Reed/jet excitation, bore losses, toneholes, register transitions, breath/tongue gestures | Attack/legato/dynamics across registers; pressure/fingering changes behave causally |
| O4 | Trumpet, horn, trombone and tuba | Bore/radiation and continuous player control | Lip oscillation, valves/slide, nonlinear propagation, articulation and pitch locking | Soft entries, slurs, register changes and bright fortes without instability |
| O5 | Timpani, drums, cymbals and orchestral percussion | Contact, structural modes and spatial output | Coupled membranes/cavities, snare collisions, nonlinear plates and broadband detail | Rolls and varied strike positions; no repetitive identical-hit artifact or uncontrolled metallic modes |
| O6 | Chamber ensemble, then full orchestra | All validated families and one room | Player behavior, divisi, sections, resource allocation and ensemble balance | Exposed solos, chamber interplay and a dense original tutti all retain clarity and musical intent |

Harp tuning changes must account for changing boundaries/tension and their work. Bow and wind exciters remain nonlinear feedback systems; they are not alternate attack envelopes attached to the piano. Flute jets are a different mechanism from reeds. Cymbals are a particularly hard nonlinear broadband problem; a few static resonances are an exploratory baseline, not the endpoint.

Voice/choir requires vocal-fold and vocal-tract work and intelligibility controls. It is outside the initial orchestra scope.

Each O-stage inherits all three verdicts. Verify family-specific canonical anchors before musical expansion: bowed-string periodic motion and a documented bow-force region; reed onset thresholds; cylinder/cone bore impedance; membrane/bar modes and cavity coupling. Verify assumptions and cite the chosen primary methods. Free-pitch families need a virtual-player intonation controller with declared pitch/gesture goals and cents-error reports that retain intended vibrato/bends.

Qualify the evaluator again on instrument identity, articulation and ensemble balance. Piano ratings do not establish that competence. R5 motivates reed/bore research, not an assumed passive implementation; prove the selected update's energy behavior.

One audio thread remains the baseline. Measure chamber groups before choosing O6 scheduling. If projected costs miss deadlines, compare quality reductions with explicit parallel-render/latency support as a toolchain decision. Do not presume several worklets execute concurrently or mandate parallelism before measuring alternatives.

### 9.3 A section is a group of players

Model independent but coordinated gestures, instrument variation within plausible bounds, seating and directional radiation. Give players persistent identities, not fresh white-noise timing offsets every note. Distinguish a shared conductor/phrase intent from each player's bowing, breathing and small intonation variation. Use explicit seeded behavior for reproducibility.

A string section must support shared bowing and independent motion, divisi, entrances and quiet sustained intervals without phase-locked doubling or excessive chorus. Keep exact expressive solos even when background sections use a validated reduction.

Progress through a piano/violin duet, string quartet, mixed chamber group and an original orchestral passage before scaling player count. A long-term target is a 60-player orchestration with a shared room in real time; it is an unmeasured research target and not part of the piano's acceptance contract. Set device budgets from the measured per-family costs before promising a full orchestra on this ThinkPad.

The orchestra shares a room; it does not need an expensive all-to-all mechanical coupling between instruments. Add inter-instrument acoustic excitation only if an audible, measurable benefit justifies it.

## 10. Sources and their use

Sources accessed 2026-10-29. They inform original code and this plan. No code, audio, preset or fitted parameter set was imported. Research descriptions are not performance measurements of Wrela.

- **R1 — Fazioli, F308.** [Manufacturer description](https://www.fazioli.com/pianoforti/f308/). Artistic context: power, clarity and expressive quiet playing. No design copying or numerical parameter source.
- **R2 — J. Chabassier, A. Chaigne and P. Joly (2014), “Time domain simulation of a piano. Part 1: Model description.”** [Institutional record and DOI](https://researchportal.ip-paris.fr/en/publications/time-domain-simulation-of-a-piano-part-1-model-description/). Coupled strings, nonlinear hammer, orthotropic board and surrounding acoustics; basis for the detailed-reference concept.
- **R3 — B. Bank and L. Sujbert (2004), “A Piano Model Including Longitudinal String Vibrations.”** [DAFx paper](https://www.dafx.de/paper-archive/2004/P_089.PDF). Motivates testing longitudinal/stretch contributions in bass tones. Its recording-derived soundboard procedure and parameter data are not adopted.
- **R4 — E. Matusiak, V. Chatziioannou and M. van Walstijn (2025), “Numerical modelling of elasto-plastic friction in bow–string interaction with guaranteed passivity.”** [Institutional record](https://pub.mdw.ac.at/pubmdw/publication/e0518e7b-5d7d-4ff0-aaf1-83f35848189e). Bow contact and energy-stable methods for the later string family; the record identifies the article as CC BY 4.0, not a licence for an unrelated implementation.
- **R5 — G. Scavone, “Digital Waveguide Modeling of Air-Driven Reed Generators for the Synthesis of Brass and Woodwind Instrument Sounds.”** [Conference source](https://compmus.ime.usp.br/sbcm/1995/papers/Gary_Scavone.html). Research direction for reed/bore systems; verify full method and applicable rights before choosing an implementation.
- **R6 — “An algorithm for a valved brass instrument synthesis” (DAFx 2015).** [University-hosted paper](https://www.pure.ed.ac.uk/ws/portalfiles/portal/21857362/DAFx_15_harrison.pdf). Later brass excitation, bore, valves and radiation research; no implementation imported.
- **R7 — University of Edinburgh, NESS, “3D Percussion,” including Bilbao and Webb's timpani work.** [Project description](https://www.ness.music.ed.ac.uk/archives/systems/3d-embeddings.html). Membrane/air coupling and physical percussion; no demonstration audio imported.
- **R8 — W3C, Web Audio API.** [Specification](https://www.w3.org/TR/webaudio/). Audio rendering/timing context; implementation must still inspect the pinned Wrela host's actual contract.
- **R9 — ITU-R BS.1770-5 (2023), loudness and true-peak measurement.** [Official recommendation](https://www.itu.int/rec/R-REC-BS.1770-5-202311-I). Independent metering implementation; a loudness score does not measure beauty.

## 11. Execution status

- [x] Plan written against the ThinkPad checkout; original physical instrument and later orchestra scope distinguished.
- [ ] P0 experiment, environment and evaluation capabilities established.
- [ ] P0-N numerical prerequisites and cost model verified.
- [ ] P1 single-string reference/runtime verified.
- [ ] P2 coupled three-note instrument passed.
- [ ] P3 all 88 keys and original physical definition passed.
- [ ] P4 complete pianist controls passed.
- [ ] P5 radiation and room passed.
- [ ] P6 ThinkPad Concert/Ensemble and aggregate consumer runtime passed.
- [ ] P7 autonomous voicing and listening passed.
- [ ] P8 final evidence, integration, provenance and PR ready.
- [ ] O1–O6: later scopes, not started or authorized for implementation by this piano plan.

**First implementation action:** P0. Verify the ThinkPad's pinned toolchain, audio host and evaluator; inventory current piano consumers; establish and hash the original fixture corpus before changing the sound.

## 12. Opus 5.5 review and disposition — 2026-10-29

Requested by Ryan, executed read-only through Claude Code CLI 2.1.301 on the ThinkPad with explicit `--model claude-opus-5-5 --effort high`. Initialization/result reported `claude-opus-5-5`; no substitute model was used. Review baseline: `e5bca960`. The reviewer inspected this plan and engine, studio and browser artifacts. The lead agent adjudicated and revised the plan; no DSP was implemented.

| Finding | Disposition and amendment |
|---|---|
| B1: evaluator availability | Accepted capability gate/local defect suite (§5.4/§5.6). Rejected a categorical claim that no approved evaluator can exist or permission must precede discovery. New processing/spending still needs authorization. |
| B2: consumer budgets | Accepted early throughput/cost model and required Ensemble tier, whole-game and two-piano budgets (§5.5/P0/P6/P8); no old-data fallback. |
| B3: coupling cost | Accepted actual update/factor lifetime, varying dampers, passivity and 88-key-size benchmark (§4.3). A dense per-step solve is not presumed inevitable. |
| B4: numerical infrastructure | Accepted P0-N/small dense plate model. Rejected the inference that original numerical Wrela code is a prohibited workaround or full sparse FEM is mandatory (§4.1). |
| H1: energy | Accepted E1–E3, local windows, named energies and precision checks (§5.3). Individual pole checks alone do not prove coupled passivity. |
| H2: nonlinear/contact | Accepted reduced potentials, compliance/contact gates (§4.2). No mandatory uncited SAV method; compare justified stable alternatives. |
| H3: evaluation bias | Accepted sealed seeds/generator, limited feedback, fixed paired batch and qualification cap (§5.2/§5.4). Rejected commit-only seeds, human-population inference from one judge and a sample-derived acoustic anchor. |
| H4: controls | Accepted gesture/clock schema, demonstrated host gaps, lateness policy and internal chunk invariance (§4.6), without demanding unsupported browser block sizes. |
| H5: dormancy | Accepted bounded sleep/wake behavior and 120-second tail fixture (§4.5); CPU penalties must be measured. |
| H6: metrology | Accepted eigenvalue checks, estimator calibration/identifiability and decay/beat measures (§5.6). FFT bin spacing alone does not establish estimator error. |
| H7: provenance | Accepted typed origins and inherited constant/test/tuning/evaluator audit (§3.1/§8); old attribution stays while old work remains. |
| M1: browser evidence | Accepted exact pin/rate, timing/continuity audit, pilot-capture scope and graphics/power/thermal conditions (§5.5/P0). |
| M2: independent checks | Accepted units/independent derivations and analytic anchors (§5.3); no universal closed-form two-stage decay or isotropic formula applied to an orthotropic board. |
| M3: search limits | Accepted bounded jobs/sessions/campaigns and honest below-target/unevaluated outcomes (§7.3); these are reports, not app-goal tool instructions. |
| M4: room scope | Accepted bounded P5 approximations; arbitrary-room wave simulation is later (§4.4). |
| M5: audio presentation | Accepted direct renders, float/24-bit audio, BS.1770 matching and separately calibrated short probes (§5.4). |
| M6: durable ledger | Accepted evidence outside build caches and recovery verification (§7.1). |
| M7: orchestra | Accepted family anchors, intonation, evaluator qualification and scheduling feasibility (§9.2); parallel audio is a measured option, not an assumed prerequisite. |
| Low: sympathy, aliasing, velocities | Clarified the feed-forward bank (§2), alias-isolation procedure (§5.3) and physical velocity units (§5.2). |

Full review, request and provenance manifest are retained at `/home/ryan-wible/wrela-evidence/grand/plan-review-2026-10-29/` on the ThinkPad. Review improves the execution contract; it does not establish instrument quality, runtime feasibility or evaluator availability.

## 13. Implementation evidence — mechanics checkpoint, 2026-10-29

Implementation is in progress on the ThinkPad. **No P-stage is marked complete, no musical
champion has been promoted, and no “best piano” claim is made.** The current deliverable is
a working numerical laboratory and physical reference primitives, not the finished instrument.

- Installed the pinned official Wrela 0.10.0 Linux release and verified its published archive
  SHA-256. The compiler identifies itself as `57e38f1c6827e39d4eb377bdd297cfb14592a11b`.
- `engine/acoustics.wrela` implements mass-normalized stiff-string modes, finite-width felt
  contact, bounded discrete-work contact solves, explicit viscous losses and a positive
  series compliance derived from omitted modes. The latter is a declared quasistatic
  approximation, not fitted excitation.
- `studio/acoustics/reference.wrela` independently integrates the instantaneous equations
  with RK4. It does not call the runtime contact update, its propagator or its energy function.
  The rigid-wall check also uses an independent energy integral.
- `engine/acoustic_modes.wrela` and `acoustic_plate.wrela` supply small generalized
  eigenproblems and a simply supported orthotropic plate with ribs. This is not yet the
  grand's actual rim, static equilibrium or bridge geometry.
- `acoustic_system.wrela` verifies small coupled mechanical systems. `acoustic_bridge.wrela`
  and `acoustic_waveguide.wrela` additionally verify a shared, power-balanced termination
  between ideal strings and structural modes. Unstruck strings receive energy; a decoupled
  ablation does not. The waveguide has integer delays and no string dispersion yet.
- 23 focused tests pass without diagnostics. They include independent eigenvalues,
  reciprocity, contact work, rebound, damping, modal phase, SIMD roundoff, travelling-wave
  energy, shared-body port work and preserved parked-string state.
- Nine low/middle/high register × 0.3/1.5/4 m/s probes pass the current contact experiment.
  At 48 kHz, worst peak-force error is **1.662%**, first-separation-time error **0.0664%**,
  and post-contact return-speed error **0.00879%**, against the more resolved independent
  reference. First separation and final return are distinct: a bass string can separate
  while the hammer is still moving forward. The reference uses eight times the retained
  modes and 1.536 MHz; halving its time step changed the checked observables by at most
  0.000486%. The last modal refinement changed peak force by 0.00133%, 0.0108% and 0.600%
  respectively. These are narrow fixture results, not full-key, damped, nonlinear-tension
  or audible-aliasing qualification.
- The first point-contact/truncated model failed: hard bass peak force depended strongly on
  retained modes. Finite width, analytically calculated tail compliance and 128 retained bass
  modes corrected the tested case. Failure reports are retained. Port-energy checks also
  caught an incorrect factor in the first shared-bridge scattering equation; the corrected
  expression is `F = Z (2 a - v)`, with the same observation and drive transform.
- Representation costs are recorded in `docs/bench.md`. Both a large SIMD modal bank and the
  complete scalar travelling-wave/body prototype miss the final budget. Their cheap
  component-only measurements do not qualify the complete instrument. The next runtime
  experiment is a spatially reduced bridge connection and SIMD coupling, with error
  measured against the full reference.
- The available tools had no qualified audio-input judge. A local Qwen2.5-Omni-7B Q8 model
  with an F16 audio projector was installed outside the repository, using verified hashes
  and its Apache-2.0 source licence, solely for textual evaluation. It **failed** a blinded
  preliminary control: eight seconds of exact digital silence were described as a sustained
  metallic sine note. Its judgments are rejected. A different local model is being screened;
  no listening gate is passed.
- The workspace allowlist check reports only 15 pre-existing files under the unrelated
  untracked `runtime/fixtures/first-light/page/` tree. They were preserved. New files conform
  to the workspace's existing Wrela-source/manifests rules.

Replay from the ThinkPad checkout:

```text
wrela test engine/tests/grand --json
wrela tool acoustics -- contact
wrela tool acoustics -- truncation
wrela tool acoustics -- plate
wrela tool acoustics -- cost
wrela tool acoustics -- simd
wrela tool acoustics -- waveguide
wrela tool acoustics -- bridge-cost
wrela tool acoustics -- render
```

The last command produces an explicitly labelled **isolated-string diagnostic**, plus known
damaged controls, in `build/acoustics/`. It observes string velocity; it is not board radiation,
a finished piano demo, or evidence of good sound. Its five files use one fixed calibration and
are only a preliminary evaluator screen, not the BS.1770-matched musical comparison protocol.

Authoritative source archives, command results, failures, environment and checksums are under
`/home/ryan-wible/wrela-evidence/grand/mechanics-2026-10-29/`; checkpoint-1 includes the
exact source archive for its tests and numerical reports. Listener requests, exact input hashes,
outputs and the silence audit are under
`/home/ryan-wible/wrela-evidence/grand/listener-qualification-2026-10-29/`.

Outstanding: verified musical evaluator, original full instrument definition and provenance
contract, detailed bridge/rim equilibrium and radiation, production reduction, all-key controls
and pedals, browser/consumer measurements, autonomous voicing, holdout and release-candidate
integration. Existing piano consumers and Salamander attribution remain in place; this
checkpoint does not satisfy P8's dependency-closure requirement.


## 14. Coupled-reference checkpoint and unresolved musical gate

**Status: verified mechanical building blocks, musically unqualified; implementation remains
incomplete.** This is not the P2 three-note deliverable, a finished 88-key instrument or P8.
All work and model inference stayed on the ThinkPad. The single implementation PR remains
a draft; no existing piano consumer has been migrated and no release has been published.

Additional implementation since checkpoint 1:

- A moving-endpoint stiff-string assembly derives cross mass and added board mass/stiffness
  from the kinetic and elastic energy integrals. Independent spatial quadrature checks those
  matrices. A three-neighboring-string/16-board-mode surrogate reaches 976 coupled modes;
  eigenpair residual and mass orthogonality are checked against the original matrices.
- Simultaneous nonlinear contacts use a bounded coupled solve and report their remaining
  equation/energy residuals. Multiple string contacts can share one hammer mass. The symmetric
  three-string limit matches an independent effective-mass single-contact solution, and
  reversing contact order gives the same physical trajectory within the test tolerance.
- A local viscous damper substep removes only motion at its physical contact footprint and
  accounts for removed energy. It is a numerical primitive, not a calibrated half-pedal model.
- Rigid bridge sections and SIMD body/string storage retain independent waves and preserve
  reciprocal work. Their accuracy tests compare to expanded f64 contacts. Benchmarks in
  `docs/bench.md` still exclude important complete-instrument costs.
- A lossless allpass scattering primitive passes independent impulse/phase/energy checks.
  Its bounded stiffness-fit experiment fails in bass and treble; it is not selected for the
  instrument. The next representation hypothesis is a mixed modal/dispersive design by
  register, with contact/truncation and complete callback costs evaluated together.
- A symmetric rib layout exposed floating-point cancellation in separately accumulated
  off-diagonal matrix entries. Assembly now computes each integral once and mirrors it,
  preserving exact symmetry without weakening the eigensolver's input validation.

The focused set has **34 passing tests with no diagnostics**. The new coupled experiments
are narrow engineering fixtures, not a demonstration of good sound. For one midrange strike,
40 retained modes per string plus calculated omitted compliance at 48 kHz gives a peak-force
error of about **0.172%** relative to 320 modes per string at 384 kHz. With three simultaneous
0.3/1.5/4 m/s strikes, the largest contact force differs by about **1.017%**. The latter
reference's 160-to-320 refinement changes that peak by about **0.00565%**. Coupled-system
energy errors stay below 7e-14 relative in these runs. The three-note fixture uses neighboring
midrange strings; it does not yet cover P2's bass/midrange-unison/treble set, all contact
observables, repeated actions, damper transitions, or complete time/radiation refinement.
The original isolated-string RK4 checks in section 13 remain separate evidence.

Replay additions:

```text
wrela tool acoustics -- coupled
wrela tool acoustics -- coupled-contact
wrela tool acoustics -- simultaneous
wrela tool acoustics -- section-cost
wrela tool acoustics -- body32-cost
wrela tool acoustics -- combined32-cost
wrela tool acoustics -- packed-cost
wrela tool acoustics -- dispersion
```

`mechanics-2026-10-29/checkpoint-2/` and `checkpoint-3/` retain sources, exact commands,
exit status, reports and hashes. Higher-resolution solves are offline references; their
elapsed times are not audio callback measurements. The pinned public input/audio API was
also inspected; its live capture-time/clock-mapping gap is recorded in `docs/toolchain-gaps.md`.

### Audio evaluator qualification result

No musical parameter search or musical promotion has occurred. Three local candidates were
screened, all outside the repository; none generated sound or instrument coefficients:

| Candidate/configuration | Observed failure | Decision |
|---|---|---|
| Qwen2.5-Omni-7B, llama.cpp, verified Q8/F16 files | Exact digital silence described as a sustained metallic note | Reject |
| Voxtral Mini 3B, llama.cpp, verified Q4/Q8 files | Four non-silent controls all described as silence | Reject |
| Same Voxtral model, official Transformers path, original verified weights, CPU float32 | Same failure after correcting the runtime hypothesis | Reject |
| Phi-4 multimodal, official pinned code/weights, CPU bfloat16 | Silence described as a drum; original labelled piano; inserted gaps/cutoff/clipping not identified as requested | Reject |

The second screening epoch was recorded before Phi inference: its explicit music/audio
benchmark coverage and MIT licence supplied a specific new capability hypothesis. It used
the same frozen controls and neutral prompt and did not erase the previous failures. Music
Flamingo and Audio Flamingo were only source-screened and excluded because their declared
noncommercial terms do not fit the requested licensing boundary.

Input audits prove the silent control is zero and the others are nonzero. The original has
RMS 0.1116 and peak 0.5052 at the shared gain of 0.2; the clipped control has RMS 0.4649.
The exact llama decoder independently produced nonzero samples for those inputs. This is
preliminary evaluator screening, not BS.1770 musical comparison or a formal holdout attempt.
The full local record, including exact audio, decoder audit, prompts, responses, runtime
versions and downloaded-file checksums, is in `listener-qualification-2026-10-29/`.
These outcomes do not establish that every local audio model is incapable. They establish
that no tested path can currently justify the plan's autonomous musical promotions.

### Next work and claims boundary

A qualified audio-evaluation path is required before musical hill climbing or a “best sounding”
claim. Ryan subsequently authorized an OpenAI audio-capable model to perform the review through a subagent. That review may send the diagnostic audio to OpenAI; implementation and file handling remain on the ThinkPad. An API client was prepared but never called. Ryan then specified Codex without an API key; the resulting native-input capability check and engineering-only review are recorded in section 15. This does not authorize unrelated off-host computation or additional services. Independently,
the engineering still needs a feasible full string scale and static load/board/rim design,
a selected runtime reduction, real action/pedal behavior, radiation and room, the full-key
corpus, browser and two-piano consumer budgets, and P8 migration/holdout/soak checks.
A new evaluator hypothesis must preserve this failed qualification history and pass a recorded
protocol; it must not simply provide favorable comments on the same candidate. No stage's
musical subgate or final acceptance checkbox is marked passed by this checkpoint.


## 15. Original scale, three-key pressure render and Codex review

**Status: physically checked reference work; implementation incomplete and musically
unevaluated.** No stage, listening promotion, browser qualification or final holdout is passed
by this checkpoint. The new 88-key scale is a design proposal; the runnable sound fixture still
has three keys. Existing piano consumers have not migrated.

### Original scale proposal

`engine/grand_design.wrela` derives all 88 pitches from an authored scale with 230 speaking
strings. Lengths saturate smoothly below 2.65 m; plain/wound construction, core radii and
hammer/contact dimensions are explicit. Tension is solved from the pinned stiff-string
fundamental including bending stiffness. Winding adds mass under a freely slipping winding
approximation; its radius is mass-equivalent, not a claim of a fully designed helix.

Version 1 sums to **112,502.84 N of string tension** and **1.4868 kg of speaking string mass**.
Its maximum calculated core stress is **1.17893 GPa** against an **authored 1.4 GPa ceiling**.
Density, modulus and that ceiling are nominal model assumptions, not certified material
properties. These calculations do not establish frame strength, static board/downbearing
equilibrium, winding/end-condition fidelity, manufacturing feasibility, loaded tuning or
musical quality. The proposal still needs those checks and a spatial layout.

The complete string inventory contains 5,367 / 9,788 / 16,446 analytic modes below
5 / 10 / 20 kHz respectively. This is a workload inventory, not a perceptual cutoff or
measured callback cost. No all-key runtime representation is selected from these counts.

### Actual three-key coupled mechanics and radiation

`grand-probe` now combines a bass string, three slightly different midrange unison strings
under one shared hammer, and a treble string on the same board. Across 16/36/64 board bases,
the 20 ms 48 kHz fixture reports energy error below 4.3e-15 and contact-equation residual
below 1e-12. This extends the earlier neighboring-midrange fixture; it does not replace its
separate high-resolution contact evidence. Small changes in peak force across board bases
are not proof of convergence of the full radiated waveform.

The first pressure renderer propagates the board acceleration to two listener positions with
the Rayleigh integral. Its assumptions are explicit: a prescribed planar board in an infinite
rigid baffle, homogeneous linear air, no fluid back-reaction, no lid and no room. It does not
subtract radiation energy from the mechanics or count an unimplemented radiation loss twice.
Nominal air density is 1.2 kg/m³ and sound speed is 343 m/s. Structural loss in this diagnostic
is an authored mass-proportional viscous term, C = 0.7 M, not a calibrated piano decay model.

Independent tests cover the exact axial circular-piston integral, far-field area/symmetry,
propagation delay/refinement, signed-mode convolution and its full tail. The reconstruction
of board acceleration is checked against physical-coordinate masses with a shared hammer,
nonzero contact gaps and omitted compliance; the expected contact forces are prescribed
independently of the solver. The focused set now has **42 tests**. One combined run hit an intermittent pinned-compiler
optimizer panic before producing results; the unchanged source passed on an immediate retry.
The failure and retry are preserved, with the unresolved compiler defect in
`docs/toolchain-gaps.md`; no compiler-reliability claim is made.

The eight-second diagnostic launches bass, unison and treble at 0/2/4 s with 1.5 m/s hammer
velocity. It retains a raw float WAV in pascals and a separate listening file with one
whole-clip gain and an explicitly labelled 100 ms terminal fade. The first render's peak is
0.6008 Pa and stereo RMS 0.1025 Pa. Neither normalization nor that fade is part of the physical
model. The numerical audit finds finite, unclipped samples and confirms the declared
presentation transform. This is not a perceived-quality result.

Spatial and delay errors remain recorded. At the off-symmetry listener, changing the grid from
64×32 to 128×64 changes the checked nonzero transfer amplitudes/phases by up to about 0.478%
in complex relative error. Linear fractional-delay interpolation at 48 kHz contributes up to
about 4.152% complex error in the checked 5 kHz transfers; at 192 kHz that checked maximum is
about 0.229%. These are different error sources. There is no full-band convergence, aliasing
or complete pressure-reference qualification yet, and this direct offline convolution has no
runtime qualification.

A recorded numerical correction retains that linear baseline and adds 64-point Kaiser-windowed
sinc delay reconstruction. At the **128×64 grid, y=0.4 listener, modes 0/3/15 and 5 kHz**, the
largest checked complex delay error falls from **4.1519% to 0.0017925%**. At the renderer's
**64×32 grid** the corresponding maxima are **4.2807% and 0.0016946%**. These comparisons
hold spatial quadrature fixed and isolate delay interpolation; they are not perceptual scores.
Separate single-patch tests check three fractional delays at frequencies through 20 kHz against
exact phase, with relative error below 0.02% in those tests.

The refined filter requires at least 32 samples of geometric delay, so its support never reads
future input. Its band-limited front can precede geometric arrival by 32 samples; output metadata
now records that support and the actual frames/duration including the propagation tail. It does
not restore aliased acceleration, validate the spatial grid above the checked band or include
fluid loading. The initial linear-pressure files and the new corrected files are retained
separately. Global cumulative launch-work balance in the diagnostic is not a substitute for the
plan's required local energy windows.

### Codex review disposition

Ryan selected Codex and has no API key. The subagent attempted to deliver a complete,
newly blinded WAV as native audio content. The interface returned exactly:

> audio content omitted because you do not support audio input

It stopped the listening attempt without classifying the clip from metadata or remembered
control identities. The subsequent review was explicitly engineering-only. It demonstrated
no consequential functional error under the declared approximations, verified the
pressure-file/presentation arithmetic, requested the missing Woodhouse citation, and
recommended a shared-contact/compliance acceleration check. The citation and that targeted
check are now added. A focused follow-up review of the sinc correction found its support and
normalization consistent, and requested clearer support/duration metadata and grid-qualified
error claims; those reporting corrections are applied. This satisfies the available Codex engineering review; **actual sound
comprehension remains unavailable through this interface**, so no musical verdict exists.
No API calls, credential requests beyond the superseded route, or OAuth reuse followed.

Replay:

```text
wrela test engine/tests/grand --json
wrela tool acoustics -- scale
wrela tool acoustics -- grand-probe
wrela tool acoustics -- radiation
wrela tool acoustics -- grand-render
```

Pressure/scale sources, raw results, exact commands and hashes are retained in
`mechanics-2026-10-29/checkpoint-4/`.
The next implementation work is a qualified pressure reference and feasible complete runtime
reduction, then the remaining instrument/action/room/product work in P3–P8. A usable listening
interface is still an external dependency for autonomous musical selection; engineering
progress cannot turn the blocked musical gate into a pass.
