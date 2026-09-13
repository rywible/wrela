# Temporary ground contact source — 2026-09-12

Status: source candidate frozen for the root agent's combined build and native review. This report does not establish visual acceptance. No build, GPU workload, native app, or generated illustration was used by this author for this candidate.

## Ownership and production contract

`Games/Sanctuary/Project/GroundTracePresentation.swift` authors the meshes, cached support deformation, and saved-age recovery. `SanctuaryProject.swift` registers `ground-boot-trace` and `ground-soft-trace`; it also registers the separately authored `BiomeConiferDesign.generator` at the root agent's request. No vegetation source, engine renderer, shader, collision, or simulation file was changed in this task.

The root agent owns `SanctuaryExperience` integration and the content-owned supporting-soil query. The presentation entry point is:

```swift
func items(
  snapshot: SurfaceInfluenceSnapshot,
  playerPosition: V3,
  supportRevision: UInt64,
  support: (GroundInfluenceEvent, Float, Float) -> Support?
) -> [RenderItem]
```

`Support(height:strength:)` defaults to the authored muted mud RGB `(0.32, 0.29, 0.23)` and `.boot`; callers may explicitly choose `.softContact` and another soil color. The presentation has no source-ID or species switch. Current player `.foot` contacts can produce boots. Generic animal `.body` sweeps produce no imprints. The soft contact subject is an available authored shape, not a claim that current animal contacts are suitable paw events.

Only `.foot` events whose start/end differ by at most 2 cm are considered. This rejects legacy coalesced walking sweeps, which would otherwise slide a footprint through the ground. Production contact cadence, alternating feet, serialization, and soil eligibility remain content responsibilities. The intended support query rejects submerged terrain, built floors, unsuitable soil, and materially changed saved contact heights.

## Shape and recovery

The boot is a shallow rounded sole bowl with a narrower waist, broader forward toe, darker center, and displaced rim. At the default 0.13 m radius its footprint is approximately 0.30 m long and 0.14 m wide. The optional soft shape combines one soft pad and three smaller forward pads. Both use roughness 0.97, plain material 7, double-sided surfaces, and no cast shadows. There is no glow, object exposure compensation, texture dependency, or new shader.

These are thin relief meshes above existing terrain. They do **not** displace terrain, change collision, or constitute physical mud simulation. The uncompressed raised rim is approximately 14 mm high; the response scales that relief and gradually lowers it beneath the supporting surface. This is a rendering approximation whose final visible depth and soil integration require real native images.

Game and Soundstage call the same `recoveryPose(event:time:strength:)`, using `GroundInfluenceEvent.response(at:)` and saved `Double` time. Compression and supplied soil strength scale the response. The shared response recovers over its saved time scale, reaches zero at eight time scales, and contains no presentation clock accumulator. Backward time and explicit reset reconstruct the same pose. No test-only animation implementation or simulation mutation is introduced.

The registered `recover` and `settled` clips expose radius, compression, and recovery time scale. Studio time scale is bounded to 0.1–7 seconds so a complete eight-time-scale study fits within the studio duration limit; production events retain the shared event's wider allowed interval. Diagnostics expose elapsed milliseconds, shared response, source triangle count, and zero terrain displacement.

## Ground support and cache

Each accepted event samples the support callback on a 3×3 oriented grid. All nine samples must be finite, have positive soil strength, agree on shape, and lie within the event's saved support-height tolerance. A failed sample rejects the entire footprint. Nine affine tangent planes and bilinear skin weights conform the cached shape to those samples. A planar slope is represented exactly; curvature and narrow surface boundaries between samples remain approximations. Nine samples do not certify collision equivalence.

The cache compares the **full saved event** and `supportRevision`, including rejected results. It retains at most the nearest 64 active candidates within 28 m. An unchanged frame performs zero support queries; a fully accepted cold set costs at most 576 queries. Changed events invalidate only their own entry. A support-revision change or backward time clears the cache; explicit forward-time snapshot replacement must call `reset()`. The root's revision derives from relevant saved garden, construction, and effective water facts rather than advancing ecology time alone.

The cap is applied before support eligibility, so rejected nearby candidates can reduce the number of rendered distant traces. This bounds callback work as well as rendering. Cached terrain support is only as current as the content-provided revision.

## Geometry and upload budget

| Shape | Vertices | Triangles | Vertex/index/skin source buffers |
| --- | ---: | ---: | ---: |
| Boot | 241 | 432 | 28,320 bytes |
| Soft contact | 388 | 672 | 45,312 bytes |
| Both cached meshes | 629 | 1,104 | 73,632 bytes |

The byte counts use the existing 64-byte vertex, 32-bit index, and 32-byte four-weight skin representation. The constructor uploads each shared mesh once. Counts exclude meshlet metadata, two initial 80-byte instance buffers, visible-meshlet buffers, and Metal allocation overhead. There is no per-frame mesh compilation or vertex/index/skin-weight buffer upload.

At the maximum 64 rendered traces, there are 64 items and nine skin matrices per item: 576 matrices, or 36,864 bytes of logical palette data per frame. The current renderer binds the palette to both vertex and mesh stages in its meshlet path, producing **73,728 bytes of palette API binding payload**, plus **15,360 bytes of instance binding payload** across vertex/object/mesh stages. Indexed fallback binds 36,864 palette bytes and 5,120 instance bytes. These are source-derived API payload counts, not measured driver copies, DRAM traffic, or GPU time; common material/count/corrective bindings add overhead.

The worst source geometry is 27,648 triangles for 64 boots, or 43,008 for 64 soft shapes. Existing skinned rendering has no mesh LOD and bypasses meshlet frustum culling; the CPU distance and count limits therefore matter. Sixty-four independently supported items still have a real draw cost. Native profiling must distinguish steady cached frames from support invalidations and contact insertion.

## Required native review

1. Select Sanctuary and inspect `ground-boot-trace`, then `ground-soft-trace`, using quarter, above, and low side views at time 0. Use a close physical framing that resolves the millimetre rim.
2. Inspect default recovery at 0, 3, 9, and 24 seconds, and scrub backward to 0. Save/reopen the study and verify the same pose and diagnostics. Compare noon, overcast/rain, golden hour, and indoor lighting without changing object exposure.
3. Inspect a real eligible creek/wetland or rainforest soil contact from above and oblique views. Verify heading, stride spacing, support slope, and that the mesh does not float, form a bright outline, or look like a rigid slab.
4. Replay saved contacts after reopening the expedition and verify remaining age. Check submerged soil, built floors, and changed support heights reject traces. Generic body sweeps must produce no paw marks.
5. Observe diagnostics on an unchanged frame (zero support queries), a revision change (bounded cold queries), and a 64-contact workload. Record frame spikes separately from steady cost.

Root owns builds, tests, native controls, and serialized GPU validation. No visual approval is implied by the source budget or a successful build.

## Frozen fingerprints

SHA-256 at source freeze:

```text
6b552da457f5d84533deb94b226686cf82e9d3dd59d38b2ea62f8a85e9c35984  Games/Sanctuary/Project/GroundTracePresentation.swift
3095380f0fc140e8f779e02d8956a1dfd9f46322fffef11ede37e9aff46e0d36  Games/Sanctuary/Project/SanctuaryProject.swift
3e76ef6de32dbe6829d39783c2eb79a51a065484a8e2b57ba7348fb78f1024c6  Engine/FieldCore/GroundInfluence.swift
96340beb364be0ebeb8b5daf7e76a9e55cc4ec828f8ccb2a3bb4b47dc677d89c  Engine/FieldEngine/Resources/Surface.metal
1e1f2a798c3ffc3f417397362a8e603e23cde6972afe7963fa49e54870bb202e  Games/Sanctuary/Authoring/Materials.metal
```

The shader is owned by another agent and may advance after this freeze. Native review must capture the actual compiled candidate's full fingerprints rather than assume these source-time values are the final binary.
