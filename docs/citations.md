# Citations

*The outside works wrela uses: what each is, its rights, how we use it and why. Add a row in the change that starts to use one. Standard specs and vendor documentation that code follows are not listed.*

## The rules

- **Learn, don't copy.** A reference teaches light, colour, form, scale or composition. We don't trace it, sample it, or rebuild a specific shot, character or design from it.
- **No generative image, sound, music, voice or video model makes anything in wrela.** Every image and sound comes from wrela code. Ryan directs; Claude and Codex agents write the code.
- **Ship only what a licence allows, with its credit.** A reference file in the repo has its source, author and licence in the `references` package beside it. Work adapted from a CC BY source carries its credit into the game.
- **Style references stay private.** Images used only to judge the look are kept outside the repo.

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
| D. Goldberg, ["What Every Computer Scientist Should Know About Floating-Point Arithmetic"](https://docs.oracle.com/cd/E19957-01/806-3568/ncg_goldberg.html), Theorem 8 (ACM Computing Surveys, 1991; authorized reprint) | `studio/appearance/fixture/source.wrela` | Compensated-summation trial for spatial references; the emitted shader retained it but the measured AMD results were unchanged. The final reference accumulator uses exact sums of Q20 values instead. |
| W. D. Hillis and G. L. Steele, "Data Parallel Algorithms" (CACM 1986) | `engine::fog` (`integrate`) | The froxel column's front-to-back integral as a scan in workgroup memory, log2 of its slices steps |
| A. Vlachos, "Water Flow in Portal 2" (SIGGRAPH 2010) | `engine::water` (`advected`) | The ripples carried by the flow in two phases half a period apart, each started again when it weighs nothing |
| A. Estevez and C. Kulla, "Production Friendly Microfacet Sheen BRDF" (Sony Imageworks, 2017); M. Ashikhmin and S. Premoze, "Distribution-based BRDFs" (2007) | `engine::light` (`sheen_light`) | Cloth's sheen: Charlie's distribution and the visibility it's paired with |
| A. Sannikov, "Radiance Cascades: A Novel Approach to Calculating Global Illumination" (2023, github.com/Raikiri/RadianceCascadesPaper) | `engine::cascades` | Probes in cascades of intervals, each farther cascade's probes sparser and its directions finer; each interval's radiance and transmittance merged from the farthest down |
| D. P. Kingma and J. Ba, "Adam: A Method for Stochastic Optimization" (ICLR 2015); K. He, X. Zhang, S. Ren and J. Sun, "Delving Deep into Rectifiers" (ICCV 2015) | `engine::neural` | The small nets' optimizer and their weights' start |
| T. Zeltner and others, "Real-Time Neural Appearance Models" (ACM TOG 43(3), 2024); L. Belcour and A. Benyoub, "Hardware Accelerated Neural Block Texture Compression with Cooperative Vectors" (arXiv 2506.06040, 2025) | `engine::impostor` (`net_texel`), `studio/fit` | A small MLP evaluated in the shader; a learned grid of features decoded by it (the ladder's rungs C and D) |
| S. Bako et al., ["Kernel-Predicting Convolutional Networks for Denoising Monte Carlo Renderings"](https://la.disneyresearch.com/publication/deep-learning-denoising/) (ACM TOG 36(4), 2017) | `studio/reconstruct/edges/raw.wrela` (experimental kernel) | Predict positive scalar weights shared across RGB and normalize over observed samples; our own geometry-only temporal MLP, not their static denoiser or weights |
| K. Vaidyanathan et al., ["Random-Access Neural Compression of Material Textures"](https://research.nvidia.com/labs/rtr/neural_texture_compression/assets/ntc_small_size.pdf) (SIGGRAPH 2023) | External `appearance-grid-v1` development experiment, documented in `docs/neural-appearance.md` | Joint quantized feature-grid/decoder optimization; concatenated fine-grid corners and interpolated coarse features. Our own smaller per-view foliage trial; no copied code, weights or external training assets. |
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

The lookdev rubric's "hero" anchor is a private set of 83 images, not in the repo: 23 public-domain paintings from The Met's Open Access (CC0), 52 official Studio Ghibli stills ([free to use](https://www.ghibli.jp/info/013344/) "within the bounds of common sense"), and 8 Nintendo store screenshots (private study only). An agent looks at them beside a render to judge light, colour, value and depth.

## Neural reconstruction instrument

[Pharr, Jakob and Humphreys, *Physically Based Rendering*, 4th ed., chapter 8](https://pbr-book.org/4ed/Sampling_and_Reconstruction)
(2023) describes sampling a continuous radiance function and combining samples into pixels.
`engine::supersample` uses f32 sums of scene-linear RGB; `studio/reconstruct` offers a regular
spatial grid and a reference sampler that crosses spatial strata with all 64 shading phases.
The latter follows [§8.5, stratified sampling](https://pbr-book.org/4ed/Sampling_and_Reconstruction/Stratified_Sampler): sample inside regions and separate sampling dimensions. It uses our existing hash
for spatial offsets; no PBRT code was copied. Both accumulate before any develop curve. This is our small quadrature instrument, not PBRT code
or a claim that the existing engine's lighting is physically unbiased. The temporary diagnostic
view is explicitly `max(x,0)/(1+max(x,0))` at fixed exposure 1; its PSNR is not a perceptual gate.

The offline development-image evaluator uses [NVIDIA's FLIP 1.7](https://github.com/NVlabs/flip)
(BSD 3-Clause), installed as the external package `flip-evaluator==1.7`, with NumPy 2.2.6.
The named LDR-FLIP viewing conditions and float preprocessing are in
[neural-rendering-progress.md](neural-rendering-progress.md); no FLIP implementation is vendored.

The continuous-clip evaluator uses Zhang et al., [“The Unreasonable Effectiveness of Deep
Features as a Perceptual Metric” / LPIPS](https://github.com/richzhang/PerceptualSimilarity)
(CVPR 2018; implementation BSD 2-Clause). External package `lpips==0.1.4`, AlexNet metric
version 0.1 with learned calibration, runs on Pop!_OS's CPU using PyTorch 2.5.1+cpu and
torchvision 0.20.1+cpu. RGB is the named exposure-1 Reinhard view, encoded as sRGB and scaled
to [-1,1], without resizing. Effective weight hashes, software versions, controls and the
complete evaluator are embedded in each result. No package code or pretrained metric weights
are shipped in the engine. The pinned CPU combination follows [PyTorch's versioned installation
instructions](https://pytorch.org/get-started/previous-versions/#v251).


The external recurrent-training prototype uses [PyTorch 2.5.1's reverse automatic
differentiation](https://raw.githubusercontent.com/pytorch/pytorch/v2.5.1/docs/source/notes/autograd.rst)
to differentiate through the candidate's own frame history, and checks the smooth surrogate
against central finite differences. The prototype, dependency environment and artifacts
remain outside the tracked repository; the runtime model uses our Wrela implementation.
Device-specific sampler/half-target behavior is measured from independent AMD captures and
recorded in `build/neural-evidence/recurrent-gate-v1`, not assumed to be portable.

The external source-initialized volume experiment follows the general idea
of differentiable ray integration described by [Mildenhall et al., NeRF
(ECCV 2020)](https://www.matthewtancik.com/nerf) and compares that context with
[Fridovich-Keil, Yu et al., Plenoxels (CVPR 2022)](https://alexyu.net/plenoxels/).
Our implementation predicts unlit material moments from quantized features
initialized using Wrela's source geometry; it is neither paper's reproduction.
No paper implementation, dataset or pretrained weights are copied. See
[the actual experiment and limitations](neural-appearance.md#source-initialized-volume-field).
