# Wrela Hub Technical Plan

Date: 2026-02-01

## Scope and Constraints
- Single-repo hosting: Wrela Hub hosts this repo as the canonical origin.
- HTTPS-only Git smart HTTP; no SSH.
- Auth: JWT-based HTTP auth.
- Storage: Wrela storage (KV + CAS + scan) with write-through to object storage (S3). SSE-S3 enabled by default.
- CI: first-party CI runners on Fly Sprites; single-use runners; canonical CI job locked to BDFL/stewards.
- Spec tests required from day one for any spec change.
- Releases: “when ready” cadence; support current minor only.
- Package bundling: Option B (vendored snapshots in repo).
- Governance: BDFL-forever with scoped domain authority; stewards handle moderation.
- Identity: real name preferred, optional; required above a trust tier with one public identity link (LinkedIn optional).
- Wrela-only: all Hub services built on Wrela runtime primitives; missing features are added to runtime.

## Social Principles (Why the System Exists)
These are invariants enforced by product design and policy. They are not optional.

- Power is earned through concrete impact, not popularity.
- Authority is scoped by domain, never global by default.
- All authority is reversible (decays with inactivity or abuse).
- Governance actions are observable and auditable.
- Process must make good work easier and low-signal behavior harder.
- The system must prefer clarity and accountability over “vibes.”

### BDFL-forever, Constitutional Not Vibes
- Final authority rests with the BDFL for language semantics, governance, and repository control.
- BDFL decisions are absolute but not silent: every veto carries a reason category and rationale.
- BDFL power is operationally scoped: routine merges are delegated; BDFL intervenes on escalation.
- Succession plan exists for catastrophic absence (designated successor, not a council).

### Succession Plan (Catastrophic Continuity)
- A designated successor is named (public or private).
- The successor can be changed at any time by the BDFL.
- If the BDFL is inactive for 180 days, the successor activates.
- The project remains fork-friendly if succession fails.

### Decision Rubric (Published)
When evaluating proposals, prioritize:
1) semantic clarity over feature power
2) composability over convenience
3) long-term readability over short-term ergonomics
4) concept count minimization
5) teaching cost

## “No” Policy (Prevents Drama)
- Rejection is normal and expected.
- Rejection is not a judgment of competence.
- Rejections must use a reason category:
  - vision conflict
  - semantic risk
  - complexity budget
  - timing
  - personal call
- Rejected ideas can be resubmitted if constraints change.

## Runtime Primitives Already Available
- HTTP server/routing: `crates/runtime/src/http.rs`
- Auth (password + JWT + email verification models): `crates/runtime/src/auth.rs`
- RBAC (roles/permissions/scopes): `crates/runtime/src/rbac.rs`
- Storage (KV + CAS + scan): `crates/runtime/src/storage/mod.rs`
- Jobs/queues: `crates/runtime/src/jobs.rs`
- PubSub: `crates/runtime/src/pubsub.rs`
- Realtime rooms/inbox: `crates/runtime/src/realtime.rs`
- Search (inverted index): `crates/runtime/src/search.rs`
- Files/blob storage: `crates/runtime/src/files.rs`
- Rate limiting: `crates/runtime/src/rate_limit.rs`

## Runtime Gaps to Implement
1. Git smart HTTP server (single repo).
2. Git object storage abstraction backed by Wrela storage with write-through to S3.
3. Hook pipeline (pre-receive/update) integrated with governance checks.
4. Diff/patch utilities for PR review UI and spec test enforcement.
5. CI runner that provisions Fly Sprites and collects logs/artifacts.

## Repository Structure (Required)
```
wrelahub/
  README.md
  CHARTER.md
  CONTRIBUTING.md
  SECURITY.md
  LICENSE

  .wrelahub/                  # Governance metadata only (no code)
    domains/
      compiler.yml
      runtime.yml
      tooling-lsp.yml
      spec.yml
      infra.yml
      packages.yml
      docs.yml
    policies/
      dependency-rules.yml
      promotion-rules.yml
      review-rules.yml
      release-rules.yml
    templates/
      rfc.md
      mandate.md
      analysis-report.md

  apps/
    hub/                      # Wrela Hub service code (Wrela runtime app)
      README.md
      src/
        main.wr
        modules/
          auth/
          governance/
          repo/
          packages/
          rfc/
          spec/
          messaging/
          moderation/
          ci/
          audit/
          search/
          files/
          api/
      tests/
      assets/

  core/
    spec/
      README.md
      decisions/
      tests/
    compiler/
      crates/ or src/
      tests/
    runtime/
      src/
      tests/
    tooling/
      lsp/
      cli/
      formatter/
    bundles/
      default/
        <pkg>/
      default.bundle.toml

  packages/
    experimental/
    incubating/
    maintained/

  rfcs/
    draft/
    active/
    accepted/
    rejected/

  infra/
    ci/
    release/
    dev/

  tools/
  scripts/
  docs/
```

Notes:
- `.wrelahub/` is metadata and templates only.
- `.wrelahub/domains/*.yml` defines structure, scope, and rules (not user assignments).
- User-to-role assignments live in Hub storage and change via governance actions.
- `apps/hub/` is the actual Hub service.
- `core/bundles/**` is the only source for bundled packages.
- `packages/**` is development workspace only.

## Storage Model (KV + CAS)
- Truth via append-only audit log; read models are materialized views.
- CAS required for all mutable updates.

Example keyspace (prefix-based):
- Users: `user:<id>`, `user:by_email:<email>`, `user:by_handle:<handle>`
- Governance: `domain:<id>`, `role:<id>`, `assign:<scope>:<user>`
- Promotions: `promotion:<id>`
- Mandates: `mandate:<id>`
- RFC/spec/decisions: `rfc:<id>`, `spec:section:<id>`, `decision:<id>`
- Packages: `pkg:<name>`, `pkg:version:<name>:<ver>`
- Bundles: `bundle:default` (manifest list)
- Messaging: `msg:room:<room>:<id>`
- Moderation: `report:<id>`, `action:<id>`
- Audit: `audit:event:<ts>:<id>`

## Governance Model (Behavioral + Mechanical)

### Trust Tiers (Contribution Gates)
- Reader: read-only access.
- Observer: can comment on existing discussions; no proposals.
- Analyst: can open Analysis Reports; messaging access begins here.
- Contributor: can open PRs and RFCs; limited review authority.
- Reviewer: can approve PRs in scoped domains; cannot merge own PRs.
- Maintainer: can merge within domain; resolve local deadlocks.
- Steward: governance + moderation authority; handles reports.
- BDFL: final authority on vision, semantics, and governance.

### Slow Onramp Mechanics (Anti-slop)
- No “issues.” Replace with Analysis Reports.
- Analysis Reports require:
  - observed behavior
  - expected behavior
  - minimal reproduction
  - why this matters to the language
  - why it might *not* be a bug
- No blank fields. Structured templates required.
- One active Analysis Report per person initially.
- New submissions are review-visible immediately, but public only after triage.

### Authority Decay
Roles decay with inactivity (soft decay at 90 days, removal at 180 days). Re-earning is faster than first-time promotion.

### Objection-Based Governance
No polls. Changes proceed by default unless a qualified objection is raised.
Objections must be scoped, reasoned, and actionable. Unresolved objections escalate through domain owners to BDFL.

### Mandates (“Fuck ya, go build it”)
Mandates are explicit delegations of ownership:
- Defines scope, success criteria, timebox, and kill conditions.
- Grants temporary authority in the scoped area.
- Protects against bikeshedding; objections must be specific.
- One active mandate per person.

### Ownership Protection (Anti-bikeshed)
- Once ownership is assigned, objections must be concrete and actionable.
- Vibe-based critique is explicitly invalid.
- Design debates that matter must be written down (RFC/Decision Note), not left in chat.

### Review Bandwidth as a First-Class Constraint
- Early limits:
  - one open PR per contributor (initially)
  - one active mandate per person
  - one open RFC draft per person
- Review debt thresholds trigger a temporary freeze on new proposals until backlog drops.

### Decision Notes (Authoritative Resolution)
- All escalations end with a Decision Note.
- Decision Notes are short, auditable, and linked to affected artifacts.
- They are the only authoritative source of “what was decided” outside the spec.

## Messaging and Moderation (Signal First)

### Messaging Model (Pre-Castle)
- Messaging is gated to Analyst tier and above.
- No global chat. No proximity chat until the castle layer exists.
- Domain rooms only; slow-mode default; messages are logged and prunable.
- DMs disabled in MVP; later opt-in by recipient only.

### Message Gravity (Where Decisions Live)
- Proximity/ambient chat (later): zero gravity (never authoritative).
- Domain rooms: low gravity (coordination only).
- RFC comments + Decision Notes: high gravity (authoritative).

### Reporting and Bans
- Messaging is a privilege, not a right.
- Reports target harassment, threats, or persistent unwanted contact.
- Responses are graduated: throttle → warning → mute → suspension → ban.
- Messaging bans are separate from contribution bans.
- All actions are logged and auditable by stewards.

## Identity Policy (Accountability Without Exclusion)
- Real name and photo are preferred but not mandatory.
- Above a defined tier, one public identity link is required (LinkedIn optional).
- Optional third-party ID verification may exist for steward-level roles only.
- Anonymity remains allowed, but slows trust progression.

## Community Norms (Product Enforced)
- “Chat is for exploration, not decisions.”
- “If it matters, write it down.”
- “Objections must be specific and actionable.”
- “Silence is allowed.”
- “Ownership implies protection from drive-by critique.”

## Compatibility Contract (Promise Surface)
- Define what counts as “breaking” for language/toolchain/packages.
- Stable surface area is explicit; experimental/incubating is not promised.
- Support window: current minor only.
- Maintained packages must declare toolchain compatibility ranges.

## Contribution Licensing (DCO)
- Use Developer Certificate of Origin (DCO) sign-offs for all contributions.
- DCO attests the contributor has the right to submit under the project license.
- No CLA required for initial phase; revisit only if relicensing becomes necessary.

## Funding and Sponsorship (Founder-Run)
- Donations and sponsorships go to the BDFL/founder by default.
- Funds are the founder’s personal money to use as desired.

## Enforcement Details (Must Be Explicit)

### Domain Boundary Policy
- Domain membership is determined by path globs in `.wrelahub/domains/*.yml`.
- A PR that touches multiple domains requires approval from each affected domain.
- Spec-touching changes are defined as any change under `core/spec/**` or `rfcs/**`.
- Path overrides are allowed only by BDFL/stewards (explicit and audited).

### Contribution Gate Checklists (Definition of Done)
- Analyst promotion:
  - One accepted Analysis Report with required sections completed.
  - No unresolved moderation actions.
- Reviewer promotion:
  - At least 3 accepted PRs in the domain.
  - At least 5 high-signal reviews accepted by maintainers.
  - Low revert rate over a 90-day window.
- Maintainer promotion:
  - Sustained reviewer performance in domain.
  - Demonstrated architectural judgment under conflict.
  - On-call expectation for domain health (response time: 7 days).

### Rejection and Veto Taxonomy (Required in UI/API)
- All rejections and vetoes must include a reason category:
  - vision conflict
  - semantic risk
  - complexity budget
  - timing
  - personal call
- Optional freeform rationale is required for vision conflict and semantic risk.

### Audit Log Schema (Immutability)
- Immutable events: role changes, mandate changes, RFC decisions, spec changes, bundle promotions.
- Moderation events are immutable but may be redacted for privacy with a public tombstone.
- Audit log is append-only; updates create a new event rather than editing history.

### Spec Test Enforcement
- Every spec section must have at least one test that fails if the section is removed.
- Spec tests live in `tests/spec/**`.
- CI blocks any `core/spec/**` change without a corresponding test change.
- RFCs require spec tests only when accepted and implemented.

### Spec Test Format
- Required: ordinary Wrela tests under `tests/spec/**` using `assert value` / `assert identity`.
- Allowed: inline examples in spec docs, but non-authoritative unless backed by a test.

### Git Storage Format and Limits
- Git objects are stored in Wrela storage with S3 write-through for large objects.
- Packfiles and refs are tracked via storage keys and audited on write.
- Max object size: 50 MB. Max pack size: 50 MB.
- Unreferenced objects are garbage-collected on a 30-day cadence (no immediate GC on force-push).

### CI Runner Security Model
- Untrusted PRs always run in isolated single-use Sprites.
- Secrets are not injected for untrusted PRs.
- Artifacts/logs are retained for 30 days.

### Package Promotion Criteria (Explicit)
- Experimental → Incubating:
  - Charter, owner, minimal docs, tests, and versioning policy.
- Incubating → Maintained:
  - Stability window, compatibility CI, usage signal, and maintainer commitment.

### Bundle Promotion Approval
- Promotion into `core/bundles/**` requires:
  - BDFL approval, and
  - package maintainer approval, and
  - steward approval.

### Release Checklist (Required)
- Spec tests pass.
- Bundle manifest frozen and reviewed.
- Signed tag applied by BDFL/stewards.
- Version bump + release notes generated.

### CI Definition Location
- Canonical CI configuration lives in `.wrelahub/policies/ci.yml`.
- Runner code lives in Hub/infra; PRs cannot modify CI policy.

### Release Artifacts
- Toolchain binaries
- Spec version stamp
- Bundle snapshot
- Release notes
- Checksums/signature file

### Identity Tier Thresholds
- A public identity link is required at Reviewer tier and above.

## Git Hosting (HTTPS Smart HTTP)
- Implement Git smart HTTP endpoints (`/info/refs`, `git-upload-pack`, `git-receive-pack`).
- JWT auth for push/fetch.
- Pre-receive hooks enforce:
  - domain permissions (path-based)
  - spec test presence for `core/spec/**` changes
  - protected files (governance docs, bundle manifest)
- Git objects stored in Wrela storage with S3 write-through for large objects.

## CI on Fly Sprites
- Jobs enqueued via runtime jobs.
- CI worker provisions a Sprite per job (single-use).
- Sprite clones repo over HTTPS, runs canonical job.
- Logs/artifacts stored via files runtime.
- CI config changes restricted to BDFL/stewards.

## Governance and Workflows
- Trust tiers: Reader → Observer → Analyst → Contributor → Reviewer → Maintainer → Steward → BDFL.
- Real-name preferred; required above a threshold with one public identity link.
- Domain-scoped capabilities enforced by hooks and UI.
- Mandates: explicit delegation with scope + timebox.
- RFC lifecycle: Draft → Discussed → Accepted → Implemented/Rejection.
- Spec: authoritative; changes require spec tests from day one.
- Releases: trunk-based; release/X.Y branches; current minor only.
- Packages: experimental/incubating/maintained with promotion rules.
- Bundles: only `core/bundles/**` are shipped.

## Toolchain CLI Integration
Integrate a minimal set of Wrela Hub commands into the Wrela CLI so contribution flows are always available.

Initial command set (read-only + safe writes):
- `wrela hub status` (auth status, current tier, domain capabilities)
- `wrela hub whoami` (profile + identity status)
- `wrela hub rfc new` (create RFC from template)
- `wrela hub mandate list` / `wrela hub mandate view <id>`
- `wrela hub pkg promote <name> --to bundle:default` (BDFL/steward only)
- `wrela hub pr open` (create PR with required structured fields)

Constraints:
- CLI uses the same JWT auth as HTTPS Git.
- Commands are thin clients of Hub APIs.
- Any write command honors governance permissions and audit logging.

## Phased Implementation Roadmap

### Phase 0 — Repo Restructure
- Create new structure with `apps/hub/`, `core/`, `packages/`, `.wrelahub/`.
- Move existing compiler/runtime/lsp into `core/`.
- Add governance docs and templates.

Exit: structure in place; domain mapping exists.

### Phase 1 — Git Hosting (HTTPS)
- Implement smart HTTP server in runtime.
- JWT auth for Git operations.
- Store git objects in Wrela storage with S3 write-through.
- Add hook pipeline (read-only initially).

Exit: clone/fetch/push works against Wrela Hub.

### Phase 2 — Governance Core + Identity
- Implement trust tiers + RBAC scopes.
- Enforce domain permissions in hooks.
- Add identity requirements for higher tiers.
- Audit log for all governance actions.

Exit: permissions enforced; promotions audited.

### Phase 3 — RFC / Spec / Mandates
- RFC lifecycle UI + storage.
- Spec sections with required tests.
- Mandate object + lifecycle enforcement.

Exit: spec changes blocked without tests; mandates trackable.

### Phase 4 — Packages + Bundle Snapshots
- Package tiers and charters.
- Bundle snapshot workflow (promote maintained → bundle).
- Bundle manifest enforcement in release builds.

Exit: bundles ship only from `core/bundles/**`.

### Phase 5 — CI on Fly Sprites
- CI worker provisioner.
- Canonical job execution.
- Results and logs stored.

Exit: PRs blocked on CI success.

### Phase 6 — Messaging + Moderation
- Domain room messaging, gated to Analyst tier.
- Reports + steward actions + audit log.

Exit: safe, gated comms in place.

### Phase 7 — Release Automation
- Release branch + tag tool.
- Signed tag enforcement.
- Backport workflow for current minor only.

Exit: releases are one command + CI.

### Phase 7.5 — Toolchain CLI Integration
- Implement `wrela hub` subcommands in the CLI.
- Add auth token management (login/logout) using Hub JWT.
- Wire commands to Hub APIs and enforce permission checks.

Exit: core contribution actions available from the toolchain CLI.

### Phase 8 — Castle Layer (Later)
- Read-only projection of governance into spatial UI.

Castle principles:
- The castle is a visualization layer, not the source of truth.
- Power lives in repo governance; the castle reflects it.
- No XP, grinding, or popularity mechanics.
- Optional participation; no required presence.

## Open Questions (Deferred)
- Git object format details (packfiles in object storage vs local FS cache).
- CI runner warm pool vs cold start.
- Signed commits (currently: only signed tags).

## Governance Schemas (Concrete)

### Domain Config (`.wrelahub/domains/*.yml`)
Domain rules are uniform across domains, but allow optional extra checks.
```yaml
name: runtime
path_globs:
  - "core/runtime/**"
required_approvals: 1
reviewer_min_tier: reviewer
merge_min_tier: maintainer
allow_self_merge: false
escalation_role: bdfl
extra_checks:
  - "no-breaking-api"
```

### Extra Checks Allowlist
- Domains may only reference checks defined in `.wrelahub/policies/review-rules.yml`.
- Unknown checks cause CI to fail.

### Spec Test Format (`tests/spec/**`)
- Each test is a normal Wrela test file.
- `to test_*` functions are discovered by the test runner.
- Use `assert value` / `assert identity` for expectations.

### Language Test Framework
The test framework is defined in `docs/testing.md` and implemented in the toolchain.

### CI Policy (`.wrelahub/policies/ci.yml`)
```yaml
name: default
steps:
  - name: tests
    run: "cargo test"
    timeout_secs: 1800
  - name: fmt
    run: "cargo fmt --check"
    timeout_secs: 300
  - name: clippy
    run: "cargo clippy --workspace --all-targets --all-features"
    timeout_secs: 1800
```
