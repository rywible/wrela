# Sanctuary: living storybook world

Design record, September 12, 2026. This records the current user-directed flagship
game and supersedes the rescue/capture product direction in BUILD_PLAN.md. It is
a target specification, not a claim about implemented features. Sanctuary is the
working title. Existing engine ownership and validation rules still apply.

## Experience

A human explores a beautiful world in first person, discovers expressive wildlife,
earns lasting trust, and shapes habitats through nature magic. Creatures respond
to the player and the landscape, inspiring the next exploration or project.
There is no required victory, combat, catastrophe, deadline, or upkeep debt while
the game is closed. The player is alone; wildlife provides companionship.

The player should want to discover one more unfamiliar species, understand its
behavior, visit familiar animals, and see how a place they shaped has changed.
Observation and preserving an existing habitat are worthwhile choices. The whole
world is shapeable; the player need not want to change every beautiful place.

## Decisions from the design conversation

- Human, first-person perspective with physical grounding. Begin with a small
  set of convincing interactions rather than assuming complete hand animation.
- Warm three-dimensional storybook art inspired by the feeling of Winnie the
  Pooh: inviting landscapes, tactile and expressive creature surfaces, strong
  silhouettes, convincing eyes and motion. Create original characters and places.
  Wide emotional range includes tiny/cute, strange, proud, majestic and enormous.
- Quiet cabin opening, then a view that reveals genuinely reachable destinations.
  An inheritance letter, predecessor story, and hostile external force have been
  retired. A familiar creature greeting is a provisional implementation choice.
- All discussed environments belong in the full game: woodland, meadow, creek,
  wetlands, lake, mountains/alpine terrain, desert, rainforest, coast, tidepools,
  and ocean. Some are habitats within larger biomes. Do not substitute a single
  valley for this scope or count unreachable background geometry as exploration.
- Expressive nature magic shapes terrain, water and planting. Home building,
  decorating, paths, bridges and observation places support personal expression.
- Capabilities grow through knowledge, tools/magic and animal cooperation.
  Powerful companions can move boulders; riding and flying expand traversal.
  Basic building must be enjoyable before a rare companion is found.
- Wildlife is noncombat. Animals can refuse, retreat, obstruct or discourage an
  intrusion without attacking the player. Territorial boundaries remain legible.
- Animals work on their own habitats. A dam builder may rebuild a removed dam.
  Player changes have observable, recoverable consequences; displaced animals
  relocate without dying because of a building experiment.
- Day/night, weather, growth and young animals provide change during play. No
  elapsed-real-time deterioration on return. Growth has no old-age death loop.
- Local ground and vegetation respond to physical presence: grass bends as the
  player and creatures pass; soft or muddy ground retains temporary footprints.
  These traces come from actual movement, recover gradually, and use bounded,
  reusable simulation and rendering systems rather than decorative canned trails.
- Grow the world from environmental constraints: landform, drainage, weather,
  moisture and soil should help explain biome and habitat distribution. Research
  uplift/tectonic-inspired mountain generation as one possible source; a literal
  planet simulation is not required. Integrate through reusable field systems,
  with reachable landmarks and player edits preserved across content revisions.
- Rare/endangered species provide optional discovery and recovery aspirations.
  Tracks, calls, nests, silhouettes and journal observations hint at remaining
  discoveries. Avoid blind rare-spawn waiting and urgency-based rescue chores.
- Friendship can emerge through different combinations of patient presence,
  play, preferences and help. Trust persists; it is not a meter that decays when
  the player chooses another activity. A familiar animal may approach for company
  or play, not just to issue requests.
- Creatures are emotionally expressive animals with intelligence slightly beyond
  a dog's. Species tendencies and individual preferences are authored, while
  actual shared experiences and relationships persist. Temperaments span wild,
  wary, curious and proud; only some individuals are willing to be companions.
- Player-to-creature communication starts with typing. Creature-to-player
  communication starts fully indirect: gaze, posture, calls, movement, waiting,
  and interactions with real objects. Understanding does not imply obedience.
  Do not silently turn this into speaking NPCs, needs subtitles or quest markers.
- On-device language models are an authorized research direction, not an assumed
  proven solution. All creatures should feel alive without a model call per animal
  per frame. A shared, bounded interpreter may help with occasional interactions.
- Development is autonomous, including agent playtesting. Creature interaction,
  shaping and exploration are concurrent priorities. Use up to eight dynamically
  allocated workers; do not reserve fixed model slots or manufacture busywork.

## Playable loops

Explore a visible destination; notice a real clue; observe an unfamiliar creature;
try an interaction; learn through its response; shape a place or earn cooperation;
return to find an ecological or social consequence that suggests another project.

Building also works in the opposite direction: create an appealing wetland first,
then discover which animals arrive. Multiple layouts can support flourishing.
There is no universal maximum-density habitat recipe.

An established session offers immediate exploration, building and interaction
while longer changes give reasons to revisit places. Simulation should not demand
passive waiting or daily maintenance to keep the experience rewarding.

## Grounded creature intelligence

Production simulation owns identity, perception, memory, trust, preferences,
abilities, movement and consequences. A language model receives only relevant
facts and proposes typed, bounded intents. The game validates target identity,
range, visibility, willingness, ability and world revision before applying them.
Free-form model text must not invent ecological facts or mutate world state.

Requests run asynchronously outside the fixed-step loop. Responses can be stale,
unavailable, refused or invalid; ordinary behavior continues. Memory is stored by
the game from actual events, not accepted as invented model recollection. Record
accepted external decisions for deterministic replay rather than re-querying a
probabilistic model during a replay. Never describe a phrase parser as an LLM.

Prototype Apple Foundation Models first and assess Core ML custom models only
when a concrete gap warrants the extra engineering. Verify installed OS/model
availability. Do not assume Neural Engine-only execution or zero rendering cost.
Measure latency, memory, ordinary live frame spikes, behavior consistency and
whether the interaction feels more alive than authored behavior alone.

## Quality and completion

The complete game includes all environments above as reachable, distinct places
with characteristic vegetation, water/ground treatment, wildlife or discovery
evidence, and routes that work with walking/riding/flying as appropriate. Scope
does not imply a literal planet-sized simulation or underwater gameplay; choose
a coherent finite world and document its boundaries and ocean traversal policy.

The game must connect native first-person exploration, typed creature interaction,
lasting individual relationships, nature magic, ecological responses, companion
assistance, riding/flying, home construction, discovery and versioned saving.
Requirements are complete only when accessible through ordinary game controls,
with actual native play and images supporting claims. Command-only demonstrations,
data catalogs, empty biome labels and passing builds are incomplete work.

Keep native 1920x1080 at 60 Hz on the M4 MacBook Air as the target. Measure live
clouds, wind, ecology, editing and model activity; distinguish steady cost from
update spikes. Use short measurements, preserving p95/max and provenance.

Visual assets remain Swift fields/procedural sources. Use Soundstage studies and
the real renderer for art acceptance. Generated illustrations are not renderer
evidence. Preserve existing player saves and source work. Do not silently accept
visual baselines or call provisional art final.

## Autonomous choices and research

The agent may choose reversible controls, implementations, content names, initial
world scale and species counts, documenting choices as provisional where taste or
playtesting remains uncertain. Do not repeatedly seek approval for routine work.
No paid external API, asset purchase, publishing or messaging is implied by use of
the user's existing Codex subscription.

Research is bounded by a concrete question, prototype, evidence and integration
decision. Useful parallel investigations include expressive communication,
on-device interpretation, terrain/water editing, biome streaming/detail, and
tactile creature rendering. Research does not replace implementing playable loops.

Minimap, exact progression pacing, detailed construction controls, world extent,
ocean traversal and initial content counts remain implementation choices. Favor
indirect discovery over an opening objective tracker inherited from the old pitch.



## Raised engine and quality bar

User direction, September 12 continuation: the game and engine must aim for world-class,
AAA quality and push what fields, simulation, terrain and biome systems can do. Only
Astra authors 3D models and animation. This is an acceptance ambition, not evidence
that any current system meets it. Prioritize coherent editable source fields, rich
living habitats, distinct procedural vegetation, broad connected traversal and
measured native presentation. Research must resolve concrete technical risks through
experiments, reusable production systems and retained evidence; terminology, world
size, feature counts or a passing build cannot stand in for the result.
