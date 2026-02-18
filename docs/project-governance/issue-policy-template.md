# WRE Issue Policy + ABI Authoring Template

For non-umbrella issues in `Wrela Global DB (Spanner-Style, HLC) Big Bang`,
append the following exact policy block to the issue description.

```md
## Wrela-First + ABI Boundary Policy

- Implement as much as possible in native Wrela (`language/packages/db/*`).
- Keep Rust runtime work limited to kernel/hot-path primitives.
- ABI boundaries must be small, explicit, and versioned; no ad-hoc runtime surface expansion.
- Any ABI expansion requires explicit contract update in `WRE-509` plus ABI snapshot/gate coverage.
- Acceptance tests must exercise Wrela package entrypoints first.
```

## AC Mapping

- G1-3 requires all open non-umbrella issues to include the policy block.
- New issue creation should include this block at creation time.
- Any issue intentionally breaking the policy must include an exception and a remediation date.

