"""Research figures from measured results; no generated or retouched game imagery."""
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

root = Path(__file__).resolve().parents[2]
docs = root / "docs/research"
data = json.loads((docs / "transport-compilation-results.json").read_text())
gpu = data["gpu"]
bg, panel, ink, muted = "#101a25", "#162330", "#ecf1ed", "#a7bbc9"
colors = {"point": "#778798", "meanNormal": "#a98bbf", "samples64": "#dfa56c",
          "samples256": "#eed394", "stochastic64": "#e87974", "phaseLut": "#78a8dc",
          "spectralPhasor64": "#62d9b2"}
plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 10, "text.color": ink,
                     "axes.labelcolor": muted, "xtick.color": muted, "ytick.color": muted,
                     "axes.edgecolor": "#3b5061", "axes.facecolor": panel, "figure.facecolor": bg,
                     "savefig.facecolor": bg, "axes.titleweight": "bold"})
fig, axes = plt.subplots(2, 2, figsize=(14, 10), gridspec_kw={"hspace": .43, "wspace": .30})
fig.subplots_adjust(top=.86, bottom=.12, left=.075, right=.97)
fig.text(.075, .965, "COMPILE THE LIGHT RESPONSE", size=24, weight="bold")
fig.text(.075, .925, "Measured compiler experiments · Apple M4 / WebGPU · 65,536 shading queries", size=12, color=muted)

ax = axes[0, 0]
case = gpu["spectral"][0]
names = {"samples64": "64 samples", "samples256": "256 samples", "stochastic64": "64 stochastic",
         "phaseLut": "Anisotropic LUT", "spectralPhasor64": "Compiled: 16 modes"}
offsets = {"samples64": (-75, 13), "samples256": (-100, 0), "stochastic64": (8, 13),
           "phaseLut": (8, 8), "spectralPhasor64": (8, 8)}
for key, label in names.items():
    x = case["times"][key]["median"] * 1000
    y = case["accuracy"][key]["rms"]
    ax.scatter(x, y, s=75, c=colors[key], zorder=5)
    ax.annotate(label, (x, y), xytext=offsets[key], textcoords="offset points", color=colors[key], size=9)
ax.set(xscale="log", yscale="log", xlim=(2.5, 1700), ylim=(.00013, .15),
       xlabel="GPU microseconds  → slower", ylabel="RMS radiance error  → worse")
ax.set_title("Distant water: quality and speed together", loc="left", pad=14)
ax.grid(alpha=.12)
speed = case["times"]["samples256"]["median"] / case["times"]["spectralPhasor64"]["median"]
quality = case["accuracy"]["samples256"]["rms"] / case["accuracy"]["spectralPhasor64"]["rms"]
lut_error = case["accuracy"]["phaseLut"]["rms"] / case["accuracy"]["spectralPhasor64"]["rms"]
ax.text(.03, .95, f"{speed:.1f}× faster than 256 samples; {quality:.1f}× less error\nLUT is cheaper, with {lut_error:.1f}× more error", transform=ax.transAxes, size=9, color=ink, va="top")

ax = axes[0, 1]
vis = gpu["visibility"]
x = np.arange(3)
ax.bar(x - .17, [v["times"]["baseline"]["median"] for v in vis], .32, color="#dfa56c", label="Skin every instance")
ax.bar(x + .17, [v["times"]["certificate"]["median"] for v in vis], .32, color="#62d9b2", label="Prove, compact, skin")
ax.set(yscale="log", ylim=(.012, 6), xticks=x, xticklabels=["0% hidden", "51.4% hidden", "99.3% hidden"], ylabel="GPU milliseconds")
ax.set_title("Crowd work: gain depends on actual occlusion", loc="left", pad=14)
ax.legend(frameon=False, labelcolor=ink, fontsize=9, loc="upper left")
crowd_speed = vis[-1]["times"]["baseline"]["median"] / vis[-1]["times"]["certificate"]["median"]
ax.text(2.17, .037, f"{crowd_speed:.1f}×", ha="center", color="#62d9b2", weight="bold")
ax.grid(axis="y", alpha=.12)

ax = axes[1, 0]
material = data["algebra"]["material"]
material_values = [material["filteredInputs"]["rms"], material["samples64"]["rms"], material["samples256"]["rms"], material["plans"][0]["error"]["rms"], material["plans"][2]["error"]["rms"]]
ax.bar(np.arange(5), material_values, color=["#a98bbf", "#dfa56c", "#eed394", "#62d9b2", "#3a9f86"])
ax.set(yscale="log", xticks=np.arange(5), xticklabels=["Average\ninputs", "64\nsamples", "256\nsamples", "16\nmodes", "64\nmodes"], ylabel="RMS radiance error", ylim=(.0003, 5))
ax.set_title("PBR: preserve roughness / metal / color correlation", loc="left", pad=14)
ax.grid(axis="y", alpha=.12)
material_quality = material["samples256"]["rms"] / material["plans"][2]["error"]["rms"]
ax.text(.32, .95, f"CPU accuracy · fixed light/view\n64 modes: {material_quality:.0f}× less error than 256 samples", transform=ax.transAxes, size=9, va="top")

ax = axes[1, 1]
sky = gpu["atmosphere"][0]
for key, label, col in [("midpoint16", "16 steps", "#dfa56c"), ("midpoint64", "64 steps", "#eed394"),
                        ("midpoint256", "256 steps", "#fff0b0"), ("bounded16", "16 bounded segments", "#62d9b2"),
                        ("bounded32", "32 bounded segments", "#3a9f86"), ("lut", "LUT", "#78a8dc")]:
    tx, er = sky["times"][key]["median"] * 1000, sky["accuracy"][key]["rms"]
    ax.scatter(tx, er, c=col, s=65)
    offset = (8, -16) if key == "midpoint64" else (-92, -14) if key == "midpoint256" else (7, 7)
    ax.annotate(label, (tx, er), xytext=offset, textcoords="offset points", color=col, size=8.5)
ax.set(xscale="log", yscale="log", xlim=(3, 360), ylim=(.000004, .005), xlabel="GPU microseconds  → slower", ylabel="RMS transmittance error")
ax.set_title("Atmosphere: a useful bound, no universal speed win", loc="left", pad=14)
ax.grid(alpha=.12)
fig.text(.075, .046, "Crowd: 4,096 × 1,024 vertices; includes culling passes. Comparator: all-instance skinning, not a Hi-Z renderer.", color=muted, size=10)
fig.text(.075, .020, "Independent synthetic kernel / numerical experiments, not whole-game FPS or proof of AAA image quality.", color=muted, size=10)
fig.savefig(docs / "transport-compilation-evidence.png", dpi=180)
fig.savefig(docs / "transport-compilation-evidence.svg")
plt.close(fig)

fig, axes = plt.subplots(3, 4, figsize=(14, 11))
fig.subplots_adjust(top=.87, bottom=.10, wspace=.10, hspace=.30, left=.065, right=.97)
fig.text(.065, .965, "WHAT THE FILTER PRESERVES", size=24, weight="bold")
fig.text(.065, .925, "Radiance over two authored phases · each row contrast stretched to its reference range; values outside clipped", color=muted, size=11)
columns = [("reference", "Dense reference"), ("samples64", "64 shading samples"), ("phaseLut", "Anisotropic response LUT"), ("spectralPhasor64", "Compiled phase program")]
for row, name in enumerate(["distant", "oblique", "long-correlated"]):
    fixture = json.loads((root / f"output/transport-research/spectral-{name}.json").read_text())
    reference = np.array(fixture["images"]["reference"]).reshape((fixture["side"], fixture["side"]))
    low, high = float(reference.min()), float(reference.max())
    for col, (key, title) in enumerate(columns):
        ax = axes[row, col]
        values = np.array(fixture["images"][key]).reshape(reference.shape)
        ax.imshow(values, cmap="magma", vmin=low, vmax=high, interpolation="nearest", origin="lower")
        ax.set_xticks([]); ax.set_yticks([])
        if row == 0: ax.set_title(title, size=10, pad=10)
        rms = 0 if key == "reference" else fixture["accuracy"][key]["rms"]
        ax.set_xlabel("Reference" if key == "reference" else f"RMS error {rms:.3g}", size=9)
        if col == 0: ax.set_ylabel({"distant": "Distant", "oblique": "Oblique", "long-correlated": "Long correlated"}[name], color=ink, size=11)
fig.text(.065, .035, "The bottom row deliberately stresses a long, narrow footprint aligned with equal phase motion. Ordinary mip filtering loses its surviving beat.", color=muted, size=10)
fig.savefig(docs / "transport-compilation-filtering.png", dpi=170)
plt.close(fig)
