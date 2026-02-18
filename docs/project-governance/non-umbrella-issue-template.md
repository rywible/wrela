# Non-Umbrella Issue Template (WRE-636 / WRE-610 Guardrail)

## Context

Explain the problem and why this issue exists.

## Scope

- List concrete implementation tasks.

## Acceptance Criteria

- Define objective, testable outcomes.

## Testing

- List required test commands/suites and expected evidence artifacts.

## Wrela-First Ownership and ABI Boundary

- Implement as much as possible in native Wrela (`language/packages/db/*`).
- Keep Rust runtime work limited to kernel/hot-path primitives.
- ABI boundaries must be small, explicit, and versioned; no ad-hoc runtime surface expansion.
- Any ABI expansion requires explicit contract update in `WRE-509` plus ABI snapshot/gate coverage.
- Acceptance tests must exercise Wrela package entrypoints first.

## Governance Completeness Checklist (WRE-610)

- Assignee is set.
- Due date is set.
- At least one dependency edge exists (`blockedBy` or `blocks`).
