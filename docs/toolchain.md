## Which toolchain runs

- The root `wrela.toml` pins a release (`toolchain = "X.Y.Z"`). Any installed `wrela` runs that one, downloading it to `~/.wrela/toolchains` first.
- `wrela toolchain` says which toolchain runs and why.
- `WRELA_TOOLCHAIN=~/projects/wrela-compiler` runs a compiler checkout instead, built with Cargo, while you change both repositories. Unset it before you run the checks for a PR.
- The host of a pinned release is `~/.wrela/toolchains/X.Y.Z/bin/wrela-host`. A build only runs on a host of the same command stream version.

## The working loop

```bash
wrela check games/last-green              # types, borrows, effects: fast, no build
wrela test engine frame_                  # the tests whose names hold `frame_`
wrela suite wrela::engine/tests/frame::   # one member's tests, as the suite runs them
wrela fmt engine/frame.wrela              # format in place (`--check` to only report)
wrela suite                               # everything, before a PR: fmt, allow, every test
```

- `wrela suite <name>` takes exactly one of `fmt`, `allow`, `wrela::<member>::` or `wrela::<member>::<test>`. A name that selects nothing fails, and so does a member that has no tests.
- `allow` fails on any file `wrela.toml`'s `allow` list doesn't name. A stray scratch file in the workspace fails it, so keep scratch files outside.

## Finding your way

| Command                                                                   | Gives                                                                                                                                                                                             |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `wrela context <item> <pkg> [--budget n]`                                 | An item's source, with its doc comment, and what it needs nearby                                                                                                                                  |
| `wrela doc <item> [<pkg>]`                                                | An item's signature and docs (`wrela doc std::gpu::timed`); a built-in type's too (`wrela doc f32`, `vec3`, `str`, `[T]`), with its methods. `wrela doc builtins` lists them                      |
| `wrela query <pkg> "callers F"`                                           | Call graph, types, impls, effects, borrows, instantiations: `callers`, `callees`, `type file:line:col`, `fields`, `impls`, `effects`, `borrows file:line`, `instantiations`, `search <signature>` |
| `wrela primer [area]`                                                     | The language's rules by area (`mem`, `gpu`, `eff`, `trait`, ...)                                                                                                                                  |
| `wrela explain E0513`                                                     | What an error code means, with a program that has it and the same program fixed                                                                                                                   |
| `wrela pipelines <pkg>`                                                   | Each GPU entry point, its instantiations and where CPU code reaches it; a site under a runtime condition is marked `(under a condition)`; `--json` gives each render pipeline's `attachment_bytes`; `--set` as `build`'s |
| `wrela refactor <pkg> rename\|move\|add-param\|change-mode ... --dry-run` | A checked edit across the package, as a diff first                                                                                                                                                |
| `wrela refactor <pkg> extract-const\|inline\|extract-fn ... --dry-run`    | Extract a constant (`file:line:col NAME`) or lines into a function (`file:first-last name`), or inline a function; each refuses where what the code computes would change                         |
| `wrela fix <pkg> [--dry-run]`                                             | The compiler's suggested fixes, applied (`--dry-run` shows the diff)                                                                                                                              |
| `wrela migrate <pkg> [--dry-run]`                                         | The migrations' fixes alone: run it on each member after moving the pin                                                                                                                           |

`query` answers within one package: a caller in another package isn't listed. Check what a `refactor` misses there too. Fall back to `rg` or `git grep`.

## Running a program

```bash
wrela build games/capybara -o build/cand/capybara
~/.wrela/toolchains/0.10.0/bin/wrela-host build/cand/capybara --frames 600 --size 640x360 \
    --workers 1 --rgba out.rgba                        # prints the state hash last
wrela run games/capybara                               # serve it to a browser
```

- `--input <script.json>` plays input events; `wrela script docsite scripts::KEYS -o keys.json` writes one from a `Script` constant.
- `wrela run --lan` (and a tool's `serve::start(..., lan: true)`) serves the LAN too, and prints each of this machine's addresses (`http://<ip>:<port>/`): a phone or a tablet opens the page there, and a test-mode run there may `PUT` its results back. Over plain HTTP a browser elsewhere gives the page no WebGPU nor `SharedArrayBuffer` (they need a secure context): start that device's Chrome with `--unsafely-treat-insecure-origin-as-secure=http://<ip>:<port>`, or serve a release page over HTTPS (Safari has no such flag: use the release page). Test mode runs on a private network's address as on `localhost`; `#test&...&show=1` draws the run's summary over the canvas at its end (the intervals' 50th, 95th and 99th percentiles, the frames over 33 ms, when it said `playable`, its first frame, its pipelines, the adapter), for a device that can't put its results.
- A tool's `browser::navigate(b, url)` takes an open browser to a page again, in the same tab and profile, so its caches are warm: a page's warm run, after which `run(b, Script::new().wait(Until::done(), ...))` waits for that page's own result.
- `--workers 1` makes a run repeat exactly. With more workers, work that streams or runs in parallel can land on other frames.
- The hash covers every command byte submitted. Equal hashes mean equal commands; GPU pixels can still differ by a step or two between runs of one build.
- A build keeps its storage beside it (`<build>/storage`). A game that stores settings (the capybara) draws differently once they're there, so start each comparison from the same storage.
- `wrela-host --timestamps` prints each pass's GPU time; `--png`, `--every` and `--png-from` save frames; `--until TEXT` runs until the program prints `TEXT`.
- `wrela-host --no-gpu <build> --ticks N --log out.ticks` runs a ticked program's ticks alone and records them; `--replay out.ticks` checks each tick's hash and names the first that differs.

## The console's server

The console's program has a server build: accounts and the games' boards (platform/server.wrela, console/server). Its passkeys are for `wrela.dev` from `https://wrela.dev` (`RP_ID` and `ORIGIN` in console/main.wrela). It runs on Cloudflare, in a Worker of its own (the catalog's `console.api`, `wrela-console-api`) on a route of `wrela.dev/api/*`, which takes those paths before the console's Worker does: the console's Worker, like every other, serves static assets alone, so no request of theirs runs a script. Toolchain 0.18.0's build writes `server/cloudflare.js` (the host: one Durable Object class, `WrelaServer`, holding the server's named instances, each with its own SQLite store; `api`, the Worker's router for `/api/*`, which gives each request to a fresh edge instance of the program; and the Worker Loader binding `WRELA_LOADER` for archived verifiers), `server/cloudflare.json` (the Wrangler members it needs) and `server/verifier/` (the build's verifier, wrela-verify-1), and `wrela tool release` puts them beside the server's Worker's script (`<api>.server/`, studio/release/site.wrela). It needs **Workers Paid**: Workers Free gives an invocation 10 ms of CPU, and a run's verification at the edge takes more (developers.cloudflare.com/workers/platform/limits/).

**Set once, by Ryan, before a deploy that has the sharded server** (docs/plan-online-play.md step 14):

- **The bucket of builds**, where each release's deploy keeps its server's verifier (`verifiers/<game>/<board>/<sha256>/`, release.json's `verifier`), which the API Worker binds as `WRELA_BUILDS`: `npx wrangler@4.151.0 r2 bucket create wrela-builds`. A new bucket, not the unused `wrela-releases`, so its name says what it holds (studio/release/site.wrela's `BUILDS_BUCKET`). The deploy's `CLOUDFLARE_API_TOKEN` then needs **Workers R2 Storage: Edit** beside its Workers permissions, since it puts each file with `wrangler r2 object put --remote` before any Worker deploys, unless `wrangler r2 object get ... --pipe --remote` finds the build's `verifier.json` there already (studio/deploy's `keep_verifier`).
- **The ceremonies' keys**, two Worker secrets of the API Worker: `WRELA_CEREMONY_KEY`, the key a sign-up's and a sign-in's challenge is made with (an HMAC-SHA256 tag), and `WRELA_CEREMONY_KEY_OLD`, the one before it, which still finishes a challenge made just before a rotation. Each at least 32 bytes: `openssl rand -base64 48 | npx wrangler@4.151.0 secret put WRELA_CEREMONY_KEY --name wrela-console-api` (and the same for `WRELA_CEREMONY_KEY_OLD`, any other value until the first rotation). To rotate: put the current value into `WRELA_CEREMONY_KEY_OLD`, then a new one into `WRELA_CEREMONY_KEY`; a challenge lives 5 minutes, so the old one may go after that. A secret survives deploys, which never name it. The Worker gives them to the program as the headers `x-wrela-key` and `x-wrela-key-old` on a `POST` to `/api/accounts` alone, removing any a client sent from every request (docs/toolchain-gaps.md, "A server can't read its Worker's secrets"); without a key of 32 bytes the server begins no ceremony.

**When a board closes** (games/capybara/replay.wrela's `BOARD` goes up): add the board before it and its last release's `release.json` `verifier` path to games/capybara/boards.wrela's `ARCHIVED`, and set studio/release's catalog `console.board` to the new one (a test checks it matches); runs of the old board go on through its archived verifier (`std::server::call_build`), and a board left out of `ARCHIVED` is closed.

To run it on this machine, in the real Workers runtime:

```bash
wrela build console -o build/cons --set 'RP_ID="localhost"' --set 'ORIGIN="http://localhost:8787"'
```

Then stage it in one Worker, which the release splits in two: the console's files in `wrela/` as its assets, with `server/cloudflare.json`'s `assets.run_worker_first` and an `ASSETS` binding; its `server/` as `wrela.server/`; a `wrela.worker.js` that exports `WrelaServer` and `WrelaRoom` and calls `api` first with the keys set as `api_script` sets them, then the assets; a `wrela.wrangler.json` with `server/cloudflare.json`'s members, `WRELA_ORIGINS` set to `http://localhost:8787`, `WRELA_LIMITS` to `[]` and an `r2_buckets` binding `WRELA_BUILDS` to `wrela-builds`; and a `.dev.vars` with `WRELA_CEREMONY_KEY` and `WRELA_CEREMONY_KEY_OLD`. Run `npx wrangler@4.151.0 dev --config wrela.wrangler.json` there. Its Durable Objects, R2 bucket and Worker Loader run locally; `wrangler r2 object put ... --local` puts an archived verifier in its bucket. The local explorer (`/cdn-cgi/local/explorer/api`, its Durable Objects' `query`) reads each instance's store (`kv`) and outbox (`wrela_outbox`).

- A browser makes passkeys only on a secure context whose host is the relying party's ID: `http://localhost:<port>` is one, and its `__Host-` cookies work there too.
- The router refuses a `POST` whose `Origin` isn't in `WRELA_ORIGINS` (403), and a client past a rate limit (429: studio/release's `RATE_LIMITS`, `WRELA_LIMITS`); the server never sees an address.
- `WRELA_PRINT=1` in the Worker's variables sends the program's printed lines to its log.
- Its tests run on a fake host of many instances that commits, answers, calls and delivers as 0.18.0's host does (`wrela test platform`).
- Sign-ups are closed on the live server (console/server/serve.wrela's `close_sign_ups`, rywible/wrela#352) until Ryan reopens them: reopening is deleting that line.
- `ONLINE` (console/main.wrela) is true; `--set ONLINE=false` builds a console without a server, whose chip opens only the health warning and the legal pages.

## Delivery

A release (`wrela tool release`, studio/release) puts what one release shares with the next at content addresses at each Worker's root, so a deploy that doesn't change a file doesn't change its URL (studio/release/addresses.wrela):

- **Payloads** at `/b/<sha256>.<ext>`: each build's manifest's `wasm` and `wasm_relaxed`, each pipeline's `shader`, the modules' `.lines`, and its streamed files (`files/`, but `files/icons/`, which its page links against its own address).
- **Runtimes** at `/r/<sha256 of the set>/`: a build's `main.js`, `worker.js`, `worklet.js` and their siblings, together, so `import.meta.url` still finds each.
- **Each release's own files** under `releases/<id>/`: its pages, manifests, `net.json`, icons, `catalog.tsv`, `prefetch-<id>.txt`, `release.txt`, `files.json`, and each build's `addresses.json` beside its manifest: `{"game.wasm": "/b/<sha256>.wasm", "files/floor/t1.bin": "/b/<sha256>.bin", ...}`, each payload's path in the build and its URL. Toolchain 0.18.0's browser host reads it; 0.17.0's doesn't, so a release built on 0.17.0 doesn't load.
- `/b/*`, `/r/*` and `/releases/*` are `public, max-age=31536000, immutable`. `files.json` lists the shared files by their root-absolute paths; the deploy keeps each kept release's own files and the shared files they name, each once (studio/deploy).

Ryan sets two things on the `wrela.dev` zone once, in the Cloudflare dashboard:

- **The Compression Rule**: Rules, Overview, Create rule, Compression Rule; match the default content types (they include `application/wasm` and `text/javascript`: developers.cloudflare.com/speed/optimization/content/compression/); under Compression options, Custom: Zstandard, Brotli, Gzip (developers.cloudflare.com/rules/compression-rules/settings/). Once a deploy's log says each WASM module and runtime script "comes back as Zstandard" (and has no `warning:` line for one), set `ZSTD_RULE` to `true` in studio/deploy/main.wrela: from then on a console's or game's WASM or runtime script that doesn't come back with `content-encoding: zstd` (asked with `accept-encoding: zstd, br`) fails the deploy. The rule stays only if zstd's bytes are within 5% of Brotli's (docs/bench.md, "Zstandard against Brotli").
- **Early Hints**: Speed, Settings, Content Optimization, Early Hints on (developers.cloudflare.com/cache/advanced-configuration/early-hints/). Each bootstrap page's `<link rel="preload" as="fetch">` is a `Link: <...>; rel=preload; as=fetch; crossorigin` header for that page's path in `_headers`, which Cloudflare sends ahead of the page as a 103 over HTTP/2 and HTTP/3. Check it with `curl -sv --http2 https://wrela.dev/ -o /dev/null 2>&1 | grep '< HTTP'` (a `103` before the `200`), and in Chrome the WASM's resource timing entry has `initiatorType` `early-hints` (docs/bench.md, "Early Hints").

## Watching and measuring

```bash
wrela trace games/capybara --watch capybara --frames 120 --csv --no-gpu
wrela bisect games/capybara --until "capybara.z > 0" --frames 600
wrela profile games/last-green --size 1920x1080 --workers 4 --compare build/base/last-green --pairs 3
wrela same games/capybara --against build/base/capybara --frames 120   # behaves as the saved build?
wrela profile games/last-green --chrome --compare build/base/last-green --pairs 3   # in headless Chrome
wrela-host --compare-ticks a.ticks b.ticks   # two builds' tick logs: equal, or the first tick that differs
```

- `trace` prints what exports say after each frame, and `bisect` finds the first frame a condition holds (`<`, `<=`, `>`, `>=`, `==`, `!=`, `nan <export>`, `<export>.x`). Both run a ticked program's ticks in lockstep, and both are debug builds unless `--release`.
- `profile --compare <build-dir>` alternates pairs with a saved build and says what's faster or slower. Both builds must come from the same toolchain release.
- `profile --serial` runs each pass alone, so its times add up; in the default mode passes overlap on the GPU, and only the frame's span is the whole. `--serial` doesn't combine with `--compare`: write a report with `--json`, then compare with `--baseline <report.json>`.
- `same` builds the package now (into `<package>/build/same`) and runs it beside a saved build made outside it (`wrela build ... -o`), each with one worker, its own empty storage and the same `--input`. It compares each frame's hash, the last screens (`--within`, 0.5 by default) and the pipelines, and exits 0 when they behave the same, 1 when they differ and 2 on an error. With `--no-gpu` the commands are checked but not run, so the screens aren't compared. Use it to show a refactor or a pin move changed nothing.
- `profile --chrome` measures in headless Chrome: GPU spans and the executor's counters from a saturated run, then CPU frame times, phases and frame intervals from a paced run (two frames in flight), as a player's are. The command checker is off, as in a release build (`--checked` keeps it); `--features none` hides the device's optional features, so a program's fallback paths are measured; `--optimize size|speed` profiles the shipped `wasm-opt` build. Every profile reports the executor's counters a frame: passes, dispatches, draws and draw calls, writes, bind groups and pipelines set.
- `profile --worst n` (5 by default) names the slowest frames: in Chrome by their paced interval (the paced run now asks for timestamps, so each frame's labels are timed too), natively by their call. Each shows its interval, its call, its GPU span, its phases and its labels' GPU ms, largest first; `--json` has them as `"worst"`. A paced run's `results/frames.json` carries each frame's `labels` (`[[name, ms], ...]`) beside its `cpu_ms` and `phases`.
- `wrela-host <build> ... --log out.ticks` writes a ticked program's tick log; `--compare-ticks` compares two builds' logs on the same input (the sim unchanged) without checking their WASM hashes, which `--replay` does.
- `wrela precision <pkg> <module>::<fn> --box lo..hi,lo..hi,...` says, operation by operation, where a half-float twin of a function of a point would be safe: it walks the function's interval as an `f32` rounds and as a half float rounds, over each `--box` (several are a sampled domain), and gives each operation `safe`, `watch` or `unsafe` (by its extra width beside its magnitude, `--tol`, 1e-3 by default; past 65504, or an integer past 2048, by range), and the first that isn't safe. It's advice: a twin written where it says `safe` is proven by measuring it.
- `wrela build --relaxed-simd` (release only) writes `game.relaxed.wasm` beside `game.wasm`: the code only `frame` reaches fuses its vector multiply-adds. Chrome loads it, Safari the standard one; `wrela-host --relaxed` and `wrela profile --relaxed-simd` run it natively. Both modules run the same ticks (`--compare-ticks`).
- `wrela build <pkg> --report data` lists the twenty largest constants by their laid-out size, with each one's share of zero bytes, and the data section's total: a large constant read only at load is better shipped as a file (`std::io::Shipped`).
- `wrela build <pkg> --report consts [--json]` lists each constant the build computes: its value's bytes, the fuel computing it used, this build's seconds on it, and whether it was computed or read from the cache. The cache is `<pkg>/build/consts/<version>`; its key is the constant's code, its fuel, the build's options, the bytes of each file it embeds and the toolchain's version, so another toolchain's entries are never read (0.10.0).
- `wrela build <pkg> --set NAME=VALUE` (repeatable) builds with another value for a `pub const` of the program's `main.wrela`: a number, `true`/`false`, a quoted text, or a variant's name for a constant of a fieldless enum (`--set TIER=Medium` is `Tier::Medium`). Its uses fold with the value (a length it gives, an `if NAME`, a `match NAME`), and the manifest records the sets. `wrela pipelines`, `wrela same` and `std::tool::toolchain`'s `BuildOptions::set` take it; `wrela same` says two builds whose sets differ differ (0.10.0). It points a `pub type` of the program's `main.wrela` at another type too (0.11.0): `wrela build examples/vale --set LOOK=styles::fixtures::Every` builds the vale in that look (`engine::style`).
- `WRELA_FEATURES=none` (or a comma list to keep) hides optional GPU features from a program on the native host, and `wrela test` passes the same mask to Chrome.
- Run profiles with nothing else building or using the GPU. Report GPU span, CPU frame call and Chrome frame rate as the separate things they are.

## Tests

| Attribute                              | Runs                                                                                                                                |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `@test`                                | Code, as the build runs constants. It can't make GPU resources (a texture is `nondet`): test the pure rule in a function of its own |
| `@test(frames: n)`                     | `init`, `n` frames on the CPU host (640×480, 60 a second), then the test with the state                                             |
| `@test(frames: n, gpu: true)`          | The same on the hardware GPU; readbacks are answered before the next frame                                                          |
| `input: SCRIPT`                        | A `std::input::Script` constant's events, by frame                                                                                  |
| `golden: "goldens/x.png", within: 0.5` | The screen after the frames against a PNG; `wrela test --bless` writes it                                                           |
| `pass: "label"` (with a golden)        | That pass's target instead of the screen (floats clamped, depth grey)                                                               |
| `hold: [("files/x.bin", 120)]`         | That path's fetches and storage reads wait until frame 120                                                                          |
| `hosts: both`                          | The frames in Chrome too: the same state hash and frame, or it fails                                                                |
| `@test(ticks: n)`                      | A ticked program's ticks alone, with no frames                                                                                      |
| `@test(tool, read: [...], budget: s)`  | A tool test, on the tool host, with fake sites and programs                                                                         |

- A failed golden under `wrela test` writes what it saw and how far each pixel is under the package's `build/goldens/`.
- A tool test can build, check and drive other packages: `std::tool::toolchain` (`build`, `test`, `check` with the build's pipelines), and `std::tool::drive` (frames, captures, hashes, recorded commands, `Hold` and `Release` of a path).
- A program can read its labelled work's GPU time with `std::gpu::timed(label)`. A test may check that a time came, never how small it is.
- Assert what would fail if the code broke. Before trusting a new test, break the code it guards and watch it fail.

## The workspace's tools

`wrela tool <name> -- <args>` runs a tool declared in `wrela.toml`, with the grants it declares.

| Tool                 | Does                                                                                                      |
| -------------------- | --------------------------------------------------------------------------------------------------------- |
| `lookbook`           | A program's lookbook shots as stills and clips (`--package examples/clearing`, `--stills`, `--view clay`) |
| `motion`, `round`    | The Last Green in motion; a play round in Chrome                                                          |
| `studio`             | The lens on a subject, a floor's map or a game's course                                                   |
| `serve`              | A directory served to a browser                                                                           |
| `audio`, `listening` | A piece's take and measurements; blind listening tests                                                    |
| `reference`          | Replicas of reference meshes, and how far a surface is from one                                           |
| `route`, `keys`      | The capybara's fastest runs; its piano keys page                                                          |
| `release`, `deploy`  | A release, and its deploy (CI does these from `main`)                                                     |
| `bench`              | What the engine's scenes cost on this machine, in Chrome and natively, added to `docs/bench.md` (`--scene herd`, `--compare out/<report>.json` for a report in build/bench); `--lan` for another device |
| `export`             | A floor's ground as a 16-bit PNG and its JSON (`height games/last-green --out <file.png>`), a scene's camera path as JSON (`path ... --out <file.json>`): what the other engines' scenes take |
| `passes`             | A frame's passes and dispatches, in order, as a page |
| `fit`                | A small network trained and its weights written as source (`--net sine --out content/...`); the impostor's ladder run and printed (`--net impostor --rungs ABCD`, the best run written with `--out`); or the learned rejector's truth, dataset, baseline and rung (`--net rejector --frames n [--out ...]`): `engine::neural` |

## Benching another device

`wrela tool bench -- --starts <n> [--scene <name>]...` runs each scene's cold and warm starts alone, `n` of each, and prints each `playable_ms` and their medians (docs/plan-world-class-3.md §C5's three runs). A cold start's salt is new to the machine (the seconds since the epoch, a thousand to each): macOS keeps a shader cache an app, keyed by a shader's source, across Chrome's thrown-away profiles, and the salts 1, 2, 3, ... that the bench used before C5 were served from it, so its cold numbers before 2026-10-29 are starts with that cache warm. Each run's `results/starts.json` (0.10.2) says when each pipeline was built and each command that waited for one: what a start waited for.

`wrela tool bench -- --tier <tier>` builds each scene whose program names a tier (`pub const TIER`, `engine::tier`) at that tier, and `--look <type>` each whose program names a look (`pub type LOOK`, `engine::style`) in that look (`--look styles::fixtures::Every`); a row names them, `vale-walk (Medium, Every)`. A scene whose program names neither is a usage error with the one it lacks.

`wrela tool bench -- --lan [--scene <name>]...` measures the scenes on a device on this machine's network instead of here. It builds each scene, serves it to the LAN, and prints its four pages' URLs (gpu, play, cold, warm, each with `show=1`); a person opens them on the device in that order, each once the one before has finished, and the warm one twice (the tool says when). The device puts each run's results back, and the report names it by its adapter and its browser's user agent, under `docs/bench.md`'s "Devices". The native run is skipped: it would be this machine's.

1. The device and this machine on one network; note the device's OS, its browser and its version.
2. WebGPU and `SharedArrayBuffer` need a secure context, which plain HTTP to another machine's address isn't. On Chrome (Windows, Linux, macOS, Android with a desktop's flags page) start it with `--unsafely-treat-insecure-origin-as-secure=http://<ip>:<port>` (the origin the tool printed) and a fresh profile (`--user-data-dir=<dir>`), so the cold start is cold; Safari, iOS and a phone where flags can't be set need the page over HTTPS: a released build is, at the release's worker (CI deploys `main`), and the same hashes open it there with `show=1`, its numbers typed in as step 4 says; a bench build of a branch has no HTTPS serving in this workspace yet.
3. Open each URL as the tool prints it and let it finish (the page draws its summary when it's done). Run each scene three times; keep the median cold start, as the plan's §B8 says.
4. A device that can't put its results (a tablet, a phone on a served release page) still shows its summary on the page: type its numbers in, `wrela tool bench -- --row "<machine>;<scene>;<p50>;<p95>;<p99>;<over 33>;<playable ms>"`, which adds a line marked "by hand" under "Devices".

## What trips agents up

- **`pub fn` in a program's `main.wrela` is an export.** It takes numbers, bools and vectors (and `frame`'s state), and returns those or structs, tuples and arrays of them. Put shared code in another module; a library package has no `main.wrela`.
- **A dependency's `main.wrela` is an ordinary module** (`pkg::main::x`), but its `pub fn`s are still exports in its own build.
- **Overlapping `mut` arguments are refused** (E0513). Keep the parts a call changes in sibling fields: `frame.draw(mut s.scene, aim)` beside `s.frame`.
- **A place of a non-`Copy` type isn't copied by `let`.** Write `borrow x = place` to read it, `mut x = place` to change it, or `take` to move it. A `.clone()` that is only read is W0010: borrow it instead.
- **A draw behind a runtime `if` is still a pipeline in the build**, and `wrela pipelines` counts it, marked `(under a condition)`. Leave a stage out by the program's structure, or by a constant: what only an `if` on a `false` constant reaches isn't built, and a `match` on a constant that holds a fieldless variant builds only the arm it takes (0.10.0), and an `if Q::X` on an associated constant folds however long its function, outside a debug or a lifted build (0.10.1), so a tier or a style is a `pub const` of an enum that `--set` chooses.
- **An entry point's `const` argument may be an associated constant** (`blur.bind(taps: S::TAPS)` under `S: Taps`): each instantiation's value is a pipeline of its own, its `false` switches' code left out of its shader. The constant's initializer must fold: literals, constants, other associated constants, arithmetic, comparisons, `!`, `&&`, `||` and `if` (E0603 otherwise).
- **A `const` argument is bound in the `draw` or `dispatch` that records it.** Through a helper, `fullscreen(p, k.bind(.., on: Q::ON))`, it's E0603 (the helper's parameter is a value): write the draw, `draw(p, full.bind(z: 0.5), k.bind(.., on: Q::ON), vertices: 3)`.
- **A program's frame switches are a type** (`engine::switches::Switches`, whose defaults are the approved set): `Frame<R, Q>`, and a scene's lit methods take `lt: Lighting<Q>` with `Q: Switches` among their generics. A switch is never a run-time value: a second value is a second build (`--set` a `pub const` that the program's type reads, as the floor's `LEARNED`).
- **`package` is a keyword**, so it can't name a variable. The keywords, reserved ones too, are in `spec/lexical.md`.
- **A tool test builds its fixtures through overrides.** When a package gains a dependency, the tests that write its manifest or list its overrides need it too.
- **A test of code runs as a constant does:** no GPU, no clock, no randomness. One that reaches no `assert` and no panic can't fail (W0011).
- **A request is answered on a later call.** A `Pending` polled in the call that made it is `None` there (W0012): keep it in the state and poll it next frame.
- **Every error says what to do next**, as a help or a fix. `wrela fix` makes the fixes that are the only one.
