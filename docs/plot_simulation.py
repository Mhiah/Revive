"""Draws docs/simulation.png from docs/simulation.json: supply over 60 days for each scenario,
with each day's state (alive, takeover vote, flatlined) as a strip underneath."""
import json, pathlib
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Patch
from matplotlib.lines import Line2D

here = pathlib.Path(__file__).parent
data = json.loads((here / "simulation.json").read_text())
INK, DIM, LINE, BLUE = "#0B0F19", "#64748B", "#E2E8F0", "#2563EB"
STATE = {"alive": "#15803D", "voting": "#B45309", "vote_closed": "#B45309", "flatlined": "#B91C1C"}
plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 10, "text.color": INK, "axes.labelcolor": DIM,
                     "xtick.color": DIM, "ytick.color": DIM, "axes.edgecolor": LINE})

sc = data["scenarios"]
fig, axes = plt.subplots(len(sc), 1, figsize=(11, 1.55 * len(sc) + 1.3), sharex=True)
fig.patch.set_facecolor("white")
for ax, s in zip(axes, sc):
    days = [d["day"] for d in s["days"]]
    supply = [d["supply"] / 1e6 for d in s["days"]]
    ax.set_facecolor("white")
    ax.step(days, supply, where="post", color=BLUE, lw=2)
    ax.set_ylim(880, 1012)
    ax.set_yticks([900, 1000])
    ax.set_yticklabels(["900M", "1,000M"])
    ax.grid(axis="y", color=LINE, lw=0.8)
    for side in ("top", "right", "left"):
        ax.spines[side].set_visible(False)
    ax.tick_params(length=0)
    # State strip under the line.
    for d in s["days"]:
        ax.axvspan(d["day"] - 0.45, d["day"] + 0.45, ymin=0, ymax=0.09, color=STATE.get(d["status"], DIM), lw=0)
    for e in s["events"]:
        if e["kind"] == "revived":
            ax.annotate("revived", (e["day"], supply[e["day"]]), xytext=(4, 8), textcoords="offset points", fontsize=8.5, color=INK)
    last = s["days"][-1]
    ax.set_title(f'{s["name"]}', loc="left", fontsize=11, fontweight="bold", pad=3)
    ax.text(1.0, 1.02, f'day 60: {last["supply"]/1e6:,.1f}M · {last["status"]} · {last["lives"]} {"life" if last["lives"] == 1 else "lives"}',
            transform=ax.transAxes, ha="right", va="bottom", fontsize=9, color=DIM)
axes[-1].set_xlabel("day")
axes[-1].set_xlim(-1, 61)
fig.suptitle("Supply over 60 days: the real contract, six communities", x=0.01, y=0.995, ha="left", fontsize=14, fontweight="bold")
fig.text(0.01, 0.948, "Supply only falls when holders revive the coin, and only by the creator's unvested tokens.", ha="left", color=DIM)
fig.legend(handles=[Line2D([0], [0], color=BLUE, lw=2, label="total supply"),
                    Patch(color=STATE["alive"], label="alive"), Patch(color=STATE["voting"], label="takeover vote"),
                    Patch(color=STATE["flatlined"], label="flatlined")],
           loc="lower left", ncol=4, frameon=False, bbox_to_anchor=(0.01, 0.0))
fig.tight_layout(rect=(0, 0.04, 1, 0.945))
fig.savefig(here / "simulation.png", dpi=160)
print("wrote", here / "simulation.png")
