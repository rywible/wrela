# Local ground response — September 12, 2026

## Concrete risk and decision

Player/animal contact must survive pause/replay without grass snapping or bending
on unrelated floors. Using the existing32×32 periodic10m wind grid would alias
small contacts and repeat them across world space. Per-blade loops over every actor
would multiply work by scene density. Chosen experiment: one derived128² grid over
32m at0.25m spacing, sourced from≤256 saved events, and fixed four-tap sampling at
blade roots. Scene, mesh and shadow vertices use one Astra-authored helper. The
field rejects incompatible support heights and never wraps at its edge.

The existing procedural mesh renderer remains primary. No second terrain renderer,
external asset pipeline, paid service or replacement simulation was introduced.
World FieldCore owns units, bounds, validation and raster math; Sanctuary owns
collision-resolved contact facts, timestamps and soil eligibility; project source
owns appearance. Footsteps are immutable alternating contacts every0.65m from
actual accepted walking paths. Continuous body sweeps serve grass independently.

## Bounded primary-source reading

AMD's [procedural grass article](https://gpuopen.com/learn/mesh_shaders/mesh_shaders-procedural_grass_rendering/)
uses generated blade patches, reduces distant blade count, and compensates coverage
with blade width and fractional transitions. This supports retaining our existing
mesh/grass representation while improving contact and later density LOD. Its RDNA
numbers/API are not M4/Metal evidence; no performance claims were transferred.

Sucker Punch's [VFX development account](https://blog.playstation.com/2021/01/12/how-stunning-visual-effects-bring-ghost-of-tsushima-to-life/)
reports separate broad/fine procedural gusts shared across foliage, plus character
and horse displacement with damped recovery. This supports separating global wind
from local contact, and testing the recovery shape rather than snapping to rest.
Our saved-event grid is a different implementation and does not reproduce their
particle system or establish equivalent appearance.

Sony Santa Monica's [interactive vegetation talk abstract](https://www.gdcvault.com/play/1026036/Interactive-Wind-and-Vegetation-in)
identifies spatial wind, contact settling and shadow proxies as separate concerns.
Only the public abstract was read; no unseen slides/video implementation is claimed.

## Experiment and acceptance

Eight FieldCore tests cover bounded cells, narrow-contact prefiltering, no periodic
wrap/recentering, support-height conflicts, large Double play age, ordering/replay,
atomic rejection and culling displacement. Game tests cover actual movement,
flight/rejection suppression, immutable feet, cadence restore, legacy fields and
history limits. Combined quick passed374 unit tests, boundaries and smoke in `20260912-191856-quick-f49bc`. New grass-contact studio hook postdates this run.

Native experiment next: Soundstage boot/soft-contact inspection; production walking
through grass/soft banks, paused matched captures before/recovery/restore; reject
water/building/changed-height supports. Profile short1080p live contact/no-contact
windows with normal clouds/wind/cache updates, preserving p95/max and CPU field
cost. Root serializes GPU use. Initial budgets≤0.4ms field CPU and≤0.5ms addedGPU
are targets, not measurements. Three32-byte-cell buffers allocate1,572,864bytes.

Limits: this is a bounded artistic contact response, not force-calibrated plant
mechanics or soil-volume simulation. One dominant support layer per cell may omit
a weaker simultaneous stacked-floor contact. Footprint relief uses cached skinned
surface meshes, not changed terrain collision. At most64nearby eligible fixed
prints render; temporary effects may be evicted by the bounded history. Actual
native appearance, recovery feel and live cost remain to be measured.
