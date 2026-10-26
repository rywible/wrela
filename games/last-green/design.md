# The Last Green's design contracts

What the floor's look and its frame's work rest on, as settings Ryan can change. Each names where
it lives; the numbers marked **proposed** are the plan's first guesses (docs/plan-world-class-2.md),
measured in docs/bench.md, and his to set.

## The frame's switches (`engine::frame::Quality`, `ProbeQuality`)

| Switch | Default | What it does | Measured |
|---|---|---|---|
| `probes.visibility` | on | each probe weighed by what it sees of the point (B9a) | the walk's re-trace and lookups +0.28 ms; the probes +7.6 MB |
| `distance` | on | the solids' distance volume: the probes' rays step through it, the contact shadows and the ambient occlusion read it (B9b) | a slab 0.32 ms on a sixth of the walk's frames; +10.9 MB |
| (steering, `frame::STEERED`) | on | the probes traced again where the sun and the sky changed, nearest the eye first (B9c) | the strips under the eye within 3 frames of a sun step |
| `fog` | on | the air as froxels: fog, the canopy's haze, the falls' mist (B10) | the opening's fog 0.78 ms; +16.6 MB |
| `look.output` | SDR | HDR output where the screen is HDR (B13.3, D11) | |
| planar reflection (D15) | absent | not built (B12) | |
| `neural` | off | the great tree's far crowns read through its net (`content/kit/great_tree_net.wrela`, B14.2) | 13.86 dB against its atlas; every kind through a net: the frame 25× (docs/bench.md, B14) |
| `cascades` | off | radiance cascades over the solids' volume light the sky's term in place of the probes' and the occlusion (B14.4); `spacing` 8, `steps` 16 | 3.2 ms at the coarsest rung, 13.1 at the finest (asked: 2.0) |
| `learned_resolve` | off | temporal AA's share of each frame from a small net (`content/kit/rejector_net.wrela`, B14.3) | docs/bench.md, B14 |

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
