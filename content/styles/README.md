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
| `MARKS` | the marks drawn (`engine::marks`: `LINES`, `CREASES`, `DABS`, `STROKES`, `HATCH`) | `LINES`: the characters' lines |
| `FLAT_CLIP` | how hard temporal AA holds a flat tone flat (0 as the physical look; 1 the history clipped to this frame's colour where the substance's tone is flat) | 0 |
| `type Out`, `written` | what its shading's fragments write: `vec4`, or `substance::Written` (the colour and the substance) for a look that reads its substance | `vec4` |
| `shade` | the light a surface shows, from what reaches it (`light::Lit`: the sun's share, the sky and bounce, what passes through, the reflected light, the glint, the sheen), its substance and its class (`style::GROUND` …) | their sum (`light::world`) |
| `shade_leaf` | a leaf's: its gloss on its side that faces the eye | the matte light and that gloss |
| `palette` | the colour a material's albedo becomes, from its class and its history | the albedo |
| `mark` | how dark a mark makes a pixel | none |
| `lens` | the image to the screen: exposure, grade, vignette | the tone curve, the grade, the vignette |

The characters, the water and the climber's garment keep their own lighting (their cel bands, the
water's optics); a look reaches them through `palette` and its lens.

## The fixtures

`fixtures.wrela`'s looks measure and test the substance and the slots (`Every`, `Formed`,
`Caused`, `Toned`, `Normals`); no program ships one, and `wrela tool styles` leaves them out.
