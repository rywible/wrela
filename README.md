# Wrela

Wrela is an agent-native game studio. The first compiler component is a Rust source frontend for the ordinary language core.

The frontend preserves authored bytes, recognizes typed syntax with pinned LALRPOP, and diagnoses malformed input. Syntax acceptance does not perform semantic checking or execute programs.

See [the syntax policy](docs/frontend/SYNTAX.md), [the approved feature](docs/frontend/FEATURE.md), and [the frontend API/build contract](docs/frontend/README.md).
