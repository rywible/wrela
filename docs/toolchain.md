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
| `wrela pipelines <pkg>`                                                   | Each GPU entry point, its instantiations and where CPU code reaches it; a site under a runtime condition is marked `(under a condition)`                                                          |
| `wrela refactor <pkg> rename\|move\|add-param\|change-mode ... --dry-run` | A checked edit across the package, as a diff first                                                                                                                                                |
| `wrela refactor <pkg> extract-const\|inline\|extract-fn ... --dry-run`    | Extract a constant (`file:line:col NAME`) or lines into a function (`file:first-last name`), or inline a function; each refuses where what the code computes would change                         |
| `wrela fix <pkg> [--dry-run]`                                             | The compiler's suggested fixes, applied (`--dry-run` shows the diff)                                                                                                                              |
| `wrela migrate <pkg> [--dry-run]`                                         | The migrations' fixes alone: run it on each member after moving the pin                                                                                                                           |

`query` answers within one package: a caller in another package isn't listed. Check what a `refactor` misses there too. Fall back to `rg` or `git grep`.

## Running a program

```bash
wrela build games/capybara -o build/cand/capybara
~/.wrela/toolchains/0.8.0/bin/wrela-host build/cand/capybara --frames 600 --size 640x360 \
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
| `bench`              | What the engine's scenes cost on this machine, in Chrome and natively, added to `docs/bench.md` (`--scene herd`, `--compare out/<report>.json` for a report in build/bench) |

## What trips agents up

- **`pub fn` in a program's `main.wrela` is an export.** It takes numbers, bools and vectors (and `frame`'s state), and returns those or structs, tuples and arrays of them. Put shared code in another module; a library package has no `main.wrela`.
- **A dependency's `main.wrela` is an ordinary module** (`pkg::main::x`), but its `pub fn`s are still exports in its own build.
- **Overlapping `mut` arguments are refused** (E0513). Keep the parts a call changes in sibling fields: `frame.draw(mut s.scene, aim)` beside `s.frame`.
- **A place of a non-`Copy` type isn't copied by `let`.** Write `borrow x = place` to read it, `mut x = place` to change it, or `take` to move it. A `.clone()` that is only read is W0010: borrow it instead.
- **A draw behind a runtime `if` is still a pipeline in the build**, and `wrela pipelines` counts it, marked `(under a condition)`. Leave a stage out by the program's structure.
- **`package` is a keyword**, so it can't name a variable. The keywords, reserved ones too, are in `spec/lexical.md`.
- **A tool test builds its fixtures through overrides.** When a package gains a dependency, the tests that write its manifest or list its overrides need it too.
- **A test of code runs as a constant does:** no GPU, no clock, no randomness. One that reaches no `assert` and no panic can't fail (W0011).
- **A request is answered on a later call.** A `Pending` polled in the call that made it is `None` there (W0012): keep it in the state and poll it next frame.
- **Every error says what to do next**, as a help or a fix. `wrela fix` makes the fixes that are the only one.
