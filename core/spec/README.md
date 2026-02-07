# Wrela Spec

The authoritative language spec is core/spec/spec.wr.
Spec changes must include tests under tests/spec that fail if the spec section is removed.

Decision notes live in core/spec/decisions.

Thin-core guardrails:
- `core/spec/thin_core_snapshot.txt` locks runtime ABI/intrinsic/export surface.
