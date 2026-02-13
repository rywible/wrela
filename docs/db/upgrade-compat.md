# DB Upgrade Compatibility

## Format Contracts

- WAL record format: `major=1 minor=0`
- Snapshot manifest format: `major=1 minor=0`
- RPC frame format: `major=1 minor=0`

All boundary decoders must validate `major` against policy:

- reject too old (`< min_readable_major`) with typed `TooOld`
- reject too new (`> current_major`) with typed `TooNew`

## Policy

Default runtime policy:

- `min_readable_major = 1`
- `current_major = 1`

Migration-safe window example:

- `min_readable_major = 2`
- `current_major = 3`
- read allowed with `needs_migration=true` for major 2 artifacts.

## Verification Commands

```bash
cargo test -p wrela_runtime --test db_upgrade_compat
cargo test -p wrela_runtime db::versioning::
```

## Change Control

Any format major bump must include:

- `runtime/src/db/versioning/mod.rs` update
- compatibility tests update in `runtime/tests/db_upgrade_compat.rs`
- release blocker acknowledgement in `docs/db/release-blockers.md`
