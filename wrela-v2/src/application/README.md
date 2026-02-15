# application

Use-case orchestration and pipeline coordination live here in v2.

`main.wr` is the bootstrap entrypoint placeholder for progressive CLI parity bring-up.
`check_pipeline.wr` defines staged check boundaries (`load -> lex -> parse -> type`).
`check_command.wr` currently runs the pipeline plus optional shadow-parity check against
frozen v1 when `WRELA_V1_BIN` and `WRELA_WORKSPACE_ROOT` are set.
`parity_command.wr` runs bootstrap frozen-contract scenarios against the frozen v1 binary.
`init_command.wr` creates a minimal project scaffold (`src/main.wr`, `tests/basic_test.wr`).
