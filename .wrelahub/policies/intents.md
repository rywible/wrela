# Intents: Schema, States, Promptability

This is the canonical policy for what an **Intent** is in WrelaHub: the required sections, lifecycle states, transition rules, and the promptability checklist.

## Required sections (schema)

Every Intent MUST include:

- **Problem**: what is broken / missing, stated precisely.
- **Impact**: why it matters (users, cost, risk, time).
- **Constraints**: invariants, guardrails, compatibility requirements.
- **Non-goals**: explicit exclusions to prevent scope creep.
- **Success criteria**: testable outcomes and measurable checks.

Implementation reference: `apps/wrelahub/src/domain/intent.wr` (`A Intent` fields).

## Lifecycle state machine

States are linear with an escape hatch to Archived:

- `Seed` → `Sharpening` → `StressTested` → `Endorsed` → `Realized`
- Any non-terminal state MAY transition to `Archived`
- `Archived` is terminal

Implementation reference: `apps/wrelahub/src/domain/intent.wr` (`A IntentState`, `to transition(...)`).

## Transition rules (with explicit reasons)

All transitions MUST include an explicit, human-readable `reason` and MUST be audited as immutable events.

Minimum rule set:

- `Seed` may transition to `Sharpening` or `Archived`
- `Sharpening` may transition to `StressTested` or `Archived`
- `StressTested` may transition to `Endorsed` or `Archived`
- `Endorsed` may transition to `Realized` or `Archived`
- `Realized` may transition to `Archived`
- Self-transitions are invalid

Application services MUST enforce these rules (not the UI).

Implementation reference: `apps/wrelahub/src/application/intent_service.wr`.

## Promptability checklist

An Intent is “promptable” when it is safe to hand to an executor (human or AI) without mind-reading.

- No undefined terms (acronyms expanded; domain nouns defined or linked)
- Success criteria are testable (no vibes; include thresholds, examples, or checks)
- Decision budget is defined (time/complexity/compute boundaries)
- Includes examples (inputs/outputs, edge cases, counterexamples)
- No open questions (or they’re explicitly enumerated and gated behind “do not proceed”)

