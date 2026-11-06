# Prompt bible

*How to prompt generative models so everything they make for wrela looks like one world. The rules on what models may do, and the log every output goes in, are in [citations.md](citations.md) and [generated.md](generated.md). This page is about getting good, coherent output. Written 2026-11-06 from the model vendors' own guides and an A/B test on our targets.*

Generated output is never shipped. It is a target to judge against, or a source a converter turns into wrela code. So "on style" here means: close enough to the Kinetic Ink look that an agent can aim wrela code at it without learning the wrong lesson.

## The short version

1. **Every image prompt is built from the same blocks, in the same order:** subject and action first, then the canon description of anything recurring, then the house style block, then the mood line, then camera. Copy the blocks below word for word. Don't paraphrase them.
2. **The shared blocks name no things.** Anything a shared block mentions gets drawn. Our first mood line said "the sea is indifferent", and a cathedral door and a stone idol both came back standing in the sea. World nouns go in the subject, never in the prefix.
3. **No negative prompts for images.** FLUX.2 doesn't take them, and naming a thing to forbid it in an image prompt tends to summon it. Say what you want instead (the rewrite table below). Stable Audio 2.5 has no negative input either, but there a short "no music" clause at the end did work in the combat-sound refs, so the audio templates keep it.
4. **Recurring things have one locked description** (the canon sheet). The idol is always "three separate heads side by side on one thick neck … exactly three arms". Never improvise a new phrasing for a canon thing.
5. **A fixed seed ladder** (101, 202, 303, 404) for every new prompt, so prompt changes can be compared at equal seeds.
6. **Check before use:** `onstyle.py` for palette, value and line; then a blind critic pass for canon and mood. Log every output, kept or not.
7. **Lore and style blocks are for image models only.** For motion they make things worse (Kimodo can't do "anime"); for sound only the mood words carry over; for image-to-3D the style comes from the concept image, not the prompt.

## House blocks (images)

Version: **style v3**, 2026-11-06. If a block changes, bump the version and say so in the log's `note`.

**STYLE**

> Rendered in clean stylized CG with a hand-inked finish: sculptural simplified forms built from big flat planes, bold black ink contour lines of varying weight on every silhouette and major fold, thick near the viewer and thin and broken in the distance, graphic lighting in three flat value steps with hard-edged shadow shapes and deep ink-black darks, full value range from black to bone white, distance dissolving into flat pale haze. Cold desaturated colour: slate, grey-green, fog grey and bone white; no warm colours unless named.

**MOOD**

> The mood is cosmic dread and quiet: everything is far too big, and the light is cold and rare.

**REF** (only when style anchors are attached, see below)

> Match the rendering style, ink line weight and palette of the reference images exactly; take only their style, not their subjects or composition.

Why this wording, from the test:

- Style v1 listed the palette as hex codes, gold included. FLUX.2 treats a hex code as an order to paint that colour somewhere, so a gold beam appeared in two of three frames. BFL's guide says the same: attach a hex code to a named object or leave it out. Style v2 names colours in words and makes warmth opt-in.
- Style v2 still said "grey-green sea", and the mood line named the sea; together they put the cathedral doors and the idol in the sea. Style v3 names only colours, and with the noun-free mood line the doors and the idol came back on land at both seeds tried (4242, 101).
- "Deep ink-black darks, full value range" is there because v1 frames, with and without reference images, came back washed out without it.
- "Hand-inked finish" and "contour lines of varying weight" are there because plain "ink lines" drifted between no lines (grand-03) and heavy outlines (sail-03). The test shows line work on every v2 and v3 frame, but how heavy still varies with the subject (near-black share 0.3% to 18%), so it is not yet tight.

## Canon sheet

Use these exact phrases whenever the thing is in frame. Add a row when something new recurs; change a row only when Ryan changes the design.

| Thing | Locked description |
|---|---|
| The open sea | The open sea is broad heavy swells like slow moving hills of dark water, their tops torn by wind into flat streaks of white foam. |
| The sky | a low overcast storm ceiling of heavy layered cloud, rain hanging under it in grey veils |
| The skiff | a small wooden sail skiff with a single mast and one pale triangular sail |
| The floating rock | a colossal floating island of dark jagged rock hanging in the sky, its underside a mass of hanging stone roots, water falling from its edges and dissolving into mist |
| The cathedral | a vast pale stone gothic cathedral with flying buttresses and tall spires, its windows dark by day. Its details are at real size and repeated many times, never a small church made big (see the distance rules: more parts, not bigger parts). |
| The idol | a colossal ancient stone idol man of heavy blocky weathered pale granite: three separate heads side by side on one thick neck (left head turned left, middle head forward, right head turned right), and exactly three arms: one huge arm at each side and a third arm growing from the middle of the chest. Calm, severe faces; carved line patterns; cracks. |
| Hakuro (the player) | a young fighter in a long open near-black coat lined in gold, sleeved on one arm with the other arm bare, a white fox mask with vermilion fire marks and pointed fox ears, black combat boots. Add `--warm` to the check: the gold and vermilion are named accents. |
| Light | cold, grey, from above and behind cloud; a single break of light is allowed when the subject asks for it |

Scale: give real sizes ("three hundred metres", "each door ten times a person's height") and a person or the skiff for scale. Things get big by having more parts, never by being a normal thing scaled up.

## Say this instead

FLUX.2 has no negative prompt. Write the positive version.

| Instead of forbidding… | Write |
|---|---|
| a curling "great wave" crest (it appeared in 4 of 6 sea frames with "rolling swells with crests") | the canon sea line: "slow moving hills of dark water, their tops torn by wind into flat streaks of white foam" |
| a mushroom cloud (sky-02's first try) | "a long low wall of cloud, its flat lower edge running straight from one side of the horizon to the other like a lid" |
| a built tower when you meant natural rock | "natural", "eroded by waves", "cracked uneven sides"; avoid "layered", which reads as masonry |
| bright warm colour | leave warmth out; the style block makes the image cold. Name an accent only where it belongs ("the coat's gold lining") |
| a fairy-tale castle | "gothic cathedral … flying buttresses … windows dark by day" |
| photo realism | the style block; don't add camera bodies or film stocks (BFL uses those to push towards photos) |
| anime faces or cute characters | the hero wears his mask; other people are small, far, or in silhouette |
| text, logos, watermarks | don't mention text at all |

## Images: FLUX.2 [pro] on Replicate

Model `black-forest-labs/flux-2-pro`, licence key `flux-2-pro-api`.

**Prompt order** (FLUX.2 weights early words more): subject and action → canon rows for anything recurring → setting → STYLE → MOOD → REF if anchors → camera and composition. 50 to 150 words is right for us; under 30 words gives generic results.

**Template**

```
<Shot type> of <subject doing action, with real sizes and a scale figure>. <Canon rows for each recurring thing in frame>. <Setting, weather, where light comes from>. STYLE. MOOD. [REF.] <Composition: where the horizon sits, what is in the foreground, wide horizontal frame>.
```

**Settings**

| Input | Value |
|---|---|
| `aspect_ratio` | `16:9` for scenes, `1:1` for concept sheets and 3D sources, `9:16` only for tall subjects |
| `resolution` | default (1 MP); 2 MP only for a final target |
| `output_format` | `png` |
| `seed` | always set; the seed ladder below |
| `input_images` | style anchors only, at most 3 (see below) |
| `safety_tolerance` | default |

**Structured JSON prompts.** FLUX.2 reads a JSON prompt as well as prose (BFL's guide gives the schema: `scene`, `subjects[]` with `description`/`position`/`action`, `style`, `color_palette`, `lighting`, `mood`, `background`, `composition`, `camera`). Use it for crowded shots where prose mixes subjects up; put STYLE in `style`, MOOD in `mood`, canon rows in each subject's `description`.

**Style anchors.** FLUX.2 [pro] takes up to 8 reference images. Attaching on-style frames with the REF line made a set of frames match each other more closely than text alone did. It also pulled them towards the anchors' haze: 5 of 6 anchored frames had a lower value range than the same prompt without anchors, and 4 of 6 fail `onstyle.py`. (Unanchored v1 frames also lacked ink, so the anchors aren't the only cause.) So:

- Default: text blocks only.
- Use anchors when a set of frames must match each other (a sequence, a turnaround) and the text-only frames drift. Then add a contrast reminder to the subject.
- Current anchor set (provisional, chosen by Claude, Ryan to confirm): `genai-targets/sail-02-from-helm`, `grand-01-rock-far` and `sky-02-squall-line`, downscaled copies in the project's `genai-bible/anchors/`. All three pass `onstyle.py`. The A/B test used `idol-03-impact` in place of the squall line; it was dropped because its Hakuro is off-canon (two sleeves, no gold lining). Upload them with Replicate's files API and pass the URLs; logs record them under `inputs.input_images`.
- Never use an anchor that shows a canon thing drawn wrong; the model copies details too.
- Don't use a FLUX output to train or fit a network, and don't present one as made by a person (BFL's terms; generated.md).

## Image to 3D: TRELLIS on Replicate

Model `firtoz/trellis` (community model, needs `--version`), licence key `trellis-mit`. The style is in the concept image, so its prompt is about what TRELLIS can read, not the mood.

**Concept image prompt** (FLUX.2, `1:1`):

```
Single <object> concept for a game, clean stylized CG sculpt, full body three-quarter front view, long lens with minimal perspective, the whole object in frame with margin on every side, isolated on a plain light grey background, even soft studio light with no cast shadows, matte surfaces. <Canon row>. <Shape details: silhouette, big forms, carved lines>. Hard-surface forms, bold readable silhouette.
```

Why (vendor guides written for TRELLIS 2; our model is TRELLIS 1, whose Replicate schema has the same inputs and defaults): TRELLIS reads depth from shading and edges, so hard shadows become geometry and a busy background becomes missing geometry. A three-quarter view tells it more than a dead-on front view. One object per image. Shiny, glass or very thin parts fail. Strong perspective distorts the mesh, hence the long lens. The idol's first source used a straight front view; try three-quarter against it before switching.

**Settings**

| Input | Value |
|---|---|
| `randomize_seed` | `false`, always (the default is true and then the seed isn't reported) |
| `seed` | the seed ladder |
| `ss_guidance_strength` | 8 to 9 for hard-surface stone; default 7.5 otherwise |
| `slat_guidance_strength` | default 3; 4 to 5 if fine carving is lost |
| `ss_sampling_steps`, `slat_sampling_steps` | default 12; 20 for a final source |
| `mesh_simplify` | 0.95; never below 0.8 (thin parts collapse) |
| `generate_model` | `true` |

Change one input at a time at a fixed seed. The back of the object is a guess; when the back matters, generate a back-view concept with the same seed and prompt and compare. The mesh is a source for fitting field parts, so judge it on silhouette and big forms, not texture.

## Sound: Stable Audio 2.5 on Replicate

Model `stability-ai/stable-audio-2.5`, licence key `stable-audio-2.5-api` (row in generated.md). Sound refs are fitted into `engine::sound` and instruments; the soundtrack itself is written by us as wrela score (no generated music).

Stability's guide is written for music: genre or type first, then instruments or sources, then mood, then details. It has no sound-effect advice and no negative prompt; we use the same order for effects and state the envelope and the room, because those are what the fitter needs.

**Templates**

- One-shot effect: `One single <sound> sound effect: <source and material> <action>, <envelope: attack, body, decay>, then silence, dry, close mic, no music.`
  Example that fitted well: "One single punch sound effect for a fighting game: a bare fist smashing into a jaw, tight punchy thump with a crisp skin slap, very short decay, silence after, dry, close mic, no music".
- Ambience: `Field recording of <place and weather>, <what is heard, near to far>, steady ambience, no music, no voices.`
- Instrument note (for the soundboard): `Solo <instrument>, a single sustained note <pitch>, played <dynamic>, <attack>, natural decay to silence, dry studio recording, no other instruments.`

The mood words carry over here ("cold", "vast", "lonely", "ominous"); world nouns only when they are the sound's source.

**Settings:** `cfg_scale` 7, `steps` 8, `duration` as short as the sound (1 s for a punch, 15 s for an ambience, 4 to 6 s for a note), seed ladder. Generate the whole ladder for a prompt and let the fitter pick.

## Motion: Kimodo

Model `sprited/kimodo`, licence key `kimodo`. No lore, style or mood blocks. Kimodo is trained on real motion and game, anime or superhero words gave weaker motion. Style comes from our timing and pin layer. The tested rules are in the project's `kimodo/prompt-lessons.md`; in short: start with "A person…", one action at medium detail, force words ("angry", "heavy", "powerful"), body mechanics in one clause, and hand pins for control.

## Seeds, names and files

- **Seed ladder:** 101, 202, 303, 404 for a new prompt. Once a seed is picked, keep it and change only the prompt, so the difference is the prompt's. A seed reproduces only with the same prompt, inputs and model version; the log keeps all three.
- **Names:** `<area>-<nn>-<subject>[-v<k>]_<seedindex>`, such as `sky-02-squall-line-v2_0`. Areas: `sky`, `ocean`, `sail`, `grand`, `idol`, `hero`, `island`, `sfx`, `amb`, `note`.
- **Where:** targets in the project's `genai-targets/`, tests in `genai-bible/`, area work in the area's folder. Run models through the project's `genai/gen.py`; it writes the log lines.
- **Log:** every output, including rejects (`use: discarded`) and tests. Put the bible version in `note` (for example `prompt bible style v3`).

## Is it on style?

Two gates. An output that fails either is `discarded` (still logged), or fixed by changing the prompt, never by picking around the problem.

**1. Automatic: `python3 -I genai/onstyle.py IMAGE… [--warm]`** (in the project's files). It checks what drifts most and is cheapest to measure:

| Rule | Measure | Pass |
|---|---|---|
| Cold palette | mean saturation | ≤ 0.24 |
| No loud colour | 95th percentile saturation | ≤ 0.42 |
| No stray warmth | share of saturated warm pixels | ≤ 1% (≤ 6% with `--warm`) |
| Ink present | share of near-black pixels, at full resolution | 0.3% to 30% |
| Line work | share of strong edges | 3.5% to 19% |
| Value range | 5th to 99th percentile luminance | ≥ 0.50 |

The thresholds were hand-set from the same frames they are checked on (the 15 Nov 6 targets and the A/B frames), so treat them as a first calibration and widen or tighten them when they disagree with a critic on new frames. On those frames it flags `grand-03-cathedral` (no ink), `sky-03-after-rain` (warm, low contrast), the washed-out anchored frames and one fogged doors frame. "Warm" covers red through gold. It does not catch a wrong subject or a cliché shape (the green curling wave passes), which is why there is a second gate.

**2. Critic pass.** A fresh agent that hasn't seen the prompt gets the image, the canon sheet and two anchor frames, and scores 0 to 2 on each:

1. Look: hand-inked line, flat value steps, sculptural planes (not a soft painting, not a photo, not a cartoon).
2. Palette and light: cold, desaturated, one break of light at most, warmth only where named.
3. Canon: every recurring thing matches its row exactly (count the idol's heads and arms; one triangular sail; dark windows by day).
4. Scale: big things read huge through repeated real-size parts and haze, not miniature.
5. Mood: dread and quiet; nothing cute, heroic-poster or busy.
6. Clichés: no great-wave curl, mushroom cloud, fairy-tale castle, lens flare, text.

Pass: 9 of 12 or more, and no 0 on canon. Before showing frames to Ryan, run at least two critic passes, as for all visual work.

## Test behind this page (2026-11-06)

Three new subjects (a sea stack with the skiff, the cathedral doors with Hakuro, the idol's three heads), FLUX.2 [pro], seed 4242 unless noted. One seed per cell, so read it as direction, not proof. All 21 frames are in the project's `genai-bible/abtest/` and logged.

| Arm | Prompt | What happened |
|---|---|---|
| A | the old one-line style suffix | Strong drama and contrast; ink varies from almost none (stack, 0.6% near-black) to heavy (idol, 29%) |
| B | style v1 (hex palette with gold) + mood naming the sea | Gold beams in 2 of 3; the doors and the idol came back standing in the sea; curling "great wave" crests |
| C | style v1 + 3 anchor images | Frames matched each other best, but washed out and lost ink |
| B2 | style v2 (colours in words, value range) + mood still naming the sea | Gold gone, line work on every frame, all three pass `onstyle.py`; but the sea still leaks into the doors and the idol, and the crests still curl. Only the critic pass catches these |
| C2 | style v2 + anchors | Consistent, still lower contrast than B2 |
| B3 | style v2 + the noun-free mood + canon sea line + "natural … cracked uneven sides" (stack, seeds 4242, 77) | No curl, natural rock; both pass `onstyle.py` |
| B4 | style v3 (no "sea" anywhere in the blocks) + noun-free mood (doors and idol, seeds 4242, 101) | Sea gone from all four. Idol frames strong. One doors frame (101) is fogged and fails the value check |

Not yet tested: the full seed ladder per cell, the new anchor set, and the JSON prompt form.

## Sources

- Black Forest Labs, [FLUX.2 prompting guide](https://docs.bfl.ai/guides/prompting_guide_flux2), read 2026-11-06: prompt order and length, no negative prompts, hex codes tied to objects, up to 8 reference images for [pro], JSON prompts.
- Replicate model schemas for `black-forest-labs/flux-2-pro`, `firtoz/trellis` and `stability-ai/stable-audio-2.5`, read through the API on 2026-11-06.
- fal, [TRELLIS 2 image-to-3D prompt guide](https://fal.ai/learn/devs/trellis-2-image-to-3d-prompt-guide), and 3D AI Studio, [How to use TRELLIS 2](https://www.3daistudio.com/blog/how-to-use-trellis-2-online-image-to-3d-tutorial) (vendor advice, not the authors'): clean background, soft light, three-quarter view, guidance ranges, mesh_simplify floor. Microsoft, [TRELLIS README](https://github.com/microsoft/TRELLIS): multi-image conditioning is tuning-free and may not suit every input.
- Stability AI, [Stable Audio 2.5 prompt guide](https://stability.ai/learning-hub/stable-audio-25-prompt-guide) and [audio-to-audio tips](https://kb.stability.ai/knowledge-base/tips-for-using-the-audio-to-audio-api): descriptor order, descriptive not instructive prompts.
- Our own: `kimodo/prompt-lessons.md` (Nov 6 Kimodo trials), the combat-sound prompts that fitted, and the distance rules for big things.
