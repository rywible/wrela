# An original Wrela grand piano, and a path to an orchestra

*Execution plan, 2026-10-29. Requested by Ryan. Status: planned; no new instrument or performance result is claimed. Baseline inspected on the ThinkPad at Wrela commit `807f9c080727c3ecae29d8e6bf43d0eb4b2a7c67`, toolchain pin 0.10.0.*

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
| Sympathetic resonance | Two partials per key; excitation and common damping controlled by sustain-pedal lift | Make resonance depend on each string's state and damper, including held keys with the sustain pedal up |
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

Tune unisons and stretched octaves from the simulated string scale, then voice the instrument across registers. Document chosen deviations and their listening rationale. A defect gets a physical explanation or an explicit approximation; do not bury it in an unexplained per-note EQ table.

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

Use energy-conserving or dissipative discretizations with an explicit energy budget. A moving hammer, bow or boundary can supply energy; record that input rather than applying an incorrect blanket “energy always decreases” rule. Track stored energy, external work, dissipated energy and the residual.

The detailed solver initially covers a few strings and a modest board, with high precision and tighter time/space resolution. Grow resolution only when convergence studies show it matters. A larger solver is not automatically ground truth: check analytic limits, independent small-system solutions and refinement before treating its output as a reference.

### 4.2 Strings and nonlinear contact

For the reference, resolve transverse motion, stiffness, frequency-dependent losses and hammer contact explicitly. Add both transverse polarizations and longitudinal/stretch coupling where their contribution is significant; the low register is an important probe [R3]. Wound strings need a documented effective model and its limitations.

Benchmark reduced modal and dispersive waveguide representations against that reference. Default to a modal implementation to exploit existing SIMD work, but retain it only if it meets accuracy, stability and cost gates. Do not mix two full solvers for the same degree of freedom or count their energy twice.

Contact must see the current string motion: a repeated strike acts on a ringing string. Solve contact without attractive felt forces, implausible penetration or unbounded iteration on the audio thread. Prefer a stable bounded solve; if it fails, report a numerical failure and preserve finite output rather than concealing it in a limiter.

Use oversampling only where nonlinear contact or mode generation requires it. Include anti-alias filtering, its delay and cost. A permanently larger sample rate is not the first optimization.

### 4.3 Coupled unisons, bridge and body

Connect every string to shared structural degrees of freedom. The strings drive the board; the board changes their boundary motion. Derive a reduced positive-energy model from the detailed structure, retaining the coupling needed for decay, beating and sympathetic resonance.

Use low-rank shared modes or another justified reduction to avoid dense all-to-all string work. Passivity must survive reduction and interpolation. A quiet unstruck string can still receive energy; its state cannot be discarded because no note-on event occurred.

Generate attack, two-stage decay, sympathetic response and body motion through this system. Compare with intentionally decoupled ablations to show that the coupling is working for the right reason.

### 4.4 Acoustic output

Keep structural feedback separate from observation: moving a virtual microphone must not change how the strings decay. Include radiation loading in the physical model if required; count that damping only once.

Precompute directional radiation from our structure, then reduce it for runtime. Treat bass wavelength, body size, lid position and stereo coherence explicitly. A key's pitch alone must not determine its stereo placement.

For the room, begin with a physical approximation: geometry-derived early paths and a passive late field whose decay follows room volume and boundary absorption. This is a hybrid acoustic approximation, not a full room wave solve. Compare representative rooms to a refined offline acoustic model before making accuracy claims. No downloaded room impulse responses. A convolution kernel generated by our own solver is permissible with its provenance.

### 4.5 Runtime boundaries

Allocate and cook before playback; keep audio work bounded, preallocated and independent of rendering-frame timing. Use sample-timed controls where the host supports them, handle the actual host sample rate, and explicitly adapt to the pinned host's block contract. Never assume a Web Audio block duration is end-to-end latency [R8].

All 88 keys keep state, including retriggers and sustained tails. Quality reductions can merge or truncate demonstrably negligible modes, with smooth state transfer and error bounds. They cannot erase a held note, a perceptible tail, or an unstruck string's ability to resonate.

CPU/WASM SIMD is the default runtime path. GPU work is suitable for offline generation experiments on the ThinkPad; putting it in the real-time path requires evidence that it meets deadlines during graphics contention and a viable fallback. No GPU promise precedes those measurements.

## 5. Evidence and acceptance contract

### 5.1 Three independent verdicts

Every candidate reports:

1. **Physical/numerical:** does the implementation follow its model, stay stable and converge?
2. **Musical/perceptual:** does the rendered instrument satisfy the listening rubric?
3. **Runtime/product:** does the actual browser stay responsive within the budgets?

Passing one never substitutes for the others. “Original” and “entirely physical” are provenance/architecture claims; “beautiful” is a listening judgment; “competitive with a concert grand” requires suitable external listening evidence. Do not infer any of these from a spectrum alone.

### 5.2 Fixed corpus and held-out evaluation

Before tuning, generate and hash an original fixture corpus. Store the generator, seeds, exact events, control trajectories and expected causal effects. Retain recordings outside tracked source under normal build output.

Cover all 88 keys and at least 12 hammer-velocity levels in coarse sweeps; include long bass decays, exposed treble, adjacent-key transitions, dense chords, rapid repetitions, trills, silent keys held against a struck harmonic relative, pedal flutter, half-pedal sweeps, late pedal catch, sostenuto, una corda, overlapping same-key events, transport reset and control discontinuities.

Keep three sets:

- **Development:** cheap, diagnostic probes used for tuning.
- **Validation:** different notes/velocities/gestures used for milestone promotion.
- **Final holdout:** unseen seeds and original musical phrases generated only after freezing a candidate and acceptance rules.

Hold out physical regimes and combinations, not just identical notes with different filenames. If a holdout fails and informs a repair, it becomes development evidence; create and disclose a fresh final holdout under the same rules. Keep the complete failure history. No unlimited “fresh holdout” retries until a lucky pass.

Musical phrases must expose independent melody/accompaniment dynamics, legato, detached playing, bass clarity, high-register attacks and pedalled harmony. Write original test material. Historical pieces already in the repo can be supplemental with their existing attribution; no third-party performance recordings are required.

### 5.3 Initial numerical targets

Freeze the measurement implementation in P0. These starting budgets are design targets to be measured, not promises that the existing toolchain already exposes them.

| Gate | Initial acceptance target |
|---|---|
| Finite bounded behavior | No NaN/Inf, solver failure or growing unforced oscillation in every adversarial fixture |
| Silent state | Zero input from exact rest remains at rest; no unsolicited noise floor or random excitation |
| Analytic verification | Undamped isolated-string modal frequencies within 1 cent of analytic values; measured damped decay within 2% in fixtures long enough to identify it |
| Reference energy balance | Normalized cumulative residual no greater than 1e-6 in bounded f64 canonical fixtures; halve the step to check convergence, with denominator fixed from initial energy plus positive supplied work |
| Runtime energy behavior | Residual/error bounded against the verified reference; starting normalized budget 1e-3 on matched canonical fixtures, with no systematic energy creation |
| Reduced vs detailed sound | On aligned isolated-note probes, median salient-partial frequency error <= 1 cent, 95th percentile <= 3 cents; median band-energy error <= 1 dB, 95th <= 3 dB; decay-time error <= 10% |
| Aliasing | Unexplained folded energy below -70 dB relative to the fixture peak in selected nonlinear probes against an oversampled reference |
| Lifecycle | Reset releases voices and buffers; retrigger/pedal/quality transitions preserve continuous state without an unintended discontinuity |

Salient-partial comparisons exclude bins below a fixed -60 dB relative-to-note-peak floor; report excluded energy and separate attack, sustain and tail windows. A method that drops a reference partial must count its missing energy, not remove it from the match. Compare delays with one declared alignment rule; never warp away the candidate's attack.

An accurate physical change can produce a complex waveform difference. Use causal tests, energy and spectral/time-envelope measures together; exact sample equality is required only where the host's contract actually promises it. Separate native and browser renders and disclose arithmetic differences.

The detailed model itself must converge across two refinements: its change in each reported acoustic metric must be less than one third of the corresponding runtime error budget before it is an admissible reference. Analytic tests are independent of this reduction comparison.

### 5.4 Listening rubric and autonomous review

Give actual audio to an available audio-capable evaluator. Fix and record the evaluator/version, rubric and presentation. Reviewing a waveform image, text description or metadata is not listening. An evaluator produces critique only; it must not generate replacement audio.

Score these dimensions from 1 (broken/unconvincing) through 3 (usable with a clear defect) to 5 (convincing and musically compelling):

- Bass pitch clarity and depth.
- Midrange singing sustain and separation of voices.
- Treble brilliance without brittle ringing or clicks.
- Soft-to-loud tonal continuity and expressive control.
- Attack, release and repeated-note coherence.
- Pedal/resonance realism without wash or pumping.
- Register consistency with intentional character.
- Spatial solidity and natural decay.

Freeze dry, player-position and room comparisons separately. Use the same performance and one constant gain per whole rendered excerpt to match perceived loudness with enough headroom; do not normalize individual notes, compress the pair or erase dynamic range. Keep an unnormalized calibrated version for dynamics checks.

Blind candidate labels and shuffle order. Include duplicated controls and an intentionally degraded anchor to check evaluator consistency. Compare a frozen champion to one challenger, retaining raw judgments and audible reasons. Do not claim a human-population preference probability from repeated calls to one model.

Promotion target: median >= 4/5 on every dimension over the fixed listening set, no severe defect, no repeatable regression against the champion, and either a clear recurring preference or equal quality with a measured resource improvement. Repeat ambiguous judgments in reversed order; ties keep the simpler/lower-cost champion. A lower score is a diagnostic, not an invitation to change the rubric.

If no actual audio evaluation path is available, continue the physical, runtime and provenance work independently and mark musical review pending. Do not claim that the full piano goal is complete. Ryan's manual listening is welcome but never a routine dependency.

An optional later acoustic-reference comparison may use our own authorized recordings or clearly suitable public-domain material after documenting rights. It must not feed the parameter optimizer, and reference media need not ship. No commercial library or third-party recording is required to finish this original-instrument plan. Without an acoustic-reference comparison, use the claim “agent-evaluated original physical grand,” not “indistinguishable from an acoustic piano.”

### 5.5 ThinkPad runtime targets

First inventory CPU, RAM, OS, browser, power mode, sample rate and audio backend on the actual ThinkPad. Use the installed supported browser and record its exact version. Do not extrapolate an offline render speed to browser stability.

Initial **Concert** target, with all 88 keys addressable and 88-key resonance state:

- Stereo 48 kHz nominal output, with the actual supported host rate recorded; rate adaptation explicitly measured.
- At a 128-frame/48 kHz callback, p99 instrument-plus-spatializer compute <= 1.0 ms, p99.9 <= 1.5 ms and no callback over its 2.667 ms period in three 10-minute representative runs. At a different supported block/rate, use the same period fractions and record them.
- Zero observed audio underruns in those runs, both alone and while a fixed representative Wrela scene runs. Report instrumentation coverage; callback timing does not prove absence of device/backend underruns.
- Instrument-owned working memory <= 64 MiB and compressed generated instrument data <= 8 MiB. Report application/compiler overhead separately; do not omit it from total memory/download figures.
- No playback-time allocation, blocking I/O, synchronous GPU wait or compilation. Report events-to-output timing separately from compute. Physical keyboard-to-speaker latency remains unmeasured unless a proper loopback setup is available.
- A 30-minute final mixed-interaction soak, with bounded memory and no state leak.

Provide **Studio** offline quality and **Concert** real-time quality first. A later **Ensemble** tier preserves gesture/attack/pedal behavior at lower cost; it is not permission for Concert to miss its target. Change quality smoothly at safe points, never by dropping a ringing instrument.

These are ambitious initial budgets. If infeasible, keep optimizing and report the precise tradeoff. A formally revised budget must appear as a dated target revision with old/new values and rationale; it cannot count as passing the original budget. Do not stall useful work just because a target is missed.

## 6. Milestones and exit evidence

Each stage leaves working code, a minimal focused regression set and an evidence record. Run cheap local checks during experiments; broaden only for changed dependencies, integration or final readiness.

| Stage | Deliverable | Exit evidence |
|---|---|---|
| **P0: establish the experiment** | ThinkPad inventory; baseline capture; original corpus; candidate/champion harness; rights/data inventory; verify available toolchain and audio-evaluation capabilities | Reproducible baseline hashes; calibration and measurement self-tests; fixed budgets, manifests and known gaps |
| **P1: one struck string** | Original string and felt model; high-precision reference; bounded runtime contact; analytic and refinement checks | Physical gates pass on low/mid/high probes; audible continuous dynamic response; no sample-derived parameters |
| **P2: a three-note instrument** | A bass note, midrange multi-string unison and treble note on one coupled board | Repeated strikes and damping pass; coupling ablation behaves as predicted; listening identifies no dominant synthetic artifact before expansion |
| **P3: the full grand** | All 88 keys, authored scale and hammer progression, persistent string states and selective dampers | Full-key sweeps, held-key sympathetic tests, register continuity and dense sustained chords pass; no hard 32-key cap |
| **P4: pianist control** | Continuous sustain/half-pedal, late catch, sostenuto, una corda, repeat/release behavior, timestamped live and score controls | Gesture fixtures pass in native and browser paths; no stuck notes or block-boundary-dependent control result beyond declared numerical tolerance |
| **P5: instrument radiation and room** | Position/lid-dependent output, separate physical room approximation and common calibrated gain | Dry gate remains passed; spatial ablations, mono compatibility, no doubled body damping, and independent room review |
| **P6: Concert runtime** | Verified reduced modes/contact/radiation, state-preserving quality changes and bounded scheduling | All ThinkPad runtime targets pass with the graphics scene; reduced/reference comparison passes on validation set |
| **P7: autonomous voicing** | Parameter search inside plausible design bounds, musical corpus, champion archive and defect-driven iterations | Listening targets pass without numerical or runtime regressions; repeated baseline/champion comparisons stored |
| **P8: release candidate** | Integrate showcase and real consumers; retire old default data path; expose useful controls; documentation and provenance audit | Fresh holdout, soak, integration checks and no sample-derived production dependency; reviewable PR with all claims scoped to evidence |

Dependencies: P0 -> P1 -> P2 -> P3 -> P4. Radiation experiments can begin after P2, but P5 integrates after P4. Measure cost at every stage; P6 is the final optimization gate, not the first timing measurement. Begin listening at P1 rather than waiting for P7. P7 refines the instrument through the whole loop; P8 freezes it.

Do not make a full high-resolution 3D piano simulation a prerequisite for P1. Begin with an independently verified reduced physical problem and add complexity when a specific error or missing phenomenon demands it.

### P0 must resolve these actual unknowns

1. Is the pinned Wrela command installed and usable on the ThinkPad? The planning inspection did not establish that. Follow the documented install/pin process if needed; do not compile on the Mac.
2. Can the current tool host generate matrices and perform the required bounded numerical solves? Prototype the smallest case before selecting a large eigenproblem.
3. Does the browser host expose callback durations, underruns and sample-timed event injection? Inspect current compiler/std/host contracts; record demonstrated gaps in `docs/toolchain-gaps.md`.
4. Can the current evaluation path consume and assess audio? Test with our generated controls and report capability honestly.
5. Which existing consumers import `GRAND` or call the piano APIs? Inventory them before migration; preserve score and interactive behavior.
6. Does the audio host require fixed 128-frame blocks and a fixed rate? Implement against its actual contract, not a newer web specification inferred from a search result.

For a toolchain gap, produce a minimal reproduction and a precise general capability request. Continue independent physical work while it is unresolved. If compiler implementation becomes necessary, use the ThinkPad's compiler checkout and its instructions; engine policy stays here. A missing host measurement is “unmeasured,” not zero.

## 7. The autonomous hill-climbing procedure

### 7.1 Durable experiment record

Implement a resumable ledger under build output and a concise durable milestone summary in this plan or a dedicated allowlisted report. Each experiment records:

- ID, parent champion, source commit and dirty-diff hash, toolchain version.
- Physical-definition hash, generated-data hash, units and provenance.
- Hypothesis; one intended change; predicted audible and physical effects.
- Corpus/seed IDs, solver resolutions, evaluation version and level-matching settings.
- Hardware/browser/rate/block/graphics-load conditions.
- All gate results, timing distributions, memory/data sizes and raw audio/report paths.
- Listening judgments including contradictions, decision and explanation.
- Best next experiment and any externally blocked gate.

Keep candidate and champion outputs separate. A journal entry or a source constant alone is not a reproducible instrument: generation inputs, tool version and output hashes must agree. Store bulky renders as build artifacts on the ThinkPad; commit compact summaries and the code that regenerates them. Never commit someone else's reference audio.

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

- Freeze sensible parameter bounds from the authored design and cited mechanics before a search. Use continuous physical controls; add a key-specific exception only with an explicit mechanical reason.
- A parameter batch starts with at most 24 variants and short probes. Only the best few reach the full listening set. Increase the batch only when its previous evidence justifies it.
- After three batches without a reproducible improvement, change the hypothesis or representation. Inspect residuals and ablations; do not keep twiddling the same parameters or ask Ryan to pick a winner.
- After a failed approach, retain the evidence, remove unused code and return to the champion. Keep only experiments with a current purpose.
- If all relevant hard gates pass and three materially different challengers cannot improve the frozen champion on validation, freeze a release candidate and run P8. This is convergence of this search, not proof of a global optimum.
- If a genuine external blocker prevents a gate, document it and continue independent stages. Do not hide it, replace the goal, fabricate evidence or silently declare completion.
- A budget or rubric change is a versioned experiment-design revision with fresh validation. It never erases the historical failure.
- Stop as complete only after P8's required evidence exists. If audio review or actual browser measurement is unavailable, report that limited completion state precisely.

This plan grants implementation judgment, not an unbounded unattended compute job. Use bounded runs and resumable checkpoints. Scheduling future sessions or spending external compute requires its own user instruction.

## 8. Product and delivery

The showcase should let a user play, hear a short original performance, change touch response, use the standard pedals, move from the bench to an audience position, and adjust the lid/room. Put scientific diagnostics in a separate developer view. No implementation jargon in the musical controls.

The default preset is one finished concert grand, not a shelf of experiments. Additional voicings come later if they preserve a coherent original instrument and have their own evidence. Good headphones may reveal details, but the mix must also be checked in mono and through a declared small-speaker simulation; no claim of actual speaker listening without doing it.

Before the implementation PR:

- Move every intended consumer to the new default and run the smallest relevant integration tests.
- Trace the shipped dependency/data closure; confirm no Salamander tables or library-derived coefficients enter it.
- Remove obsolete runtime defaults and dead experiments; retain historical attribution where required.
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

## 11. Execution status

- [x] Plan written against the ThinkPad checkout; original physical instrument and later orchestra scope distinguished.
- [ ] P0 experiment, environment and evaluation capabilities established.
- [ ] P1 single-string reference/runtime verified.
- [ ] P2 coupled three-note instrument passed.
- [ ] P3 all 88 keys and original physical definition passed.
- [ ] P4 complete pianist controls passed.
- [ ] P5 radiation and room passed.
- [ ] P6 ThinkPad Concert runtime passed.
- [ ] P7 autonomous voicing and listening passed.
- [ ] P8 final evidence, integration, provenance and PR ready.
- [ ] O1–O6: later scopes, not started or authorized for implementation by this piano plan.

**First implementation action:** P0. Verify the ThinkPad's pinned toolchain, audio host and evaluator; inventory current piano consumers; establish and hash the original fixture corpus before changing the sound.
