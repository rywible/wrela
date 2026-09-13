# Contact-shadow projection experiment

Run from the repository root, outside a native performance measurement:

```sh
python3 Tools/Experiments/SanctuaryContactShadows/experiment.py \
  --output .build/sanctuary-native-20260912/contact-shadow-cpu
```

Standard-library Python only. It writes `results.json`, `contact-profiles.csv`
and `footprint.svg`. No Swift compilation, app launch, GPU access, save mutation
or source renderer modification occurs.

The source geometry is the current bench's two radius-55 mm capsules and two
boxes, transformed through its captured 90-degree yaw. The terrain is a local
plane fitted through the placement author's two source-triangle support samples;
the unsampled X slope is explicitly zero. The model uses the captured 36-degree
sun altitude/-35-degree azimuth and world-anchored 130 m / 2048 projection.

Each shadow texel analytically intersects those source primitives and the plane.
The depth model uses the actual receiver bias and nine-fetch paired 5×5 outdoor
comparison positions. Its raster slope comes from the analytic surface tangent,
with the existing clamp and a representative depth32Float ULP. This is a source
proxy: neither compiled caster triangles, GPU rounding, foliage, visual shading,
skin deformation nor the precise M4 rasterizer is reproduced.

Two single-factor diagnostic variants are modelled: 3×3 receiver-relative PCF
with unchanged bias, and unchanged PCF with raster slope scale zero. The model
also sweeps 16 texel phases and nine sun/terrain-slope pairs. Ground-plane checks
validate the model's derivative/comparison signs; they are not renderer tests.
Depth-cache capacity is 32,768 samples. `cpuSeconds` measures this experiment,
not the game's frame time. The SVG is a physical scale diagram, not a render.

See [the study](../../../docs/studies/SANCTUARY_CONTACT_SHADOWS.md) for evidence,
primary references, measured outputs, limitations and root's native A/B gate.

The CPU experiment is complete. The next artifact is the
[root-operated diagnostic packet](NATIVE_DIAGNOSTICS.md): separate A/B patches,
full source fingerprints, guarded apply/revert, and isolated native capture and
performance commands. No shared renderer source was changed by this packet.
