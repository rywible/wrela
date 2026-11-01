# Looks

A look is how a program's frame draws: a type the program names (`pub type LOOK = Physical` in
its `main.wrela`, which `wrela build <pkg> --set LOOK=styles::<file>::<Look>` points at another),
implementing `engine::style::Style` (docs/plan-world-class-3.md, Part D). Each is a file here,
`<name>.wrela`, with `pub struct <Name> {}`, `impl Style for <Name>`, and its parameters as its
`pub const`s, so the fit (`wrela tool fit --style`) can write them and the lens can drag them.

## Making one

Copy `physical.wrela` (today's image), change one slot at a time, and build the vale in it
(`wrela build examples/vale --set LOOK=styles::<name>::<Name>`). `wrela tool styles -- --check`
builds the vale in every look here and checks that adding one changed nothing in `engine/` (D10).

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

## Fitting a look

`wrela tool fit -- --style <cel|painterly>` (studio/fit) searches a look's `FITTABLE` constants
(its hand: the cel look's `TERMINATOR`, `PALETTE_N` and `HUE_PULL`; the painterly look's
`PALETTE_N`, `HUE_PULL` and `SKY_SIMPLE`) by CMA-ES on a shot, against a set of paintings'
statistics or one reference, and writes the result as a look of its own (`<Look>Fitted`):

```bash
wrela tool studio/fit --write ws=. --write out=build/fit --read refs=$WRELA_REFERENCE_DIR/styles -- --style cel --package examples/vale --shot D5 --reference-set refs/cel --out content/styles/cel_fitted.wrela
```

The search builds the program once, in the look's fitting variant (`<Look>Fitting`, whose fittable
constants are the light's: `engine::style::Knob`), and sets each candidate through its `fit`
export. Each range stops where the look still holds its coherence bars: fitted without that, the
cel look's ramp went to 0.589 and an old oak's flicker missed its bar (5.77 ×10⁻⁶ against 1.64),
and the painterly look's sky to 0.015 simplified and the canopy against the sun's missed (0.56
against 0.42; 0.5 holds it); so `TERMINATOR` is at most 0.2 and `SKY_SIMPLE` at least 0.5
(docs/bench.md, "D8").

## The fixtures

`fixtures.wrela`'s looks measure and test the substance and the slots (`Every`, `Formed`,
`Caused`, `Toned`, `Normals`), the marks (`Dotted`) and the flattening's cost (`Unflattened`); no
program ships one, and `wrela tool styles` leaves them out.
