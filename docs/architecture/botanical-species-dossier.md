# Botanical reference dossier

Reviewed September 23, 2026. This anchors anatomical constraints and identifies missing calibration data. It does not certify developmental accuracy. We link reference publications; no reference photographs or third-party textures are redistributed.

## Rocky Mountain lodgepole pine

Taxon: *Pinus contorta* var. *latifolia*. The [Flora of North America treatment](https://www.efloras.org/florataxon.aspx?flora_id=1&taxon_id=233500929) describes a generally conic crown, mostly horizontal main branches without ascending tips, needles usually 5–8 cm long and 1–2 mm wide (with exceptions), and bark separating into loose plates rather than prominent furrows. Its [species treatment](https://efloras.org/florataxon.aspx?flora_id=1&taxon_id=233500927) specifies two needles per fascicle, twist, and multi-year retention. These are descriptive taxonomic ranges, not probability distributions or measured cohort covariance. Reference text remains owned by its publisher.

The [USDA Silvics chapter](https://research.fs.usda.gov/silvics/lodgepole-pine) supports low shade tolerance and strong competition effects. Those observations justify causal responses, not our numeric resource coefficients.

Implementation consequences: persistent paired attachments; development-generated counts are even; needle dimensions no longer shrink merely because a shoot is short; juvenile and adult shoot lengths vary separately from needles. Uniform light provides no directional tropism cue. Main limbs relax toward a near-horizontal orientation. Plate scale and branch inclination still require close image review and measured distributions. Cone development is not implemented.

## Paper birch

Taxon: *Betula papyrifera*. The [USDA Silvics chapter](https://research.fs.usda.gov/silvics/paper-birch) describes a shade-intolerant pioneer, drought-related leaf loss, declining growth with age, and young bark remaining golden/brown before whitening. The reported whitening age cannot be transferred to our uncalibrated growth steps. The [Flora of North America key](https://www.efloras.org/florataxon.aspx?flora_id=1&taxon_id=103887) distinguishes paper birch by leaf bases, vein counts and mature exfoliating bark. The [University of Florida species sheet](https://hort.ifas.ufl.edu/trees/BETPAPA.pdf) gives a 2–4 inch leaf-blade length range.

Experimental solver implementation: separate petioles, broad serrated blades, deciduous cohort renewal, and brown small twigs transitioning toward the mature bark appearance. The diameter-based bark transition is provisional, not a fitted age/diameter relationship. Short shoots attach proximally, make one approximately 2.5 mm extension, and renew three leaves; subsequent short-shoot extension and release into long shoots are absent. Old long shoots no longer retain their previous foliage. Resource coefficients and the reduction in leader extension are uncalibrated. The user rejected the rendered proportions and sparse crown; this solver is parked pending a convincing architectural reference.

The primary study [Clausen and Kozlowski, 1965](https://www.nature.com/articles/2051030a0) reports contrasting long and short shoots in paper birch, with short internodes and early leaves. This supports separate shoot habits; it does not supply our probability or resource coefficients. [Caesar's comparative morphology study](https://thesis.lakeheadu.ca/bitstream/handle/2453/854/CaesarJ1983m-1b.pdf?isAllowed=y&sequence=1) describes foliage-bearing first-year long shoots and persistent short shoots. We have not calibrated to a specimen dataset from either publication.

## Calibration protocol and current gaps

Keep the existing 32 seeds × four environments × four developmental stages per species as an implementation study. Preserve rejected model revisions and distinguish graph descriptors from rendered geometry. A passing topology/resource sweep establishes deterministic valid structures only.

Before marking species traits calibrated, obtain independent, scale-bearing measurements with site and age context. Record specimen IDs, provenance, permission/license, measurement uncertainty and environment. Split by specimen/site before fitting; do not tune against held-out specimens. Fit branch-order lengths, branch inclinations, basal area versus height, live crown ratio, leaf area and retention jointly. Set each held-out tolerance from its measurement uncertainty before optimization. Compare descriptor distributions and covariance, plus native rendered specimens under multiple lights. Our chosen coefficients and staged tree dimensions are currently hypotheses; source publications do not supply enough data to claim that this calibration is complete.

No interval is labeled a year, no arbitrary threshold is presented as measured botany, and no appearance pass is inferred from structural tests.
