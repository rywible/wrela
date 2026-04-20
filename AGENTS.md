## Workflow Surface

- `just` is the canonical repo front door. Prefer named repo lanes over raw `cargo` for routine work.

- `cargo` is the Rust substrate and low-level escape hatch for narrow debugging or implementation-local checks.

- `wrela` is the authored-world and product-facing workflow surface (`test`, `perf`, `preview`, and similar commands). `just` is allowed to compose both `cargo` and `wrela` when the truthful proof spans both surfaces.

- The current canonical `just` lanes are: `check`, `check-clean`, `build`, `build-release`, `test`, `test-clean`, `test-all`, `test-runtime`, `test-compiler`, `test-cli`, `test-query`, `perf-smoke`, `perf-closure`, `lint`, `fmt`, `fmt-check`, `fix`, and `ship`.

## Intended Dev Loop

- You have carte blanche to use subagents as needed.

- Start with the cheapest truthful lane for the change. Prefer focused lanes like `just test-runtime`, `just test-compiler`, `just test-cli`, or `just test-query` before broader repo lanes.

- Use `just check` for fast compile feedback while iterating.

- Use `just test` as the fast default repo lane. It is allowed to combine Rust-native and authored-world proof when both are part of the real contract.

- Use `just test-all` for the full local semantic lane.

- Use `just perf-smoke` for cheap perf sanity when touching perf-sensitive code, and `just perf-closure` only when working the representative 1080p120 closure lane.

- Use `just check-clean` and `just test-clean` when you need cleanroom validation with isolated artifacts and incremental compilation disabled.

- Run `just ship` before handoff unless the task explicitly scopes a smaller proof surface.

## Completion Gate

- After completing acceptance criteria for a given project, that project is not complete until you launch an independent subagent to review your work for correctness, architecture, maintainability, and performance.

- It should verify that the project has been fully completed based on the expected outcomes of the plan.

- It should not continue your thread, it should launch fresh from your prompt.

- When you launch the subagent, in your message, tell it that it is a review subagent. Provide the subagent with any test findings that you already ran. It does not need to rerun tests.

- If you are reading this and you have been told you are a review subagent, YOU ARE THE SUBAGENT. Do not launch your own subagent. Just do the code review and return with your findings. Don't run any tests, just do a code review.
