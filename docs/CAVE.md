# The Last Survey

Run `./scripts/run-cave`. Follow the chalk marks, retrieve the survey beacon deep
in the cave, and return to the entrance. WASD walks, Shift runs, dragging/arrows
look, F toggles the flashlight, E recovers the beacon, and P pauses.
The creature responds to the beam only when the field query confirms visibility.
Progress saves under `.cave/saves/`; the default slot is `expedition`.

This is a short enclosed-world stress test for the shared engine, with deliberately
bounded encounters and prototype artwork. It does not replace Sanctuary.

## Author the same content

Open Soundstage and select **The Last Survey** in the Object section, or:

```sh
./scripts/stagectl project cave
./scripts/stagectl subject watcher
./scripts/stagectl rig flashlight
./scripts/stagectl view quarter
./scripts/stagectl shape --set height 1.08 --set mask 0.95
./scripts/stagectl creature --mode lunge --seconds 0.3
./scripts/stagectl motion --set sway 0.8
./scripts/stagectl creature --mode behavior --scenario beam --seconds 1.2
```

The Watcher's anatomy, animation clips and beam/dark/occluded/return scenarios
come from CaveProject, not from special cases in the editor. Save studies before
experimenting; publish with `assetSave` when the recipe is ready.

```sh
./scripts/stagectl subject cave
./scripts/stagectl rig flashlight
./scripts/stagectl view passage
./scripts/stagectl view junction
./scripts/stagectl view chamber
./scripts/stagectl shape --set width 1.1
./scripts/stagectl assetSave
./scripts/gamectl --project cave reloadAssets
```

Interior views place the camera and flashlight inside the generated field.
Dragging changes the look direction; WASD moves the inspection camera while
playing. Standard views return to the object orbit. Interior camera poses persist
in studies. The game reloads both cave geometry/collision and the Watcher
atomically. If an edited wall blocks the player, reload is rejected; move to the
entrance before trying again.

`gamectl --project cave` offers status, pause, step, camera, move, query, capture,
flashlight, reloadAssets and named expedition slots. `move` uses collision;
`camera` is a debug teleport and must stay inside the cave. `query` reports the
solid field value and visibility from the player. `scripts/validate-cave` checks
the real running game and restores the original slot afterward.
