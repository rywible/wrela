# Charted forms

How to author a part of a creature as a charted form: a limb, a trunk, a sleeve. A charted form is drawn from its function at the creature's posed bones, with no mesh and no skin weights. You write its structure; a tool solves its numbers.

The code is `engine/chart.wrela` (the form and how it's posed and drawn), `studio/forms` (the tool that solves the numbers), `content/figure` (the human species' forms) and `content/hakuro` (what the hero is over them). `studio/judge` puts two builds before critics blind.

## The idea

- **A chain** is a line of joints: a shoulder, an elbow, a wrist. The form's path runs through them.
- **A place on the chart** is how far along the path (`f`, 0 to 1) and at what angle round it (`theta`, from the chain's `up`, toward `up × along`).
- **The radius** at a place is a sum of splines along the path times a Fourier series round it (`Radial<A, R>`: `A` control values along, `R` terms round, `R` odd). It is linear in its numbers, so fitting it to a reference is one exact solve (`chart::fit`): no starting guess, the same answer every run.
- **Posed**, the path is laid where the bones are (`Chain::posed`) and each vertex is worked out from its number (`place_of`, `Limb::side`). A bone's turn about its own length is spread along it (`Chain::spread`).
- **A character** is the species' numbers plus a difference (`Radial::plus`). A right limb is the left's, mirrored (`Radial::mirrored`, `Chain::mirrored`).
- **Drawn**, a program binds the engine's vertex (`chart::vertex`, over whatever it sees with: `chart::Seen`) and its own pixel, which reads its normal from the form (`chart::Vertex::normal_from`) and paints it.

## Authoring a part

1. **Write its chain** in `content/figure/chains.wrela`: its joints at rest, from the root's end to the tip, and `up` (angle 0). A chain can also be made from a body's rig where the tool runs (`studio/forms/second.wrela`'s `arm_chain_of`). Start the path a few centimetres inside the part it hangs from, as a first joint that part's bone carries, so the root stays under what covers it. Set `spread` below 1 for a bone whose skin turns with it near its joint (an upper arm, a thigh: about a third).
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
7. **Declare the part** where the creature is assembled (`figure::limbs::arm` is the pattern): an `engine::chart::Part` with its chain, its bones, its form, its joints' changes, its band, its folds and its caps.
8. **Add its test** in `studio/forms/tests.wrela`: the written file equals a fresh fit, and the miss is under what you state.

## Ends, and parts that don't bend

- **An end that shows closes** (`Part::caps`, `chart::closed`): over a stated share of the path from its start or before its end, the radius falls along a quarter circle to nothing, so the end is round and meets the side without a crease. A trunk closes at the fork of the legs; a head closes under the chin. An open end is a tube's cut rim, and a rim that stands out from what's next to it draws as an ink tick.
- **A rigid part is one form on a line its own bone carries whole.** Hakuro's head is charted about a straight line from under his chin to his crown, carried by his head's bone alone, and his neck is its own form through the neck's joints. Charted about the neck's joints, his jaw (which hangs below the head's joint) stayed with the neck when the head went back, and stood out from the throat as a spike.

## What changes with the pose

A form made again along its posed path already bends. How the path takes a bent joint matters: a limb's chain cuts the joint's corner (`Chain::cut` 1), following the arc that meets both bones where the bend's rounding starts and ends. Read about that path, the reference body posed is nearer itself at rest than about a path through the joint (10.1 mm rms left against 12.1), and a deep fold reads as one arm, not two lobes. A longer rounding than 5 cm fitted worse.

Three things are added on top, all zero at rest:

- **A joint's change** (`chart::Change`, `Radial::bent`): two forms written for a bend toward angle 0, one times the bend's angle and one times its square, turned to where the bend is. The tool solves them against the reference body posed by its own skin weights, over poses of captured motion (`studio/forms/joints.wrela`). A change holds at the deepest bend it was solved over (`upto`), since its square would carry it past anything it was solved against. Each joint's change is kept to its own stretch of the path, as far as halfway to the joints either side: left free, the solve trades one joint's change against the next's where the poses bend both, and a straight arm hanging at the side had its forearm thinned 14 mm by its shoulder's change.
- **A solved change is only as good as the reference's skin weights.** At a shoulder they collapse the cap, so the arm takes half of what was solved there (`figure::limbs::SHOULDER_TAKEN`). Judge a change in the poses the solve saw least: a limb hanging straight, a limb raised.
- **Named changes** (`content/figure/flex.wrela`): what a muscle does, as a bump on the chart in the bend's own frame, and the point of an elbow on the outside of its bend (the path cuts a bent joint's corner, so without it a slight bend reads as a hose's). These are an author's estimates; say so where you write one, and keep them small: a bump twice as high read as a lump, a point twice as high as a kink.
- **A root that a deep bend swings into sight keeps its width.** A leg's root was narrowed inside the hip (`tuck` 0.6), and in a crouch or a roll the hip's fold showed the narrowed stretch: critics called the thigh thin where it left the trunk. More bulk at the seat as a named change, and less of the hip's solved change, were each tried and neither changed the picture. The leg's root now keeps its whole width (`tuck` 1) and is drawn 6 cm under the trunk, which is itself under what's worn: nearer than the trunk, the full root showed as a patch at the belt.
- **A fold** (`chart::Fold`): inside a hinge's bend, the radius is limited to the room to the plane that halves the bend, so the limb's two lengths meet at a crease. Name the hinge in the part's `folds`.

## Shading

A pixel reads its normal from the chart itself (`chart::normal_at`): its place on the chart and the section's plane come down from the vertices, and the radius's slopes there give the normal. It is as exact on a coarse grid as on a fine one, which a mesh's normals are not. Read it from the posed form with its finest harmonics damped (`Radial::softened`): a fit's ripples are a fraction of a millimetre on the silhouette, but they tilt the surface enough to fray a cel's bands of light. Where a folded hinge holds the side in, or inside a cap, use the vertex's normal (`Limb::folded`). The damped harmonics can be dropped outright (`Radial::kept`): the pixel then sums fewer terms for the same light, within a tenth of a millimetre of radius.

## Garments

- **A tight garment** is a difference over the body's form on the same chain (`fitted_over`). One with an edge of its own (a wrap round a forearm, a gauntlet) is a second part on the arm's chain and bones, its form the arm's plus a little, drawn over its own band. Stiff leather stands 3.5 mm off, and the ink draws its edge as one line. Wound cloth sits 1.5 mm off, under what the ink takes for an edge: at 2.5 mm the ink broke its edge into dashes, and at 3.5 mm it read as a boxy cuff.
- **A loose garment about the same bones** is its own form on the body's chain (the jacket on the trunk's), so it goes where the body goes.
- **A body under a garment that isn't charted with it** is drawn under the garment the whole way (`Part::sunk` reaching past its end), closed at both ends, and on a coarser grid (`Part::coarser`): it shows no outline of its own. Hakuro's trunk under his sampled jacket is drawn 4 cm under and two levels coarser. Without the first, the trunk came through the jacket at the chest and the hip when he leant back; the second took 0.4 ms off the paint pass.
- **Openings** are a `chart::Cover`: a bit a place, set where the garment is, written by the tool from where rays met the reference. The pixel is discarded where `covered` is under a half.
- **Paint** is by place on the chart, or by where the place is on the creature at rest.

## When a species' limb changes

A character's limb is the species' plus a difference, so fitting the species' limb again would move the character's. The tool keeps the character's limb where it is (`kept_over`): forms about one path add, so the new difference is the old one plus the old species' limb less the new, and no reference is read. Hakuro's arms are kept so; the sampled grids they were first fitted to are gone, and `studio/forms/measured.wrela` holds 416 of each grid's radii to measure the arms against.

## Judging a change

`wrela tool judge -- build/judge/<name> --against <revision>` renders the workspace and another revision on a list of shots and writes each critic a directory of pairs, marked A and B, with a brief; `--tally` reads their answers through the sealed key. Use a panel of ten or more: five critics gave the same pictures 30 of 30 and then 20 of 30. Each shot is shown each way round to half the panel, because critics lean toward a letter. Read what critics say of each side as well as how they vote: the descriptions repeat from panel to panel when the votes don't.

## What a chart is poor at

- Surfaces square to the path: a shoulder's top, a collar, an end cap. The radius must change steeply there, and the splines round it off. Hakuro's jacket misses most at its collar and shoulders.
- Anything not star-shaped about its path: fingers held together, an ear. Those stay implicit fields.
- A pose far past what the joints' changes were solved over: the changes hold there, and nothing says the held shape is right.
- Where a charted limb leaves a garment that isn't charted with it. The limb's root is drawn under the garment (`Part::sunk`), and from behind a raised arm the garment's armhole stands off the arm as a thin line.

## Numbers so far

| Form | Size | Miss (rms; worst) |
|---|---|---|
| The species' arm, from the CC0 body's triangles | 16 by 13 | 1.03 mm; 4.6 mm |
| The species' leg | 16 by 13 | 1.08 mm; 9.4 mm |
| Hakuro's arms, over the species' (against 416 radii kept of each sampled arm) | 16 by 13 | 1.06 and 0.99 mm; 3.6 mm |
| Hakuro's trouser leg, over the species' | 16 by 13 | 1.06 mm; 15.0 mm |
| Hakuro's trunk | 24 by 21 | 1.47 mm; 10.5 mm |
| Hakuro's neck | 16 by 13 | 1.64 mm; 6.7 mm |
| Hakuro's head, under his mask and hair | 16 by 13 | 2.53 mm; 17.4 mm |
| Hakuro's jacket | 24 by 21 | 2.25 mm; 19.3 mm |

Each is measured on rays between the fit's own places, on toolchain 0.32.0. `wrela tool forms` writes every one in about ten seconds.
