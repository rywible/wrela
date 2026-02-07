# Wrela Hub

Wrela Hub is the canonical repository and governance surface for the Wrela language.
This repo hosts the compiler, runtime, spec, tooling, and Hub service.

Thin-core status (February 2026):
- Core runtime/stdlib intentionally stay minimal.
- Compiler + runtime kernel remain in Rust.
- High-level product modules (auth/storage/http/jobs/realtime/etc.) were removed from core stdlib/runtime.
- Ecosystem packages for those domains are planned separately.

Key documents:
- CHARTER.md
- CONTRIBUTING.md
- SECURITY.md
- core/spec/spec.wr
- .wrelahub/policies/builder-invites.md
- .wrelahub/policies/intents.md

Repo layout (high level):
- apps/hub: Hub service (Wrela runtime app)
- core: compiler/runtime/spec/tooling
- packages: experimental/incubating/maintained workspaces
- rfcs: RFC lifecycle storage
- tests/spec: executable spec tests
- .wrelahub: governance metadata and templates
