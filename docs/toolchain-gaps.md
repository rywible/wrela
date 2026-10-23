# Toolchain gaps

*Capabilities this repository needed from the toolchain ([rywible/wrela-compiler](https://github.com/rywible/wrela-compiler)) and found missing or broken. AGENTS.md's rule holds: a gap is fixed in the toolchain, released, and the pin moved; never worked around here in another language. Started 2026-10-21 at Ryan's request.*

Each entry says what was needed, what happens instead, how it was found, the evidence, what this repository does meanwhile, and its status. Add an entry when a gap is found; when the fix ships, say in which release and keep the entry.

## Open

### A pointer can't press a semantic tree's button, and a tick can't find what the frames drew

- **Needed:** a ticked game's pause menu that a mouse works as the keys and a controller do. The menu must be the sim's, so a restart chosen in it replays from the tick log; then a click on an item has to reach the sim as which item it was.
- **What happens:** a tick's records hold a pointer press in screen pixels, but not the screen's size or anything the frames drew, so the sim can't tell which item is under it. The semantic tree (`std::semantics`, `std::host::publish`) would say so, as `Event::Assist` on the item's node, but toolchain 0.3.3's browser host lets a pointer press only a link with an `href` (`.wrela-semantics a.wrela-node.wrela-hit { pointer-events: auto }`, `runtime/browser/src/semantics.ts`). A `Button` or a `MenuItem` is under the canvas's pointer events: the keyboard's Tab and a screen reader press it, a mouse can't. And a link pressed keeps the keyboard's focus in the overlay, where key presses don't reach the program.
- **Found:** 2026-10-23, building the capybara game's menus (`games/capybara/world.wrela`, `screens.wrela`).
- **Meanwhile:** the capybara game publishes each page's items as links to anchors of the page (`#1-0`), which a pointer presses; and it numbers each item's node anew after every choice (`world::node`), so the host lets go of the pressed node and the keys come back to the game. A screen reader hears the items as links, not menu items.
- **Status:** open. Fix: let a pointer press a button's and a menu item's node (a hit class for every role that is activated, not only links), and give the canvas back the keyboard's focus after a pointer press on a node; then the game's items become `MenuItem`s, numbered once.

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

### The browser's canvas isn't in the device's preferred format

- **Needed:** the console's opening and library cost as little as they can a frame, on any device, so the page stays cool and saves battery while it is left open.
- **What happens:** toolchain 0.3.1's browser runtime configures the screen's canvas as `rgba8unorm` (`SCREEN_FORMAT`, `runtime/browser/src/abi.gen.ts`). Chrome on a Mac prefers `bgra8unorm` (`navigator.gpu.getPreferredCanvasFormat()`) and warns: "WebGPU canvas configured with a different format than is preferred by this device ("bgra8unorm"). This requires an extra copy, which may impact performance." Every frame a program presents pays a copy of the whole screen.
- **Found:** 2026-10-22, optimizing the console, in the Claude desktop app's browser pane (Chrome 152) on an M4 MacBook Air.
- **Meanwhile:** the console presents fewer frames (it draws only what changes, at most 30 a second while idle), which pays the copy less often.
- **Status:** fixed in 0.3.3 (rywible/wrela-compiler#22): the manifest marks the screen's targets (`"screen": true`, manifest version 9), and a normal run configures the canvas in `getPreferredCanvasFormat()` and builds the screen's pipelines in it; test mode keeps `rgba8unorm`, which it reads back. Checked in Chrome on the M4 Air: no warning.

### A tool's write empties the file before it writes it

- **Needed:** the keys tool (`games/capybara/keys`) writes takes.wrela again, every take Ryan has kept, each time he keeps one; the route tool writes routes.wrela again. A write that fails must leave the file as it was.
- **What happens:** toolchain 0.3.1's `files::write` (and `write_blob`) empties the file, then writes into it (`runtime/native/src/tool/caps.rs`, `open_file` then `write_with`). A write that fails part way, on a full disk say, leaves it empty or half written, and the page says only "Not kept": the takes kept since the last commit are lost.
- **Found:** 2026-10-22, in a council review of the capybara branch, from toolchain 0.3.0's source.
- **Meanwhile:** nothing; commit takes.wrela after keeping takes.
- **Status:** fixed in 0.3.2 (rywible/wrela-compiler#21): a write goes to a new file beside the old, which then takes its place. The pin is 0.3.3, which holds it.

### A failed load can't be told from nothing stored

- **Needed:** the capybara game keeps the player's best runs in its storage (`games/capybara/main.wrela`, `store_runs`). It must store a new run over the stored ones only once it has read them, or knows there are none: storing after a load that merely failed loses every run stored before.
- **What happens:** a `load` of a path nothing was stored at and a `load` that failed (the storage couldn't be read) give the same `IoError`, with one fixed message and no kind. So the game took every failed load as "no runs yet" and stored over them.
- **Found:** 2026-10-22, in a review of the capybara branch, from toolchain 0.3.0's source (`std::io`, the hosts' `request_status`).
- **Meanwhile:** nothing; the game's next finished run could store over runs it couldn't read.
- **Status:** fixed in toolchain 0.3.1 (rywible/wrela-compiler#20): a storage read of a path that holds nothing is answered `REQUEST_MISSING` by both hosts, and `IoError::missing` says so. The game stores its runs only after it has read them or the load was `missing`, and makes a load that failed otherwise again after 2 s.

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
