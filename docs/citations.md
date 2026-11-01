# Citations

*The outside works wrela uses: what each is, its rights, how we use it and why. Add a row in the change that starts to use one. Standard specs and vendor documentation that code follows are not listed.*

## The rules

- **Learn, don't copy.** A reference teaches light, colour, form, scale or composition. We don't trace it, sample it, or rebuild a specific shot, character or design from it.
- **No generative image, sound, music, voice or video model makes anything in wrela.** Every image and sound comes from wrela code. Ryan directs; Claude and Codex agents write the code.
- **Ship only what a licence allows, with its credit.** A reference file in the repo has its source, author and licence in the `references` package beside it. Work adapted from a CC BY source carries its credit into the game.
- **Style references stay private.** Images used only to judge the look are kept outside the repo.

## Public visual references and benchmark context

The vale's look is judged against freely licensed photographs and footage of the real subjects
it draws (docs/plan-world-class-3.md D29, Ryan's 2026-10-28 revision): agents alone judge, so a
reference must be licensed for a copy shown to an agent, and freely licensed Unreal 5 imagery of
these subjects barely exists. Public engine visuals may still inform look development as cited
context, without installing Unreal or acquiring its assets. Select and cite the actual works when they are first used; the policy links below do
not grant rights to a particular image or clip. Public availability and attribution alone do
not authorize copying. Keeping a reference private does not create permission either.

- Record the original creator/publisher, title, source URL, image locator or video timestamps,
  access date, known rendering conditions, rights/permission URL and required credit. Record
  unknown settings and rights as unknown. For the vale, `references/vale.wrela` holds
  the subject mappings and per-use decisions; it contains metadata, not reference media.
- Default to links to the original page. Use source-page viewing or an official player only
  as its terms permit. Check separately before downloading, extracting frames, cropping,
  re-encoding, uploading to an agent service or publishing a comparison containing the media.
  Prefer clearly licensed or creator-permitted references for agent review; an inaccessible
  or restricted source is replaced with another suitable source, not bypassed or guessed at.
- Preserve required credits and notices wherever a permitted copy is displayed. Use an
  attributed review if blinding would hide required credit. Do not remove watermarks. Keep
  reference copies and viewers containing them outside the repo and release. Public reports
  normally contain our renders, original observations, scores and source links; reproducing
  third-party media needs a grant covering publication. Do not label it MIT or imply endorsement.
- Evaluation references teach visual qualities; they do not supply textures, geometry,
  reconstructed shots or training/fitting inputs. A separate fitting workflow must use its
  own authorized inputs. No acquired Fab assets are required for this reference workflow.
- Public benchmark summaries cite the original publisher, date, workload, engine version,
  hardware, resolution, settings/upscaling and metric/method, with unknowns labelled. Keep
  them separate from measurements made here. Do not infer an M4 result or engine speedup
  from another device's numbers, or copy a chart/table without appropriate permission.

Primary terms consulted 2026-10-27 (recheck the applicable terms when selecting a work):

| Source | What to check |
|---|---|
| [YouTube Terms of Service](https://www.youtube.com/static?template=terms), “Permissions and Restrictions” | Viewing, official-player use, copying and automated access have distinct conditions. A playable public video is not a grant to download or rehost it. |
| [Creative Commons Attribution 4.0 legal code](https://creativecommons.org/licenses/by/4.0/legalcode.en), §§2–3 | For a work actually offered under CC BY 4.0, retain the required attribution and licence information and identify modifications; do not imply endorsement. Check rights not covered by the grant too. |
| [Fab licence page](https://www.fab.com/eula) | Its summary distinguishes using acquired assets in projects from standalone redistribution and points to the binding agreement. It is not a blanket permission for a creator's promotional screenshots or videos. |

### The vale's references

Photographs and footage from Wikimedia Commons, each licence read from its own page on 2026-10-28. Copies live outside the repo under `$WRELA_REFERENCE_DIR/vale` with a `CREDITS.md`; `references/vale.wrela` holds the metadata and each file's SHA-256. Shown to agent judges with the credit and licence; never used as textures, geometry, fitting or training inputs.

| ID | Set | Subject | Creator | Licence | Source |
|---|---|---|---|---|---|
| vale-e01-floor | evaluation | forest floor: ferns and undergrowth, trunks beyond | W.carter | [CC0](http://creativecommons.org/publicdomain/zero/1.0/deed.en) | [Commons](https://commons.wikimedia.org/wiki/File:Beech_and_ferns_in_Gullmarsskogen.jpg) |
| vale-e02-canopy | evaluation | beech leaves against the sun | Sixflashphoto | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0) | [Commons](https://commons.wikimedia.org/wiki/File:Blacklick_Woods_-_Sunlight_through_American_beech_1.jpg) |
| vale-e03-trunk | evaluation | an old oak's trunk with moss | Dietmar Rabich | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0) | [Commons](https://commons.wikimedia.org/wiki/File:Senden,_Venner_Moor_--_2013_--_2345.jpg) |
| vale-e04-water | evaluation | a small fall in a temperate wood | David Medcalf | [CC BY-SA 2.0](https://creativecommons.org/licenses/by-sa/2.0) | [Commons](https://commons.wikimedia.org/wiki/File:A_waterfall_on_the_stream_at_Oerddwr_-_geograph.org.uk_-_7747358.jpg) |
| vale-e05-rock | evaluation | a layered rock face in a wood | James St. John | [CC BY 2.0](https://creativecommons.org/licenses/by/2.0) | [Commons](https://commons.wikimedia.org/wiki/File:Cross-bedded_quartzose_sandstone_(Dundee_Sandstone,_Middle_Pennsylvanian;_cliffs_along_Walnut_Creek_Bottom_Road,_west_of_Dundee,_Ohio,_USA)_3_(23232718273).jpg) |
| vale-e06-meadow | evaluation | a flower meadow at a wood's edge | Peter O'Connor aka anemoneprojectors | [CC BY-SA 2.0](https://creativecommons.org/licenses/by-sa/2.0) | [Commons](https://commons.wikimedia.org/wiki/File:Wildflower_meadow,_Heartwood_Forest_(28861233370).jpg) |
| vale-e07-mist | evaluation | sun through morning mist among trees | Dietmar Rabich | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0) | [Commons](https://commons.wikimedia.org/wiki/File:D%C3%BClmen,_Umland,_Sonnenaufgang_--_2012_--_8069.jpg) |
| vale-e08-vista | evaluation | a lake among hills at sunset | Sanu N | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0) | [Commons](https://commons.wikimedia.org/wiki/File:Sun_Set_over_Phewa_Lake.jpg) |
| vale-e09-deer | evaluation | fallow deer under trees in dappled light | Johan Pisuisse | [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0) | [Commons](https://commons.wikimedia.org/wiki/File:Hind_(228949475).jpeg) |
| vale-e10-walking | evaluation | a roe buck walking in a wood | Jg44.89 | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0) | [Commons](https://commons.wikimedia.org/wiki/File:Samiec_sarny.jpg) |
| vale-e11-deadwood | evaluation | bracket fungi on a rotting log among ferns | John Robert McPherson | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0) | [Commons](https://commons.wikimedia.org/wiki/File:Bracket_fungi_on_fallen_log_Border_Track_Lamington_National_Park_Queensland_IMGP2516.jpg) |
| vale-e12-overcast | evaluation | a beech wood's interior in even light | Ermell | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0) | [Commons](https://commons.wikimedia.org/wiki/File:Kitschentalrangen-20200629-RM-155854.jpg) |
| vale-d1-floor | development | ferns on a woodland floor | W.carter | [CC0](http://creativecommons.org/publicdomain/zero/1.0/deed.en) | [Commons](https://commons.wikimedia.org/wiki/File:Beech,_ferns_and_lily_of_the_valley_in_Gullmarsskogen_1.jpg) |
| vale-d2-canopy | development | beech leaves backlit in a dark wood | W.carter | [CC0](http://creativecommons.org/publicdomain/zero/1.0/deed.en) | [Commons](https://commons.wikimedia.org/wiki/File:Sunlight_on_beech_leaves_in_Gullmarsskogen_ravine_1.jpg) |
| vale-d3-trunk | development | an oak's trunk in autumn bracken | Nicholas A. Tonelli from Northeast Pennsylvania, USA | [CC BY 2.0](https://creativecommons.org/licenses/by/2.0) | [Commons](https://commons.wikimedia.org/wiki/File:Round_Top_(Revisited)_(7)_(10282805645).jpg) |
| vale-d4-water | development | a small woodland fall into a pool | David Smith | [CC BY-SA 2.0](https://creativecommons.org/licenses/by-sa/2.0) | [Commons](https://commons.wikimedia.org/wiki/File:Waterfall_and_stream_pool,_Hafod_Estate_-_geograph.org.uk_-_7521443.jpg) |
| vale-d5-meadow | development | a yellow meadow with trees behind | Mat Fascione | [CC BY-SA 2.0](https://creativecommons.org/licenses/by-sa/2.0) | [Commons](https://commons.wikimedia.org/wiki/File:Wildflower_meadow_near_Bite_Farm_-_geograph.org.uk_-_7514202.jpg) |
| vale-d6-vista | development | a sunset over a lake among hills | Ncearle | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0) | [Commons](https://commons.wikimedia.org/wiki/File:Sunset_Over_Pitt_Lake.jpg) |
| vale-d7-deer | development | a roe deer in long grass | Peter Trimming | [CC BY-SA 2.0](https://creativecommons.org/licenses/by-sa/2.0) | [Commons](https://commons.wikimedia.org/wiki/File:Roe_Deer_at_Forest_How_-_geograph.org.uk_-_4074511.jpg) |
| vale-d8-overcast | development | a beech wood with a path, even light | Ermell | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0) | [Commons](https://commons.wikimedia.org/wiki/File:Kitschentalrangen-20200629-RM-170853.jpg) |
| vale-motion-walk | motion | a wild roe buck walking across a meadow by bracken | Peter Trimming | [CC BY 2.0](https://creativecommons.org/licenses/by/2.0) | [Commons](https://commons.wikimedia.org/wiki/File:Roe_deer_in_England.webm) |

## Assets and data in the repo

| Source | Rights | How and why |
|---|---|---|
| Six photos of bison and wolves (`references/grazer/`, `references/wolf/`) | US NPS, FWS and ARS photos on Wikimedia Commons; public domain. URLs and authors in `references/*.wrela` | Shown beside an authored creature to check its proportion and silhouette. Nothing of a photo goes into a creature. |
| Drew Vaughan (3ddrew), "Fantasy wolf miniature", [Thingiverse 680145](https://www.thingiverse.com/thing:680145) (`references/wolf/sculpt-standing.obj`) | CC BY 4.0; credit in `references/wolf.wrela` (`SCULPT`) | Measured and rebuilt as lofts and sweeps (`wrela tool reference`), to learn whether wrela's forms can hold a great shape. A game that ships it credits him. |
| Two CC0 photographs of lone oaks on Wikimedia Commons: Widgar52, ["Die Eiche"](https://commons.wikimedia.org/wiki/File:Die_Eiche.jpg) (Schmarbeck, 2024-01-18) and Qasinka, ["2020 Nebory památný strom"](https://commons.wikimedia.org/wiki/File:2020_Nebory_pam%C3%A1tn%C3%BD_strom.jpg) (*Quercus robur*, Třinec-Nebory, 2020-05-08) | CC0; kept outside the repo under `$WRELA_REFERENCE_DIR/oaks` | Cut out by hand to silhouettes and fitted against (`wrela tool grow -- --fit`, docs/plan-world-class-3.md §E6): their IoUs are in docs/bench.md. Only the silhouettes are compared; nothing of a photo goes into a tree. |
| Salamander Grand Piano V3 (a Yamaha C5), recorded by Alexander Holm; FreePats' retuned SFZ | CC BY 3.0; credit in `engine/piano_grand.wrela` | `engine::piano` is fitted to measurements of these recordings. The fitted numbers ship, not the recordings. The listening tests play the recordings beside wrela's piano, from a copy outside the repo. |
| Satie, Gymnopédie No. 1 (1888), the Mutopia Project's edition (Mutopia-2014/12/14-37) | Public domain | Transcribed into `examples/gymnopedie/score.wrela`, for wrela's piano to play. |
| Inter and JetBrains Mono (`ui/fonts/`); Manrope (`ui/brand/fonts/`, set at weight 500 with fontTools) | SIL OFL 1.1, licences beside them | The interface's text. |
| [ab_glyph](https://github.com/alexheretic/ab-glyph) 0.2 | Apache-2.0; used as a library | Drew `ui/tests/text/goldens/reference.png` once, from exact outline coverage, so the GPU's text can be compared with it. To draw it again, run `text::text_matches_a_reference_rasterizer` with `--ignored` (wrela-compiler, `compiler/tests/tests/suite/text.rs` at commit cc0be254). |

## Methods the code follows

Each is our own Wrela code written from the published idea. No code was copied.

| Source | Where | What it gives |
|---|---|---|
| Lekner and Dorf, [Why some things are darker when wet](https://doi.org/10.1364/AO.27.001278), *Applied Optics* 27(7) (1988) | `land::cover::wood_floor`'s wet ground, `examples/vale/landscape.wrela`'s `wetted` | Wet ground darker: water in its pores reflects back what would have scattered out |
| A. Chaigne and A. Askenfelt, "Numerical simulations of piano strings" (JASA 1994); B. Bank and others, "Physics-based sound synthesis of the piano" (2000); P. J. Huber, robust regression (1964) | `engine::piano` | Each partial's frequency-dependent decay, fitted to each key by Huber-weighted least squares |
| G. Weinreich on coupled strings; H. Conklin on phantom partials | `engine::piano` | A unison's aftersound; partials from the string's stretch |
| J.-M. Jot and A. Chaigne, feedback delay networks (1991); J. Abel and P. Huang, echo density (2006); modal synthesis | `engine::piano` | The room (16-line FDN, Hadamard mix), its test's echo density measure, the soundboard's knock |
| W. Pałubicki, K. Horel, S. Longay, A. Runions, B. Lane, R. Měch and P. Prusinkiewicz, [Self-organizing tree models for image synthesis](https://algorithmicbotany.org/papers/selforg.sig2009.html) (SIGGRAPH 2009); A. Runions, B. Lane and P. Prusinkiewicz, [Modeling trees with a space colonization algorithm](https://algorithmicbotany.org/papers/colonization.egwnp2007.html) (2007); R. Borchert and H. Honda, "Control of development in the bifurcating branch system of *Tabebuia rosea*" (Botanical Gazette 1984) | `engine::growth` (`grow_graph`), `content/trees/genomes.wrela` | A tree grown from its light: buds that shoot by the light they compete for (the shadow grid, the Borchert-Honda split of the tree's resource by apical control), markers of free space each year, shedding of the shaded |
| P. Prusinkiewicz and A. Lindenmayer, [*The Algorithmic Beauty of Plants*](https://algorithmicbotany.org/papers/#abop) (1990), ch. 4 | `content/trees/genomes.wrela` (`divergence`) | Phyllotaxis: the golden angle (137.5°), and 180° for two-ranked leaves |
| M. Roberts, ["The unreasonable effectiveness of quasirandom sequences"](https://extremelearning.com.au/unreasonable-effectiveness-of-quasirandom-sequences/) (2018): the R_d sequence; R. Cranley and T. Patterson's random shift (1976) | `engine::growth` (`Space::of`) | A crown's space markers spread evenly each year, offset by a draw, so a tree's shape is its causes' more than its seed's |
| K. Shinozaki, K. Yoda, K. Hozumi and T. Kira, "A quantitative analysis of plant form: the pipe model theory" (Japanese Journal of Ecology 1964) | `engine::growth` (`girth`), the genomes' `pipe` | A branch's section the sum of its children's |
| S. Lefebvre and F. Neyret, [Synthesizing bark](https://hal.inria.fr/inria-00537474) (EGWR 2002) | the genomes' `bark` and `cracks` (§E1.4) | Bark's cracks from the trunk's growth stretching it |
| M. J. Jaffe, "Thigmomorphogenesis: the response of plant growth and development to mechanical stimulation" (Planta 1973) | `engine::growth` (the genomes' `wind_*`) | A tree in the wind grows shorter and thicker |
| A. Friberg, R. Bresin and J. Sundberg, the KTH performance rules; N. Todd's phrase arch (1992); Friberg and Sundberg's final ritardando (1999); P. Desain and H. Honing, "Tempo curves considered harmful" | `engine::perform` | Expressive playing of a score |
| V. Zavalishin, *The Art of VA Filter Design* (2012); G. Marsaglia, "Xorshift RNGs" (2003) | `engine::sound` | The state-variable filter and the seeded noise |
| S. Hillaire, ["A Scalable and Production Ready Sky and Atmosphere Rendering Technique"](https://sebh.github.io/publications/egsr2020.pdf) (EGSR 2020) | `engine::air` | The sky and aerial perspective from precomputed tables; Earth's Rayleigh and ozone values |
| J. Jimenez, interleaved gradient noise | `engine::sky` | The cloud march's per-pixel jitter |
| B. Karis, the analytic environment BRDF and the bright-sample squash before a temporal resolve; S. Lagarde, specular occlusion | `engine::light`, `engine::temporal` | Specular scale and bias; a history that bright samples don't dominate; how much sky light a surface keeps |
| P. Bénard and others, dynamic solid textures (2009) | `engine::paint` | Octaves blended by distance, so painted marks keep their size on screen |
| S. Worley, "A Cellular Texture Basis Function" (SIGGRAPH 1996) | `engine::plant` | Bark's plates |
| M. Mikkelsen, ["Bump Mapping Unparametrized Surfaces on the GPU"](https://mmikk.github.io/papers3d/mm_sfgrad_bump.pdf) (2010) | `engine::plant` | Bark relief from height derivatives |
| The British National Vegetation Classification (W14, W10, W16, W8, W7, MG5, U4, M23) | `engine::grass` | Which plants grow together in a wood's floor and a meadow |
| B. Karis, R. Stubbe and G. Wihlidal, "A Deep Dive into Nanite Virtualized Geometry" (SIGGRAPH 2021); T. Ulrich, "Rendering Massive Terrains using Chunked Level of Detail Control" (SIGGRAPH 2002) | `engine::virtual`, `engine::terrain` | Clusters cut by projected error; small triangles rasterized in a kernel beside the hardware's; skirts that close the gaps between levels |
| C. Burns and W. Hunt, "The Visibility Buffer: A Cache-Friendly Approach to Deferred Shading" (JCGT 2013); C. Schied and C. Dachsbacher, "Deferred Attribute Interpolation for Memory-Efficient Deferred Shading" (HPG 2015) | `engine::virtual` | A pixel keeps its triangle's id; the resolve fetches and interpolates its attributes |
| Scthe, [nanite-webgpu](https://github.com/Scthe/nanite-webgpu) (MIT), as prior art | `engine::virtual` | Software rasterization with WebGPU's 32-bit atomics: a depth pass, then an id pass at the winning depth |
| U. Haar and S. Aaltonen, "GPU-Driven Rendering Pipelines" (SIGGRAPH 2015); N. Greene, M. Kass and G. Miller, "Hierarchical Z-Buffer Visibility" (SIGGRAPH 1993) | `engine::cull` | Two-phase occlusion: last frame's visible drawn first, a depth pyramid built from them, the rest tested against it |
| J. Jimenez, X. Wu, A. Pesce and A. Jarabo, "Practical Realtime Strategies for Accurate Indirect Occlusion" (SIGGRAPH 2016 courses) | `engine::ambient` | Ground-truth ambient occlusion: horizons found in the depth buffer, a slice at a time |
| R. Fernando, "Percentage-Closer Soft Shadows" (SIGGRAPH 2005 sketch) | `engine::shadow` (`Soft::Pcss`) | A blocker search, then a filter as wide as the sun's size makes the penumbra at the blockers' distance |
| G. Gribb and K. Hartmann, "Fast Extraction of Viewing Frustum Planes from the World-View-Projection Matrix" (2001) | `engine::cull` (`in_frustum`) | A frustum's planes as sums of a view-projection's rows, for the one bounds list's culling |
| Z. Majercik, J.-P. Guertin, D. Nowrouzezahrai and M. McGuire, "Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields" (JCGT 2019) | `engine::probes` | Probes traced again a few columns a frame and blended with the old by a hysteresis, so the sky's and the sun's changes reach them; each probe's sight (its rays' mean and squared hit distance per octahedral texel) and the Chebyshev and backface weights a lookup gives it (B9a) |
| B. Wronski, "Volumetric Fog: Unified Compute Shader-Based Solution to Atmospheric Scattering" (SIGGRAPH 2014 Advances); S. Hillaire, "Physically Based and Unified Volumetric Rendering in Frostbite" (SIGGRAPH 2015) | `engine::fog` | Froxels over the frustum, injected, integrated front to back (each slice's light through its own depth), jittered and reprojected; applied before the aerial table (B10) |
| W. D. Hillis and G. L. Steele, "Data Parallel Algorithms" (CACM 1986) | `engine::fog` (`integrate`) | The froxel column's front-to-back integral as a scan in workgroup memory, log2 of its slices steps |
| A. Vlachos, "Water Flow in Portal 2" (SIGGRAPH 2010) | `engine::water` (`advected`) | The ripples carried by the flow in two phases half a period apart, each started again when it weighs nothing |
| A. Estevez and C. Kulla, "Production Friendly Microfacet Sheen BRDF" (Sony Imageworks, 2017); M. Ashikhmin and S. Premoze, "Distribution-based BRDFs" (2007) | `engine::light` (`sheen_light`) | Cloth's sheen: Charlie's distribution and the visibility it's paired with |
| A. Sannikov, "Radiance Cascades: A Novel Approach to Calculating Global Illumination" (2023, github.com/Raikiri/RadianceCascadesPaper) | `engine::cascades` | Probes in cascades of intervals, each farther cascade's probes sparser and its directions finer; each interval's radiance and transmittance merged from the farthest down |
| D. P. Kingma and J. Ba, "Adam: A Method for Stochastic Optimization" (ICLR 2015); K. He, X. Zhang, S. Ren and J. Sun, "Delving Deep into Rectifiers" (ICCV 2015) | `engine::neural` | The small nets' optimizer and their weights' start |
| T. Zeltner and others, "Real-Time Neural Appearance Models" (ACM TOG 43(3), 2024); L. Belcour and A. Benyoub, "Hardware Accelerated Neural Block Texture Compression with Cooperative Vectors" (arXiv 2506.06040, 2025) | `engine::impostor` (`net_texel`), `studio/fit` | A small MLP evaluated in the shader; a learned grid of features decoded by it (the ladder's rungs C and D) |
| D. Craig, "Upgrading PSSR on PlayStation 5 Pro" (SIGGRAPH 2026 Advances) | `engine::temporal` (`learned_share`), `studio/fit` | Learning only the recognition the closed form gets wrong: the history's share, from the heuristics' inputs |
| J. C. Hart, "Sphere Tracing: A Geometric Method for the Antialiased Ray Tracing of Implicit Surfaces" (The Visual Computer, 1996) | `engine::distance`, `engine::probes` (`trace`) | A ray stepping as far as the distance to the nearest surface says nothing can be met |
| Infinity Ward, variable-rate ray tracing (SIGGRAPH 2026, §9 of docs/plan-world-class-2.md) | `engine::probes` (`next_strip`) | A fixed count of rays steered to where the image changed: the strip the sun or the sky changed most, nearest what the eye sees, traced again first (B9c) |
| O. Olsson, M. Billeter and U. Assarsson, "Clustered Deferred and Forward Shading" (HPG 2012) | `engine::lights` | Local lights binned into froxels; a pixel reads its froxel's list |
| M. McGuire and M. Mara, ["Efficient GPU Screen-Space Ray Tracing"](https://jcgt.org/published/0003/04/04/) (JCGT 2014); B. Karis, "Real Shading in Unreal Engine 4" (SIGGRAPH 2013) | `engine::reflect` | A screen-space march for glossy reflections and contact shadows; a specular cube prefiltered by GGX importance samples |
| J. Jimenez, "Next Generation Post Processing in Call of Duty: Advanced Warfare" (SIGGRAPH 2014) | `engine::paint` | The bloom pyramid: 13 taps down, a tent filter up |
| M. McGuire and L. Bavoil, "Weighted Blended Order-Independent Transparency" (JCGT 2013); N. Satish, M. Harris and M. Garland, "Designing Efficient Sorting Algorithms for Manycore GPUs" (IPDPS 2009) | `engine::particles` | Veils composited without sorting; particles sorted by depth with a two-pass radix sort |
| J. M. P. van Waveren, "Real-Time DXT Compression" (Intel 2006); J. M. P. van Waveren and I. Castaño, "Real-Time YCoCg-DXT Compression" (2007) | `engine::compress` | Textures compressed to BC blocks on the GPU as they're made |
| V. Chlumský, "Shape Decomposition for Multi-channel Distance Fields" (Master's thesis, CTU Prague, 2015; msdfgen, MIT) | `ui::ttf`, `ui::text` | Glyphs as three-channel distance fields, the median of which keeps their corners sharp at any size |
| Heimsath, Dietrich, Nishiizumi and Finkel, "The soil production function and landscape equilibrium" (*Nature* 1997) | `land::soil` | Rock weathers fastest under thin soil, so crests are thin and hollows deep |
| J. Braun and S. D. Willett, an O(n) implicit stream power solver (*Geomorphology* 2013) | `land::landform` | Channels cut at K A^m S, stable for any step |
| R. Barnes, C. Lehman and D. Mulla, "Priority-flood" (*Computers & Geosciences* 2014), the epsilon variant | `land::landform` | Every cell drains |
| F. K. Musgrave, C. E. Kolb and R. S. Mace, eroded fractal terrains (SIGGRAPH 1989) | `land::landform` | Slopes past their rock's threshold shed downhill, so hard caps stand in cliffs |
| P. Davy and D. Lague, erosion and deposition (*JGR* 2009) | `land::landform` | What water cuts it lays down where its load is heavy, so valleys fill and fans spread |
| J. J. Roering, J. W. Kirchner and W. E. Dietrich, nonlinear hillslope creep (*WRR* 1999) | `land::landform` | Hillsides run straight from rounded crests |
| D. R. Montgomery and W. E. Dietrich, "Where do channels begin?" (*Nature* 1988) | `land::landform` | Water cuts only where enough gathers |
| G. Cordonnier and others, terrain from tectonic uplift and fluvial erosion (*CGF* 2016) | `games/last-green/geology.wrela` | The world is authored through its causes (rock, old land, base level), not by drawing valleys |
| G. Chaikin, corner cutting (1974) | `games/last-green/geology.wrela` | A stream's course, smoothed |
| K. Beven and M. Kirkby, the topographic wetness index (1979) | `games/last-green/geology.wrela` | How wet the land is; the wood grows by it |
| J. H. von Thünen's land-use rings (1826); settlement siting from human geography | `games/last-green/peoples.wrela` | Where the elves settle, farm, graze and cut wood, and when they found a new settlement |
| H. Hediger's flight distance (1950) | `games/last-green/behaviour.wrela` | The fawn watches, then flees inside a distance. The distances are ours. |
| C. Ericson, *Real-Time Collision Detection* (2005), §5.1.9 | `content/climber/fight.wrela` | Closest points of two segments, for the sword and the bite |
| M. Müller and others, "Position Based Dynamics" (2007); T.-Y. Kim, N. Chentanez and M. Müller-Fischer, long-range attachments (2012) | `content/climber/cape.wrela` | The cape that can't stretch |
| B. T. Lowerre, beam search (*The Harpy Speech Recognition System*, 1976) | `games/capybara/route/search.wrela` | The fastest-route search |
| Inigo Quilez's articles: [intersectors](https://iquilezles.org/articles/intersectors/), [2D distance functions](https://iquilezles.org/articles/distfunctions2d/), [ellipse distance](https://iquilezles.org/articles/ellipsedist/), [smooth minimum](https://iquilezles.org/articles/smin/), [soft shadows](https://iquilezles.org/articles/rmshadows/), [occlusion](https://iquilezles.org/articles/nvscene2008/) (MIT) | `console/`, `ui/draw.wrela`, `docsite/` | Ray–capsule and ray–round-cone tests, the ellipse, diamond and triangle distances, smooth joins, soft shadows and ambient occlusion; the soft shadow's penumbra in `engine::distance::contact_through` |
| K. Narkowicz's fit of the ACES filmic curve; quaternion slerp; separable Gaussian bloom | `console/` | The opening's tone, rotation and glow |
| P. Felzenszwalb and D. Huttenlocher, ["Distance Transforms of Sampled Functions"](https://theoryofcomputing.org/articles/v008a019/) (2012) | `ui/raster.wrela` | A system glyph's distance field |
| Y.-D. Liang and B. A. Barsky, line clipping (*ACM TOG* 1984) | `examples/board` | A connector's route tested against items |
| [ttf-parser](https://github.com/RazrFalcon/ttf-parser) (MIT or Apache-2.0) | `ui::ttf` | How TrueType's tables are read, and their fallbacks |
| Lucide's settings-2 icon (ISC) | `console/` | Recreated from its 24-unit geometry |
| [Tailwind CSS's zinc palette](https://tailwindcss.com/docs/colors) (MIT) | `docsite/` | The docs' neutral greys |
| Z. H. Cigolle, S. Donow, D. Evangelakos, M. Mara, M. McGuire and Q. Meyer, ["A Survey of Efficient Representations for Independent Unit Vectors"](https://jcgt.org/published/0003/02/01/) (*JCGT* 2014) | `engine::substance` | The substance's normals and principal direction in octahedral coordinates |
| D. DeCarlo, A. Finkelstein, S. Rusinkiewicz and A. Santella, ["Suggestive Contours for Conveying Shape"](https://gfx.cs.princeton.edu/proj/sugcon/) (SIGGRAPH 2003); T. Judd, F. Durand and E. Adelson, ["Apparent Ridges for Line Drawing"](https://people.csail.mit.edu/tjudd/apparentridges.html) (SIGGRAPH 2007) | `engine::marks::creased`, `styles::cel` | Lines where the form folds, from its curvature: a crease where the larger principal curvature per pixel is high and long enough along its ridge |
| P. Bénard, A. Bousseau and J. Thollot, ["State-of-the-Art Report on Temporal Coherence for Stylized Animations"](https://doi.org/10.1111/j.1467-8659.2011.02075.x) (*Computer Graphics Forum* 2011) | `engine::marks` | Marks anchored in 3D and sized in 2D, faded across a lattice's levels: the compromise between the shower door and sliding it recommends |
| E. Praun, H. Hoppe, M. Webb and A. Finkelstein, ["Real-Time Hatching"](https://hhoppe.com/proj/hatching/) (SIGGRAPH 2001) | `styles::ink` | Tonal art maps: a tone is a count of hatching's layers, each coming in past a threshold, coherent as the surface nears and recedes |
| Native Plant Trust, Go Botany, [*Fagus sylvatica*](https://gobotany.nativeplanttrust.org/species/fagus/sylvatica/) (European beech; after *Flora Novae Angliae*) | `engine::plant::VEINS` | A beech leaf's secondary veins: 4 to 9 pairs (the American beech's 9 to 14) |
| N. Hansen, ["The CMA Evolution Strategy: A Tutorial"](https://arxiv.org/abs/1604.00772) (2016) | `studio/toolkit/cma.wrela` (the looks' fit, `studio/fit`; the trees', `studio/grow`; the gaits', `studio/gait`) | The fit's search: (μ/μ_W, λ)-CMA-ES with rank-one and rank-μ updates and cumulative step-size adaptation, its default parameters |

### The grazer's genome

The vale's creature's numbers (`kit::genome::ROE`) and the methods that grow it (docs/plan-world-class-3.md §F1–F5). Numbers not from these are marked **proposed** where they're set.

| Source | Where | What it gives |
|---|---|---|
| [Animal Diversity Web: *Capreolus capreolus*](https://animaldiversity.org/accounts/Capreolus_capreolus/) | `kit::genome::ROE` | A roe deer's height at the withers (66–83 cm), its grown mass (22.6–30 kg), and a fawn's 60–70 % of it by its first autumn |
| [GBIF: *Capreolus capreolus*](https://gbif.org/species/195657926) | `kit::genome::ROE` | A fawn's mass at birth (1.3–1.9 kg) |
| J. S. Huxley, *Problems of Relative Growth* (1932) | `kit::genome::quadruped_at` | Allometric growth: each part's size y = b·x^k against the body's mass, a fawn's limbs and head large for its body |
| A. M. Turing, "The Chemical Basis of Morphogenesis" (*Phil. Trans. R. Soc. B* 1952); J. E. Pearson, ["Complex Patterns in a Simple System"](https://doi.org/10.1126/science.261.5118.189) (*Science* 1993); J. D. Murray, *Mathematical Biology II*, ch. 3 | `kit::coat` | The coat's Gray-Scott reaction-diffusion, its spots' feed and kill rates (Pearson's), and its wavelength scaled to the body by the diffusion rates (Murray's size-to-pattern result) |
| R. McN. Alexander, *Principles of Animal Locomotion* (Princeton 2003) | `kit::muscles` | Muscles sized at a constant stress: a section by the peak torque it bears over its moment arm |
| R. A. Mendez and A. Keys, "Density and composition of mammalian muscle" (*Metabolism* 1960) | `kit::muscles::MUSCLE_DENSITY` | Muscle's density, 1.06 g/cm³ |
| P. B. Stark and R. L. Parker, ["Bounded-Variable Least-Squares: an Algorithm and Applications"](https://www.stat.berkeley.edu/~stark/Preprints/bvls.pdf) (*Computational Statistics* 1995) | `engine::loads::allocate` | The feet's forces: a bounded least squares by an active set (the vertical forces non-negative, the horizontals free) |
| R. Featherstone, *Rigid Body Dynamics Algorithms* (Springer 2008) | `engine::loads::joint_loads` | Inverse dynamics: each joint's torque by Newton–Euler over its subtree's masses |
| M. Hildebrand, ["Symmetrical Gaits of Horses"](https://doi.org/10.1126/science.150.3697.701) (*Science* 1965) | `kit::genome::ROE`'s gait ranges | The hind legs' lag: a walk's limb phase kept among the lateral-sequence walks (a tenth to two fifths of a cycle), short of a trot's half |
| D. M. Loscher, F. Meyer, K. Kracht, J. A. Nyakatura, ["Timing of head movements is consistent with energy minimization in walking ungulates"](https://doi.org/10.1098/rspb.2016.1908) (*Proc. R. Soc. B* 283: 20161908, 2016) | `engine::gait::nodded`, `kit::gaits::NOD` | A walking ungulate's head oscillates twice a stride, highest in its forelimbs' double support and lowest in their single support, out of phase with its thorax; a warmblood's head ranges 9.1 cm and its thorax 3.2 cm |
| T. Geijtenbeek, M. van de Panne and A. F. van der Stappen, ["Flexible Muscle-Based Locomotion for Bipedal Creatures"](https://doi.org/10.1145/2508363.2508399) (SIGGRAPH Asia 2013) | `kit::gaits` | The gait's objective's shape: effort, stability and speed weighed, searched by CMA-ES (ours a kinematic walker's, its effort a work proxy from the poses' loads) |
| F. Thomas and O. Johnston, *The Illusion of Life: Disney Animation* (Abbeville 1981) | `engine::motion`, the looks' motion constants (`content/styles`) | The stylizer's levers: anticipation, follow-through, exaggeration and timing on twos, from the twelve principles |
| R. McN. Alexander and A. S. Jayes, ["A dynamic similarity hypothesis for the gaits of quadrupedal mammals"](https://doi.org/10.1111/j.1469-7998.1983.tb04278.x) (*Journal of Zoology* 1983) | `kit::roe::walk_at` | A fawn's walk from the grown roe's: speed by the root of the legs' length, cadence by its inverse |

## Read for principles

These taught what natural forms and good renderers do. Nothing was copied, and none is implemented whole.

| Source | What it taught |
|---|---|
| Runions, Lane and Prusinkiewicz, [space colonization](https://algorithmicbotany.org/papers/colonization.egwnp2007.html) (2007); Palubicki and others, [self-organizing trees](https://algorithmicbotany.org/papers/selforg.sig2009.html) (2009) | Branch taper, the pipe model, competition for light: uneven crowns with gaps |
| Kharlamov, Cantlay and Stepanenko, [GPU Gems 3, ch. 4: SpeedTree rendering](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-4-next-generation-speedtree-rendering) (2007) | Trunk silhouettes, leaf transmission, and why cutouts shimmer |
| USDA Forest Service FEIS (Arizona fescue), PNW-GTR-817 (oxeye daisy) and Kew's Digitalis account | Basal foliage apart from a thin flowering stem; grass leaf sizes |
| USDA NRCS stream restoration guide (2012) and Stream Visual Assessment Protocol 2 (2009) | Pools, riffles, bars and uneven banks |
| WMO International Cloud Atlas and NOAA's glossary on cumulus; A. Schneider, [Nubis](https://www.guerrilla-games.com/read/nubis-authoring-real-time-volumetric-cloudscapes-with-the-decima-engine) (SIGGRAPH 2017) | Flat grey bases and lit relief; density profiles apart from edge erosion |
| US National Park Service on weathering and monuments | How rock splits and masonry weathers |
| Koichi Hayashida (Nintendo), interviewed by Christian Nutt: [The Structure of Fun: Learning from Super Mario 3D Land's Director](https://www.gamedeveloper.com/design/the-structure-of-fun-learning-from-i-super-mario-3d-land-i-s-director) (Gamasutra, 13 April 2012), and the four steps it is known for (introduce, develop, twist, conclude: kishōtenketsu) | A level introduces a thing alone before it combines it with others: the capybara game's checks (games/capybara/route/design.wrela) that a lesson has nothing else to manage and that a new foe comes alone |

## Programs the tools run

None is vendored or in a game. Each is a granted program in `wrela.toml`, found on `PATH`.

| Program | Rights | Use |
|---|---|---|
| [sfizz](https://sfz.tools/sfizz/) `sfizz_render` | BSD 2-clause | Plays the reference piano for the listening tests |
| [FFmpeg](https://ffmpeg.org/) with x264 | GPL | Makes the motion tool's clips from frames |
| [Wrangler](https://developers.cloudflare.com/workers/wrangler/), through `npx` | MIT or Apache-2.0 | Deploys releases to Cloudflare |

## Style references

Named in words; no image is copied from them.

| Influence | What it stands for |
|---|---|
| E. H. Shepard's drawings for Winnie-the-Pooh | Storybook softness and texture |
| Anime | Shapes, characters, stylized faces |
| *The Legend of Zelda: Breath of the Wild* (Nintendo) | Landform, distance, haze |
| Studio Ghibli's films, Kazuo Oga's backgrounds | Painted light and colour in forests and fields |
| *Clair Obscur: Expedition 33* (Sandfall Interactive) | Ryan's comparison for the lore: a world as a dying, preserved piece of itself |
| A grim reaper; the mood of a dementor | Ryan's words for the climber, an original character |
| Miro and Lucid; shadcn/ui's documentation | Interaction conventions for the board; the polish the docs aim past |

The lookdev rubric's "hero" anchor is a private set of 75 images, not in the repo: 23 public-domain paintings from The Met's Open Access (CC0) and 52 official Studio Ghibli stills ([free to use](https://www.ghibli.jp/info/013344/) "within the bounds of common sense", Ghibli's statement of September 2020). Ryan accepted that statement on 2026-10-28 as covering private review by an agent judge (docs/plan-world-class-3.md §B3.1): it is the rights holder's own public invitation, though not a formal licence, and it covers only stills from Ghibli's own release, not reposts. An agent looks at them beside a render to judge light, colour, value and depth. Eight Nintendo store screenshots that were in it were removed on 2026-10-28: their terms allowed private study only, not showing them to an agent judge (docs/plan-world-class-3.md D38).

## The fit's reference sets

The paintings and prints a look is fitted to (`wrela tool fit --style`, docs/plan-world-class-3.md
§D8): CC0 (public domain) in The Met's Open Access, each its museum's record (`isPublicDomain`
true), kept outside the repo under `$WRELA_REFERENCE_DIR/styles/<set>/<id>.jpg` with its record
(`<id>.json`), fetched from its API's web-size image (`primaryImageSmall`) on 2026-10-30. Only their
statistics are read (luma quintiles, edges, palette size, chroma); none is copied into wrela. The
judge's hero set (above) isn't on this machine, so neither set was checked against its 23
paintings.

| Set | Work | The Met |
|---|---|---|
| cel | Katsushika Hokusai, *South Wind, Clear Sky (Gaifū kaisei), also known as Red Fuji, from the series Thirty-six Views of Mount Fuji (Fugaku sanjūrokkei)* (ca. 1830–32) | [36490](https://www.metmuseum.org/art/collection/search/36490) |
| cel | Katsushika Hokusai, *Storm below Mount Fuji (Sanka no haku u), from the series Thirty-six Views of Mount Fuji (Fugaku sanjūrokkei)* (ca. 1830–32) | [36492](https://www.metmuseum.org/art/collection/search/36492) |
| cel | Utagawa Hiroshige, *Sudden Shower at Shōno, from the series Fifty-three Stations of the Tōkaidō* (1834–35) | [36521](https://www.metmuseum.org/art/collection/search/36521) |
| cel | Utagawa Hiroshige, *The Kiso Mountains in Snow* (dated 8th month of the Snake year, 1857) | [36545](https://www.metmuseum.org/art/collection/search/36545) |
| cel | Utagawa Hiroshige, *A Snowy Evening at Kanbara Station* (ca. 1833–34) | [36937](https://www.metmuseum.org/art/collection/search/36937) |
| cel | Utagawa Hiroshige, *Spring Rain at Tsuchiyama, from the series Fifty-three Stations of the Tōkaidō* (1834–35) | [36971](https://www.metmuseum.org/art/collection/search/36971) |
| cel | Utagawa Hiroshige, *Okabe* (ca. 1842) | [37027](https://www.metmuseum.org/art/collection/search/37027) |
| cel | Utagawa Hiroshige, *“Azuma Bridge from Komagatadō Temple,” from the series One Hundred Famous Views of Edo (Meisho Edo hyakkei, Komagatadō Azumabashi)* (1857) | [37282](https://www.metmuseum.org/art/collection/search/37282) |
| cel | Katsushika Hokusai, *Reflection in Lake at Misaka in Kai Province (Kōshū Misaka suimen), from the series Thirty-six Views of Mount Fuji (Fugaku sanjūrokkei* (ca. 1830–32) | [54868](https://www.metmuseum.org/art/collection/search/54868) |
| cel | Utagawa Hiroshige, *Sudden Shower over Shin-Ōhashi Bridge and Atake (Ōhashi Atake no yūdachi), from the series One Hundred Famous Views of Edo (Meisho Edo hyakkei)* (1857) | [55433](https://www.metmuseum.org/art/collection/search/55433) |
| cel | Katsushika Hokusai, *Noboto Bay (Noboto no ura), from the series Thirty-six Views of Mount Fuji (Fugaku sanjūrokkei)* (ca. 1830–32) | [56216](https://www.metmuseum.org/art/collection/search/56216) |
| cel | Utagawa Hiroshige, *Snowy Gorge* (undated) | [56683](https://www.metmuseum.org/art/collection/search/56683) |
| cel | Katsushika Hokusai, *Kajikazawa in Kai Province (Kōshū Kajikazawa), from the series Thirty-six Views of Mount Fuji (Fugaku sanjūrokkei)* (ca. 1830–32) | [56727](https://www.metmuseum.org/art/collection/search/56727) |
| cel | Utagawa Hiroshige, *Station Thirty-Eight: Fujikawa, Scene at the Border, from the Fifty-Three Stations of the Tokaido* (ca. 1833–34) | [56906](https://www.metmuseum.org/art/collection/search/56906) |
| painterly | Camille Corot, *River with a Distant Tower* (1865) | [435983](https://www.metmuseum.org/art/collection/search/435983) |
| painterly | Charles-François Daubigny, *Landscape with a Sunlit Stream* (ca. 1877) | [436090](https://www.metmuseum.org/art/collection/search/436090) |
| painterly | Camille Pissarro, *Barges at Pontoise* (1876) | [437302](https://www.metmuseum.org/art/collection/search/437302) |
| painterly | Camille Pissarro, *Poplars, Eragny* (1895) | [437307](https://www.metmuseum.org/art/collection/search/437307) |
| painterly | Camille Pissarro, *Morning, An Overcast Day, Rouen* (1896) | [437308](https://www.metmuseum.org/art/collection/search/437308) |
| painterly | Alfred Sisley, *The Bridge at Villeneuve-la-Garenne* (1872) | [437680](https://www.metmuseum.org/art/collection/search/437680) |
| painterly | Alfred Sisley, *View of Marly-le-Roi from Coeur-Volant* (1876) | [437682](https://www.metmuseum.org/art/collection/search/437682) |
| painterly | Alfred Sisley, *Sahurs Meadows in Morning Sun* (1894) | [437683](https://www.metmuseum.org/art/collection/search/437683) |
| painterly | Alfred Sisley, *The Road from Moret to Saint-Mammès* (1883–85) | [437684](https://www.metmuseum.org/art/collection/search/437684) |
| painterly | Alfred Sisley, *The Road from Versailles to Louveciennes* (probably 1879) | [437685](https://www.metmuseum.org/art/collection/search/437685) |
| painterly | Alfred Sisley, *Rue Eugène Moussoir at Moret: Winter* (1891) | [437686](https://www.metmuseum.org/art/collection/search/437686) |
| painterly | Camille Pissarro, *Haystacks, Morning, Eragny* (1899) | [438738](https://www.metmuseum.org/art/collection/search/438738) |
| painterly | Alfred Sisley, *Allée of Chestnut Trees* (1878) | [459121](https://www.metmuseum.org/art/collection/search/459121) |
| painterly | Alfred Sisley, *The Seine at Bougival* (1876) | [901617](https://www.metmuseum.org/art/collection/search/901617) |
