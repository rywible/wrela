# Prompting for references

*How to get useful references out of generative models. The rules on what models may do, and the log every output goes in, are in [citations.md](citations.md) and [generated.md](generated.md). Written 2026-11-06.*

Generated output never ships, so it doesn't have to share one look. The game's cohesion comes from its own art style, the wrela renderer that every source is converted into. A reference only has to teach the thing we're building: its form, scale, light and mood. So we gather a **bunch** of references for each thing, judge them against what we want, and tune from there.

We tried the other way first: a shared style prefix and an automatic style check on every prompt (2026-11-06, 21 test frames in the project's `genai-bible/abtest/`). It made frames match each other, but Ryan judged the plain prompts' frames better, with more drama and more varied light. So there is no mandatory style prefix.

## Gathering a reference set

1. **One subject per set.** Write what the thing is, plainly and specifically, then where it is, then the light and weather. Add the mood or a style hint only if it helps show the thing (FLUX.2 weights early words most, so the subject goes first).
2. **Go wide, then narrow.** First round: 8 to 12 frames across angles (near, far, from below, from the skiff), times of day and weather, and a few style treatments. Second round: take the 2 or 3 that teach the most and vary only those.
3. **Use the seed ladder** (101, 202, 303, 404) when comparing two prompts, so the difference is the prompt's. A seed reproduces only with the same prompt, inputs and model version; the log keeps all three.
4. **Judge each frame on what it teaches.** Does the form read? Does the scale feel right? Is the canon right (below)? A frame can be off-style and still be the best reference for the silhouette.
5. **Keep the set in one folder** (`genai-targets/<area>-<subject>/` in the project's files), with a `PROMPTS.md` listing each prompt and what each frame is good for. Log every output, discarded ones included.

## Canon facts

Things Ryan has decided. Get these right in any reference that shows them, or the reference teaches the wrong design.

| Thing | Facts |
|---|---|
| The idol | a colossal ancient stone idol man of heavy blocky weathered pale granite: three separate heads side by side on one thick neck (left head turned left, middle head forward, right head turned right), and exactly three arms: one huge arm at each side and a third arm growing from the middle of the chest |
| Hakuro (the player) | a young fighter in a long open near-black coat lined in gold, sleeved on one arm with the other arm bare, a white fox mask with vermilion fire marks and pointed fox ears, black combat boots |
| The skiff | a small wooden sail skiff with a single mast and one pale triangular sail |
| The cathedral | a vast pale stone gothic cathedral on a floating rock; real-size details repeated many times, never a small church made big; windows dark by day |
| The world | endless overcast ocean, storms, cosmic dread; no sea battles or big ships |

Give real sizes ("three hundred metres", "each door ten times a person's height") and a person or the skiff for scale.

## Things that helped in testing

- **No negative prompts in FLUX.2.** Naming a thing to forbid it tends to summon it. Describe what you want instead:
  - For a sea without the curling "great wave" crest: "broad heavy swells like slow moving hills of dark water, their tops torn by wind into flat streaks of white foam".
  - For a squall line that isn't a mushroom cloud: "a long low wall of cloud, its flat lower edge running straight from one side of the horizon to the other like a lid".
  - For natural rock rather than a built tower: "natural", "eroded by waves", "cracked uneven sides" (avoid "layered", which reads as masonry).
- **Hex codes get painted.** FLUX.2 treats a hex code as an order to put that colour somewhere. Attach it to a named object ("the coat's lining in color #d9a521") or leave it out.
- **Anything you mention gets drawn.** A mood line saying "the sea is indifferent" put the cathedral doors in the sea. Keep each prompt to the subject's own world.
- **Reference images** (FLUX.2 [pro] takes up to 8 as `input_images`) make a set match each other but drift towards the references' look. Use them when frames must match, such as a turnaround of one object.
- Don't use a FLUX output to train or fit a network, and don't present one as made by a person (BFL's terms; generated.md).

## Model notes

**Images: FLUX.2 [pro]** (`black-forest-labs/flux-2-pro`, key `flux-2-pro-api`). Settings: `aspect_ratio` 16:9 for scenes, 1:1 for objects; `output_format` png; always set `seed`. 50 to 150 words per prompt. For crowded shots a JSON prompt (`scene`, `subjects[]`, `lighting`, `mood`, `camera`…) keeps subjects apart.

**Image to 3D: TRELLIS** (`firtoz/trellis`, community model, needs `--version`; key `trellis-mit`). The concept image matters most:

```
Single <object> concept for a game, clean stylized CG sculpt, full body three-quarter front view, long lens with minimal perspective, the whole object in frame with margin on every side, isolated on a plain light grey background, even soft studio light with no cast shadows, matte surfaces. <Canon facts>. <Silhouette and big forms>.
```

TRELLIS reads depth from shading and edges, so hard shadows become geometry and busy backgrounds lose geometry. One object per image; shiny, glass and very thin parts fail. Settings: `randomize_seed` **false** (the default is true, and then the seed isn't reported); `ss_guidance_strength` 8 to 9 for hard-surface stone; `slat_guidance_strength` 3, or 4 to 5 if carving is lost; 20 sampling steps per stage for a final source; `mesh_simplify` 0.95, never below 0.8. Change one input at a time at a fixed seed. Judge the mesh on silhouette and big forms; it is only fitted into field parts. (The vendor guides behind these numbers were written for TRELLIS 2; `firtoz/trellis` is TRELLIS 1 with the same inputs and defaults.)

**Sound: Stable Audio 2.5** (`stability-ai/stable-audio-2.5`, key `stable-audio-2.5-api`). Sounds are fitted into `engine::sound` and instruments. The soundtrack is written by us as wrela score.
- One-shot effect: `One single <sound> sound effect: <source and material> <action>, <attack, body, decay>, then silence, dry, close mic, no music.`
- Ambience: `Field recording of <place and weather>, <what is heard, near to far>, steady ambience, no music, no voices.`
- Instrument note: `Solo <instrument>, a single sustained note <pitch>, played <dynamic>, natural decay to silence, dry studio recording.`

Settings: `cfg_scale` 7, `steps` 8, `duration` as short as the sound, the whole seed ladder per prompt, and the fitter picks. Unlike in images, a short "no music" clause did help here.

**Motion: Kimodo** (`sprited/kimodo`, key `kimodo`). No game, anime or style words; they gave weaker motion. Start with "A person…", give one action at medium detail, add force words ("angry", "heavy", "powerful") and the body mechanics in one clause, and use hand pins for control. The full tested rules are in the project's `kimodo/prompt-lessons.md`.

## Sources

- Black Forest Labs, [FLUX.2 prompting guide](https://docs.bfl.ai/guides/prompting_guide_flux2), read 2026-11-06.
- Replicate model schemas for `black-forest-labs/flux-2-pro`, `firtoz/trellis` and `stability-ai/stable-audio-2.5`, read 2026-11-06.
- fal, [TRELLIS 2 image-to-3D prompt guide](https://fal.ai/learn/devs/trellis-2-image-to-3d-prompt-guide); 3D AI Studio, [How to use TRELLIS 2](https://www.3daistudio.com/blog/how-to-use-trellis-2-online-image-to-3d-tutorial); Microsoft, [TRELLIS README](https://github.com/microsoft/TRELLIS).
- Stability AI, [Stable Audio 2.5 prompt guide](https://stability.ai/learning-hub/stable-audio-25-prompt-guide).
- Our own trials: the 2026-11-06 A/B frames, the combat-sound prompts, `kimodo/prompt-lessons.md`.
