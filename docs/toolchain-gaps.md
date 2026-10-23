# Toolchain gaps

*Capabilities this repository needed from the toolchain ([rywible/wrela-compiler](https://github.com/rywible/wrela-compiler)) and found missing or broken. AGENTS.md's rule holds: a gap is fixed in the toolchain, released, and the pin moved; never worked around here in another language. Started 2026-10-21 at Ryan's request.*

Each entry says what was needed, what happens instead, how it was found, the evidence, what this repository does meanwhile, and its status. Add an entry when a gap is found; when the fix ships, say in which release and keep the entry.

## Open

### `wrela trace` and `wrela bisect` don't run a program's ticker

- **Needed:** an agent watching a ticked game's state frame by frame (`wrela trace <package> --watch <export> --input script.json`), and finding the first frame a condition holds (`wrela bisect`).
- **What happens:** the ticker never ticks. Exports that read the sim's snapshots stay at the first world's values for every frame.
- **Found:** 2026-10-21, building the capybara movement lab (`games/capybara`). With the run script, `wrela trace games/capybara --watch capybara,clock` printed tick count 0 for 120 frames, with or without `--no-gpu` and `--workers 2`. The same build, under `wrela-host --frames N --input`, and as a frame test (`@test(frames: 180, gpu: true, input: ...)`), ticks two ticks a frame in lockstep.
- **Cause, from the source:** `compiler/cli/src/trace.rs` steps each frame with the host's `frame()`; the native host's lockstep path (`lockstep_frame` in `runtime/native/src/program.rs`) is what runs the ticks due before a frame.
- **Meanwhile:** traces of the capybara's movement come from tests that step the sim directly; stills come from `wrela-host`.
- **Status:** open. Fix: trace and bisect run frames on the lockstep schedule when the program starts a ticker, as `wrela-host` and frame tests do, with a test in the toolchain's `trace` suite.

### A ticked program doesn't start in the Claude desktop app's browser pane

- **Needed:** an agent playing a ticked game in the browser pane beside the session (and anyone in an embedded Chrome).
- **What happens:** the page stops with "wrela stopped: the ticker's thread failed: it couldn't start". The page is cross-origin isolated, with `SharedArrayBuffer` and WebGPU (the pane's Chrome is 152).
- **Found:** 2026-10-21, with `games/capybara` and `examples/climber-lab`, under `wrela run` and as a static build (`wrela tool serve`). Both fail alike.
- **Likely cause, not confirmed:** the runtime starts the ticker as a module worker from inside the render worker (`new Worker(self.location.href, { type: "module" })` in `worker.js`), and the pane's error event carries no message. Nested module workers may be unavailable there.
- **Meanwhile:** play in Chrome itself. In-Chrome play of the capybara lab is still to be checked: a headless Chrome run in test mode ended before writing results, so it says nothing either way.
- **Status:** open; the cause needs confirming before a fix. If nested workers are the cause, a fix is for the page to start the ticker's worker and hand its port to the render worker.

## Fixed

### No passkeys: no credential API, signature checks or secure random

- **Needed:** passkey accounts for the console (`games/capybara/design.md`, "Accounts and leaderboards"): a page asks the browser for a credential (`navigator.credentials.create` and `get`, WebAuthn), and the server checks each assertion's signature with the platform's crypto through std (the design forbids crypto written here), against a challenge it made from secure random bytes.
- **What happens:** std has none of it. The browser host has no credential service (language.md §17), std has SHA-256 (`std::hash::sha256`) but no signature verification (ECDSA P-256, Ed25519 or RSA, the algorithms authenticators use), and no source of secure random bytes. A room knows a peer only by a 64-bit token the peer chooses (`std::net`, `Hello { token }`; language.md §19.1), so the server can't tell who a player is.
- **Found:** 2026-10-22, planning the capybara's leaderboards and accounts, from toolchain 0.2.2's reference (`wrela doc --export`).
- **Meanwhile:** nothing here depends on accounts yet; replays, ghosts, the watch mode and the route search are local.
- **Status:** fixed in toolchain 0.3.0 (rywible/wrela-compiler#19): the browser host's passkey ceremonies (`std::host::create_passkey`, `use_passkey`), a server's randomness and signature checks (`std::crypto`: ES256, Ed25519 and RS256, by the RustCrypto and dalek crates), and the WebAuthn checks of an authenticator's answer (`std::webauthn`). Binding a connection to an account is the server program's: a connection's first message carries a session token the accounts server issued, and the server checks it against its store.

### A server has no store of its own that outlives its build

- **Needed:** leaderboards: the server keeps every verified run and its replay, serves a slice of them (a board's top runs, one replay to watch), and keeps them through a new server build. A board is tied to a build of the game's sim, not of the server.
- **What happens:** a server's only durable state is a room (language.md §19). A room's state is every peer's: each peer applies the whole log, and a peer far behind is sent the state in one 64 KiB frame, so a board of replays would go to every player and soon past the frame. And a room's journal is read only by the server build that wrote it (`<data>/<build hash>/`, §19.3), so each server deploy would start every board empty.
- **Found:** 2026-10-22, with the gap above.
- **Meanwhile:** the boards are local: a player's best runs and the route search's, kept in the game's own storage.
- **Status:** fixed in toolchain 0.3.0 (rywible/wrela-compiler#19), after Ryan chose persistent storage (SQLite on the server, backed up to a bucket): `std::store`, keys and values in key order in one SQLite file beside the rooms' journals, which every room and every server build shares. Reads are requests; writes commit when a call returns, before its records are kept and its sends go. A client asks a server for a board or a replay as a message on its connection, which the server program answers from the store; a room's log is only for what its peers share.
