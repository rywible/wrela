# Capybara: design

*The game's brief: what Ryan decided, what is proposed and what is open. Working title "capybara" until the game, the capybara and the seagull have names. Started 2026-10-21 from Ryan's brief and two rounds of questions; reviewed by Codex (GPT, via the Codex CLI) the same day, and its findings folded in.*

Ryan owns the goals, the lore and the art direction. A line marked **proposed** is an agent's suggestion that Ryan has not confirmed. A section marked **contract** is an implementation decision that code and tests hold to; the numbers it names live in the code (`feel.wrela`), not here. Everything else Ryan decided.

## The premise

A capybara wants a snack. A seagull keeps taking it. Each level ends with the seagull taking the snack away again: "your snack is in another castle".

The game is set in a zoo, as a day out. The easter eggs are cute, small and kind.

## The pillars

1. **Simple and functional, as Super Mario Bros. is,** with a few modern additions: the slide, swimming, and a hidden ride.
2. **Perfectly speedrunnable.** The simulation is deterministic, every run is a replay, and a run's time is a count of ticks.
3. **Easy to finish, hard to master.**
4. **A production game:** it ships on the console, with a cover and a release, when it is good. There is no date.

## Shape

- **2.5D:** a 3D world on a fixed side view. Play is in one plane.
- **The ground is curves,** not tiles: chains of points with rounded corners, so slopes are continuous.
- **One level first.** The number of levels and worlds is open until that level plays well. **Proposed:** the zoo's areas as the worlds (entrance and gardens, savanna, reptile house, aquarium, aviary, rainforest, a hot-spring area), with the food court as the seagull's last "castle".

## Movement

- **Run and jump.** Momentum (as in Mario: acceleration, skids, a run button) or snappy (as in Celeste) is open. The lab has both as presets, and Ryan picks by playing.
- **The slide.** Hold down while landing and the capybara slides. Its speed along the ground is its landing velocity projected onto the ground's tangent: land tangent to a downslope and it keeps all its speed, land flat-footed on a steep slope and it loses most of it. While it slides, gravity pulls it along the slope and flat ground slowly takes its speed. It can jump out of a slide and keeps the speed. Holding down on a slope also starts a slide; on flat ground it is a crouch.
- **Crests.** The capybara leaves the ground where the ground falls away faster than gravity would pull it down, so a fast run over a crest launches it along the tangent.
- **Swimming, in water sections.** Tap to stroke, in the direction held. There is no breath limit (real capybaras hold their breath for minutes). Swimming up through the surface is a dolphin leap that keeps the speed (and flips), so a route can chain water, air and slide.
- **The camera** scrolls both ways and looks further ahead the faster the capybara goes. On the ground it moves up or down only when the capybara lands at a new height; while the capybara swims, leaps or falls past a band round the view's middle, it follows. **Proposed.**

### Ground and contact (contract)

- **The ground is exact lines and circular arcs.** A level authors chains of corner points, each with a rounding radius (0 for a sharp corner). The build turns each chain into line pieces and the arcs that round its corners. The simulation collides with these pieces exactly; the view draws them sampled. So there is no polyline whose segment density could change a contact, a tangent or a launch.
- **Closed chains are solid. Open chains are one-way ledges:** they hold the capybara only when its middle is on the ledge's upper side at the tick's start and it moves into the ledge.
- **The capybara's collider is a circle.** A contact's normal points from the nearest point of the ground to the circle's middle.
- **Ground or wall.** A contact whose normal is within the steepest walkable angle of straight up is ground; any other is wall or ceiling, which removes the velocity's component into it and nothing else.
- **On the ground, the capybara moves along its chain.** It is on one piece, at a distance along it, with a signed speed along the tangent. Each tick it advances by its speed, piece by piece, with no collision search for its own chain. Other chains still collide with it as walls and ceilings.
- **Leaving the ground** is decided from the pieces, with no positional tolerance, so it doesn't depend on the tick rate:
  - On a convex arc, the circle's middle moves on a circle of radius ρ (the arc's radius plus the collider's). It leaves when v²/ρ > g cos θ, where θ is the slope, with its velocity along the tangent.
  - At a sharp convex corner, and at the end of an open chain, it leaves at once, as off a ledge.
  - Where the contact's normal stops being walkable (up a quarter-pipe, or down a slope too steep to stand on), it leaves with its velocity.
- **In the air, the circle sweeps:** each tick is cut into substeps of at most half the collider's radius. A substep is short enough that the circle can't pass through a piece, so its contacts are the pieces it overlaps at the substep's end. They are resolved nearest first, at most four, deterministically by distance and then by the level's order: each pushes the circle out along its normal. A walkable contact the circle moves into is a landing; any other removes the velocity's component into it. If the circle still overlaps a piece after four, the substep's motion is undone and the velocity zeroed, so the circle never ends a tick inside the ground.
- **A corner point is never ground.** A circle that touches only a sharp corner (its middle outside both pieces' sides) is in the air and slides off it, as a ball off an edge.
- **Contact changes use one rule.** Landing, and passing a concave corner on the ground, set the speed along the ground to `dot(velocity before the contact, the new tangent)`. Its sign is the direction of travel along the tangent, so a backward landing slides backward, and no velocity is normalized, so a landing at almost no speed has almost no speed. If the new piece isn't walkable, the capybara leaves the ground with that velocity.

### Movement states (contract)

| State | Entered when | While in it | Left when |
|---|---|---|---|
| Walk | landing without down held; leaving a slide or a crouch | input accelerates the speed along the ground up to the walk or run speed; speed above it decays to it; no input brakes it to rest; slopes pull it a little | the ground falls away (Air); a jump (Air); down held (Slide or Crouch) |
| Crouch | landing with down held, or down held on the ground, below the slide's least speed, on ground flatter than the slide's least slope | it stays; the collider doesn't change in the lab | down released (Walk); a jump (Air); the ground falling away (Air); the slope or speed reaching a slide's (Slide) |
| Slide | landing with down held at or above the slide's least speed; down held on the ground at or above the slide's least speed or least slope | gravity's component along the tangent changes the speed (an uphill slide slows and runs back down); a small friction takes speed towards rest; input does not steer it; the speed is capped | down released (Walk, keeping the speed); a jump (Air); the ground falling away (Air); below the stopping speed on ground flatter than the least slope (Crouch) |
| Air | a jump; the ground falling away; leaving water | gravity (less while a jump is held and the capybara rises); air control; down held tucks (the intent to slide on landing) | landing (Walk, Crouch or Slide); entering water (Swim) |
| Swim | the capybara's middle goes below a water surface | below, under Swimming | leaving the water (Air), below |

- **A jump** adds a fixed world-up speed (more at a run) to the velocity along the ground, so a jump out of a slide keeps the slide's speed. A jump counts a short time after the ground falls away (coyote time) and a press counts a short time before landing (the buffer).

### Swimming (contract)

- **Water is a rectangle with a flat surface,** set inside solid ground that makes its sides and bottom. The capybara swims while its middle is inside the rectangle. Leaving through a side or the bottom (where a level leaves one open) puts it in the air with its velocity.
- **In water:** linear drag slows the velocity; buoyancy lifts the capybara towards floating with its head out, a little below the surface.
- **A stroke** is a jump press while under water. It adds a fixed speed along the direction held, as a unit vector (a diagonal is 1/√2 on each axis): eight directions, opposite directions cancel, and no direction means up. Strokes have a short cooldown; one press during it is kept, and strokes on the tick the cooldown ends, in the direction held then. A stroke can't raise the speed above the swimming cap, and doesn't lower a faster speed (from a dive).
- **Holding a direction without stroking** paddles slowly.
- **Floating at the surface** (the middle within a small band under it), a jump press is not a stroke: it jumps out of the water at a fixed upward speed, whatever the direction held. So the surface can't be used for repeated boosts: every exit from it has the same speed, and the next waits for gravity to bring the capybara back.
- **Entering water** keeps the velocity; the drag then slows it.
- **Leaving the water:** a swim up through the surface (the middle crossing it upward at the leap's least speed or more) is the dolphin leap. It leaves with its velocity, and at least the leap's own upward speed, which clears a wall well above the water. It rises on a jump's light gravity to its top without jump held, and the capybara flips once in the air. Slower, it stops at the surface and floats. The hop from floating leaves at its own speed and isn't a leap. (Changed after Ryan's first play, 2026-10-21: he couldn't find the leap, which then needed a fast enough swim and jump held.)

## Rules

- **No lives.** Checkpoints and an instant restart.
- **The yuzu:** a yuzu on the capybara's head is the power-up that takes one hit, as capybaras in Japanese hot springs wear them.
- **No coins.** Each level hides a few snacks to find; finding them all is the 100% category.
- **Enemies, proposed:** pests and grumps, not predators. The seagull's crew (pigeons and smaller gulls) that swoop and steal; geese as the basic walking enemy; caimans in the water, whose snouts can be bounced on.
- **Komodo dragons** look like the zoo's fiercest animals and are its sweetest. They are stomped on like enemies, but on one level, one that is approached gives the capybara a ride to the level's end.
- **Easter eggs** can be speedrun tricks, but not all of them.

## Speedrunning

- **Categories:** any% and 100% for the whole game, and a time for each level.
- **Ghosts.** A ghost is a replay's inputs, run beside the player and drawn as an outline. For a level: one ghost, chosen in the pause menu (the player's best by default; the world record and friends as options; the route search's ghost after the level is beaten). For a whole-game run: split times at each level instead of a ghost. **Proposed.**
- **Offline play** needs no account. A run made offline can be uploaded later: the replay is its proof.

### Input (contract)

- **The sim ticks 120 times a second** on the engine's tick driver (`engine::run`), `@deterministic`, with the host's input events as each tick's records (language.md §16.2). The frame rate changes only what is drawn.
- **A tick's input is a pad:** the directions, jump and run held at the tick's end, and whether jump was pressed during the tick. A press and its release within one tick still count as a press, and two presses within one tick (8 ms) count as one. The sim turns records into pads itself, so the host's tick log replays it.
- **Opposite directions cancel.** A stick's axis counts as a direction at half its travel or more, on each axis apart, so the stick gives the same eight directions as the keys; the sim sees no analog values and a replay is bits. Keys and sticks held together combine (either holds a direction). Key repeats are ignored.
- **A replay** is the pads of the level's ticks, run-length coded, with the build that made it (sim and content), the level, the preset, the tick rate and the first state's hash. The first state is the level's start, a constant of the build, so an offline run needs nothing from the server to start, and the server rebuilds it from the build to check it. Tests play the same records at different frame rates and check the same state hashes, and a tick log made in Chrome must replay on the native host (`wrela-host --replay`). Safari and Firefox get the same check when the project first tests them (vision.md: Chrome only so far).
- **No randomness in play** for now. If play ever needs it, it is a seeded stream in the sim, apart from the presentation's.

### Timing (contract)

- **A level's time** is a whole number of ticks: from the first tick the player has control (counted as 1) to the tick on which the capybara first reaches the goal, inclusive. No float clock. Every attempt starts from the level's start state, with the chosen preset.
- **A death** returns the capybara to its checkpoint; the clock keeps running. **A restart** is a new attempt: the clock and the attempt's found snacks reset. The saved game keeps best times and found snacks apart from the attempt, so a 100% run counts only what its attempt found.
- **Pause** is a recorded input; the level doesn't tick and the clock doesn't run while paused, so a replay's pads are only the level's ticks. The replay lists its pauses, each as the level tick it came after and how long it lasted in real time. A live stream sends a pause and a resume as chunks of their own, and its deadline doesn't run between them, up to a limit. Resuming has a short fixed delay. **Proposed.**
- **Returning to the console** ends the attempt; it isn't a run.
- **A whole-game run** starts with a new game and plays the levels in order in one sitting; its time is the sum of its levels' times, restarts and deaths included, menus and pauses not. **Proposed.**
- Ticks the host drops when it falls behind, or while the page is hidden, are not run, so they never count.

### Releases and replays

- **Physics is locked at release.** In practice a board is tied to one build of the sim and its content: a replay names that build, and the server verifies it on that build. The verifier for each board (the game's build and the host that runs it) is kept for as long as the board is. A fix that changes play starts a new board version; old replays stay valid on their own board. Before release, boards can be reset.

## Accounts and leaderboards

- **Accounts belong to the console** (`platform/`), so every wrela game can use them. Today `platform/` has only local saves and sessions: accounts, the server and the verification service are new work, with toolchain work for the server's crypto.
- **Passkeys only, our own server.**
  - Sign-in and sign-up happen on one origin, the console's. The relying party ID is `wrela.dev`; the server accepts only the console's origin and no iframes. Each challenge belongs to one ceremony, is used once and expires. Each credential belongs to one account, and an assertion is accepted only for its credential's account. The server checks every assertion as WebAuthn requires: the challenge, the origin, the relying party ID's hash, user verification, the signature (with the platform's crypto through std, never crypto written here) and the counter.
  - The account's session stays on that one host, in a `__Host-` cookie (HttpOnly, Secure, SameSite), with a session table so sessions can be revoked. Every request that changes something checks its Origin and a CSRF token.
  - A game never sees the session. Through the console, it gets a short-lived token for one account and one game's origin, which can only submit runs.
  - **Recovery:** more than one passkey per account, and recovery codes at sign-up (high entropy, stored hashed, single-use, rate-limited). A recovery code signs in on its own, without a passkey: it is used up, a new passkey must be registered at once, and every other session ends. Adding or removing a passkey otherwise needs a fresh passkey sign-in. An account whose passkeys and codes are all lost is lost.
  - If too many players stop at sign-up, an SSO provider (probably Discord) can be added, but linking accounts is a takeover path of its own and needs its own design first.
- **What anti-cheat can and can't do.** Each layer is evidence, not proof, except the first, which proves only that a run obeys the rules.
  1. **Re-simulation.** The server issues an attempt ID and the starting state, re-runs the replay on its board's build, and computes the time and the category's completion itself. A run whose outcome the rules can't produce is rejected. It does not show that a human played: legal inputs made by a tool pass.
  2. **Live runs.** Online, the pads stream to the server in numbered chunks, in tick order, and the server keeps each chunk as it arrives, unchanged. A chunk later than a deadline, a buffer past a limit, or more than a few pauses makes the run uploaded rather than live. This makes slowing a run down or editing it after it is played harder; it doesn't prove either didn't happen, and a precomputed or spliced stream played at real speed still passes.
  3. **Near the best known time.** A run within a few ticks of the route search's best known time is flagged for review. The search's time is not a lower bound, so a faster human run may only show the search's weakness.
  4. **A humanness score** on input timing flags runs; it never rejects them.
  5. **Video** for the top runs of each board: more evidence, not a guarantee.

  Build 1 and 2 with the leaderboards, 3 when the route search exists, 4 and 5 only if they are needed. Uploads are bounded (ticks per run, bytes, runs per account per hour), and verification runs in a queue with limits, since a replay is work an attacker chooses.
- **One board.** Live and uploaded runs are ranked together; the server records which a run was, and a run near the top must have been live or come with a video.
- **A separate tool-assisted board,** which can include the route search's results.

## Look and sound

- **Cel style.**
- The cast is the biggest risk: agent-built creatures have been the weakest part of the project so far. A chunky, stylized capybara is easier than a realistic one.
- **Music is open.** The engine has only a piano; a cheerful zoo game probably wants more instruments.

## The agent loop

The goal is that an agent can play the game, see it and check it without Ryan. Checks show behaviour, not good feel: Ryan's play of the lab picks the preset and judges the feel, and an agent tunes as far as measurement goes before that.

- **The sim steps on a pad** (above). Tests, proofs and searches drive the sim with sequences of pads; the game's ticks make the same pads from the host's events.
- **Proofs.** Each intended jump or route is a recorded run (pads and how many ticks each is held) that must reach its goal. A physics change that breaks one fails the suite.
- **Feel metrics** in metres and ticks, each with an accepted range for each preset: jump heights, gap reach, slide exit speed by landing angle, time to top speed. Edge cases get bounded regressions: crests of different radii at many speeds, corners, ledges, the water's surface.
- **Traces and stills:** the game exports the capybara's state for `wrela trace`, and `wrela-host <build> --frames n --input script.json --png out.png` draws any frame. The toolchain's `trace` and `bisect` don't run a program's ticker yet, so until they do, traces come from tests that step the sim. The lab's view has a GPU frame test on the hardware GPU; its frame time, and the sim's time per tick, are profiled apart from the checks, against vision.md's budgets.
- **The camera** has tests too: it keeps the capybara in view through the lab's pool and over its crests.
- **Route search** (next): a bounded search (ticks, states and time) over the sim's states, which treats two states as one only when they are equal in everything that affects the future (a hash match, then a comparison), and finds the best known time per segment, from a segment's defined entry state, as a ghost. Proving a jump possible is a replay; proving one impossible needs an exhaustive bounded search and is harder.

## Plan

1. The movement lab: the sim (run, jump, slide, swim) to the contracts above, a lab course with ramps, crests, gaps, ledges and a pool, a blockout 2.5D view, the proofs and the feel metrics. Then Ryan plays it and picks a preset.
2. One vertical-slice level with a blockout capybara, a goose and a slide curve.
3. The capybara in cel style, and Ryan's verdict on feel.
4. The replay format is fixed in step 1; accounts and leaderboards after the slice.

## Open questions

- The names: the game, the capybara, the seagull.
- How many levels and worlds.
- Momentum or snappy controls.
- Music.
