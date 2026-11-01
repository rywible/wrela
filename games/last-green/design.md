# The Last Green's design contracts

What the floor's look and its frame's work rest on, as settings Ryan can change. Each names where
it lives; the numbers marked **proposed** are the plan's first guesses (docs/plan-world-class-2.md),
measured in docs/bench.md, and his to set.

## The frame's switches (`engine::switches::Switches`: constants of the build, the floor's `Tiered<FloorTier>` in main.wrela)

| Switch | Default | What it does | Measured |
|---|---|---|---|
| `SIGHT` | on | each probe weighed by what it sees of the point (B9a) | the walk's re-trace and lookups +0.28 ms; the probes +7.6 MB |
| `DISTANCE` | on | the solids' distance volume: the probes' rays step through it, the contact shadows and the ambient occlusion read it (B9b) | a slab 0.32 ms on a sixth of the walk's frames; +10.9 MB |
| (steering, `frame::STEERED`) | on | the probes traced again where the sun and the sky changed, nearest the eye first (B9c) | the strips under the eye within 3 frames of a sun step |
| `FOG` | on | the air as froxels: fog, the canopy's haze, the falls' mist (B10) | the opening's fog 0.78 ms; +16.6 MB |
| `look.output` | SDR | HDR output where the screen is HDR (B13.3, D11) | |
| planar reflection (D15) | absent | not built (B12) | |
| `NEURAL` (`--set`) | off | the great tree's far crowns read through its net (`content/kit/great_tree_net.wrela`, B14.2) | 13.86 dB against its atlas; every kind through a net: the frame 25× (docs/bench.md, B14) |
| `CASCADES` | off | radiance cascades over the solids' volume light the sky's term in place of the probes' and the occlusion (B14.4); `Quality::cascades`' `spacing` 8, `steps` 16 | 3.2 ms at the coarsest rung, 13.1 at the finest (asked: 2.0) |
| `LEARNED` (`--set`) | off | temporal AA's share of each frame from a small net (`content/kit/rejector_net.wrela`, B14.3) | docs/bench.md, B14 |
| `GROWN_WOOD` (`--set`, main.wrela) | off | the vale's grown wood (plan three's §E): the trees' kinds grown (`trees::kinds::kind_plant`'s `grown`, and every table and fork read from them), the fallen trees rotting, the grass's flora grown (`engine::grass::Grass::new`), the bark's, the ground's and the rock beds' moss, lichen and litter caused (`Bark::grown`, `land::cover::wood_floor`, `RockLook::grown`); off, the floor is drawn as before §E. The great tree stays authored either way (`trees::great_tree`: a character of the lore) | off: the floor's stills against the merge base's (docs/bench.md, "E: the floor and the clearing kept"). On (with `GROWN_FAWN`): the same pipelines at every tier and look; its G-look on the 23 shots "before" on 18, so it stays off (§H1, docs/bench.md "H1"); the frame here 34.11 → 32.02 ms (`wrela profile`: the vegetation's passes −4.2 ms, the terrain +1.7, the far cascade +7.5) |
| `GROWN_FAWN` (`--set`, main.wrela) | off | the floor's fawn the roe grown from its genome (plan three's §F: `kit::roe` at `kit::roe::FAWN_AGE`, 1.0, **proposed**: the vale's grazer's age), its face attending (F8), in play following the sim's fawn (`fawns.wrela`); off, the hand's (`fawn::character`) | the same pipelines; its G-look on its four shots "after" on two (the glade, the stream) and "before" on two the shots hide it in, so it stays off (§H1) |
| `LOOK` (`--set`, main.wrela: a type) | `styles::physical::Physical` | the floor's look; `--set LOOK=styles::cel::Cel` builds the cel floor (plan three's Part D) | Cel: one pipeline more at every tier (Epic 171); its G-look for a new style: 22 of its 23 shots hold the physical floor's bars (the wildwood's trail misses, 12 px against 5), its judges' means 1.96, 2.09, 1.91, 2.17, 1.13 and style 1.04 (§H1); the walk here GPU p50 32.62 ms against 32.20, its peak 763.2 MiB against 743.7 |
| `TIER` (`--set`, main.wrela) | `Epic` | the floor's tier (`engine::tier`, D37): `High`, `Medium` and `Low` each draw less | the pipelines Epic 170, High 168, Medium 166, Low 162 (§H1); the M4's sustained GPU p50 Epic 15.54, High 15.09, Medium 12.46, Low 11.30 ms (docs/bench.md, "C7, the tiers") |

## The air (`stage::fog`, `engine::fog::FogField`)

- The height fog: `d0` 0.004 per metre at the floor's level ground (y 0), falling by e every 12 m
  above it, albedo 0.9 (**proposed**).
- The falls' mist: a sphere at each fall's foot and at its steepest step (`history::MISTS`), 0.12
  per metre at its middle (the mist pass's thickest, 0.22, as its noise averaged it).
- The canopy's haze: 0.02 per unit of the probes' crown density (**proposed**).
- B turns it off and on.

## The water (`engine::water::LOOKS`, F6 cycles them)

Clear, peat-dark and painted: the looks are Ryan's (D25). The ripples are carried by the flow in
two phases of 2 s (B12). Each fall within 60 m of the eye throws spray (about 200 a second) and a
column of foam (60 a second) (**proposed**).

## Rock (`engine::rock::RockLook`, B11d)

The stones, the standing slabs, the waystones, the crags' beds and the bank stones; the dressed
masonry (the blocks, the walls, the tower) keeps its own look.

| Parameter | Value |
|---|---|
| Bands along the bedding (up) | 0.7 m mean depth, warped ±0.6 m; tones 0.72 to 1.18 of the rock's |
| Cracks | Worley edges on 1.6 m cells, up to 0.45 darker |
| Lichen | faces under 30°, over 0.5 m above the ground, in patches: a grey-green, 20% at most |
| Wetness | within 0.6 m of the nearest water's level: 0.6 of the colour, roughness 0.35 |

All **proposed**.

## The climber's coat (`content/climber/draw.wrela`)

Velvet: sheen 0.6, tinted (0.08, 0.06, 0.12), dark in the hood's tunnel (B13.2, **proposed**).

## Levels of what's built (`blockouts::far_band`, B11a)

Each built thing's far level is its field four times coarser, taking over where its largest cell
covers about four pixels (a stone's band's middle at 50 m); the tower keeps 64 to 96 m.
