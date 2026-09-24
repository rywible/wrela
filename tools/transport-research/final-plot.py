"""Measured research plots. The parameter images are not game screenshots."""
import json
from pathlib import Path
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.colors import LogNorm
import numpy as np

root = Path(__file__).resolve().parents[2]
docs = root / "docs/research"
data = json.loads((docs / "rendering-compiler-final-results.json").read_text())
gpu = data["gpu"]
bg, panel, ink, muted = "#101a25", "#162330", "#ecf1ed", "#a7bbc9"
green, gold, blue, red = "#62d9b2", "#eed394", "#78a8dc", "#e87974"
plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 10, "text.color": ink,
                    "axes.labelcolor": muted, "xtick.color": muted, "ytick.color": muted,
                    "axes.edgecolor": "#3b5061", "axes.facecolor": panel, "figure.facecolor": bg,
                    "savefig.facecolor": bg, "axes.titleweight": "bold"})
fig, axes = plt.subplots(2, 2, figsize=(14, 10), gridspec_kw={"hspace": .50, "wspace": .31})
fig.subplots_adjust(top=.86, bottom=.12, left=.075, right=.97)
fig.text(.075, .965, "COMPILE A BETTER INTEGRATION DOMAIN", size=24, weight="bold")
fig.text(.075, .925, "Final research pass · Apple M4 / WebGPU · 16,384 queries per kernel", size=12, color=muted)

ax = axes[0, 0]
case, truth = gpu["coherent"][1], data["accuracy"]["reports"][1]
labels = {"regular64": ("64 regular", gold, (-62, -16)), "regular256": ("256 regular", gold, (-78, 10)),
          "pole4": ("4 rejection", red, (6, 10)), "warp4": ("4 warped", green, (8, 8)),
          "warp8": ("8 warped", green, (8, -12)), "fixed4": ("4 fixed warp", blue, (-35, -8))}
for key, (name, color, offset) in labels.items():
    x, y = case["times"][key]["median"] * 1000, truth["errors"][key]["rms"]
    ax.scatter(x, y, color=color, s=65, zorder=5)
    ax.annotate(name, (x, y), xytext=offset, textcoords="offset points", color=color, size=9)
ax.set(xscale="log", yscale="log", xlim=(6, 350), ylim=(2e-6, .3),
       xlabel="GPU microseconds  → slower", ylabel="RMS radiance error  → worse")
ax.set_title("Moving view and light · roughness 0.06", loc="left", pad=14)
ax.grid(alpha=.12)
ax.text(.04, .94, "4 warped: 21.8× faster; 52.7× less error\nComparison: 256 regular phase samples", transform=ax.transAxes, va="top", size=9)

ax = axes[0, 1]
sun = gpu["sun"]
keys = ["rays9", "rays64", "rays256", "planar", "lens"]
errors = [sun["errors"][k]["fraction"]["rms"] for k in keys]
ax.bar(range(5), errors, color=[gold, gold, gold, blue, green])
ax.set(yscale="log", ylim=(7e-7, 1), xticks=range(5), xticklabels=["9 sun\nrays", "64 sun\nrays", "256 sun\nrays", "Planar\nlens", "Spherical\nlens"], ylabel="RMS visible-sun fraction error")
ax.set_title("Soft sun visibility · one spherical occluder", loc="left", pad=14)
ax.text(.04, .95, "Spherical lens: 4.10 µs including first moments\n256 rays: 46.08 µs · planar lens: 3.07 µs", transform=ax.transAxes, va="top", size=9)
ax.grid(axis="y", alpha=.12)

ax = axes[1, 0]
keys = ["regular256", "warp4", "warp8"]
x = np.arange(3)
for j, (key, label, color) in enumerate(zip(keys, ["256 regular", "4 warped", "8 warped"], [gold, green, blue])):
    vals = [d["errors"][key]["rms"] for d in data["stress"]["reports"]]
    ax.bar(x + (j - 1) * .23, vals, width=.22, color=color, label=label)
ax.set(yscale="log", ylim=(8e-7, .4), xticks=x, xticklabels=["Moderate", "Nearly collinear", "Grazing / steep"], ylabel="CPU RMS radiance error")
ax.set_title("Adversarial domains determine the fallback", loc="left", pad=14)
ax.legend(frameon=False, labelcolor=ink, fontsize=9, loc="upper left")
ax.text(.97, .94, "Thin orbit:\nsparse warp fails", transform=ax.transAxes, color=red, size=9, ha="right", va="top", bbox={"facecolor": panel, "edgecolor": "none", "alpha": .95})
ax.grid(axis="y", alpha=.12)

ax = axes[1, 1]
rough = [c["roughness"] for c in gpu["coherent"]]
for key, color, label in [("regular256", gold, "256 regular"), ("warp4", green, "4 warped"), ("fixed4", blue, "4 fixed warp")]:
    values = [c["errors"][key]["rms"] for c in data["accuracy"]["reports"]]
    ax.plot(rough, values, "o-", color=color, label=label)
ax.set(yscale="log", xticks=rough, ylim=(1e-8, .15), xlabel="Perceptual roughness", ylabel="RMS radiance error")
ax.set_title("Use regular quadrature for broad lobes", loc="left", pad=14)
ax.legend(frameon=False, labelcolor=ink, fontsize=9, loc="upper right")
ax.grid(alpha=.12)
fig.text(.075, .055, "Linear, unclipped radiance errors against independent double-precision integration. Complete coherent phase orbits only.\nLocal kernels are not full-frame speedups. Small timing differences approach the 1.024 µs timestamp quantum.", color=muted, size=10)
fig.savefig(docs / "rendering-compiler-final-evidence.png", dpi=170)
fig.savefig(docs / "rendering-compiler-final-evidence.svg")

images = json.loads((root / "output/transport-research/coherent-gpu-0.06.json").read_text())
reference = np.array(json.loads((root / "output/transport-research/coherent-reference-0.06.json").read_text())).reshape(128, 128)
fig, axes = plt.subplots(1, 4, figsize=(14, 4.8))
fig.subplots_adjust(top=.75, bottom=.17, wspace=.06, left=.03, right=.97)
fig.text(.03, .94, "THE MOVING HIGHLIGHT", size=23, weight="bold")
fig.text(.03, .865, "Each coordinate changes view and light. Shared logarithmic radiance scale; no tone-map fitting.", color=muted, size=11)
for ax, key, title in zip(axes, ["reference", "regular64", "regular256", "warp4"], ["CPU reference · 8,192 samples", "64 regular phase samples", "256 regular phase samples", "4 warped phase samples"]):
    values = reference if key == "reference" else np.array(images["images"][key]).reshape(128, 128)
    ax.imshow(np.maximum(values, 1e-6), norm=LogNorm(vmin=1e-5, vmax=1.3), cmap="magma", origin="lower")
    ax.set_title(title, size=10, pad=10)
    ax.set_xticks([]); ax.set_yticks([])
fig.text(.03, .06, "Parameter-space radiance plots, not game screenshots. Roughness 0.06; the narrow curved glint is phase-filtered.", color=muted, size=10)
fig.savefig(docs / "rendering-compiler-final-highlights.png", dpi=170)
