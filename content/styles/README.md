# Looks

A look is how a program's frame draws: a type the program names (`pub type LOOK = Physical` in
its `main.wrela`, which `wrela build <pkg> --set LOOK=styles::<file>::<Look>` points at another),
implementing `engine::style::Style` (docs/plan-world-class-3.md, Part D). Each is a file here,
`<name>.wrela`, with `pub struct <Name> {}`, `impl Style for <Name>`, and its parameters as its
`pub const`s, so the fit (`wrela tool fit --style`) can write them and the lens can drag them.

## Making one

Copy `physical.wrela` (today's image), change one slot at a time, and build a scene that names a
look in it (`wrela build <scene> --set LOOK=styles::<name>::<Name>`). `wrela tool styles -- --check`
builds a scene in every look here and checks that adding one changed nothing in `engine/` (D10).

## The slots

| Slot | What it is | The physical look's |
|---|---|---|
| `SUBSTANCE` | the substance targets the frame writes for it after the shading (`engine::substance`: `FORM`, `CAUSE`, `TONE`) | 0: none |
| `NORMAL` | the normal a surface is lit by: 0 its fine one, 1 its form's, 2 its coarse one (`style::lit_normal`) | 0 |
| `MARKS` | the marks drawn (`engine::marks`: `LINES`, `CREASES`, `DABS`, `STROKES`, `HATCH`): all but the dabs on the scene's samples before temporal AA | `LINES` and `DABS`: the characters' lines, the brush's dabs |
| `FLAT_CLIP` | how hard temporal AA holds a flat tone flat (0 as the physical look; 1 the history clipped to this frame's colour where the substance's tone is flat) | 0 |
| `type Out`, `written` | what its shading's fragments write: `vec4`, or `substance::Written` (the colour and the substance) for a look that reads its substance | `vec4` |
| `shade` | the light a surface shows, from what reaches it (`light::Lit`: the sun's share, the sky and bounce, what passes through, the reflected light, the glint, the sheen), its substance and its class (`style::GROUND` …) | their sum (`light::world`) |
| `shade_leaf` | a leaf's: its gloss on its side that faces the eye | the matte light and that gloss |
| `palette` | the colour a material's albedo becomes, from its class and its history (and the light, which carries a fit's candidate constants: `engine::style::Knob`) | the albedo |
| `mark` | how dark a mark makes a pixel | none |
| `lens` | the image to the screen: exposure, grade, vignette | the tone curve, the grade, the vignette |
| `ANTICIPATE`, `ANTICIPATE_S` | its motion's wind-up (`engine::motion`, §F6): a planted foot's heel rolled up about its toe by `ANTICIPATE` degrees over the last `ANTICIPATE_S` s of its stance | 0°, 0.08 s |
| `OVERSHOOT` | how much more a character's secondary chains (head, ears, tail) sway than its gait's springs carry them | 0 |
| `EXAGGERATE` | how much higher a swing lifts and a body bobs about its running height | 0 |
| `ON_TWOS` | whether a character's pose is made on every other frame and held between, as hand animation is | off |
| `HIGHLIGHT` | a character's glossy parts' highlight (an eye's cornea, `engine::character::highlight`): 0 the physical light's gloss, else one band where the reflection comes within this cosine of the sun (and no gloss on the rest of the character) | 0: the gloss |

With the motion constants 0 and off, a character's pose is its gait's exactly; none moves a
planted foot. The world-style table (the looks' motion, **proposed**: the twelve principles'
anticipation, follow-through, exaggeration and timing, Thomas and Johnston 1981):

| Look | `ANTICIPATE` | `ANTICIPATE_S` | `OVERSHOOT` | `EXAGGERATE` | `ON_TWOS` |
|---|---|---|---|---|---|
| `Physical` | 0° | 0.08 s | 0 | 0 | off |
| `Cel` | 4° | 0.08 s | 0.3 | 0.25 | on |
| `Ink` | 4° | 0.08 s | 0.2 | 0.15 | on |
| `Painterly` | 3° | 0.08 s | 0.2 | 0.1 | off |

The characters, the water and the climber's garment keep their own lighting (their cel bands, the
water's optics); a look reaches them through `palette` and its lens.

## The looks and their **proposed** constants

Every number below is **proposed**: a starting point for Ryan, not an approved look.

### `Cel` (`cel.wrela`, D4)

| Constant | Value | What it sets |
|---|---|---|
| `BANDS` | 3 | the sun's tones through the ramp (steps at `(k / BANDS)²` of the cosine: 0.11, 0.44) |
| `TERMINATOR` | 0.2 | how soft each step is (of the cosine; the plan's 0.08; 0.3 on the earlier instrument, and 0.2 holds an old oak's flicker bar on the fast one, where a softer ramp's steps across a trunk shimmered: 3.04 → 1.44 ×10⁻⁶ against a bar of 1.63) |
| `LEAF_SOFT` | 3 | how much softer a leaf's steps are than `TERMINATOR` (a leaf card's normal turns as it aliases, and its hard steps flickered: the canopy against the sun 37 → 16 ×10⁻⁶) |
| `HIGHLIGHT` | 0.9 | the reflection's cosine to the sun past which a class with a highlight (water, a wet stone) shows one band |
| `PALETTE_N` | 4 | each class's tones |
| `TONE_STEP` | 1.0 | how much of the way between two tones the palette's step takes (a smooth step: a hard one flickered) |
| `HUE_PULL` | 0.3 | how far a tone's hue is pulled to its class's key |
| `PALETTE`, `KEYS` | per class | the tones (darkest to lightest) and key of the ground, leaves, bark, grass, flowers, water, stone, characters |
| `CREASE` | 0.35 | the curve per pixel (`|k1|` × the footprint) a crease needs |
| `CREASE_PX` | 12 | the extent along its ridge (output pixels) under which a crease fades out (from 0.3 of it; its curve fades in from 0.6 of `CREASE` to 1.5 times it); creases on solid forms alone: the ground, bark, stone, characters; drawn on the scene's samples before temporal AA, as the lines are (`engine::marks::marks_on`) |
| `LINE_DARK` | 0.55 | a crease's darkness at 5 m (a third of it by 60 m) |
| `FLAT` | 0.12 | the luma range across a pixel about which the albedo is flattened to its mean (fully under a quarter of it, not at all over twice it); the ground and bark, not leaves (a leaf's range flickers as its cards alias) |
| `FLAT_CLIP` | 0 | how hard temporal AA holds a flattened tone (the plan's 0.8 undid temporal AA: the forest floor's flicker 84 → 1,146 ×10⁻⁶) |

Its light: `NORMAL` 1 (the form's), its sky the probes' for that normal (not the hemisphere's mean:
the passes read the probes once, for the normal they light by).

### `Painterly` (`painterly.wrela`, D5)

| Constant | Value | What it sets |
|---|---|---|
| `PALETTE_N` | 8 | a class's tones, between its darkest and lightest of `cel::PALETTE` |
| `HUE_PULL` | 0.15 | how far a tone's hue is pulled to its class's key |
| `STEP_SOFT` | 0.5 | how much of the way between two tones the palette's step takes, round the middle |
| `SKY_SIMPLE` | 0.7 | how far the sky's light is taken toward its own grey, its luma (a painter's simpler, less coloured ambient; the plan asked for "the sky's directional part halved", which `Lit`'s sky term doesn't carry apart from its mean); 0.7, not the plan's 0.5: the canopy against the sun's flicker sat on its bar at 0.5, 0.42 against 0.41 ×10⁻⁶, and 0.7 gives 0.36) |
| `STROKE_PX` | 24 | a stroke's lattice spacing (output pixels): its length where the tone is flat |
| `DAB_LONG` | 0.35 | a stroke's length where the tone is detailed, of `STROKE_PX`: with `FLAT` 0 every stroke's (8 px) |
| `STROKE_WIDE` | 0.3 | a stroke's width over its length |
| `STROKE_VARY` | 0.04 | how much lighter or darker than the paint a stroke is at most, by its seed (the plan's 0.06: 0.04 holds the fall's pop bar, 0.0066 → 0.0064 against 0.00657) |
| `FLAT` | 0 | no flattening (the plan's 0.12, the cel look's: its flattened albedo flickered under the strokes, an old oak 1.18 → 2.13 ×10⁻⁶ against a bar of 1.63, and the fall's pop past its bar) |
| `FLAT_CLIP` | 0 | temporal AA as it is (the plan's 0.4: the forest floor's flicker 286 ×10⁻⁶ against a bar of 29, as the cel look's 0.8 did) |

Its marks: the physical look's dabs and paper's tooth, its strokes (`marks::STROKES`, on the scene's
samples before temporal AA at each pixel's unjittered centre: `engine::marks::marks_on`), and the
physical look's lines at the program's darkness (the plan's faint 0.25 put dawn in the hollow's
flicker at 20.6 ×10⁻⁶ against a bar of 17.1, and moved an old oak's trail onto a lagging edge).

### `Ink` (`ink.wrela`, D6)

| Constant | Value | What it sets |
|---|---|---|
| `HATCH_PX` | 6 | the hatching's lines apart (output pixels: its lattice's spacing) |
| `HATCH_WIDE` | 1.2 | a line's width (output pixels), its edge a pixel and a half |
| `LAYERS` | 0.25, 0.5, 0.75 | the density (1 less the light) past which each layer comes in |
| `LAYER_SOFT` | 0.08 | how softly a layer comes in |
| `TURN_STEP` | 15° | the steps a layer's direction is taken in (each step's lines one family on the lattice's plane) |
| `SKY_FLOOR` | 0.15 | the most light the sky adds under the sun's (so a dark side takes every layer) |
| `WASH` | 0.25 | a thin thing's (leaves, grass, flowers) wash for each layer its density brings in, in place of its lines |
| `INK_DARK` | 0.75 | how much of the paper's light a line takes |
| `PAPER`, `GRAIN` | (0.96, 0.95, 0.91), 0.02 | the paper's white and its grain |
| `LINE_PX`, `LINE_DARK` | 2.2, 0.9 | the silhouettes' width and darkness |
| `CREASE`, `CREASE_PX` | 0.35, 12 | the creases, as the cel look's |

Its coherence is **Blocked** (docs/bench.md, D7b): its hatching flickers 3 to 150 times past the
physical look's bars.

### `Kinetic` (`kinetic.wrela`): Kinetic Ink, the MMO's look

Ryan's brief (2026-11-03): "a cinematic, hand-drawn aesthetic built on a spatially coherent 3D
world", ink contours, graphic light, atmospheric depth, deliberately imperfect, and a visual
language that grows more expressive as action intensifies. Its look-development scene is
lookdev/ink-yard; its measures are docs/bench.md's "Kinetic Ink, the same scene three ways".

| Constant | Value | What it sets |
|---|---|---|
| `STEP`, `TERMINATOR` | 0.22, 0.08 | the sun's two bands' step (of the cosine times what reaches it) and its softness, softer still on leaves; harder as the moment rises |
| `FILL`, `FILL_SURGE`, `COOL`, `SHADOW_SKY` | 0.34, 0.6, (0.62, 0.70, 0.95), 0.5 | the shadow side: a third of the sun's light (an illustrator's tone, not the physical sky's eighth), cooled, with the sky's over it; `FILL_SURGE` of it taken away at the moment's height |
| `RIM`, `RIM_LIGHT` | 0.72, 0.55 | a band of sun where a solid form's edge turns from the eye toward it |
| `PALETTE_N`, `HUE_PULL` | 3, 0.35 | the world's classes in three tones each (`cel::palette_of`); a character keeps its own colours |
| `HATCH_PX`, `HATCH_WIDE`, `HATCH_ANGLE` | 11, 2.2, 0.75 | the hatching's lines apart and wide (output pixels), at one hand's diagonal: following the form's direction, rock's lumpy surface broke its lines into noise |
| `SHADE`, `DEEP`, `SURGE`, `INK` | 0.3, 0.6, 0.3, 0.7 | the hatching's first layer past `SHADE` of the density (1 less the scene's linear luma), its second crossed at 60° past `DEEP`, lowered by `SURGE` at the moment's height; how much light a line takes. Leaves, grass, flowers and characters take a wash in place of lines |
| `CREASE`, `CREASE_PX`, `CREASE_DARK` | 0.35, 10, 0.7 | creases as the cel look's, darker |
| `LINE_DARK` | 0.12 | the lines' light (`paint::Look::line_dark`: near none, ink); their width is the program's (`line_px`), which the yard thickens with the moment |
| `BOIL_HZ`, `BOIL_PX` | 8, 0.9 | the drawing redrawn 8 times a second (on threes): each step moves every mark's lattice and wanders each line by up to `BOIL_PX` output pixels, held between (`engine::marks`) |
| `SKETCH_STROKES`, `SKETCH_PX`, `SKETCH_BREAK` | 2, 5, 0.5 | a fighter's lines at the moment's height: two more strokes each, straying up to 5 output pixels either side of the form, half of each broken away; redrawn with each of its drawings (`engine::frame::Frame::drawing`), and none while they'd be too slight to see. The world's lines stay clean |
| `ROUGH_ANGLE`, `ROUGH_WIDE`, `ROUGH_SHADE`, `ROUGH_DEEP` | 1.15, 0.8, 0.72, 0.96 | a fighter's hatching as the moment rises: rough strokes on the screen's plane (its lattice there, redrawn with its drawings), each its own width and broken along its length, in its real shadow, crossed in the deepest; a wash while quiet |
| `DRAIN` | 0.7 | how far a fighter's colour drains toward ink on paper (its luma, warmed) from half the moment's height |
| `PAPER`, `TOOTH`, `INK_COLOUR` | (0.97, 0.94, 0.88), 0.06, (0.10, 0.07, 0.06) | the paper under the highlights, its fibrous tooth (fixed to the screen: the drawing moves on it), the ink's warm black in the darks |
| (the frame's edges) | | hold nothing of the moment: a light vignette alone. The fighters carry it (Ryan, 2026-11-03: "the fraying and the movement on the lines of the moving character") |

Its switches aren't the look's: a look is a type the frame's passes call, and turning a system
off is the program's (`engine::switches`). The yard's `Inked` is the set Kinetic Ink is meant for:
no clouds, no probes, no bloom, no froxel fog, no distance volume, no probes' sights, no screen
reflections and no contact shadows. Measured, the look alone saves nothing (docs/bench.md).

## Fitting a look

The cel and painterly looks' `<Look>Fitted` variants were fitted by CMA-ES against paintings'
statistics with `wrela tool fit` (studio/fit at 5b9b305, removed with the vale it fitted on). Each
range stopped where the look still held its coherence bars (docs/bench.md, "D8"); the fitted
files are frozen data now.

## The fixtures

`fixtures.wrela`'s looks measure and test the substance and the slots (`Every`, `Formed`,
`Caused`, `Toned`, `Normals`), the marks (`Dotted`) and the flattening's cost (`Unflattened`); no
program ships one, and `wrela tool styles` leaves them out.
