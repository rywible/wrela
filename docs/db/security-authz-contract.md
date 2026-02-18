# RPC Authorization Contract (Phase 3 Slice)

## Identity Inputs

- `cluster_id`
- `node_id`
- `role` (`Voter`, `Learner`, `Gateway`, `Admin`)

## RPC Classes

- `RaftAppend`
- `RaftVote`
- `SnapshotInstall`
- `ClientRead`
- `ClientWrite`
- `ClusterAdmin`

## Fail-Closed Rules

- Missing `cluster_id` or `node_id` is always denied.
- Authorization is role-scoped:
  - `Admin`: all classes.
  - `Voter`: replication + client read/write.
  - `Learner`: append/snapshot/read only.
  - `Gateway`: client read/write only.

## Verification

- `cargo test -p wrela_runtime role_ -- --nocapture`
- `cargo test -p wrela_runtime missing_identity_fails_closed -- --nocapture`

## CDC Residency Egress (Phase 11 Slice)

- CDC sink polling is fail-closed against residency policy.
- Typed fail-closed token semantics:
  - `RESIDENCY_EGRESS_POLICY_UNSAT`: shard has no matching residency egress rule.
  - `RESIDENCY_EGRESS_DENY`: shard rule exists but sink region is not in its allowed set.
- `poll_cdc_for_sink` propagates token-prefixed `InvalidArgument` messages unchanged.

Verification:

- `cargo test -p wrela_runtime poll_cdc_for_sink_denies_cross_residency_egress -- --nocapture`
- `cargo test -p wrela_runtime poll_cdc_for_sink_denies_when_residency_policy_unsat -- --nocapture`
- `cargo test -p wrela_runtime poll_cdc_for_sink_allows_when_policy_matches_region -- --nocapture`
