# M6: the Last Green's floor — the task file

The milestone's tasks while it's open (#54 scope, #55 criteria), and the play rounds' logs (AC5).
Deleted when the milestone closes.

## Order of work

1. Language, std, hosts and tools (AC15): constants cached by their inputs; a constant's fuel set
   where it's declared, and constants on several threads; files a build ships beside the program
   (the bake's tiles, fetched); `Text` in a GPU constant; a command over 1 MiB; the allocator's
   lock; the clock; the LOD trap; `wrela run`'s page reload.
2. The engine's vocabulary and tools: `engine::place` (the map's types, tile positions),
   `engine::grow` (the history's general parts), `engine::interest` (the check).
3. The floor (`examples/last-green`): the map, the history, the bake (a cached constant, per
   tile), the check's tests (AC1–AC3).
4. Playing it (AC4): the stand-in, collision on one walkable mask, the third-person camera,
   walks as replays, tile positions.
5. Streaming and caches that follow the player (AC7, AC8): tiles fetched, cooked and dropped
   round the player; the far layer; the wildwood; the forest within its slice.
6. Water (AC9): the mere, the stream, the waterfall, springs; three looks; one surface.
7. Lookdev (AC10–AC12): ground cover, one metering rule, sun shafts (candidate), cooked
   materials, leafless kinds, trunks that differ, the tower and the ring, the far land, trails.
8. The studio (AC5, AC13): the play harness, the map lens, provenance, the lookbook.
9. Play rounds in Chrome (AC5), until the last round finds no blocker.
10. The record (AC16–AC18).

## Status

| AC | State | Notes |
|---|---|---|
| AC1 | tests pass | map in source with engine types; anatomy, area, keepers (2), reach: suite/last_green.rs |
| AC2 | mostly | cached (test), 0.65 MB/km² (test), preview vs bake (test), head room (tests.wrela); same bytes both hosts to write; preview ≤ 3 s and bake ≤ 2 min to measure (bake 1:50 seen) |
| AC3 | partly | dead, relations, openings, lost, critical path pass (long test); calibration and landmark tags need the renderer |
| AC4 | started | collision into each thing, trunks as drawn (tests.wrela); camera near plane, no trap, replays both hosts, positions to do |
| AC5 | started | the floor's walk (P) and the report every 2 s; both-hosts test, map with the check, rounds to do |
| AC7 | started | tiles, fields and far layer stream; wildwood, egg, throttled run, memory, time to play to do |
| AC8 | to do | vegetation 5.8 ms in the beech (target 4.0); load spikes |
| AC9 | mostly | mere, streams, fall, springs; one surface, where it is, WGSL bounds (tests); three looks (F6); slice to measure |
| AC10 | started | lookbook command (stills); trails drawn; trail contrast test, clips in Chrome, flicker to do |
| AC11 | started | no grass under closed canopy (test); one metering rule; settle test, sun shafts to do |
| AC12 | started | cooked bark and stone (WGSL test), leafless kinds; trunks differ test, tower relations, far land to do |
| AC13 | to do | map lens, provenance |
| AC15 | done | 5817067, b08cb06 |
| AC16–18 | to do | speed, record |

## Play rounds (AC5)

None yet.
