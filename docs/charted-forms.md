# Charted forms

How to author a part of a creature as a charted form: a limb, a trunk, a sleeve. A charted form is drawn from its function at the creature's posed bones, with no mesh and no skin weights. Where there is a reference, you write the part's structure and a tool solves its numbers; where there is none, you write its shape as a rule ("A creature written from rules").

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

It prints, station by station along the whole path, where the path is, how many rays round it meet the reference and the least and most radius they find; then every third station's section, the radius each of 16 rays round it finds. Start the span where every ray meets it. Where the sections make no sense (nothing found to the sides of a collar), the path is in the wrong place: slice the reference and look before you fit.
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

## A creature written from rules

A creature with no reference is written as rules: each part's radius is a function of its place (`chart::of`), about a chain of joints, on bones a cycle poses. `engine/tests/chart` (a hound), `content/crab`, `content/heron` and `content/serpent` are whole creatures written so, each in one file. Start from a copy of one.

Three fresh authors wrote the crab, the heron and the serpent from this doc and the hound, in 12 to 25 builds each. Three reviewers who weren't told what each was named all three and passed all three. What the authors had to find out for themselves:

- **Angles.** Angle 0 is the chain's `up`; a quarter turn on is `up × along`. For a chain running forward with `up` up, that is the creature's left, so a form's width is its radius at a quarter turn. On a flat limb, point `up` along the normal of the limb's plane.
- **Bones.** Bone `i` of a chain runs from joint `i` to joint `i + 1`, and `Part::bones` names one for each. `root` is what the chain hangs from. A rigid part (a head, a toe, a shell) is a chain of two joints that names one bone twice. A chain has eight joints at most.
- **Posing.** A bone's matrix carries a point of the creature at rest to where the pose has it. Build them as a hierarchy: each is its parent's times a turn about its own joint. Nothing plants a foot: a leg that must stay on the ground needs a two-bone reach written for it.
- **Where `f` runs.** `f` is of the path's whole length, its `before` and `after` included. Lengthening a chain's lead-in moves every rule along it.
- **Ends.** An end that shows takes a cap, and a cap closes toward the path's point; the rule shouldn't also go to nothing there. An end hidden in another part starts inside it and is narrower than it there, or it shows as a step.
- **How many terms.** A section much wider than it is deep, or a blade, needs more harmonics than a limb's three: a shell 3.7 times as wide as deep took five. Parts drawn in one loop share one size, so the hungriest part sets it.
- **Two tints on one form.** Paint by place: `chart::Vertex::place` brings how far along and the angle round to the pixel.
- **Views.** A thin creature seen head-on, and a long-legged one seen from its side, were the weakest views. Look from the side, the front and above, and at three moments of the cycle.

## Ends, and parts that don't bend

- **An end that shows closes** (`Part::caps`, `chart::closed`): over a stated share of the path from its start or before its end, the radius falls along a quarter circle to nothing, so the end is round and meets the side without a crease. A trunk closes at the fork of the legs; a head closes under the chin. An open end is a tube's cut rim, and a rim that stands out from what's next to it draws as an ink tick.
- **A rigid part is one form on a line its own bone carries whole.** Hakuro's head is charted about a straight line from under his chin to his crown, carried by his head's bone alone, and his neck is its own form through the neck's joints. Charted about the neck's joints, his jaw (which hangs below the head's joint) stayed with the neck when the head went back, and stood out from the throat as a spike.

## A hand

Each of Hakuro's hands is six forms behind a switch (`hakuro::HANDS_CHARTED`, off; `content/hakuro/hands.wrela`): four fingers and a thumb, each on its own line from inside the palm through its three joints to its tip, and a palm on the forearm's and the hand's bones.

- **A finger fits a chart well.** Read no further out than the gap to the finger beside it, each fits its sampled finger to 0.2 to 0.3 mm rms. Its path cuts a curled joint's corner over 8 mm, and its tip closes with a cap.
- **A palm starts where the arm's own form ends,** not at the wrist. Hakuro's arm was fitted as far as inside his wrap, and past that its form holds a forearm's thickness: with the palm starting at the wrist, the cuff was thick and the hand read as a paddle stuck in it. A palm runs a little past the knuckles' line before it closes, or the fingers' roots show as tubes coming out of a small palm, and is read as far out as the root of the thumb, or a notch opens there.
- **A fist drawn big** (`Limb::swell`) is its forms drawn thicker: bones that are scaled stretch a form's path and leave its thickness alone. A smear that draws finger bones out should leave the thumb with the hand, or the thumb's form comes away from the palm.
- **Judged**, over four rounds of four critics against the sampled hands: the charted fist took the impact drawing 15 votes to none ("shows knuckle and finger lines" where the sampled fist "melts into the sleeve") and the guard 6 to 2; the follow-through and the recovery were even; the open hand at the hip, from the front, lost 15 to none. So the sampled hands stay.
- **What's left.** The open hand: its fingers read as "fused into one tube" where the sampled hand's thumb and knuckles are picked out by their shade (finer shading, fingers fitted from their knuckles and a thinner wrist were each tried, and none turned a vote). A thin spike at a sharply bent wrist, where the palm's root comes out of the gauntlet's end. A thumb that stands off the fist like a stalk, where the sampled hand has a web.

## What changes with the pose

A form made again along its posed path already bends. How the path takes a bent joint matters: a limb's chain cuts the joint's corner (`Chain::cut` 1), following the arc that meets both bones where the bend's rounding starts and ends. Read about that path, the reference body posed is nearer itself at rest than about a path through the joint (10.1 mm rms left against 12.1), and a deep fold reads as one arm, not two lobes. A longer rounding than 5 cm fitted worse.

Three things are added on top, all zero at rest:

- **A joint's change** (`chart::Change`, `Radial::bent`): two forms written for a bend toward angle 0, one times the bend's angle and one times its square, turned to where the bend is. The tool solves them against the reference body posed by its own skin weights, over poses of captured motion (`studio/forms/joints.wrela`). A change holds at the deepest bend it was solved over (`upto`), since its square would carry it past anything it was solved against. Each joint's change is kept to its own stretch of the path, as far as halfway to the joints either side: left free, the solve trades one joint's change against the next's where the poses bend both, and a straight arm hanging at the side had its forearm thinned 14 mm by its shoulder's change.
- **A solved change is only as good as the reference's skin weights.** At a shoulder they collapse the cap, so the arm takes half of what was solved there (`figure::limbs::SHOULDER_TAKEN`). Judge a change in the poses the solve saw least: a limb hanging straight, a limb raised.
- **Named changes** (`content/figure/flex.wrela`): what a muscle does, as a bump on the chart in the bend's own frame, and the point of an elbow on the outside of its bend (the path cuts a bent joint's corner, so without it a slight bend reads as a hose's). These are an author's estimates; say so where you write one, and keep them small: a bump twice as high read as a lump, a point twice as high as a kink.
- **A root that a deep bend swings into sight keeps its width.** A leg's root was narrowed inside the hip (`tuck` 0.6), and in a crouch or a roll the hip's fold showed the narrowed stretch: critics called the thigh thin where it left the trunk. More bulk at the seat as a named change, and less of the hip's solved change, were each tried and neither changed the picture. The leg's root now keeps its whole width (`tuck` 1) and is drawn 6 cm under the trunk, which is itself under what's worn: nearer than the trunk, the full root showed as a patch at the belt.
- **A fold** (`chart::Fold`): inside a hinge's bend, the radius is limited to the room to the plane that halves the bend, so the limb's two lengths meet at a crease. Name the hinge in the part's `folds`.

## Exaggeration, tried and taken out

A named change follows a joint's angle. A drawing could also follow the move: muscles that stand at a blow, by the move's own timing. This was tried on the combat lab's cross and did not earn its place.

- **What was tried.** The striker's arms tensed (a form added to each arm's: the shoulder's cap, the triceps, two forearm masses), by how fast the fist was going in and held while it was on the face. Judged against the plain cross, one constant apart (`wrela tool judge`'s `--base-set`), on the cross's fastest drawings (`--shots strike`).
- **What critics said.** At 5 mm the change could not be found in the pictures. At 10 mm four critics gave 3 votes each way and 10 for no difference, and where they saw it they called it "a small lump in the contour" as often as a clearer elbow. A forearm thickening toward the fist frayed the arm's outline and was not put to critics.
- **Why, as I read it.** The cross's fastest drawings are near silhouettes, and the punching arm is in a black sleeve: there is no shading inside it for a muscle to show in, only an outline, and the ink reads small outline changes as roughness. The fist drawn big and the head squashed, which the lab already does, are changes of a size the style shows.
- **What it leaves.** `--shots strike` (the going-in, contact, landed and held drawings) for judging a strike. Critics judged stills; whether an exaggeration reads in motion is a question for a person watching at full rate.

## Shading

A pixel reads its normal from the chart itself (`chart::normal_at`): its place on the chart and the section's plane come down from the vertices, and the radius's slopes there give the normal. It is as exact on a coarse grid as on a fine one, which a mesh's normals are not. Read it from the posed form with its finest harmonics damped (`Radial::softened`): a fit's ripples are a fraction of a millimetre on the silhouette, but they tilt the surface enough to fray a cel's bands of light. Where a folded hinge holds the side in, or inside a cap, use the vertex's normal (`Limb::folded`). The damped harmonics can be dropped outright (`Radial::kept`): the pixel then sums fewer terms for the same light, within a tenth of a millimetre of radius.

## Garments

- **A tight garment** is a difference over the body's form on the same chain (`fitted_over`). One with an edge of its own (a wrap round a forearm, a gauntlet) is a second part on the arm's chain and bones, its form the arm's plus a little, drawn over its own band. Stiff leather stands 3.5 mm off, and the ink draws its edge as one line. Wound cloth sits 1.5 mm off, under what the ink takes for an edge: at 2.5 mm the ink broke its edge into dashes, and at 3.5 mm it read as a boxy cuff.
- **A loose garment about the same bones** is its own form on the body's chain (the jacket on the trunk's), so it goes where the body goes.
- **A body under a garment that isn't charted with it** is drawn under the garment the whole way (`Part::sunk` reaching past its end), closed at both ends, and on a coarser grid (`Part::coarser`): it shows no outline of its own. Hakuro's trunk under his sampled jacket is drawn 4 cm under and two levels coarser. Without the first, the trunk came through the jacket at the chest and the hip when he leant back; the second took 0.4 ms off the paint pass.
- **A structured garment is a form for each run of its surface.** Hakuro's jacket is a boxy vest: about his spine, the radius jumps 14 cm across his shoulders' flat tops, and one form missed his collar by 19 mm. As four it fits: its body about his spine up to under its shoulders (2.2 mm rms; its sides over its top 10 cm are left to its shoulders, where the body is 12 mm out), its collar about a line straight up behind his neck (1.1 mm), and each shoulder about a line along his collarbone and on down his upper arm (0.2 to 1.8 mm over what it draws).
- **What's worn over a shoulder rides the arm.** Charted about a line his chest carries, the jacket's shoulders stood out as flat plates while his arms hung. Their line bends at his shoulder's joint, as his arm's does, with his upper arm's turn spread to his elbow so the shoulder takes little of it.
- **Openings** are a `chart::Cover`: a bit a place, set where the garment is, written by the tool from where rays met the reference. The pixel is discarded where `covered` is under a half. A garment of several forms has a cover a form, held one after another (`chart::uploaded`, `covered`'s `at`), each cut where the next piece takes over, and the pieces overlap a little so no gap opens between them. The tool evens each cover's marks (`evened`): a ray grazing a rim meets it at one place and misses at the next, and marks as rays leave them drew a toothed edge with skin showing through.
- **An open sheet is drawn from both faces** (a collar's inside shows from the front). Which faces a draw keeps is fixed when a program is built, so a program writes both draws and chooses between them.
- **Paint** is by place on the chart, or by where the place is on the creature at rest.

## When a species' limb changes

A character's limb is the species' plus a difference, so fitting the species' limb again would move the character's. The tool keeps the character's limb where it is (`kept_over`): forms about one path add, so the new difference is the old one plus the old species' limb less the new, and no reference is read. Hakuro's arms are kept so; the sampled grids they were first fitted to are gone, and `studio/forms/measured.wrela` holds 416 of each grid's radii to measure the arms against.

## Judging a change

`wrela tool judge -- build/judge/<name> --against <revision>` renders the workspace and another revision on a list of shots and writes each critic a directory of pairs, marked A and B, with a brief; `--tally` reads their answers through the sealed key. Each shot is shown each way round to half the panel, because critics lean toward a letter. Read what critics say of each side as well as how they vote: the descriptions repeat from panel to panel when the votes don't.

A round is run in stages, to keep its cost down:

- **Four critics first**, on the shots where the change can show (`--critics 4 --only <shot> ...`). A clear result ends the round: the jacket's was 6 votes to 25. Four more only when the first four are close, and a result still close is reported as close: five critics once gave the same pictures 30 of 30 and then 20 of 30.
- **Critics on a small agent.** A critic needs to read a brief and its pictures and nothing else. On an agent with four tools one used 42 thousand tokens for eleven pictures; on a general-purpose agent, 92 thousand. The pictures are about a quarter of that.
- **Which model.** Over four rounds Opus critics leaned the way Fable critics did every time, at three quarters of the tokens, though they answer "no difference" about half the time. Sonnet critics were at or under a coin in two of the four. Haiku critics leaned the right way on a round whose answer was known (17 to 12, where Fable gave 27 to 15) but named parts that were the same in both pictures.

## What a chart is poor at

- Surfaces square to the path: a shoulder's top, a collar, an end cap. The radius must change steeply there, and the splines round it off. Hakuro's jacket misses most at its collar and shoulders.
- Anything not star-shaped about its path: fingers held together, an ear. Those stay implicit fields.
- A pose far past what the joints' changes were solved over: the changes hold there, and nothing says the held shape is right.
- Where a charted limb leaves a garment that isn't charted with it. The limb's root is drawn under the garment (`Part::sunk`), and from behind a raised arm the garment's armhole stands off the arm as a thin line.
- A stiff garment's edges. Hakuro's jacket as four forms was judged against his sampled jacket and took 6 votes to 25, so his jacket stays sampled (`hakuro::JACKET_CHARTED` is off). What critics marked, and what each needs: ragged edges at its shoulders and collar, worst close to the camera, where a cover's marks are 2 to 4 mm apart and the ink draws along them (an edge written as a curve on the chart, not as marks); blotches on its upper back where its shoulders lie 1.5 mm over its body (pieces that meet at a line, not overlap); a gold crescent under an armhole, the lining's paint on an inside face (the inside painted as the outside's cloth); and no thickness at a rim, which a sampled shell has (a second sheet inside the first).

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
| Hakuro's jacket's body, to under its shoulders | 24 by 21 | 2.22 mm; 63 mm (at its sides near its top, which its shoulders draw) |
| Hakuro's jacket's collar | 16 by 13 | 1.14 mm; 9.9 mm |
| Hakuro's jacket's shoulders, each (over all its rays meet) | 16 by 13 | 2.67 mm; 18.0 mm |
| Hakuro's fingers and thumbs, each | 16 by 13 | 0.21 to 0.28 mm; 3.5 mm |
| Hakuro's palms | 16 by 13 | 0.86 and 0.80 mm; 9.6 mm |

Each is measured on rays between the fit's own places, on toolchain 0.32.0. `wrela tool forms` writes every one in about ten seconds.
