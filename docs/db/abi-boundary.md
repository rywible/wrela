# DB ABI Boundary (v1 seed)

Approved runtime ABI exports:

- `__wr_db_open`
- `__wr_db_close`
- `__wr_db_submit_batch`
- `__wr_db_read_point`
- `__wr_db_read_range`
- `__wr_db_txn_begin`
- `__wr_db_txn_prepare`
- `__wr_db_txn_commit`
- `__wr_db_txn_abort`
- `__wr_db_snapshot_start`
- `__wr_db_snapshot_status`
- `__wr_db_restore`

Change control:

- Any ABI expansion requires explicit contract updates and snapshot diff review.

