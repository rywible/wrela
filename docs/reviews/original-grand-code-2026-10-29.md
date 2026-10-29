# Original grand: code review and repair disposition, 2026-10-29

## Provenance and outcome

Ryan requested a code review by Opus 5.5 at high effort through Claude Code, following
the earlier plan review. The first review ran on the ThinkPad with Claude Code
2.1.301, requested and reported model `claude-opus-5-5`, `--effort high`, and read-only
Read/Glob/Grep tools. It exited successfully at 22:19:05 UTC. Its snapshot was
`55bdc0b6` plus the recorded plate/block-solver diff; sources did not change during
the review. No sound was supplied to or heard by the reviewer.

The unedited response, request, source archive and hashes are retained at
`/home/ryan-wible/wrela-evidence/grand/code-review-2026-10-29/` on the ThinkPad.
The first response stream SHA-256 is
`927ff002d0ee2dcd1de04d33833ef2a50fe18156d1bb3e06cb9d35f0045d7baa`.
The second Opus 5.5/high review also completed, at 22:56:20 UTC, with unchanged
sources. Its response stream SHA-256 is
`401bb9ef429c46e6055543e038d9d84857dd6f19c4438e5d559ede705b12e55f`.
It found no demonstrated algebra/sign bugs in the repairs; it added the qualification
findings recorded below. This is code review, not sound approval.

The review found the core contact, moving-boundary assembly and radiation equations
consistent by inspection. It found serious defects in physical assumptions, truncation
and validation. This is **not approval of the instrument or its sound**. Ryan's rejection
remains the only actual listening result for the delivered preview.

## Findings and dispositions

| Opus finding | Action and present status |
|---|---|
| A1, critical: the 16-mode board suppresses force transfer above its represented resonances | Added direct point-force mobility and pressure qualification without time integration. The treble defect is measured below. Resolution remains insufficient; open. |
| A2, critical: implausibly soft surrogate board, two weak ribs, no bridge/rim/static equilibrium | Accepted. No nominal rib count is presented as a validated construction. Geometry, bridge and supports need physical design and transfer qualification; open. |
| A3, high: uniform `C = 0.7 M` conflates string and board decay | Removed from the pressure diagnostic. Separate physical-coordinate losses use authored board loss and causal string bending viscosity. Exact loss, force mapping and splitting are checked. Air drag, winding/termination loss, radiation backreaction and decay-versus-mobility qualification remain open. |
| A4, high: largest retained resonance is not a trustworthy bandwidth | Corrected that label, added the first omitted **bare** sine frequency, and added frequency-selected index sets with exact ragged y-block solves. The descriptor does not certify a ribbed-response band. |
| A5, medium-high: centreline listeners suppress even-y modes | Moved both drive and listener locations away from both symmetry planes after the follow-up. Added even-order and odd-x mode checks and absolute forced-pressure checks; layout 2 is specified below. |
| A6, medium-high: numerical stress-test strings used instead of the scale | Rebuilt the fixture from original-scale A0, C4 and D7: one, three and three strings, with specified shared hammers. Numerical contact probes remain separate. This is still a three-key fixture. |
| A7, medium: aggregate residual can hide low-mode errors; correlated checks overstate independence | Added a per-mode residual and fail-closed audit. It exposed a real low-mode solver error; pair-relative Jacobi rotations fix it. Block-versus-dense tests are mapping regressions; rigid-end RK4 checks validate time integration only. |
| A8, low: generic single-port mode shape is the bass drive | Explicitly documented the A0 diagnostic port. Multi-hammer integration and acceleration use explicit contact covectors. No current multi-contact bug was demonstrated. |

Two qualifications to the review's estimates matter: finite damping makes high-frequency
real mobility small, not exactly zero; and inserting the geometric mean of orthotropic
rigidities into the isotropic infinite-plate mobility formula is not a general identity.
Neither approximation was used as an exact acceptance target. Board/bass hybridization,
baffle bass emphasis and felt hysteresis remain hypotheses or declared omissions.

## Measured response failure (reviewed layout 1)

The old 2.4 × 1.25 × 0.008 m surrogate board and ribs were driven with harmonic point
force, using authored board loss factor 0.02. Five bases contained
16 / 125 / 260 / 527 / 937 modes. The last four include every bare sine function below
500 / 1000 / 2000 / 3500 Hz. The 937-mode board's largest retained ribbed resonance is
about 9.60 kHz; that is not its resolved band.

At the treble attachment and 2349.318143 Hz, left off-axis pressure transfer grows
from **0.00088979 to 0.0937031 Pa/N** between 16 and 937 modes: about **40.45 dB**.
This is a linear force-response comparison, not an audio loudness comparison.

Only **79 of 120 sampled attachment/listener/frequency combinations** have both
mobility and pressure magnitudes change by less than 1 dB between 527 and 937 modes.
The spatial-grid refinement on the 937-mode result changes pressure magnitude by at
most 0.08031 dB and complex transfer by at most 1.131% at these points. Structural
truncation dominates the remaining discrepancies.

There are only 20 discrete frequencies. These observations establish neither a continuous
converged band nor convergence around every narrow resonance. Complex changes and both
channels are retained, including near cancellations. No next preview was rendered.

## Loss and solver checks

The loss operator is `C_global = sum c_k psi_k psi_k^T`, where each `psi_k` is a row
of the physical-to-global modal map. Exact rank-one dissipative steps form a forward
half sweep before mechanics and a reverse half sweep afterward. Removed kinetic energy
is counted once. Instantaneous acceleration includes the same damping force.

Board coefficients are `c_k = eta omega_k`, authored `eta = 0.02`. String bending
assumes `EI (curvature + tau curvature_rate)`, giving
`c_n = tau EI (n pi/L)^4 / mu` in pinned mass-normalized coordinates, authored
`tau = 1e-7 s`. The linear bridge lift has zero curvature. These are physical hypotheses,
not measured material characterizations or fitted amplitude envelopes.

53 grand tests pass. New checks include a one-mode damped oscillator; nonorthogonal
loss coordinates against an independent 2×2 matrix exponential; exact loss/work;
off-resonant forced pressure against the independently tested Rayleigh observer;
ragged-basis mapping and analytic bare spectrum; invalid/overflowing audits; and
a wide-frequency-range eigenproblem that the old rotation tolerance failed.

The authored-scale fixture has 348 string modes plus 16 board modes. Its low-mode
residual was **3.929e-8** under the old global Jacobi tolerance, failing the new 1e-9
gate. Pair-relative rotations reduce the maximum to **6.971e-12**. The failed run
and pre-fix source are retained.

20 ms coupled-loss probes at 48/96 kHz have energy error at most 4.42e-14 and contact
residual below 1e-12. Removed energy changes from 3.89771e-5 to 3.98612e-5 J, about 2.2%.
That discrepancy remains a time-refinement task. Passivity does not qualify timbre,
bandwidth, construction or the renderer.

A separate Codex engineering follow-up found no nominal sign/unit/mapping defect in
the response path. It requested the fail-closed audit and absolute pressure test;
both were added. Its interface still cannot hear the files.

## Replay and remaining gate

```text
wrela test engine/tests/grand --json
wrela tool acoustics -- board-response
wrela tool acoustics -- board-bandwidth
wrela tool acoustics -- grand-loss-probe
```

Exact sources/results are retained in `code-review-2026-10-29/repairs/` and `followup/`.
Tools run on the ThinkPad with pinned 0.10.0.

Before another listening candidate: qualify board/bridge/supports, resolve the
force-to-pressure response around resonances, qualify string decay against bridge
mobility, refine contact/loss time resolution, and bound memory and aliasing.
The 1024-mode cap and frame-by-mode offline allocation remain implementation limits.
Full action, pedals, two polarizations, longitudinal motion, radiation loading, lid/room,
complete runtime, browser budgets and consumer migration remain unfinished.


## Second Opus review and final disposition

| Follow-up item | Disposition |
|---|---|
| N1: bare-frequency selection is not selection by ribbed frequency; symmetric rib nodes create artificial subfamilies | Accepted. Full-x block eigenvalue/count refinement and an actual bridge/rib/support design remain open. The bare cutoff is never relabelled as a complete ribbed band. |
| N2: magnitude-only comparisons hide imaginary mobility changes and omitted static flexibility | Added source-level gates for relative real mobility, imaginary mobility change scaled by total mobility, and complex pressure change, each with explicit 5% numerical targets. Full complex values and absolute component differences are retained. Residual-flexibility correction remains open; it must also enter the passive time-domain model consistently. At zero frequency mobility is zero: static compliance must be checked through displacement receptance or the low-frequency Y/(i omega) limit, not Y(0) itself. |
| N3: near-symmetry probes still suppress antisymmetric modes | Fixed the diagnostic layout on both x and y planes. All three drives and both listeners are at least 0.1 m from each symmetry plane. The actual (2,1) pressure ratio is checked at 300 Hz. |
| N4: claimed precision floor and missing audit assertion | Added hard valid/per-mode/orthogonality assertions in the harness. The diagnosis used pre-fix data: the actual pair-relative-Jacobi replay gives 1.66e-14 maximum per-mode residual at 937 modes, not 8.96e-10. Transfer changes from the solver correction are at most 1.59e-11 in complex-relative error. No gate was loosened. |
| N5: test damping together with stiffness, and locate time-step error | Added a three-coordinate stiffness/loss check against an independent 6×6 state exponential, at coarse omega*dt of 0.05, 0.6 and 1.3. It passes second-order refinement and energy accounting. The separate real fixture study below exposes unresolved time error. Per-string/board loss attribution remains open; its cause is not asserted from the total alone. |
| N6: bare-board mobility is not the fully loaded bridge mobility | Accepted. String-decay qualification must include partner-string loading or use coupled modal decay. No loaded-decay claim is made from the bare-board harness. |
| N7: duplicated geometry/eta; possible double damping; incomplete pressure mapping test | Centralized ribs, material constants, drive and listener positions. Added a runtime rejection of simultaneous built-in uniform damping and the physical loss model. Added a ragged multi-x/multi-y ribbed pressure check against direct per-patch modal shapes. Tiny authored unison offsets are labelled; they do not constitute a bridge design. |

Layout 2 drives are (0.6,0.4), (1.02,0.48), (2.0,0.82) m; listeners are
(1.04,0.37,1.2) and (1.47,0.37,1.2) m. The two probes are 43 cm apart; no binaural
head model is implied. Earlier 79/120 and 40.45 dB results belong to layout 1,
whose source/archive is preserved. Neither layout is a final instrument geometry.

### Finer time-step study

A separate source-captured ThinkPad checkout kept reviewed layout 1 and changed
only sample rate. The same 20 ms launch was measured at:

| Rate (kHz) | Removed material-loss energy (J) |
|---:|---:|
| 48 | 0.000038977092921 |
| 96 | 0.000039861221795 |
| 192 | 0.000045555206203 |
| 384 | 0.000046658686081 |
| 768 | 0.000046846038656 |
| 1536 | 0.000046886279478 |

The 48 kHz result is about 16.9% below the finest checked value. The last pair changes
by about 0.086%, but that is not an exact solution or a full audio convergence proof.
All six energy residuals are below 2.25e-13. This directly demonstrates that accurate
energy accounting can coexist with inaccurate time evolution. The 48 kHz renderer
is not qualified by the short loss probe. No new listening candidate was generated.

The source snapshots, exact rate-only edits, commands and outputs are retained in
`code-review-2026-10-29/temporal-study/`. This study is not a performance benchmark.


### Final layout-2 check

The actual odd-x (2,1) pressure ratio is 1.59579 at 300 Hz, passing the >0.5 diagnostic
check. With the new component/complex gates, only 47/120 sampled combinations pass.
This is not comparable as a quality score to the earlier 79/120: both the layout
and the criterion changed. Continuous-band certification remains false.

The final Codex subagent reviewed these post-Opus changes without editing or running
jobs. It found no consequential implementation error in the gate indexing, shared
layout/material settings, double-loss assertion, radiation force/work mapping or the
two added reference tests. It confirmed the newer replay contradicts N4's claimed
precision floor. The original physical-board and time-convergence gates remain open.
