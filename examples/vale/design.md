# The vale

A grown valley drawn by the engine's frame, with no game sim: the showcase that plan three's
public references judge (`docs/plan-world-class-3.md` §B1). A square kilometre of land made from
its rock and the uplift under it, its water laid from what the land drains, a wood grown over it
by a history, a walk through it and twenty fixed shots. Its composition is original: no part of
it reconstructs a source image.

Every value here marked **proposed** is a first choice, open to a later step's evidence.

## The land (`landscape.wrela`)

Made when the vale is built, by `land::landform::evolve`, and cached while nothing it reads
changes (the land's tests run in 16 s, its making included).

| What | Value |
|---|---|
| The square | (−512, −512) to (512, 512) m; the head at +z (north), the mouth at −z |
| The grid | 4 m cells, 256 × 256 (the history's preview grain; the history itself is at the bake's 2 m and reads the land between cells) |
| The making | 300,000 years in 150 steps of 2000; stream power m = 0.5 (n = 1); channels where more than 0.15 ha drains; creep 0.0025 m²/yr; laying 0.45; critical slope 1.0 × the rock's; faces failing ± 40 % |
| The base level | a 24 m outlet at the mouth's middle |
| The rock | a shale (K 3.5e-5, stands at 0.6, weathers 2e-3 m/yr, creeps fully) crossed by a band of grit 28 m thick a third of the way up from the mouth (z = −170.7, its edges wandering ± 2 m; K 1.5e-6, stands at 3.5, weathers 6e-5 m/yr, creeps 0.03); joints making either 20 % weaker or stronger from place to place (**proposed**) |
| The uplift | a dome, most at the head and along the crests: 0.9 mm/yr × (0.06 + 0.94 up the valley) × (0.6 + 0.4 toward the crests); the band rising 0.22 mm/yr faster at its middle, a 50 m Gaussian across it: a fold (**proposed**; the plan proposed 400 m of relief, the making gives 279 m) |
| The old land | nearly level, falling 6 m to the mouth and 4 m to the axis, rough by 1.2 m; an old hollow 8 m deep just above the band (its middle 80 m up from it, 70 by 55 m), which the band's rise keeps a hollow (**proposed**) |
| The floodplains | `land::landform` cuts down, not across; each gentle stream's floor (draining over 10 ha, running under 8 %) is levelled as its water would have levelled it, 6 m either side and 30 more at a square kilometre, rising 1.5 % from the stream, the valley's side coming down to it at 0.8; never the band, a lake or a stream's own cell (**proposed**) |

What the making left (`examples/vale/tests/land`):

- One main stream down its receivers, 368 cells from (−494, 482) to the outlet at (10, −510).
- The fall where the band crosses it: 13.5 m over 5.7 m of course at (26, −158), the top of a
  cascade from 72 m to a pool at 34 m within 20 m. The band rose faster than the stream cut it,
  so it stands 30–40 m over the stream on either side: the fall is in a slot.
- The mere above it: 1.13 ha, its level 73.12 m, over the fall's lip at 72.06 m; the meadow at
  its head.
- The relief: 279 m. The floor runs from 0.7 m at the mouth to 163 m at the head; the crests from
  20 m to 279 m.

The soil (`land::soil::soil`, from each cell's curvature at 12 and 32 m, its slope and its
wetness), in cells of 16 m²:

| Rock | Thin | Loam | Colluvium | Alluvium | Gley | Peat |
|---|---|---|---|---|---|---|
| 9942 | 9968 | 23796 | 16957 | 144 | 3547 | 1182 |

A cell's wetness is its topographic index (ln of the area draining through it a metre of its width
over its slope), as the floor's geology reads it; on a floodplain, as wet as its stream's floods
leave it (1 beside the stream, 0.55 at the floor's edge). The stream's bed is gley and peat, its
floodplains alluvium, its crests thin.

The ground (§E5: `landscape::SOIL_ALBEDO`, `SOIL_ROUGH`, `THIN_STONES`, `wetted`; **proposed**):
each kind's soil where it shows, its meadow over it in the open and the wood's floor under the
canopy. The history's wetness at the ground (`fields.z`) darkens whatever covers it, soaked to
0.65 of its albedo (`land::cover::wood_floor`: water in its pores, Lekner and Dorf 1988), and
smooths it, soaked to 0.7 of its roughness. The clearing and the floor keep their ground until
H1.

| Kind | Albedo (linear) | Roughness | Reads as |
|---|---|---|---|
| Rock | the rock's own (`rock_albedo`) | 0.85 | the band's grit, the shale |
| Thin | (0.105, 0.09, 0.07) | 0.92 | stony: its stones a third of its ground, in patches |
| Loam | (0.085, 0.062, 0.042) | 0.9 | brown earth |
| Colluvium | (0.07, 0.05, 0.034) | 0.88 | deeper, darker earth at the slopes' feet |
| Alluvium | (0.065, 0.052, 0.036) | 0.85 | dark loam on the floodplains |
| Gley | (0.06, 0.062, 0.05) | 0.65 | grey and wet |
| Peat | (0.03, 0.024, 0.018) | 0.6 | black |
| The mere's bed | (0.045, 0.04, 0.03) | 0.5 | silt under the water |

## The water (`water.wrela`)

Laid by `land::water::lay_with` over the land: the main stream in three courses (above the fall,
the fall itself from a cell above its lip to one below its foot, and below it), each other stream
draining more than 2 ha, and the mere a sheet at its level over its basin (the terrain's relief is
the mere's bed). Mists at the fall (`land::water::mists`). 21 courses; 233,841 vertices.

## The wood (`history.wrela`)

| What | Value |
|---|---|
| Its species | the floor's (`trees::species`): oak, beech, birch, hawthorn, blackthorn, pine, alder, with the old wood's habits (`ancient`) |
| Its mix | the floor's grown land's: oak, beech, pine and alder 0.2 each, birch 0.15, hawthorn 0.05 |
| Its grain | the bake's: 2 m cells, 2-year steps |
| Its present | the year 900 |
| Its past | none: an old wood when its history begins |
| The meadow | 34 m round, at the mere's head (6, 26), kept by grazing (**proposed**) |
| The herd | deer, come 300 years before the present, 12 trips a year, drinking at the mere's shore by the meadow; their trail runs between them (**proposed**) |
| The gale | 6 years before the present, throwing two exposed crest stands 90 m round at (330, 236) and (−300, 24), share 0.95; their gaps kept browsed while recent (**proposed**: from inside the wood the vale doesn't show; from these gaps it does) |

`land::grow` keeps an opening by grazing, by people or by a recent event, and none by water, so
the meadow is the herd's. The history: 27,867 trees, 18,676 living (oak 3622, beech 6311, birch
1676, hawthorn 2960, blackthorn 1187, pine 2344, alder 576), the oldest 1177 years. None stand in
the mere, and none over 2 m in the meadow's middle.

The bake is a file a tile and the far layer (`land::bake`). The vale's tiles are 256 m, 16 of
them (the plan proposed 128 m, 64: `land::place::TILE` is one constant through the bake, the
streaming and the clipmap's fields). A tile past the square is grown empty.

## The day (`day.wrela`)

52° north in late August, the sun's declination +10° (**proposed**). By the sun's own time it rises
a little north of east at about 5:10, stands 48° high in the south at noon, and sets a little
north of west at about 18:50. A shot's weather:

- **fair**: the floor's clouds;
- **overcast**: the sky covered, no sun reaching the ground;
- **mist**: thin cover, and the air's mist over the vale's floor (1.2 per km at 0 m, a 30 m scale
  height).

## The shots (`lookbook.wrela`)

Twelve evaluation shots, one of each class the plan fixes, in its order; eight development shots
(D1–D8) at other places, at least 80 m from the twelve's of their class. The development shots are
for every fit, every tuning and every diagnostic; the twelve are rendered for the reviews' record
and the panels alone (`wrela tool lookbook --shots dev`, `--shots evaluation`). Heights are over
the ground; every place is **proposed** until §B5's panel freezes the list.

| # | Shot | Eye (x, over the ground, z) | Hour | Weather |
|---|---|---|---|---|
| 1 | the forest floor | (−153, 1.6, 80) | 12:30 | fair |
| 2 | the canopy against the sun | (96, 1.6, −24), looking at the sun | 10:00 | fair |
| 3 | an old oak (born −111) | (115.6, 1.5, −7.0) | 15:00 | fair |
| 4 | the fall, from the gorge's mouth | (18, 1.6, −194) | 11:00 | fair |
| 5 | the band's face | (−34, 1.6, −190) | 16:00 | fair |
| 6 | the meadow at evening | (−22, 1.6, 28) | 18:12 | fair |
| 7 | dawn in the hollow | (−24, 1.7, −430) | 5:36 | mist |
| 8 | dusk from the crest (the east gap) | (336, 1.7, 243) | 18:48 | fair |
| 9 | grazing at the edge (the creature at 6 m) | (26, 1.4, 44) | 14:00 | fair |
| 10 | down the trail (the creature at 10 m) | (−9, 1.4, −14) | 17:00 | fair |
| 11 | the fallen beech (fell 84 years ago) | (−122.8, 0.9, −40.8) | 13:00 | fair |
| 12 | the wood under cloud | (−250, 1.7, 250) | 13:00 | overcast |
| D1 | the forest floor | (203, 1.6, 152) | 12:30 | fair |
| D2 | the canopy against the sun | (−300, 1.6, 40) | 10:00 | fair |
| D3 | another old oak | (−206.8, 1.5, −113.6) | 15:00 | fair |
| D4 | the stream above the mere | (−10, 1.6, 209) | 11:00 | fair |
| D5 | the mere's shore at evening | (−90, 1.6, −100) | 18:12 | fair |
| D6 | the vale from the west gap | (−330, 1.7, 48) | 18:48 | fair |
| D7 | the creature in the wood | (−64.4, 1.4, 58.4) | 14:00 | fair |
| D8 | the wood under cloud | (150, 1.7, 301.6) | 13:00 | overcast |

The creature's shots (9, 10, D7) draw the vale's grazer (`grazer.wrela`): in shot 10 it walks
toward the eye along the shot's look, stepped from the shot's start, 10 m away at the still (§F9's
strip is rendered at 3840 × 2160 and cropped round it, so its motion reads in the shot's own field); in 9 and D7 it grazes
there, three-quarters on to the eye (`Shot::cast` 0).

## The grazer (`grazer.wrela`, `kit::roe`)

A roe doe, grown (age 1 of maturity, `kit::roe::VALE_AGE`) from the roe's genome
(`kit::genome::ROE`, docs/plan-world-class-3.md §F1–F5): 0.72 m at the withers, 28 kg. (It was a
fawn in its first summer, age 0.2 and spotted; the look's judges read it as too small in the
vale's shots, §F2's G-look.) Its authored part is its body plan's twelve handles and its face; the
rest grows:

- **Its growth.** Its mass follows a logistic from 1.6 kg at birth to 28 kg grown, its rate 8.5
  a year to maturity (two thirds grown by its first autumn): 7.0 kg at its age. Each part follows
  Huxley's y = b·x^k against that mass, b the grown animal's handle: the withers by 0.28, the
  body's length 0.36, the chest 0.4, the neck 0.4, the head 0.22, the ears 0.2, the cannons 0.22
  (a share grows by its exponent less its whole's). So a fawn's legs and head are large for its
  body and its neck short: its legs 1.17 times the grown's share of it at age 0.1, its head 1.34
  times.
- **Its coat.** A Gray-Scott reaction-diffusion (feed 0.0367, kill 0.0649, the rates 0.16 and
  0.08 scaled to the wavelength: Pearson's spots) run 2,000 steps on a 128 × 64 chart of its side
  (its rest space's length and height: one chart, no seams), its wavelength a tenth of its body's
  length (the rates by the square of wanted over natural, 14.2 texels); its spots fade by age
  0.45 (`coat::spots_at`: none on the grown doe). Its hide's colours are by region (a fox-red
  back, a buff belly, greyer legs), a fawn's spots cream over the back's (`roe::CoatLook`), baked
  by the gait tool with its seed (`kit::grown`).
- **Its muscles.** Sized by Alexander's rule from the loads of its optimized walk: each frame's
  required wrench (the segments' masses, centres and inertias from its body's field) shared among
  its planted feet by a bounded least squares (vertical forces non-negative, horizontals free, in
  units of its weight and height, regularized by 1e-6), each joint's torque by Newton–Euler over
  its subtree; each group's section its peak torque (scaled to a foot's 3 body weights) over
  0.3 MPa and its moment arm, its belly the bones it spans. Drawn as the anatomy kit's spindles
  over the kit's body (`muscles::muscled`): 12.3 kg of its 29.7.
- **Its walk.** Optimized in simulation by CMA-ES (`studio/gait`), the walker **kinematic**: its
  feet are planted by construction and its springs' energy isn't locomotion's cost, so a walk is
  scored by what its poses show. Its measures: *effort*, the sum of |τ·ω| over its joints per
  metre over the feasible frames (the head's, the neck's and the tail's chains frozen in the poses
  it reads, so their springs can't game it); *stability*, the share of the frames its centre of
  mass lies outside the support of the feet planted through the frame (a fall past a quarter of a
  stride's); *contact*, a planted foot's gap from its sole and each swing's clearance over the
  ground under it; *speed* against the pace's target; and *feasibility*, a frame whose wrench its
  feet can't carry within 5 % (over 5 % of the frames, the walk is invalid). The vale's walk is
  the level ground's (at another age it'd be scaled by dynamic similarity: its speed by the root
  of its legs' length against the grown's, its cadence by its inverse).

## The walk (`walk.wrela`)

A camera path of 1,200 frames, a step a frame once W begins it, playable or not, so every run
walks the same frames. The bench (the scene `vale-walk`) presses W at frame 240, as the floor's
walk is scripted: the vale loads at rest and becomes playable first, and its 1,200 measured frames
(after 60 of warm-up) are 180 at rest and the walk's first 1,020. It runs 340 m through eight
points, passing within 20 m of six shots' eyes (11, 10, 6, 9, 3, 2), about 17 m/s. The eye goes
2 m over the ground looking 15 m ahead, at 10:30 in fair weather (**proposed**). A first walk of
a kilometre (about 50 m/s, begun before the vale was playable) never let the frame become playable
in Chrome: its near cascade and its probes never settled; nor does this one, begun at frame 60.

## How it's drawn

| What | Value |
|---|---|
| Quality | the engine's defaults, every switch on (the floor's: `Tier::Epic` once §C7 exists) |
| The far cascade | over the whole vale: centre (0, 80, 0), span 1400 m, reach 1600 m, 2048² (**proposed**) |
| The near cascade, the probes | the engine's defaults; the probes' grid the floor's (`land::stage::probe_grid`: 81 columns 2 m apart, 30 m below to 40 m above the ground) |
| Metering | the engine's default (**proposed**) |
| The ground | each soil kind's own and its meadow; rock on faces steeper than about 50°, the crests' gentle bare rock grown over between its stones; the wood's floor over the history's fields (`land::cover::wood_floor`); past the tiles, the canopy's greens |
| The wind | the floor's (**proposed**) |

## The references (`references/vale.wrela`)

Each shot's bar is a photograph of the real subject, and the walking creature's is footage
(`docs/citations.md` lists each with its creator, licence and source). The copies live outside
the repository under `$WRELA_REFERENCE_DIR/vale`, each checked against its SHA-256 (21 of 21 on
2026-10-28). A judge sees them with their credit and licence; they are never textures, geometry,
fitting or training inputs.

The evaluation and development sets use different images: a development review (every fit and
tuning) never reads the twelve's references, so the twelve stay a held-out bar.

| Shot | Reference | Shot | Reference |
|---|---|---|---|
| 1 | vale-e01-floor | D1 | vale-d1-floor |
| 2 | vale-e02-canopy | D2 | vale-d2-canopy |
| 3 | vale-e03-trunk | D3 | vale-d3-trunk |
| 4 | vale-e04-water | D4 | vale-d4-water |
| 5 | vale-e05-rock | D5 | vale-d5-meadow |
| 6 | vale-e06-meadow | D6 | vale-d6-vista |
| 7 | vale-e07-mist | D7 | vale-d7-deer |
| 8 | vale-e08-vista | D8 | vale-d8-overcast |
| 9 | vale-e09-deer | the walk's creature (F9) | vale-motion-walk (30–100 s) |
| 10 | vale-e10-walking | | |
| 11 | vale-e11-deadwood | | |
| 12 | vale-e12-overcast | | |

Checked against B1's finished shots: each reference's subject is its shot's class. The vale's fall
is a cascade in a slot, and e04's is a small fall in an open wood; the class (water, spray, wet
rock, foam) is the same. D5 is the mere's grassy shore at evening, d5 a meadow before trees. None
was replaced.

## What is grown

What the vale shows, and whether it's grown (made by a process from causes) or authored (written by
hand). Part E fills it.

| What | Grown or authored |
|---|---|
| The land's relief | grown (`land::landform` from rock and uplift) |
| The floodplains | grown (levelled by a rule standing for the streams' swing) |
| The soil | grown (`land::soil` from the land's shape and water) |
| The water | grown (laid from the land's drainage) |
| Where trees stand, their ages, deaths and falls | grown (`land::grow`) |
| The trees' forms | grown: each living kind an archetype grown at build from its species' genome (`trees::archetypes`, `engine::growth`); the 24 trees nearest the eye grown from their own records (`land::stage::near_trees`), the 6 within 40 m of the first eye before it's playable, each kept in the page's storage by its growth's every input (§E7); the far layer's crowns the forest kinds' archetypes |
| The standing dead | grown: each snag kind (a broad-leaved snag, a burnt one, a stump, a forked one, a pine's) a tree grown from its genome, broken at its form's height and its wood under its form's thickness shed (`archetypes::dead`, `growth::die`; the forked one's fork a gale's in its youth) |
| The floor's plants | grown: the 33 species' rows from their genomes (`engine::grass::GENOMES`: rosettes, tussocks, fronds), the fern's and bracken's fronds by the place's light, the sward by light and wetness |
| The fallen trees | grown: a grown trunk laid along its fall, its bark lost and its bracket fungi by its rot (on the vale's tiles, 4,914 fallen trees, 20,848 brackets, 74 % of their bark gone) |
| Moss | grown: a reaction-diffusion's patches on the ground, the logs and the trunks by wetness and light (6 % of the vale's ground in its causes) |
| Lichen | grown: on old bark by age (4,278 trees old enough). The vale's crags are its terrain's rock, whose lichen is the terrain's detail tile (authored); the diffusion-limited growth's crusts (`growth::LICHEN`) are a rock bed's (`rock::RockLook::grown`), which only the floor draws, behind its switch (`GROWN_WOOD`) |
| Litter | grown: its depth by the canopy over it and the season (0.50 deep on average over the vale's tiles) |
| The fallen trees' root plates | authored: the hand's roots on a grown trunk (12 short roots, blunt-ended: **proposed**) |
| The ground's cover | authored (rules over grown fields: `land::cover`) |
| The rock's look | authored (the terrain's rock and its lichen: `terrain`'s detail tile) |
| The sky and the clouds | authored (the floor's) |
| The creature | grown: a roe doe from its genome, its proportions by its age, its coat by reaction-diffusion, its muscles from its walk's loads, its walk optimized in simulation (`grazer.wrela`, `kit::roe`); its body plan's handles and its hide's colours by region authored |

Every plant instance the vale draws is grown (G-grown: 0 of 26,922 authored, E8). The kinds list
it shares with the floor still holds the floor's great tree and two far kinds no program places.
