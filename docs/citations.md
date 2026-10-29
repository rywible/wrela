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
| Salamander Grand Piano V3 (a Yamaha C5), recorded by Alexander Holm; FreePats' retuned SFZ | CC BY 3.0; credit in `engine/piano_grand.wrela` | `engine::piano` is fitted to measurements of these recordings. The fitted numbers ship, not the recordings. The listening tests play the recordings beside wrela's piano, from a copy outside the repo. |
| Satie, Gymnopédie No. 1 (1888), the Mutopia Project's edition (Mutopia-2014/12/14-37) | Public domain | Transcribed into `examples/gymnopedie/score.wrela`, for wrela's piano to play. |
| Inter and JetBrains Mono (`ui/fonts/`); Manrope (`ui/brand/fonts/`, set at weight 500 with fontTools) | SIL OFL 1.1, licences beside them | The interface's text. |
| [ab_glyph](https://github.com/alexheretic/ab-glyph) 0.2 | Apache-2.0; used as a library | Drew `ui/tests/text/goldens/reference.png` once, from exact outline coverage, so the GPU's text can be compared with it. To draw it again, run `text::text_matches_a_reference_rasterizer` with `--ignored` (wrela-compiler, `compiler/tests/tests/suite/text.rs` at commit cc0be254). |

## Methods the code follows

Each is our own Wrela code written from the published idea. No code was copied.

| Source | Where | What it gives |
|---|---|---|
| A. Chaigne and A. Askenfelt, "Numerical simulations of piano strings" (JASA 1994); B. Bank and others, "Physics-based sound synthesis of the piano" (2000); P. J. Huber, robust regression (1964) | `engine::piano` | Each partial's frequency-dependent decay, fitted to each key by Huber-weighted least squares |
| G. Weinreich on coupled strings; H. Conklin on phantom partials | `engine::piano` | A unison's aftersound; partials from the string's stretch |
| J.-M. Jot and A. Chaigne, feedback delay networks (1991); J. Abel and P. Huang, echo density (2006); modal synthesis | `engine::piano` | The room (16-line FDN, Hadamard mix), its test's echo density measure, the soundboard's knock |
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

## Original physical grand and orchestra planning

The [original grand plan](plan-original-grand.md) defines an original physical instrument and
an autonomous evaluation procedure. Its sources R1–R9 are linked and scoped in the plan's
source register. These are planning references, not claims that the methods are implemented.

| Source | Use and rights boundary |
|---|---|
| Fazioli, [F308](https://www.fazioli.com/pianoforti/f308/) | Artistic benchmark only; no recordings, geometry or manufacturer parameters imported. The planned instrument is Wrela's own design. |
| Chabassier, Chaigne and Joly, [Time domain simulation of a piano (2014)](https://researchportal.ip-paris.fr/en/publications/time-domain-simulation-of-a-piano-part-1-model-description/); Bank and Sujbert, [Longitudinal String Vibrations (2004)](https://www.dafx.de/paper-archive/2004/P_089.PDF) | Published physical mechanisms inform the planned independent Wrela implementation. No source code, recording-derived tables or audio imported. |
| Matusiak, Chatziioannou and van Walstijn, [passive bow–string friction (2025)](https://pub.mdw.ac.at/pubmdw/publication/e0518e7b-5d7d-4ff0-aaf1-83f35848189e); Scavone, [reed generators](https://compmus.ime.usp.br/sbcm/1995/papers/Gary_Scavone.html); [valved brass synthesis (2015)](https://www.pure.ed.ac.uk/ws/portalfiles/portal/21857362/DAFx_15_harrison.pdf); Edinburgh NESS, [3D percussion](https://www.ness.music.ed.ac.uk/archives/systems/3d-embeddings.html) | Research directions for later orchestra scopes. No implementations, parameters or demonstration media imported. The bow article's record states CC BY 4.0; other linked material is cited for its methods, not treated as freely licensed assets. |

The existing Salamander attribution above remains accurate for the current piano. Replacing
that default is a future implementation step, and this plan does not change its licence or
claim that its derived tables are original.

The plan received a read-only review through Claude Code CLI 2.1.301 using Opus 5.5
(claude-opus-5-5, high effort) on the ThinkPad on 2026-10-29. Its findings and
the lead agent dispositions are recorded in the plan, section 12. No sound was
generated by a model; this was a text/code review of the execution plan.

### Original-grand mechanics implementation and private reviewer experiment

The new `engine/acoustic*.wrela` modules and `studio/acoustics` are independently written
from their documented equations and energy identities. The Chabassier/Chaigne/Joly source
above supplies physical context for strings, felt and structure. No implementation, recorded
signal, named piano scale, fitted library table or manufacturer parameter set was imported.
Current fixture dimensions/material coefficients are authored surrogate designs, not asserted
measurements of a particular alloy, wood species or concert piano.

The private local reviewer experiment uses [llama.cpp](https://github.com/ggml-org/llama.cpp)
(MIT) and [Qwen2.5-Omni-7B](https://huggingface.co/Qwen/Qwen2.5-Omni-7B)
([source Apache-2.0 licence](https://huggingface.co/Qwen/Qwen2.5-Omni-7B/blob/ae9e1690543ffd5c0221dc27f79834d0294cba00/LICENSE)).
The [GGUF conversion](https://huggingface.co/ggml-org/Qwen2.5-Omni-7B-GGUF) and runtime
revisions/hashes are recorded on the ThinkPad. They are installed outside this repository;
neither weights nor model-generated sound are shipped. The model only produced textual
critique, and its initial judgments were rejected after a silence-control failure. This
experiment does not establish a musically competent evaluator.
