# Phase 0 ABI Envelope

Phase 0 is the only planned ABI expansion window before freeze.

## Additive Intrinsics

- `__wr_fs_read_dir`
- `__wr_fs_metadata`
- `__wr_fs_mkdir_all`
- `__wr_fs_remove_file`
- `__wr_fs_remove_dir_all`
- `__wr_fs_rename`
- `__wr_fs_set_executable`
- `__wr_process_run`
- `__wr_process_argv`
- `__wr_process_cwd`
- `__wr_process_exit`

## Policy

1. Additive only in Phase 0.
2. No removals or semantic drift from existing thin-core behavior.
3. Freeze `thin_core_snapshot` immediately after Phase 0 lands.
4. Any post-freeze ABI proposal requires an explicit exception and migration phase.

## Enforcement

- `scripts/governance/check_phase0_abi_snapshot.sh`
- `scripts/governance/check_phase0_surface_wiring.sh`
