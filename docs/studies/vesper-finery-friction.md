# Vesper finery iteration

Initial evidence: `.soundstage/captures/vesper-craft-delivery-5c38f787.png`.
First integrated review: `.soundstage/captures/vesper-production-integrated-14a83104.png`.

| Wanted edit | Obstacle | Reusable capability / authored response | Rendered outcome |
| --- | --- | --- | --- |
| Give the mane a substantial, coherent flow with fine edges | The source could only add separate tubular fibres. Thick fibres read as wires; finer fibres exposed disconnected gaps between guides. | Optional `GroomDesign.envelope` compiles a tapered, flattened and fluted guide clump, with fine surface fibres and the original guide bindings. Existing sources retain their prior output when the option is absent. | Integrated render improved mass coverage, but broad identical swept wedges resemble a rake. This is not acceptable hair finish. |
| Make the ceremonial mantle read as constructed cloth | Uniform short-wave folds, a large repeated loop motif and dozens of identical tassels overwhelmed panel construction. | `ProcessionalFinery` authors three broad folds, a turned hem, paired border cords, two panel seams, four weighted corner tassels and one split-leaf crest per side. The field parameterization remains shared with simulation and skin projection. | Integrated render is quieter and its construction reads more clearly, but still resembles a neat saddle. Second pass deepens broad folds and lengthens/asymmetrizes the weighted hem; review pending. |
| Reproduce the groom study without overwriting unrelated edits | Direct asset editing would bypass revision checks and undo. | `Games/Sanctuary/Authoring/VesperFinery.py` applies only the two identified groom records in one public craft transaction. Publication remains a separate ordinary authoring action. | Run after rebuilding/reopening Soundstage; parent preserves the actual review study. |
| Let the new tapered groom hang naturally against body proxies | Existing Vesper tip proxies remain 31 cm for mane and 15 cm for beard, wider than the new visible tapered tips. | `VesperFinery.py --contacts-only` fits conservative sampled bind-curve envelopes to the current source and replaces only groom guide radii through one revision-checked `author` call. | Awaiting surface/contact and motion review; pinned attachments may overlap intentionally. |
| Break the combed rake silhouette into a falling ruff | All sixteen authored guides swept backward at similar lengths; shallow taper and flattened cross-sections amplified the repeated wedges. | Existing guide offset and groom interfaces suffice: `VesperFinery.py --flow` authors unequal side locks falling beside the neck, shorter crown sweeps, rounder earlier-tapered masses and a shorter uneven beard. Compiler fibre/groove contrast is also reduced. | Awaiting actual second-pass render. |
| Apply the flow recipe after an added beard guide study | The initial content script assumed four beard guides; the current source had six and raised `IndexError` before submitting a transaction. | The content recipe now resamples its length rhythm for the current guide count and its curve for each guide's node count. | A dry-run command transport using the current 16/6 source confirms exactly one complete request with every non-root guide node covered. The failed earlier attempt changed no source. |

The dense clump is an opaque geometric approximation for unresolved internal
fibres. It does not implement volume hair scattering, strand self-contact or a
new dynamics model. Root and tip color, fibre radius, density, guide shape and
envelope shape are still authored source. Dynamics stays on the registered
guide nodes; no Vesper branch enters the shared compiler.

Validation: `swift test --filter GroomEnvelopeTests` passed both tests on
September 11, 2026. This covers finite generated geometry, normalized deterministic
guide bindings, dense root coverage and tip taper, rejection of invalid envelope
dimensions, and round-trip/legacy decoding. It does not assess artistic quality.

Primary technique context: NVIDIA's [Hair Animation and Rendering in the Nalu
Demo](https://developer.nvidia.com/gpugems/gpugems2/part-iii-high-quality-rendering/chapter-23-hair-animation-and-rendering-nalu-demo)
describes separating guide animation from dense rendered strands. This study
uses that separation; its opaque clump approximation is our own bounded
representation and should be judged by the actual Wrela renders.
