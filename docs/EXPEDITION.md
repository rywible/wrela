# The first resident

A small playable discovery/rescue/return loop in the existing valley. This is a
prototype for playtesting, not a finished sanctuary game or final creature art.

## Play

Open `.build/Sanctuary.app` after `scripts/build`. Walk north from the sanctuary's
stone circle. Pale footprints and ice flowers mark three signs along the path.
Press **E**, or click the contextual action, to read a sign. Continue past the old
gate to find a Frostling. Approach within a few metres, face it with a clear view,
and move gently for about four seconds. Sprinting or losing sight reduces trust.
Press **E** when it trusts you to rescue it.

Return south to the sanctuary stones. Inside the circle, **E** prepares the frost
garden and releases the resident. Ice flowers develop over about 18 simulated
seconds. Visit the settled creature for a hint about something farther north.
There is no second creature or playable legendary encounter in this build.

**WASD** walks, **Shift** runs, dragging/arrows look, and **P** pauses. **H** reveals
existing authoring/debug controls, hidden by default. The first habitat has a
fixed location: free building placement and terraforming are future work.

Progress autosaves every five simulated seconds, on successful interactions, and
on normal quit. Reopening resumes the player pose and expedition. No offline
progression is applied. `.sanctuary/saves/expedition.json` is the player's default
save; `.previous.json` retains the last valid primary. A corrupt primary falls back
to the backup with a visible recovery notice. Unknown versions are rejected and
preserved. An unreadable save is not silently reset.

## Author and test

`Authoring/Assets/frostling.json` is an ordinary composed field asset, available as
`stagectl subject frostling`. Its current pose is a collection of procedural body
parts with simple breathing/foot motion and bounded wandering. It is an initial
silhouette and gameplay stand-in, not a finished animal locomotion system.

```
./scripts/gardenctl status
./scripts/gardenctl interact
./scripts/gardenctl saveExpedition
./scripts/gardenctl expeditionSlot my-playtest
./scripts/gardenctl expeditionSlot expedition
./scripts/validate-expedition
```

A slot loads an existing save or starts a fresh expedition only when no primary or
backup exists. Switching slots saves the current one first. Slot selection is local
to the running session; ordinary launch always opens `expedition`. Tests use unique
slots and return to the original one. They never reset the player's expedition.
`status.expedition` records phase, identity, trust, signs, home/creature locations,
habitat progress and current action. Debug camera teleports are for test setup;
`move` exercises ordinary collision. The native UI and protocol interaction both
call the same rules, without special test-only rescue or release bypasses.

Run `swift test`, `scripts/check-boundaries`, and the running-game
`validate-expedition` for state, persistence, boundary and rendering checks.
`validate`, `validate-stage`, `validate-workshop`, and `validate-authoring` cover the
pre-existing renderer/tool contracts. Performance uses short live profiles with
one renderer running; inspect update hitches separately from steady-state work.
