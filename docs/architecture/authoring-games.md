# Authoring games with Wrela

A game is trusted TypeScript plus ordinary semantic source. Engine packages own compilation, simulation, rendering and authoring. `games/<id>/` owns game rules and UI; `@wrela/examples` contains optional source libraries.

Implement `GameDefinition` and `GameModule` from `@wrela/runtime`. The lifecycle is `initialize`, semantic `input`, `fixedStep`, `inspect`, `save`, `load`, optional `scene`, and `dispose`. `GameDriver` serializes steps and loads, bounds frame catch-up, and binds saves to both the game version and source identity. Validate inputs and save state before mutating rules. Release resources in `dispose`, including partially initialized resources.

The smallest complete example is `games/switch-gate/src/module.ts`: a switch changes a gate, collision blocks the closed gate, crossing the exit completes an objective, and saves retain progress. It works headlessly for verification and uses public scene APIs for its browser presentation. Winter Valley's rules and UI live in `games/winter-valley`; creature study rules live in `games/creature-study`. The existing visual showcase is a separate application module under `games/showcase`.

To add a game:

1. Add the definition and its source project under `games/<id>/src/`.
2. Register the trusted definition in `games/registry.ts`.
3. Add `games/<id>/game.json` with `version: 1`, `game`, `gameVersion`, `entry` and explicit `dynamicRoots`.
4. Run `bun run build`. The build discovers manifests and emits portable games under `dist/games/<id>/index.html`, plus `dist/games.json`.

The generic player accepts `?game=<registered-id>` or an embedded game manifest. A data import can select a registered ID; it cannot import an executable URL. `window.wrelaGame` exposes discovery, input, inspection, bounded stepping, capture, save and load. Scene rendering and game state are independently inspectable. Source delivery and cooking include only the entry dependency closure and explicitly declared dynamic roots. Add every definition that gameplay loads by ID to those roots.

For source edits, start with `discover({target})`, retrieve the particular operation schema, and inspect only relevant paths. Studio's `window.wrela.execute({method,args})` returns structured success/error envelopes; existing typed methods remain available. Unknown request fields are errors. Error responses retain codes, paths, document conflicts and a suggested next action.

A work record retains one immutable baseline and up to 32 operation alternatives. Review materializes the selected alternative and binds evidence to source identities. Adoption first retains a durable intent. Once source is accepted, a receipt failure returns a recovery status instead of claiming the edit failed. Browser adoption also saves the source before marking the publication committed. `accepted` with `persistencePending` means the live edit exists and the pending intent must be retried to finish persistence. Retry with the original work key and proposal ID; do not mint a new publication to recover the old one.

CLI `apply` accepts `{workspaceKey,batch}` with an optional stable `batch.transactionId`. Workspace identity records provenance. Read/write dependency identities govern whether independent writers can rebase. Publication and the transaction receipt share the same atomic generation pointer. An identical transaction retry returns its original receipt, including after restart; changing a request while retaining its ID is rejected. Reviewed work uses the stronger whole-baseline check because its evidence may measure the entire scene.

Large source libraries use `DocumentRepository`: metadata search does not instantiate an authoring session, and `loadWorkingSet` retrieves only an explicit dependency closure. The catalog schema ceiling is 100,000 records; the measured regression workload has 1,000. Each task retains the 256-definition and 16 MiB source budget, and bridge files retain their size limits. The file bridge exposes `author <workspace> catalog <query.json>`, `inspect <id>`, and `working-set <roots.json>`. CLI edits merge their bounded task result into the complete source catalog. Keep task budgets separate from library capacity.

Verification commands:

- `bun run check`: TypeScript, browser types, AST package boundaries and formatting/lint.
- `bun run test`: active package, application, game and tool CPU tests; historical output snapshots are excluded.
- `bun run test:architecture`: storage fault injection, cross-process publication, catalog and complete authoring-to-game workflow.
- `bun tools/architecture-benchmark.ts`: 32 alternatives over the 200-definition workload.
- `bun tools/verify-agent-game.ts`: real WebGPU play, inspection and save restoration.
- `bun run verify`: rendering, storage, device recovery and Studio UI on hardware.

These are executable engineering checks. Artistic quality, agent completion rates and player enjoyment require separate evaluated tasks and playtesting; a green infrastructure check does not establish them.
