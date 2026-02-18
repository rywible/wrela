# Shard Key Policy (Composite by Default)

`WRE-611` policy baseline:

- Composite shard keys are the default expectation.
- Single-field shard keys require explicit waiver + justification.
- CI preflight fails when projected shard skew exceeds allowed threshold.

Recommended patterns:

- `tenant_id + entity_id`
- `tenant_id + region_id + entity_id`
- `customer_id + logical_bucket + entity_id`

Avoid as primary-only shard key components:

- booleans (`is_active`)
- tiny enums with very small cardinality
- coarse locality-only keys without high-cardinality suffix

## Schema Lint Gate

Compiler-adjacent schema lint command:

```bash
python3 scripts/db-local/shard_key_schema_lint.py /path/to/schema.json
python3 scripts/db-local/shard_key_schema_lint.py /path/to/schema.json --strict-low-cardinality
python3 scripts/db-local/shard_key_schema_lint.py /path/to/schema.json --format json
```

Single-field escape hatch:

- Use `allow_single_shard_key.reason` with a non-empty, descriptive reason.
- Waivers are included in lint report output.

Example schema snippet:

```json
{
  "table": "orders",
  "fields": {
    "tenant_id": {"type": "string"},
    "order_id": {"type": "string"},
    "region": {"type": "enum", "variants": ["us", "eu", "ap"]}
  },
  "shard_key": {
    "fields": ["tenant_id", "order_id"]
  }
}
```

## CI Skew Preflight

Skew preflight command:

```bash
python3 scripts/db-local/shard_skew_preflight.py /path/to/projected-shard-load.json
python3 scripts/db-local/shard_skew_preflight.py /path/to/projected-shard-load.json --format json
```

Integrated preflight runner (lint + skew):

```bash
scripts/db-local/run_shard_key_preflight.sh /path/to/schema.json /path/to/projected-shard-load.json
```

Input load shape:

```json
{
  "profile": "global-3x",
  "shards": {
    "shard-a": 1200,
    "shard-b": 1000,
    "shard-c": 1100
  }
}
```

Gate guidance:

- Default skew threshold: `max(shard_load)/mean(shard_load) <= 1.50`
- Failing preflight indicates expected hotspots; move to composite key strategy.
