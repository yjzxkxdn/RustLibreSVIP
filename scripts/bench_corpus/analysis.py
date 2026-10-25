"""Build every figure and table of docs/benchmark.md from the raw CSVs.

Inputs (all in this directory, also committed under docs/benchmark-data/):
  phases_native.csv   native, one cold process per conversion, read/write split
  results_wasm2.csv   wasm, one fresh module per conversion
  results.csv         the older three-engine run (rust/uf3/py) used for paired ratios
  warm_cold.csv       one process, several conversions (continuous)

Outputs:
  report_data.json    every aggregate
  report_tables.md    markdown tables pasted into the page
  <repo>/docs/images/benchmark/*.png
"""

from __future__ import annotations

import collections
import csv
import json
import pathlib
import statistics

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402

BASE = pathlib.Path(r"E:\RUST_LIBRESVIP\.tmp\bench-night")
REPO = pathlib.Path(r"E:\RUST_LIBRESVIP\RustLibreSVIP")
IMAGES = REPO / "docs" / "images" / "benchmark"
IMAGES.mkdir(parents=True, exist_ok=True)

BLUE = "#3b6ea5"
ORANGE = "#d08b3e"
GREEN = "#5b8c5a"
PURPLE = "#8c5b8c"
RED = "#b5493e"
GREY = "#9a9a9a"

plt.rcParams.update({
    "figure.dpi": 140,
    "savefig.dpi": 140,
    "font.size": 9,
    "axes.titlesize": 10.5,
    "axes.labelsize": 9,
    "axes.edgecolor": "#555555",
    "axes.grid": True,
    "grid.color": "#e2e2e2",
    "grid.linewidth": 0.6,
    "axes.axisbelow": True,
    "legend.frameon": False,
})


def read_csv(name: str) -> list[dict]:
    path = BASE / name
    if not path.exists():
        return []
    with path.open(encoding="utf-8", newline="") as handle:
        return list(csv.DictReader(handle))


def number(row: dict, key: str) -> float | None:
    try:
        return float(row[key])
    except (KeyError, TypeError, ValueError):
        return None


def median(values) -> float | None:
    values = [value for value in values if value is not None]
    return statistics.median(values) if values else None


def quartiles(values):
    values = sorted(value for value in values if value is not None)
    if not values:
        return None, None, None
    return statistics.median(values), values[len(values) // 4], values[len(values) * 3 // 4]


# ---------------------------------------------------------------------------
# load
# ---------------------------------------------------------------------------

native = [row for row in read_csv("phases_native.csv") if row["ok"] == "1"]
native_failed = [row for row in read_csv("phases_native.csv") if row["ok"] != "1"]
wasm = [row for row in read_csv("results_wasm2.csv") if row["ok"] == "1"]
wasm_failed = [row for row in read_csv("results_wasm2.csv") if row["ok"] != "1"]
old = read_csv("results.csv")
warm_rows = read_csv("warm_cold.csv")

for row in native:
    row["read"] = number(row, "load_ms") or 0.0
    row["write"] = number(row, "dump_ms") or 0.0
    row["total"] = number(row, "total_ms") or 0.0
    row["engine"] = row["read"] + row["write"]
    row["size_kb"] = int(row["input_bytes"]) / 1024.0
    row["out_kb"] = (number(row, "out_bytes") or 0.0) / 1024.0
for row in wasm:
    row["convert"] = number(row, "convert_ms") or 0.0
    row["size_kb"] = int(row["input_bytes"]) / 1024.0

sources = sorted({row["source_ext"] for row in native})
targets = sorted({row["target"] for row in native})

# ---------------------------------------------------------------------------
# decode / encode per format (one axis each)
# ---------------------------------------------------------------------------

decode = {source: [row["read"] for row in native if row["source_ext"] == source] for source in sources}
decode_size = {source: [row["size_kb"] for row in native if row["source_ext"] == source] for source in sources}
encode = {target: [row["write"] for row in native if row["target"] == target] for target in targets}
encode_size = {target: [row["out_kb"] for row in native if row["target"] == target] for target in targets}

decode_stats = {}
for source in sources:
    med, p25, p75 = quartiles(decode[source])
    size = median(decode_size[source]) or 0.0
    decode_stats[source] = {
        "n": len(decode[source]),
        "median": med,
        "p25": p25,
        "p75": p75,
        "size_kb": size,
        "mb_per_s": (size / 1024.0) / (med / 1000.0) if med else None,
    }
encode_stats = {}
for target in targets:
    med, p25, p75 = quartiles(encode[target])
    size = median(encode_size[target]) or 0.0
    encode_stats[target] = {
        "n": len(encode[target]),
        "median": med,
        "p25": p25,
        "p75": p75,
        "size_kb": size,
        "mb_per_s": (size / 1024.0) / (med / 1000.0) if med else None,
    }

# ---------------------------------------------------------------------------
# pairs
# ---------------------------------------------------------------------------

pair_rows = []
for source in sources:
    for target in targets:
        mine = [row for row in native if row["source_ext"] == source and row["target"] == target]
        if not mine:
            continue
        other = [row for row in wasm if row["source_ext"] == source and row["target"] == target]
        pair_rows.append({
            "source": source,
            "target": target,
            "n": len(mine),
            "size_kb": median([row["size_kb"] for row in mine]),
            "read": median([row["read"] for row in mine]),
            "write": median([row["write"] for row in mine]),
            "native": median([row["total"] for row in mine]),
            "wasm": median([row["convert"] for row in other]) if other else None,
            "wasm_n": len(other),
        })
for row in pair_rows:
    row["engine"] = row["read"] + row["write"]
    row["startup"] = row["native"] - row["engine"]
    row["ratio"] = row["wasm"] / row["native"] if row["wasm"] else None

# per-target medians for the engine comparison chart
tool_target: dict[tuple[str, str], list[float]] = collections.defaultdict(list)
for row in native:
    tool_target[("rust", row["target"])].append(row["total"])
for row in wasm:
    tool_target[("wasm", row["target"])].append(row["convert"])
for row in old:
    if row["ok"] == "1":
        seconds = number(row, "seconds")
        if seconds is not None:
            tool_target[(row["tool"], row["target"])].append(seconds * 1000.0)

# paired ratios against the other two engines, using this round's native timings
rust_ms = {(row["path"].lower(), row["target"]): row["total"] for row in native}
paired: dict[str, list[tuple[float, str, str]]] = collections.defaultdict(list)
for row in old:
    if row["ok"] != "1" or row["tool"] not in ("uf3", "py"):
        continue
    other = number(row, "seconds")
    mine = rust_ms.get((row["path"].lower(), row["target"]))
    if other and mine:
        paired[row["tool"]].append((other * 1000.0 / mine, row["source_ext"], row["target"]))

tool_pair: dict[tuple[str, str, str], list[float]] = collections.defaultdict(list)
for row in old:
    if row["ok"] != "1":
        continue
    seconds = number(row, "seconds")
    if seconds is not None:
        tool_pair[(row["tool"], row["source_ext"], row["target"])].append(seconds * 1000.0)

# ---------------------------------------------------------------------------
# warm path
# ---------------------------------------------------------------------------

cold_native = {(row["path"].lower(), row["target"]): row["total"] for row in native}
cold_wasm = {(row["path"].lower(), row["target"]): row["convert"] for row in wasm}
warm_pairs = []
for source in sources:
    for target in targets:
        mine = [row for row in warm_rows if row["source_ext"] == source and row["target"] == target]
        if not mine:
            continue
        later = [row for row in mine if row["round"] != "1"]
        first = next((row for row in mine if row["round"] == "1"), None)
        key = (mine[0]["path"].lower(), target)
        warm_pairs.append({
            "source": source,
            "target": target,
            "file": pathlib.Path(mine[0]["path"]).name,
            "size_kb": int(mine[0]["input_bytes"]) / 1024.0,
            "native_cold": cold_native.get(key),
            "native_first": number(first, "native_total_ms") if first else None,
            "native_warm": median([number(row, "native_total_ms") for row in later]),
            "wasm_cold": cold_wasm.get(key),
            "wasm_warm": median([number(row, "wasm_convert_ms") for row in later]),
            "memory_mb": max(number(row, "wasm_memory_mb") or 0 for row in later),
        })

warm_summary = {
    "pairs": len(warm_pairs),
    "rounds": max((int(row["round"]) for row in warm_rows), default=0),
    "native_cold": median([row["native_cold"] for row in warm_pairs]),
    "native_first": median([row["native_first"] for row in warm_pairs]),
    "native_warm": median([row["native_warm"] for row in warm_pairs]),
    "wasm_cold": median([row["wasm_cold"] for row in warm_pairs]),
    "wasm_warm": median([row["wasm_warm"] for row in warm_pairs]),
    "memory_max": max((row["memory_mb"] for row in warm_pairs), default=0),
}
warm_summary["native_speedup"] = warm_summary["native_cold"] / warm_summary["native_warm"]
warm_summary["wasm_speedup"] = warm_summary["wasm_cold"] / warm_summary["wasm_warm"]
warm_summary["wasm_over_native"] = warm_summary["wasm_warm"] / warm_summary["native_warm"]

# ---------------------------------------------------------------------------
# summary json
# ---------------------------------------------------------------------------

summary = {
    "native_rows": len(native) + len(native_failed),
    "native_ok": len(native),
    "native_failed": len(native_failed),
    "wasm_rows": len(wasm) + len(wasm_failed),
    "wasm_ok": len(wasm),
    "wasm_failed": len(wasm_failed),
    "pairs": len(pair_rows),
    "decode": decode_stats,
    "encode": encode_stats,
    "paired": {
        tool: {
            "n": len(values),
            "median": median([value[0] for value in values]),
            "min": min(value[0] for value in values),
            "max": max(value[0] for value in values),
        }
        for tool, values in paired.items()
    },
    "wasm_ratio": {
        "median": median([row["ratio"] for row in pair_rows if row["ratio"]]),
    },
    "instantiate_ms": median([number(row, "instantiate_ms") for row in wasm]),
    "warm": warm_summary,
}
(BASE / "report_data.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")

# ---------------------------------------------------------------------------
# table: decode per source, encode per target
# ---------------------------------------------------------------------------

lines = [
    "| source | conversions | median input | decode median | p25 | p75 | MB/s |",
    "|---|---|---|---|---|---|---|",
]
for source in sorted(sources, key=lambda key: -(decode_stats[key]["median"] or 0)):
    stat = decode_stats[source]
    lines.append(
        f'| `{source}` | {stat["n"]} | {stat["size_kb"]:.0f} KB | {stat["median"]:.1f} ms | '
        f'{stat["p25"]:.1f} ms | {stat["p75"]:.1f} ms | {stat["mb_per_s"]:.1f} |'
    )
lines += [
    "",
    "| target | conversions | median output | encode median | p25 | p75 | MB/s |",
    "|---|---|---|---|---|---|---|",
]
for target in sorted(targets, key=lambda key: -(encode_stats[key]["median"] or 0)):
    stat = encode_stats[target]
    lines.append(
        f'| `{target}` | {stat["n"]} | {stat["size_kb"]:.0f} KB | {stat["median"]:.1f} ms | '
        f'{stat["p25"]:.1f} ms | {stat["p75"]:.1f} ms | {stat["mb_per_s"]:.1f} |'
    )
(BASE / "report_tables.md").write_text("\n".join(lines) + "\n", encoding="utf-8")

# ---------------------------------------------------------------------------
# figure: decode distribution per source, encode distribution per target
# ---------------------------------------------------------------------------


def boxplot(axis, groups, labels, color, title, xlabel):
    data = [groups[label] for label in labels]
    positions = range(1, len(labels) + 1)
    box = axis.boxplot(
        data, orientation="horizontal", positions=list(positions), widths=0.55,
        patch_artist=True, showfliers=False,
        medianprops={"color": "#333333", "linewidth": 1.2},
        whiskerprops={"color": "#666666", "linewidth": 0.8},
        capprops={"color": "#666666", "linewidth": 0.8},
        boxprops={"facecolor": color, "edgecolor": "#444444", "linewidth": 0.7, "alpha": 0.55},
    )
    del box
    rng = np.random.default_rng(7)
    for index, label in enumerate(labels, start=1):
        values = np.array(groups[label])
        jitter = rng.uniform(-0.16, 0.16, size=len(values))
        axis.scatter(values, index + jitter, s=9, color=color, alpha=0.65, edgecolor="white", linewidth=0.3, zorder=3)
    axis.set_yticks(list(positions))
    axis.set_yticklabels(labels, fontsize=8)
    axis.set_xscale("log")
    axis.set_xlabel(xlabel)
    axis.set_title(title)
    axis.grid(axis="y", visible=False)


fig, (left, right) = plt.subplots(1, 2, figsize=(9.6, 3.9))
decode_order = sorted(sources, key=lambda key: decode_stats[key]["median"])
boxplot(
    left, decode, decode_order, BLUE,
    "Decoding: cost of reading a source format", "ms per file (log)",
)
encode_order = sorted(targets, key=lambda key: encode_stats[key]["median"])
boxplot(
    right, encode, encode_order, ORANGE,
    "Encoding: cost of writing a target format", "ms per file (log)",
)
left.set_yticklabels(decode_order, fontsize=8)
right.set_yticklabels(encode_order, fontsize=8)
fig.tight_layout()
fig.savefig(IMAGES / "decode_encode_box.png")
plt.close(fig)

# ---------------------------------------------------------------------------
# figure: decode versus encode, one point per conversion
# ---------------------------------------------------------------------------

fig, (log, linear) = plt.subplots(1, 2, figsize=(9.6, 4.0))
colors = {target: color for target, color in zip(targets, (BLUE, ORANGE, GREEN, PURPLE))}
for target in targets:
    points = [(row["read"], row["write"]) for row in native if row["target"] == target]
    log.scatter([p[0] for p in points], [p[1] for p in points],
                s=14, alpha=0.6, color=colors[target], label=f"→ {target}")
    linear.scatter([p[0] for p in points], [p[1] for p in points],
                   s=14, alpha=0.6, color=colors[target])
limit = [0.2, max(row["read"] for row in native) * 1.5]
log.plot(limit, limit, color=GREY, linewidth=1.0, linestyle="--")
log.set_xscale("log")
log.set_yscale("log")
log.set_xlabel("decode (ms)")
log.set_ylabel("encode (ms)")
log.set_title("Decode vs encode, log-log, one point per conversion")
log.legend(loc="upper left", fontsize=7)
linear.plot([0, 200], [0, 200], color=GREY, linewidth=1.0, linestyle="--")
linear.set_xlim(0, 200)
linear.set_ylim(0, 200)
linear.set_xlabel("decode (ms)")
linear.set_ylabel("encode (ms)")
linear.set_title("The same points, linear, zoomed to 200 ms")
fig.tight_layout()
fig.savefig(IMAGES / "decode_vs_encode.png")
plt.close(fig)

# ---------------------------------------------------------------------------
# figure: source x target matrix
# ---------------------------------------------------------------------------

matrix = np.full((len(sources), len(targets)), np.nan)
for row in pair_rows:
    matrix[sources.index(row["source"]), targets.index(row["target"])] = row["native"]

fig, axis = plt.subplots(figsize=(7.2, 5.4))
image = axis.imshow(np.ma.masked_invalid(matrix), cmap="YlOrRd", aspect="auto", norm=matplotlib.colors.LogNorm())
axis.set_xticks(range(len(targets)))
axis.set_xticklabels([f"→ {target}" for target in targets])
axis.set_yticks(range(len(sources)))
axis.set_yticklabels(sources, fontsize=8)
for row_index in range(len(sources)):
    for column_index in range(len(targets)):
        value = matrix[row_index, column_index]
        if np.isnan(value):
            axis.text(column_index, row_index, "—", ha="center", va="center", color="#999999", fontsize=8)
        else:
            axis.text(column_index, row_index, f"{value:.0f}", ha="center", va="center",
                      fontsize=7.5, color="black" if value < 60 else "white")
axis.set_title("Median conversion time by pair (ms, native CLI, cold)")
axis.grid(False)
bar = fig.colorbar(image, ax=axis, fraction=0.035, pad=0.02)
bar.set_label("ms (log)")
fig.tight_layout()
fig.savefig(IMAGES / "matrix.png")
plt.close(fig)

# ---------------------------------------------------------------------------
# figure: ECDF of per-conversion time, native vs wasm
# ---------------------------------------------------------------------------

fig, axis = plt.subplots(figsize=(6.2, 4.0))
for values, label, color in (
    ([row["total"] for row in native], f"native CLI (n={len(native)})", BLUE),
    ([row["convert"] for row in wasm], f"WebAssembly (n={len(wasm)})", GREEN),
):
    ordered = np.sort(np.array(values))
    axis.step(ordered, np.arange(1, len(ordered) + 1) / len(ordered), where="post", color=color, label=label)
axis.set_xscale("log")
axis.set_xlabel("conversion time (ms, log)")
axis.set_ylabel("share of conversions")
axis.set_title("Cumulative distribution of a single conversion")
axis.legend(loc="lower right", fontsize=8)
axis.set_ylim(0, 1.02)
fig.tight_layout()
fig.savefig(IMAGES / "ecdf.png")
plt.close(fig)

# ---------------------------------------------------------------------------
# figure: paired speed-ups, grouped by source format
# ---------------------------------------------------------------------------

rng = np.random.default_rng(11)
fig, panels = plt.subplots(1, 2, figsize=(9.6, 4.0))
for axis, tool, label, color in (
    (panels[0], "py", "vs LibreSVIP 2.9.2 (python)", RED),
    (panels[1], "uf3", "vs UtaFormatix3", PURPLE),
):
    groups: dict[str, list[float]] = collections.defaultdict(list)
    for ratio, source, _target in paired.get(tool, []):
        groups[source].append(ratio)
    names = sorted(groups, key=lambda name: statistics.median(groups[name]))
    for index, name in enumerate(names, start=1):
        values = np.array(groups[name])
        jitter = rng.uniform(-0.18, 0.18, size=len(values))
        axis.scatter(values, np.full(len(values), index) + jitter, s=13,
                     color=color, alpha=0.65, edgecolor="white", linewidth=0.3)
        med = statistics.median(values)
        axis.plot([med, med], [index - 0.3, index + 0.3], color="#222222", linewidth=1.8, zorder=3)
        axis.text(1.02, index, f"n={len(values)}", transform=axis.get_yaxis_transform(),
                  fontsize=7, color="#555555", va="center")
    axis.set_yticks(range(1, len(names) + 1))
    axis.set_yticklabels(names, fontsize=8)
    axis.set_xscale("log")
    axis.axvline(1, color="#666666", linewidth=0.8)
    axis.set_xlabel("speed-up of this build (log, 1x = parity)")
    overall = statistics.median([value[0] for value in paired.get(tool, [])])
    axis.axvline(overall, color="#333333", linewidth=1.2, linestyle="--")
    axis.text(overall, 0.02, f"median {overall:.1f}x", transform=axis.get_xaxis_transform(),
              fontsize=8, ha="center", va="bottom", rotation=90,
              bbox={"facecolor": "white", "edgecolor": "none", "alpha": 0.8, "pad": 1.5})
    axis.set_title(f"{label} — n={len(paired.get(tool, []))}")
    axis.grid(axis="y", visible=False)
fig.suptitle("Paired speed-ups, one dot per conversion, bar = median of the group", fontsize=10)
fig.tight_layout()
fig.savefig(IMAGES / "speedups.png")
plt.close(fig)

# ---------------------------------------------------------------------------
# figure: time against input size (engine work vs wall clock)
# ---------------------------------------------------------------------------

fig, (engine_axis, total_axis) = plt.subplots(1, 2, figsize=(9.8, 4.2), sharex=True)
palette = plt.get_cmap("tab20").colors
series = collections.defaultdict(list)
for row in native:
    series[row["source_ext"]].append((row["size_kb"], row["engine"], row["total"]))
for index, source in enumerate(sorted(series, key=lambda key: -len(series[key]))):
    points = series[source]
    engine_axis.scatter([p[0] for p in points], [p[1] for p in points],
                        s=15, alpha=0.7, color=palette[index % len(palette)], label=source)
    total_axis.scatter([p[0] for p in points], [p[2] for p in points],
                       s=15, alpha=0.7, color=palette[index % len(palette)])
for axis, column, title in (
    (engine_axis, 1, "engine work only (read + write)"),
    (total_axis, 2, "wall clock (start-up + engine)"),
):
    xs = np.array([row["size_kb"] for row in native])
    ys = np.array([row["engine"] if column == 1 else row["total"] for row in native])
    mask = (xs > 0) & (ys > 0)
    slope, intercept = np.polyfit(np.log10(xs[mask]), np.log10(ys[mask]), 1)
    fit_x = np.logspace(np.log10(xs[mask].min()), np.log10(xs[mask].max()), 50)
    axis.plot(fit_x, 10 ** intercept * fit_x ** slope, color="#333333", linewidth=1.2, linestyle="--",
              label=f"fit: t ∝ size^{slope:.2f}")
    axis.set_xscale("log")
    axis.set_yscale("log")
    axis.set_xlabel("input size (KB, log)")
    axis.set_title(title)
    axis.legend(fontsize=6.5, ncol=2, loc="upper left")
engine_axis.set_ylabel("milliseconds (log)")
fig.suptitle("What scales with the project and what is a constant", fontsize=10)
fig.tight_layout()
fig.savefig(IMAGES / "scaling.png")
plt.close(fig)


# ---------------------------------------------------------------------------
# figure: cold versus continuous
# ---------------------------------------------------------------------------

fig, panels = plt.subplots(1, 2, figsize=(9.2, 4.2), sharey=True)
for axis, key_cold, key_warm, title, color in (
    (panels[0], "native_cold", "native_warm", "native CLI", BLUE),
    (panels[1], "wasm_cold", "wasm_warm", "WebAssembly", GREEN),
):
    for row in warm_pairs:
        cold = row[key_cold]
        warm = row[key_warm]
        if not cold or not warm:
            continue
        axis.plot([0, 1], [cold, warm], color=color, linewidth=0.8, alpha=0.45)
        axis.scatter([0, 1], [cold, warm], s=10, color=color, alpha=0.8)
    cold_med = median([row[key_cold] for row in warm_pairs])
    warm_med = median([row[key_warm] for row in warm_pairs])
    axis.plot([0, 1], [cold_med, warm_med], color="#222222", linewidth=2.2, marker="o", label="median")
    axis.annotate(f"{cold_med:.1f} ms", (0, cold_med), textcoords="offset points", xytext=(-6, 6), ha="right", fontsize=8)
    axis.annotate(f"{warm_med:.1f} ms", (1, warm_med), textcoords="offset points", xytext=(6, -12), ha="left", fontsize=8)
    axis.set_xticks([0, 1])
    axis.set_xticklabels(["cold\n(process per file)", "continuous\n(one process)"])
    axis.set_xlim(-0.35, 1.35)
    axis.set_yscale("log")
    axis.set_title(title)
    axis.legend(loc="upper right", fontsize=8)
panels[0].set_ylabel("conversion time (ms, log)")
fig.suptitle("Every pair gets faster when the process stays alive", fontsize=10)
fig.tight_layout()
fig.savefig(IMAGES / "warm_slope.png")
plt.close(fig)

# ---------------------------------------------------------------------------
# figure: read/write stack per pair (kept from the previous revision)
# ---------------------------------------------------------------------------

top = sorted(pair_rows, key=lambda row: row["native"], reverse=True)[:24]
top.reverse()
fig, axis = plt.subplots(figsize=(7.8, 6.4))
y = list(range(len(top)))
reads = [row["read"] for row in top]
writes = [row["write"] for row in top]
startups = [max(0.0, row["startup"]) for row in top]
axis.barh(y, reads, color=BLUE, label="read (decode)")
axis.barh(y, writes, left=reads, color=ORANGE, label="write (encode)")
axis.barh(y, startups, left=[r + w for r, w in zip(reads, writes)], color="#b8b8b8", label="process start-up")
axis.set_yticks(y)
axis.set_yticklabels([f'{row["source"]} → {row["target"]}' for row in top], fontsize=8)
axis.set_xlabel("median wall clock per conversion (ms, one cold process)")
axis.set_title("Where a cold conversion spends its time")
axis.legend(loc="lower right")
for index, row in enumerate(top):
    axis.text(row["native"] * 1.01, index, f'{row["native"]:.0f}', va="center", fontsize=7)
axis.set_xlim(0, max(row["native"] for row in top) * 1.12)
fig.tight_layout()
fig.savefig(IMAGES / "pairs.png")
plt.close(fig)

# ---------------------------------------------------------------------------
# figure: four engines (native, wasm, uf3, py)
# ---------------------------------------------------------------------------

engines = [
    ("rust", "RustLibreSVIP (native)", BLUE),
    ("wasm", "RustLibreSVIP (wasm)", GREEN),
    ("uf3", "UtaFormatix3", PURPLE),
    ("py", "LibreSVIP (py)", RED),
]
fig, (full, zoom) = plt.subplots(1, 2, figsize=(10.4, 4.2), gridspec_kw={"width_ratios": [1.35, 1]})
width = 0.2
for index, (tool, label, color) in enumerate(engines):
    values = [median(tool_target.get((tool, target), [])) or 0.0 for target in targets]
    positions = [position + (index - 1.5) * width for position in range(len(targets))]
    bars = full.bar(positions, values, width=width, color=color, label=label)
    for bar, value in zip(bars, values):
        if value:
            full.text(bar.get_x() + bar.get_width() / 2, value + max(values) * 0.02,
                      f"{value:.0f}", ha="center", va="bottom", fontsize=6, rotation=90)
for index, (tool, label, color) in enumerate(engines[:2]):
    values = [median(tool_target.get((tool, target), [])) or 0.0 for target in targets]
    positions = [position + (index - 0.5) * 0.34 for position in range(len(targets))]
    bars = zoom.bar(positions, values, width=0.34, color=color, label=label)
    for bar, value in zip(bars, values):
        zoom.text(bar.get_x() + bar.get_width() / 2, value * 1.03, f"{value:.0f}", ha="center", fontsize=7)
for axis in (full, zoom):
    axis.set_xticks(range(len(targets)))
    axis.set_xticklabels([f"→ {target}" for target in targets])
full.set_ylabel("median ms per conversion (linear)")
full.set_title("Four engines, same projects, same targets")
full.set_ylim(0, max(max(median(tool_target.get((tool, target), [])) or 0.0 for target in targets) for tool, _, _ in engines) * 1.18)
zoom.set_title("native CLI versus WebAssembly")
zoom.set_ylabel("median ms per conversion (linear)")
zoom.set_ylim(0, max(max(median(tool_target.get((tool, target), [])) or 0.0 for target in targets) for tool, _, _ in engines[:2]) * 1.2)
zoom.legend(loc="upper left", fontsize=8)
full.legend(ncol=4, loc="upper center", bbox_to_anchor=(0.5, -0.12), fontsize=8)
fig.tight_layout()
fig.savefig(IMAGES / "engines.png")
plt.close(fig)

# ---------------------------------------------------------------------------
# figure: wasm versus native, per conversion
# ---------------------------------------------------------------------------

joined = [row for row in pair_rows if row["wasm"] and row["native"]]
fig, axis = plt.subplots(figsize=(6.0, 4.6))
sizes = [max(8.0, min(160.0, row["size_kb"] / 12.0)) for row in joined]
wasm_total = [row["wasm"] + 0.0 for row in joined]
ratios = [row["wasm"] / row["native"] for row in joined]
factor = statistics.median(ratios)
axis.scatter([row["native"] for row in joined], wasm_total, s=sizes, alpha=0.75,
             color=GREEN, edgecolor="white", linewidth=0.5)
limit = [1.0, max(row["native"] for row in joined) * 1.4]
axis.plot(limit, limit, color="#888888", linewidth=1.0, linestyle="--", label="equal")
axis.plot(limit, [value * factor for value in limit], color="#bbbbbb", linewidth=0.8,
          linestyle=":", label=f"median {factor:.2f}×")
axis.set_xscale("log")
axis.set_yscale("log")
axis.set_xlabel("native CLI, median per conversion (ms)")
axis.set_ylabel("WebAssembly, median per conversion (ms)")
axis.set_title("The same conversion in the CLI and in the module")
axis.legend(loc="upper left", fontsize=8)
fig.tight_layout()
fig.savefig(IMAGES / "wasm_vs_native.png")
plt.close(fig)

raw_lines = [
    "| source → target | pairs | input | py (LibreSVIP 2.9.2) | libresvip / rust (native CLI) | uf3 (UtaFormatix3) | wasm (in-module) | read | write |",
    "|---|---|---|---|---|---|---|---|---|",
]
for row in pair_rows:
    py = median(tool_pair.get(("py", row["source"], row["target"]), []))
    uf3 = median(tool_pair.get(("uf3", row["source"], row["target"]), []))
    raw_lines.append(
        f'| `{row["source"]}` → `{row["target"]}` | {row["n"]} | {row["size_kb"]:.0f} KB | '
        f'{(f"{py:.0f} ms" if py else "—")} | {row["native"]:.0f} ms | '
        f'{(f"{uf3:.0f} ms" if uf3 else "—")} | '
        f'{(f"{row["wasm"]:.0f} ms" if row["wasm"] else "—")} | '
        f'{row["read"]:.1f} ms | {row["write"]:.1f} ms |'
    )
(BASE / "report_full_table.md").write_text("\n".join(raw_lines) + "\n", encoding="utf-8")

# ---------------------------------------------------------------------------
# warm table
# ---------------------------------------------------------------------------

warm_lines = [
    "| source → target | file | input | cold (ms) | continuous r1 (ms) | continuous r2+ (ms) | cold / continuous | wasm continuous (ms) |",
    "|---|---|---|---|---|---|---|---|",
]
for row in sorted(warm_pairs, key=lambda item: (item["source"], item["target"])):
    ratio = (row["native_cold"] / row["native_warm"]) if row["native_warm"] else 0.0
    warm_lines.append(
        f'| `{row["source"]}` → `{row["target"]}` | {row["file"][:30]} | {row["size_kb"]:.0f} KB | '
        f'{row["native_cold"]:.1f} | {(row["native_first"] or 0):.1f} | {row["native_warm"]:.1f} | '
        f'{ratio:.1f}x | {row["wasm_warm"]:.1f} |'
    )
(BASE / "warm_table.md").write_text("\n".join(warm_lines) + "\n", encoding="utf-8")

print(json.dumps(summary["paired"], indent=2))
print(json.dumps(warm_summary, indent=2))
