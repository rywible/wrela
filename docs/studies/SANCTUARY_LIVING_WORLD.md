# Sanctuary living-world integration study

September 12, 2026. Source integration study, **not visual acceptance**.

## World decision

The user rejected a 1.52 km proposal and requested a world larger than the original
9 km geography. The implementation now uses a finite 32 × 32 km extent in metres.
Walking and riding follow the continuous ground source; flying follows ground or
water clearance. The ocean is explored by flight; swimming and underwater gameplay
are not implemented. No visible location is intentionally reserved as an unreachable
background. This does not mean every region has had a real native route playthrough.

The procedural source defines eleven named environments and regional intermediate
features. A 3 × 3 neighborhood of 512 m chunks supplies local geometry and collision;
a lower-resolution complete-world mesh supplies distant landforms. Camera range is
32 km. Standard forward depth has limited precision at that distance. Distant terrain
is lowered four metres below detail; seams, transitions, horizon silhouette and water
edges still need actual-image inspection. Woodland/rainforest density is provisional.

Terrain edits refine affected base cells to one-metre sampling and match neighboring
subdivision edges, including chunk boundaries. The initial cabin region uses a fixed
one-metre grid aligned with the regional grid. Collision evaluates the continuous
source; triangle-interior error remains a representation approximation. Streaming and
mesh uploads are synchronous; crossing/edit hitches must be measured separately from
steady frames and must not be excluded from performance claims.

## Connected production behavior

The ordinary game now exposes indirect typed wildlife requests and an action picker
for nature, building and companion travel. The authored vocabulary is hello, follow,
wait, come and play, with small polite variants. This is a phrase parser, not an LLM.
Unknown and oversized requests reject without a partial relationship/save mutation.
The optional Foundation Models adapter is wired behind an explicit experimental
launch flag and disabled by default. Accepted external decisions are game-owned,
persisted and replayed without a model. No generation has been tested; see
SANCTUARY_INTERPRETER.md for availability evidence and the unmeasured experiment.

A deterministic roster spans all eleven environments, with persistent individual
preferences, trust and actual encounter records. Nearby animals run the production
brain; distant updates are coarse and deterministic. Sustained follow uses bounded
traversable hops and moving home anchors. This still needs broad route/navigation and
species-specific motion review. The new silhouettes are provisional Swift fields;
several species currently share Frostling-derived locomotion conventions.

Animal work creates persistent dams, nests, resting beds and seed caches. Intersecting
nature edits can displace them; removing the obstruction lets the same structure rebuild.
Active dams have collision and local water-rise facts. This is a bounded pool/rise
approximation, not drainage, flow, or volume-conserving hydrology. Authored young family
members grow during play, retain identity, and do not die or accumulate offline debt.
Suitable nearby patches can attract up to three animals within 480 m, including
a parent/young pair. They travel through production brain waypoints and return
when the patch is removed; there are no distant spawns or teleports. There are no
runtime births and no fully populated continental ecosystem.

Free construction includes cabin, path, bridge, deck, bench, lantern and fence. Brush
radius, spell strength, placement reach, construction scale and yaw are adjustable.
The fixed starting cabin uses the same procedural construction source. A polished
placement preview, manipulation tool, removal targeting and richer decoration remain
unfinished. Nature edits and player placements currently have bounded count limits.
Willing strong companions can move a faced source boulder in two-metre steps;
render/collision share its sparse persisted displacement and undo. The change is
currently immediate, without an authored pushing/contact animation.

Expedition document version 3 persists population, ecology, construction, discovery,
tool settings, boulder displacements, external decision records and travel alongside legacy save facts. Files are written before user
changes are published. Living autosaves occur after the completed population tick.
Legacy rescue fixtures are retained as regression tests; the ordinary game no longer
presents the retired rescue objective. Player save directories were not used for tests.

## Evidence and limitations

Focused CPU checks have exercised typed-action rejection, terrain composition,
construction collision/undo, persistent relationships, long follow, mounted movement,
save/reopen, real disk-write failure, ecological displacement/rebuilding, growth,
chunk-edge refinement and the source boundary contracts. The definitive report and
source digest are recorded in ../SANCTUARY_PROGRESS.md.

No new render baseline has been accepted. No generated illustration is used as renderer
evidence. Another Soundstage session remains open on Vesper; it was inspected read-only
through native accessibility, and was not edited or closed. Native typing, controls,
actual scene images, Sunhare/Cloud Ray motion, real routes, save/reopen UI, sound,
1080p GPU frame cost, cache-update spikes and model activity remain unverified.

## Native review sequence after the renderer session is free

1. Use an isolated Soundstage harness session. Select Sanctuary and inspect catalog;
   inspect sunhare, moonhart, cloudRay, home-cabin, and ecology-dam in the actual catalog
   (generator IDs are authoritative; do not assume aliases).
2. Save studies before inspection; use front/quarter/back/above under prescribed
   outdoor/indoor and wet/dry conditions. Run styleBoard and inspect actual reports.
3. Replay living-requests and habitat-and-building in Sanctuary, then riding-movement
   and flying-movement with native controls. Inspect terrain edit edges and vegetation
   roots, cabin doorway, companion body/camera placement, and rejection paths.
4. Traverse real regional routes in a named isolated slot, save/reopen, and fix visual,
   movement, density and pacing problems. Discovery counts do not prove enjoyment.
5. Measure short live 1920×1080 runs with clouds, wind, ecology, edits and chunk crossings;
   include p95/max frame intervals and cache/update spikes. Only then evaluate the optional
   model against the authored fallback under the same live workload.
