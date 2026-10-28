# Measuring on another device

*How a later plan adds a device's row to `docs/bench.md` (docs/plan-world-class-3.md §B6). The
MacBook Air M4 is the only device plan three measures on (D37); no other device is measured
here.*

A device's row is the vale's walk (`vale-walk`) measured in that device's browser, sustained as
plan three's bench measures the M4 (§C3): the same scene, the same build, the same four pages.

## The procedure

1. Put the device and this machine on one network. Note the device's name, its OS and version,
   its browser and version.
2. On this machine, with nothing else building or using the GPU:

   ```bash
   wrela tool bench -- --scene vale-walk --lan
   ```

   The tool builds the vale into `build/bench`, serves it to the network, and prints the four
   pages' URLs (`gpu`, `play`, `cold`, `warm`), each with `show=1`. It skips the native run:
   that would be this machine's.
3. Give the browser a secure context, which WebGPU and `SharedArrayBuffer` need:
   - Chrome (Windows, Linux, macOS; Android with a desktop's flags page): start it with
     `--unsafely-treat-insecure-origin-as-secure=http://<ip>:<port>` (the origin the tool printed)
     and a fresh profile (`--user-data-dir=<dir>`), so the cold start is cold.
   - Safari, iOS, or a phone where flags can't be set: open the release's HTTPS page (CI deploys
     `main`) with the same hashes and `show=1`, and type its numbers in as step 6 says. A bench
     build of a branch has no HTTPS serving in this workspace yet.
4. Open the pages in order (`gpu`, `play`, `cold`, `warm`), each once the one before has drawn its
   summary; the warm page twice, when the tool says.
5. Each page puts its results back to the tool, which reads the device's browser version from
   the page's `device.json` and adds the row under `docs/bench.md`'s "Devices", named by its
   adapter and user agent.
6. A device that can't put its results (a tablet, a phone on a served release page) still draws
   its summary on the page. Type its numbers in:

   ```bash
   wrela tool bench -- --row "<machine>;vale-walk;<p50>;<p95>;<p99>;<over 33>;<playable ms>"
   ```

   The row is marked "by hand" under "Devices".
7. Run each page three times, and keep the median cold start.

## What a row records

The device's name, its browser and version (`device.json`), the GPU span's p50, p95 and p99, the
frame intervals' p50, p95 and p99, the frames over 33 ms, the cold and warm playable times, and
the date and the build's commit. A row typed by hand says so.
