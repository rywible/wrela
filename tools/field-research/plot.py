"""Render the versioned measurements as a standalone research figure."""
import json
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

root = Path(__file__).resolve().parents[2]
data = json.loads((root / "docs/research/field-realization-results.json").read_text())
plt.rcParams.update({
    "font.family": "DejaVu Sans", "font.size": 10,
    "axes.spines.top": False, "axes.spines.right": False,
    "axes.titleweight": "bold", "axes.titlepad": 15,
    "axes.labelcolor": "#35434b", "text.color": "#172c37",
    "xtick.color": "#53636b", "ytick.color": "#53636b",
    "axes.edgecolor": "#b9c5ca", "grid.color": "#dce4e7",
    "figure.facecolor": "#ffffff",
})
fig, axes = plt.subplots(2, 2, figsize=(14, 10))
fig.subplots_adjust(left=.14, right=.97, top=.83, bottom=.24, wspace=.32, hspace=.72)
fig.suptitle("Wrela · surface compiler experiments", x=.14, ha="left", y=.965, fontsize=24, weight="bold")
fig.text(.14, .919, "Measured evidence, including the strong conventional control", fontsize=13, color="#52656e")
fig.text(.14, .890, "Apple / Metal 3 · WebGPU at 1920×1080 · one machine · 21 September 2026", fontsize=10, color="#647982")

runs = data["gpu"]["runs"]
styles = [
    ("mesh24", "Current extracted mesh · 7,548 triangles", "#2166ac"),
    ("mesh12", "Coarsest extracted mesh · 1,868 triangles", "#798a95"),
    ("meshLod", "Parametric LOD · 120 triangles", "#c17a17"),
    ("analytic", "Exact quadric · 12 proxy triangles", "#138771"),
]
ax = axes[0, 0]
for mode, label, color in styles:
    ax.plot([r["instances"] for r in runs], [r["results"][mode]["gpuMs"]["median"] for r in runs],
            "-o", color=color, linewidth=2, markersize=5, label=label)
ax.set_xscale("log"); ax.set_yscale("log")
ax.set_xticks([1, 64, 1024, 4096], ["1", "64", "1,024", "4,096"])
ax.set_ylabel("Median GPU time per geometry pass (ms)")
ax.set_xlabel("Visible stone instances")
ax.set_title("Speed depends on the representation and workload", loc="left", fontsize=11)
ax.grid(axis="y", alpha=.7)
ax.legend(fontsize=8, loc="upper left", frameon=False)

ax = axes[0, 1]
accuracy = data["gpu"]["accuracy"]
order = ["meshLod", "mesh12", "mesh24", "analytic"]
colors = ["#c17a17", "#798a95", "#2166ac", "#138771"]
values = [accuracy[m]["rmsRayDepthErrorMetres"] * 1000 for m in order]
ax.bar(range(4), values, color=colors, width=.58, zorder=3)
for i, v in enumerate(values):
    ax.text(i, v * 1.22, f"{v:.3g}", ha="center", fontsize=10, weight="bold")
ax.set_yscale("log"); ax.set_ylim(.008, 180)
ax.set_xticks(range(4), ["120-tri\nLOD", "Mesh\nres. 12", "Current mesh\nres. 24", "Exact\nquadric"])
ax.set_ylabel("RMS ray-depth error (mm; lower is better)")
ax.set_title("Close-up accuracy · a separate single-stone view", loc="left", fontsize=11)
ax.grid(axis="y", alpha=.7, zorder=0)

ax = axes[1, 0]
cpu = data["cpu"]["results"]
labels = ["Reference bunny", "Single stone", "Blended neck", "64 separated parts"]
ratios = [r["query"]["speedupAgainstOriginal"] for r in cpu]
ax.barh(range(4), ratios, color=["#138771" if v > 1 else "#c17a17" for v in ratios], height=.55, zorder=3)
ax.invert_yaxis()
ax.axvline(1, linestyle="--", linewidth=1, color="#53636b", zorder=4)
ax.set_yticks(range(4), labels); ax.set_xlim(0, max(ratios) * 1.21)
for i, v in enumerate(ratios):
    ax.text(v + .45, i, f"{v:.2f}×", va="center", fontsize=10)
ax.set_xlabel("Exact field-query speedup vs production evaluator")
ax.set_title("Regional reduction · traversal included", loc="left", fontsize=11)
ax.grid(axis="x", alpha=.7, zorder=0)

ax = axes[1, 1]
blends = [cpu[0]["quality"]["blendOnly"], cpu[2]["quality"]["blendOnly"]]
x = np.arange(2)
for offset, key, label, color in [
    (-.17, "polynomial", "Ordinary quadratic jet", "#798a95"),
    (.17, "gauge", "Factored metric approximation", "#138771"),
]:
    values = [b[key]["mean"] for b in blends]
    ax.bar(x + offset, values, .30, label=label, color=color, zorder=3)
    for xx, value in zip(x + offset, values):
        ax.text(xx, value * 1.16, f"{value:.2g}", ha="center", fontsize=9)
ax.set_yscale("log"); ax.set_ylim(2e-8, 2e-6)
ax.set_xticks(x, ["Bunny · 429 accepted\nblend samples", "Neck · 1,173 accepted\nblend samples"])
ax.set_ylabel("Mean |F| residual (implicit units)")
ax.set_title("The new approximation · blend-only comparison", loc="left", fontsize=11)
ax.grid(axis="y", alpha=.7, zorder=0)
ax.legend(fontsize=8, loc="upper right", frameon=False)

fig.text(.14, .05,
         "Limits: the bunny accepts 27.9% of reconstruction samples; all-sample improvement is only 1.27×.\n"
         "GPU timings exclude the rest of the engine. Accuracy uses independent double-precision ground truth.\n"
         "Analytic close-up coverage misses 1 grazing pixel; current mesh misses 336. No full-world speedup is established.",
         fontsize=10, color="#53636b", linespacing=1.6)
fig.savefig(root / "docs/research/field-realization-evidence.png", dpi=160)
fig.savefig(root / "docs/research/field-realization-evidence.svg")
