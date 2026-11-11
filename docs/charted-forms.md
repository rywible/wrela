# Charted forms

How to author a part of a creature as a charted form: a limb, a trunk, a sleeve. A charted form is drawn from its function at the creature's posed bones, with no mesh and no skin weights. You write its structure; a tool solves its numbers.

The code is `engine/chart.wrela` (the form and how it's posed and drawn), `studio/forms` (the tool that solves the numbers), `content/figure` (the human species' forms) and `content/hakuro` (what the hero is over them).

## The idea

- **A chain** is a line of joints: a shoulder, an elbow, a wrist. The form's path runs through them.
- **A place on the chart** is how far along the path (`f`, 0 to 1) and at what angle round it (`theta`, from the chain's `up`, toward `up × along`).
- **The radius** at a place is a sum of splines along the path times a Fourier series round it (`Radial<A, R>`: `A` control values along, `R` terms round, `R` odd). It is linear in its numbers, so fitting it to a reference is one exact solve (`chart::fit`): no starting guess, the same answer every run.
- **Posed**, the path is laid where the bones are (`Chain::posed`) and each vertex is worked out from its number (`place_of`, `Limb::side`). A bone's turn about its own length is spread along it (`Chain::spread`).
- **A character** is the species' numbers plus a difference (`Radial::plus`). A right limb is the left's, mirrored (`Radial::mirrored`, `Chain::mirrored`).

## Authoring a part

1. **Write its chain** in `content/figure/chains.wrela`: its joints at rest, from the root's end to the tip, and `up` (angle 0). A chain can also be made from a body's rig where the tool runs (`studio/forms/second.wrela`'s `arm_chain_of`). Start the path a few centimetres inside the part it hangs from, as a first joint that part's bone carries, so the root stays tucked under what covers it. Set `spread` below 1 for a bone whose skin turns with it near its joint (an upper arm, a thigh: about a third).
2. **Write its spec** in `studio/forms/main.wrela` (`Spec`): the file and constant its numbers go to, three lines saying what it is (`about`), the chain, the `span` of the path its reference covers, how far from the path the reference is read (`most`), and how far its root is tucked (`tuck`). List where the reference is before you choose them:

```bash
wrela tool forms -- --stations <name>
```

It prints, station by station along the whole path, where the path is, how many rays round it meet the reference and the least and most radius they find. Start the span where every ray meets it.
3. **Give it a reference** (`studio/forms/targets.wrela`): `triangles_of` for a body's own triangles by part, or `Grid` for a sampled grid. The tool reads the reference along rays from the form's own path.
4. **Add its fit** beside the others in `main.wrela` (`fitted`, or `fitted_over` a base for a character's difference) and its line in `tool()`.
5. **Run the tool** and read what it prints:

```bash
wrela tool forms -- <name>
```

It writes the file and prints the miss: the root mean square, the size half are under, the size 95% are under and the worst, in millimetres across the side, measured between the places it was fitted at. Then the miss by place: a row a station along the span, a column a sector round from angle 0.

6. **Read the table before changing anything.** A bad row at the span's start or end means the span runs past the reference: shorten it. A bad column means the reference isn't reached that way (`most` too small; no effect once every ray meets it) or isn't there (an opening: the tool fills it from its neighbours). A miss spread evenly means the form is too small: more control values along for detail along the limb, more terms round for detail round it. A form's control values are spread over its whole path, not its span, so a shorter span adds no detail; a shorter path does (start the chain nearer the part). A joint where the radius changes fast (the back of a knee) is where a limb's miss gathers.
7. **Declare the part** where the creature is assembled (`figure::limbs::arm` is the pattern): an `engine::chart::Part` with its chain, its bones, its form, its joints' changes, its band and its folds.
8. **Add its test** in `studio/forms/tests.wrela`: the written file equals a fresh fit, and the miss is under what you state.

## What changes with the pose

A form made again along its posed path already bends. How the path takes a bent joint matters: a limb's chain cuts the joint's corner (`Chain::cut` 1), following the arc that meets both bones where the bend's rounding starts and ends. Read about that path, the reference body posed is nearer itself at rest than about a path through the joint (10.1 mm rms left against 12.1), and a deep fold reads as one arm, not two lobes. A longer rounding than 5 cm fitted worse.

Three things are added on top, all zero at rest:

- **A joint's change** (`chart::Change`, `Radial::bent`): two forms written for a bend toward angle 0, one times the bend's angle and one times its square, turned to where the bend is. The tool solves them against the reference body posed by its own skin weights, over poses of captured motion (`studio/forms/joints.wrela`). A change holds at the deepest bend it was solved over (`upto`), since its square would carry it past anything it was solved against.
- **Named changes** (`content/figure/flex.wrela`): what a muscle does, as a bump on the chart in the bend's own frame. These are an author's estimates; say so where you write one.
- **A fold** (`chart::Fold`): inside a hinge's bend, the radius is limited to the room to the plane that halves the bend, so the limb's two lengths meet at a crease. Name the hinge in the part's `folds`.

## Shading

A pixel reads its normal from the chart itself (`chart::normal_at`): its place on the chart and the section's plane come down from the vertices, and the radius's slopes there give the normal. It is as exact on a coarse grid as on a fine one, which a mesh's normals are not. Read it from the posed form with its finest harmonics damped (`Radial::softened`): a fit's ripples are a fraction of a millimetre on the silhouette, but they tilt the surface enough to fray a cel's bands of light. Where a folded hinge holds the side in, use the vertex's normal (`Limb::folded`).

## Garments

- **A tight garment** is a difference over the body's form on the same chain (`fitted_over`). One with an edge that shows (a wrap round a forearm, a gauntlet) is a second part on the arm's chain and bones, its form the arm's plus a few millimetres, drawn over its own band: its edge is then a real step, and the ink draws it as one line.
- **A loose garment about the same bones** is its own form on the body's chain (the jacket on the trunk's), so it goes where the body goes.
- **Openings** are a `chart::Cover`: a bit a place, set where the garment is, written by the tool from where rays met the reference. The pixel is discarded where `covered` is under a half.
- **Paint** is by place on the chart, or by where the place is on the creature at rest.

## What a chart is poor at

- Surfaces square to the path: a shoulder's top, a collar, an end cap. The radius must change steeply there, and the splines round it off. Hakuro's jacket misses most at its collar and shoulders.
- Anything not star-shaped about its path: fingers held together, an ear. Those stay implicit fields.
- A pose far past what the joints' changes were solved over: the changes hold there, and nothing says the held shape is right.

## Numbers so far

| Form | Size | Miss (rms; worst) |
|---|---|---|
| The species' arm, from the CC0 body's triangles | 16 by 13 | 1.03 mm; 4.6 mm |
| The species' leg | 16 by 13 | 1.08 mm; 9.4 mm |
| Hakuro's arms, over the species' | 16 by 13 | 1.09 and 1.00 mm; 4.1 mm |
| Hakuro's trouser leg, over the species' | 16 by 13 | 1.06 mm; 15.0 mm |
| Hakuro's trunk | 24 by 21 | 1.47 mm; 10.5 mm |
| Hakuro's jacket | 24 by 21 | 2.25 mm; 19.3 mm |

Each is measured on rays between the fit's own places, on toolchain 0.32.0. `wrela tool forms` writes every one in about ten seconds.
