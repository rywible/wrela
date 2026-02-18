# Local HA Harness

Use the local harness to build deterministic node directories for 3-node/3-region simulation.

```bash
scripts/db-local/smoke.sh
```

It creates:

- `.data/db-local/us-east-1/node-1`
- `.data/db-local/us-central-1/node-2`
- `.data/db-local/us-west-1/node-3`

Each node gets an isolated WAL path and profile metadata file.

