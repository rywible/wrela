# The open work

Goals, not a log: delete a line when it's done. Work branches off `main` and squash-merges back
(CLAUDE.md). Milestone-6's task file, with its decisions, play rounds and lookdev reviews, is
`M6-TASKS.md` on the branch `milestone-6`. The thread "Vision gap review" keeps this list
ranked from its map of the vision against the code (`vision/gap-map.md` in the project's shared
folder), reviewed each day.

## The target: ten minutes of the Last Green

One uninterrupted ten-minute playthrough of the Last Green: three exceptional places, the animated
climber, one polished fight, the place's own sounds, and frames paced at 60 fps on the M4. It is
the quality bar for every scene and system after it, so it comes before breadth (the tower, more
floors). Set 2026-10-11, from an outside audit the owner shared. In brackets, the project thread
that does each part.

- The route: an opening, an enclosed wood, a reveal, water and an encounter. Each place works on
  approach, while a player explores it, and looking back (Lookdev: shot framing).
- Three definitive tree forms, the hero oak, a beech interior and an alder waterside, whose
  construction then serves every instance (Lookdev: tree crowns; Lookdev: trunks and wood).
- The look survives movement: walking, fast turns, gusts, water, a character passing in front, sun
  to shade, and the first second after arriving, not only the settled still (Native resolution and
  clean lines).
- The climber (vision.md: a floor-length cape all round, no arms or legs, a kabuki mask, a
  telekinetic great sword) and one fight against a creature: the sword's moves, a short blink in
  place of a dodge roll, the cape as cloth that meets the ground and the grass, then the darkness
  off it; frame data; a gamepad (Climber and fight spike).
- A creature that notices the player, watches or flees, and goes back to what it was doing, with
  the same senses as the fight's enemy (Creatures that notice you).
- The place's sounds: the climber's hem through grass and over each ground, creatures' steps,
  wind in the leaves, water, a mix that keeps the fight's cues clear, tested in the game (Sound
  on main).
- Frames paced at 60 fps on the M4 over the route, with the fight in an expensive forest and water
  view (Code health and architecture; Speed up the gate on CPU).

## Next, in order

1. Measure what one accepted scene, creature and encounter cost to make, before content is
   multiplied over floors.
2. The tower: the staircase in the void, doors, a floor as a unit the engine loads, and world
   states that change in play. Freeing or preserving a world opens the way up; a preserved world
   can be visited again, a freed one can't (Staircase, doors and floor kit).
3. Creatures at scale: a floor's 4–6 kinds, its elites and its Warden, by agents, at shippable
   quality.
4. Safari and Firefox: the floor, the audio and the sim's bits in all three browsers.
5. A floor's game: rest points, waystones and fast travel, gliding, saves. Climbing waits on the
   owner (the climber has no hands); so do anchors, Keys and towns: their lore and how they play
   aren't written.
6. Shipping a floor: packaging, offline play, hosting, every budget in the gate.
7. Seasons and leaderboards (#46). Then the Titan's Back and the Clockless War (#47).

## The Last Green's floor (what M6 left open)

- The record: finish the retrospective (its draft is in the archived task file) for the owner to
  post on #55, and bring #26 and vision.md up to date.
- Measure again: the glide's pace and memory on the walk (#55 AC7); the walk's frames in Chrome
  and each system's slice (AC8); the bake's time after an edit (AC2); `wrela check` and
  `wrela build` on the floor (AC16).
- The owner's walk and verdicts: the floor's look, the water's look, the sun shafts, the far land
  (AC6, AC9 to AC12, AC14).
