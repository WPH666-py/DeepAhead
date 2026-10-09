# -*- coding: utf-8 -*-
"""
DeepAhead 四模式 (DSH/DSK/DSA/DSF) vs 原装模型 (K3/GPT-6/Fable5.1/GPT-5.6/Opus5/Fable5)
性能差距 · 价格差距 · 性价比变化 (0-50元窗口)

数据源:
  - BenchLM (2026-09-04): 综合性能分 / 各厂商 API 定价 (Kimi / DeepSeek / OpenAI / Anthropic)
  - Apidog GPT-6 Astra 指南 (2026-09-05): GPT-5.6 促销价、Astra 价、launch 基准
  - Anthropic / Moonshot / DeepSeek 官方 system card & 技术报告 (经 BenchLM 归档)
  - DeepAhead docs/WORKFLOW-ENGINES.md: 模式 = 单一 DeepSeek 运行时, 阶段开销 1-5 次额外调用
  - 汇率: 1 USD = 6.78 CNY (中国外汇交易中心 2026-09-08 中间价 6.7804)

用法: python build.py   (在 docs/value-analysis/ 下运行)
输出: value_data.json, charts/*.png, ../模型性价比对比分析.html
"""
import json
import os
import sys

try:
    sys.stdout.reconfigure(encoding="utf-8")
except Exception:
    pass

# ----------------------------------------------------------------------------
# 1. 原始数据
# ----------------------------------------------------------------------------
# legacy 语义: 每百万 token 的 USD 价格 (input, cached_input, output)
# score: BenchLM 综合分 (None = 无公开排名)
PRICES = {
    # DeepSeek (当前公开价, 2026-07-31)
    "v4flash": {"name": "DeepSeek V4 Flash", "in": 0.14, "cin": 0.0028, "out": 0.28, "score": None, "rank": "未公开排名", "ctx": "1M"},
    "v4pro":   {"name": "DeepSeek V4 Pro",   "in": 0.435, "cin": 0.003625, "out": 0.87, "score": 66.4, "rank": "-", "ctx": "1M"},
    # OpenAI GPT-5.6 系 (2026-07-30 降价后当前价; Sol 促销价至 2026-11-21)
    "luna": {"name": "GPT-5.6 Luna", "in": 0.20, "cin": 0.02, "out": 1.20, "score": 65.5, "rank": "#37", "ctx": "1.05M"},
    "terra": {"name": "GPT-5.6 Terra", "in": 2.00, "cin": 0.20, "out": 12.00, "score": 71.4, "rank": "#13", "ctx": "1.05M"},
    "sol": {"name": "GPT-5.6 Sol (促销)", "in": 4.00, "cin": 0.40, "out": 20.00, "score": 79.6, "rank": "#5", "ctx": "1.05M",
            "note": "促销价至 2026-11-21; 原价 $5/$30"},
    "astra": {"name": "GPT-6 Astra", "in": 10.00, "cin": 1.00, "out": 50.00, "score": 81.1, "rank": "#2", "ctx": "1.05M"},
    # Moonshot
    "k3": {"name": "Kimi K3", "in": 3.00, "cin": 0.30, "out": 15.00, "score": 74.9, "rank": "#7", "ctx": "1.05M"},
    # Anthropic
    "opus5": {"name": "Claude Opus 5", "in": 5.00, "cin": 0.50, "out": 25.00, "score": 80.7, "rank": "#4", "ctx": "1M"},
    "fable5": {"name": "Claude Fable 5", "in": 10.00, "cin": 1.00, "out": 50.00, "score": 80.9, "rank": "#3", "ctx": "1M"},
    "fable51": {"name": "Claude Fable 5.1", "in": 10.00, "cin": 0.25, "out": 50.00, "score": 83.0, "rank": "#1", "ctx": "1M"},
}

# DeepAhead 模式: 运行在 DeepSeek V4 上, 只烧 DeepSeek token
# cost_mult: 规划/委派/审查阶段增加的调用开销 (WORKFLOW-ENGINES.md: 1-5 次额外调用)
# target: 原装对标模型 (模式性能上限的参考对象)
MODES = {
    "DSH": {"name": "DSH — DeepSeek Harness 原生", "runtime": "v4pro", "cost_mult": 1.00,
            "target_key": "v4pro", "target_name": "DeepSeek V4 (原生)"},
    "DSK": {"name": "DSK — Kimi K3 原装工作流", "runtime": "v4pro", "cost_mult": 1.15,
            "target_key": "k3", "target_name": "Kimi K3"},
    "DSA": {"name": "DSA — GPT-6 Astra 原装工作流", "runtime": "v4pro", "cost_mult": 1.20,
            "target_key": "astra", "target_name": "GPT-6 Astra"},
    "DSF": {"name": "DSF — Fable 5.1 原装工作流", "runtime": "v4pro", "cost_mult": 1.25,
            "target_key": "fable51", "target_name": "Claude Fable 5.1"},
}

# 标准 Agent 任务 (与 Apidog "agentic coding loop" 同形):
# 20 次调用 × 每次 50K 输入 (70% 前缀缓存命中) + 每次 2K 输出
WORKLOAD = {"calls": 20, "ctx_per_call": 50_000, "cache_hit": 0.70, "out_per_call": 2_000}
FX = 6.78  # CNY per USD

# DeepSWE v1.1 (仓库级 agentic 编码基准, %)
DEEPSWE = {
    "astra": 74.1, "sol": 72.7, "opus5": 68.8, "k3": 67.5, "fable51": 67.4, "v4flash": 54.4,
}

# ----------------------------------------------------------------------------
# 2. 计算
# ----------------------------------------------------------------------------
def task_cost_usd(p):
    w = WORKLOAD
    in_miss_M = w["calls"] * w["ctx_per_call"] * (1 - w["cache_hit"]) / 1e6
    in_hit_M = w["calls"] * w["ctx_per_call"] * w["cache_hit"] / 1e6
    out_M = w["calls"] * w["out_per_call"] / 1e6
    return in_miss_M * p["in"] + in_hit_M * p["cin"] + out_M * p["out"]

model_costs = {k: task_cost_usd(p) for k, p in PRICES.items()}
mode_costs = {}
for mk, m in MODES.items():
    base = model_costs[m["runtime"]] * m["cost_mult"]
    mode_costs[mk] = {"flash": model_costs["v4flash"] * m["cost_mult"], "pro": base}

# 模式性能区间: [DeepSeek V4 Pro 可比较综合分, 原装对标模型分]
mode_scores = {mk: {"floor": PRICES["v4pro"]["score"], "ceil": PRICES[m["target_key"]]["score"]}
               for mk, m in MODES.items()}

# 边际提升 (每多花 1 元 → 性能分提升): 用户指定的几组 "廉价模式 → 原装模型" 升级
MARGINAL = [
    {"key": "DSK→K3", "label": "DSK → Kimi K3", "from_mode": "DSK", "to": "k3",
     "desc": "先从 DSK (约¥0.43–1.31) 换到 Kimi K3 (¥%.2f)" % (model_costs["k3"] * FX)},
    {"key": "DSA→Sol", "label": "DSA → GPT-5.6 Sol", "from_mode": "DSA", "to": "sol",
     "desc": "从 DSA 换到 Sol (促销价 ¥%.2f)" % (model_costs["sol"] * FX)},
    {"key": "DSA→Astra", "label": "DSA → GPT-6 Astra", "from_mode": "DSA", "to": "astra",
     "desc": "从 DSA 换到 GPT-6 Astra (¥%.2f)" % (model_costs["astra"] * FX)},
    {"key": "DSF→Opus5", "label": "DSF → Claude Opus 5", "from_mode": "DSF", "to": "opus5",
     "desc": "从 DSF 换到 Opus 5 (¥%.2f)" % (model_costs["opus5"] * FX)},
    {"key": "DSF→Fable5", "label": "DSF → Claude Fable 5", "from_mode": "DSF", "to": "fable5",
     "desc": "从 DSF 换到 Fable 5 (¥%.2f)" % (model_costs["fable5"] * FX)},
    {"key": "DSF→Fable5.1", "label": "DSF → Claude Fable 5.1", "from_mode": "DSF", "to": "fable51",
     "desc": "从 DSF 换到 Fable 5.1 (¥%.2f)" % (model_costs["fable51"] * FX)},
]
for m in MARGINAL:
    mode = MODES[m["from_mode"]]
    from_lo, from_hi = mode_costs[m["from_mode"]]["flash"], mode_costs[m["from_mode"]]["pro"]
    to_cost = model_costs[m["to"]]
    floor, ceil = mode_scores[m["from_mode"]]["floor"], mode_scores[m["from_mode"]]["ceil"]
    to_score = PRICES[m["to"]]["score"]
    # Δscore 区间: 模式已达上限 → Δ=0; 模式仅达下界 → Δ=celing-floor
    d_score_lo = max(0.0, to_score - ceil)
    d_score_hi = max(0.0, to_score - floor)
    d_price_lo = (to_cost - from_hi) * FX
    d_price_hi = (to_cost - from_lo) * FX
    m["gain_lo"] = d_score_lo / d_price_hi
    m["gain_hi"] = d_score_hi / d_price_lo
    m["d_price_lo"], m["d_price_hi"] = d_price_lo, d_price_hi
    m["d_score_lo"], m["d_score_hi"] = d_score_lo, d_score_hi

# 原始模型之间的升级链 (用于第二张图对比: 纯模型加钱买性能的斜率)
CHAIN = [
    {"label": "Luna → Terra", "a": "luna", "b": "terra"},
    {"label": "Terra → Sol", "a": "terra", "b": "sol"},
    {"label": "Sol → Astra", "a": "sol", "b": "astra"},
    {"label": "Opus 5 → Fable 5.1", "a": "opus5", "b": "fable51"},
    {"label": "DSK → K3 (模式)", "a": None, "b": "k3", "from_mode": "DSK"},
]
for c in CHAIN:
    if c["a"] is None:
        c["gain_lo"], c["gain_hi"] = 0, 0  # 由 MARGINAL 提供
        continue
    p_a, p_b = PRICES[c["a"]], PRICES[c["b"]]
    d_price = (model_costs[c["b"]] - model_costs[c["a"]]) * FX
    c["gain"] = (p_b["score"] - p_a["score"]) / d_price
    c["d_price"] = d_price

# ----------------------------------------------------------------------------
# 3. 汇总 JSON
# ----------------------------------------------------------------------------
out = {
    "generated": "2026-09-08",
    "fx": FX,
    "workload": WORKLOAD,
    "model_costs_yuan": {k: round(v * FX, 3) for k, v in model_costs.items()},
    "model_costs_usd": {k: round(v, 4) for k, v in model_costs.items()},
    "models": {k: {**{kk: vv for kk, vv in PRICES[k].items()}, "cost_yuan": round(model_costs[k] * FX, 3)}
               for k in PRICES},
    "modes": {mk: {"name": m["name"], "cost_flash_yuan": round(mode_costs[mk]["flash"] * FX, 3),
                   "cost_pro_yuan": round(mode_costs[mk]["pro"] * FX, 3),
                   "score_floor": mode_scores[mk]["floor"], "score_ceil": mode_scores[mk]["ceil"],
                   "target_name": m["target_name"]} for mk, m in MODES.items()},
    "marginal": MARGINAL,
    "chain": CHAIN,
    "deepSwe": DEEPSWE,
}

os.makedirs(os.path.dirname(os.path.abspath(__file__)), exist_ok=True)
with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "value_data.json"), "w", encoding="utf-8") as f:
    json.dump(out, f, ensure_ascii=False, indent=2)

# ----------------------------------------------------------------------------
# 4. matplotlib 图表
# ----------------------------------------------------------------------------
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.font_manager as fm
import numpy as np

for fp in [r"C:\Windows\Fonts\msyh.ttc", r"C:\Windows\Fonts\msyh.ttf", r"C:\Windows\Fonts\simhei.ttf"]:
    if os.path.exists(fp):
        fm.fontManager.addfont(fp)
        break
plt.rcParams["font.sans-serif"] = ["Microsoft YaHei", "SimHei", "sans-serif"]
plt.rcParams["axes.unicode_minus"] = False

CHART_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "charts")
os.makedirs(CHART_DIR, exist_ok=True)

COLORS = {
    "deepseek": "#2f6fed", "openai": "#10a37f", "anthropic": "#d97757",
    "moonshot": "#7c3aed", "mode": "#e11d48", "grey": "#9ca3af",
}
FAMILY = {"luna": "openai", "terra": "openai", "sol": "openai", "astra": "openai",
          "opus5": "anthropic", "fable5": "anthropic", "fable51": "anthropic",
          "k3": "moonshot", "v4flash": "deepseek", "v4pro": "deepseek"}

def short_name(k):
    return PRICES[k]["name"]

# ---- 图1: 价格 vs 性能 (0-50元, 对数横轴看清低价集群) -----------------------------
fig, ax = plt.subplots(figsize=(13.5, 7.6), dpi=150)
# 无公开排名的 Flash: 只标价格
ax.annotate("DeepSeek V4 Flash\n(无公开综合分 · 参考 DeepSWE 54.4)",
            xy=(model_costs["v4flash"] * FX, 55.5), xytext=(model_costs["v4flash"] * FX * 1.25, 53),
            arrowprops=dict(arrowstyle="->", color=COLORS["grey"]),
            color=COLORS["grey"], fontsize=8.5)

LABEL_OFF = {"luna": (8, -16), "v4pro": (10, -10), "terra": (10, 8), "sol": (8, -14),
             "k3": (10, 8), "astra": (-8, -18), "fable5": (12, 9), "opus5": (10, 10),
             "fable51": (-10, 12)}

# 模型散点
for k in PRICES:
    p = PRICES[k]
    if p["score"] is None:
        continue
    x, y = model_costs[k] * FX, p["score"]
    fam = FAMILY[k]
    ax.scatter([x], [y], s=90, color=COLORS[fam], zorder=5, edgecolor="white", linewidth=1.2)
    dx, dy = LABEL_OFF.get(k, (8, 8))
    ax.annotate("%s\n%.1f分 · ¥%.2f" % (short_name(k), y, x), (x, y),
                textcoords="offset points", xytext=(dx, dy), fontsize=8.4, color=COLORS[fam])

# 家族连线
for fam, keys in [("openai", ["luna", "terra", "sol", "astra"]),
                  ("anthropic", ["opus5", "fable5", "fable51"])]:
    xs = [model_costs[k] * FX for k in keys]
    ys = [PRICES[k]["score"] for k in keys]
    ax.plot(xs, ys, "--", color=COLORS[fam], alpha=0.45, lw=1.4, zorder=2)

# DeepAhead 模式区间 (矩形: x=Flash价~Pro价, y=下界~上限)
mode_lbl_y = {"DSH": 63.2, "DSK": 70.2, "DSA": 74.5, "DSF": 78.2}
for mk, m in MODES.items():
    x0, x1 = mode_costs[mk]["flash"] * FX, mode_costs[mk]["pro"] * FX
    y0, y1 = mode_scores[mk]["floor"], mode_scores[mk]["ceil"]
    ax.add_patch(plt.Rectangle((x0, y0), x1 - x0, y1 - y0, facecolor=COLORS["mode"],
                               alpha=0.20, edgecolor=COLORS["mode"], lw=1.5, zorder=3))
    ax.annotate(m["name"].split(" — ")[0], (x1 * 1.18, mode_lbl_y.get(mk, y1 + 1)),
                fontsize=8.8, color=COLORS["mode"], weight="bold", ha="left")

ax.set_xscale("log")
ax.set_xlim(0.32, 52)
ax.set_ylim(52, 100)
ax.set_xticks([0.4, 0.8, 1.5, 3, 6, 10, 20, 40])
ax.set_xticklabels(["0.4", "0.8", "1.5", "3", "6", "10", "20", "40元"])
ax.set_xlabel("标准 Agent 任务成本 (元, 0–50, 对数轴)", fontsize=11)
ax.set_ylabel("BenchLM 综合性能分 (0–100)", fontsize=11)
ax.set_title("DeepAhead 四模式 vs 原装模型: 价格—性能图 (2026-09-08)", fontsize=13, pad=12)
ax.grid(alpha=0.25, ls=":")
ax.axvspan(0.32, 2, color="#fef3c7", alpha=0.30, zorder=0)
ax.annotate("DeepAhead 模式区\n(¥0.37–1.42)", (0.34, 97.5), fontsize=9, color="#b45309", ha="left")
fig.tight_layout()
fig.savefig(os.path.join(CHART_DIR, "chart1_price_performance.png"), bbox_inches="tight")
plt.close(fig)

# ---- 图2: 每多花1元 → 性能提升 (分/元) ----------------------------------------
fig, ax = plt.subplots(figsize=(12.5, 6.4), dpi=150)
labels, lo, hi, cols = [], [], [], []
for m in MARGINAL:
    labels.append("%s\n(多付 ¥%.0f–%.0f)" % (m["label"], m["d_price_lo"], m["d_price_hi"]))
    lo.append(max(0.0, m["gain_lo"]))
    hi.append(m["gain_hi"])
    cols.append(COLORS["mode"])
y = np.arange(len(labels))[::-1]
for i, (yy, l, h) in enumerate(zip(y, lo, hi)):
    ax.barh(yy, h - l, height=0.55, left=l, color=cols[i], alpha=0.65)
    ax.barh(yy, l, height=0.55, color=cols[i], alpha=1.0)
    ax.annotate("%.2f ~ %.2f 分/元" % (l, h), (h, yy), textcoords="offset points",
                xytext=(6, -4), fontsize=8.6, va="center")
ax.set_yticks(y)
ax.set_yticklabels(labels, fontsize=9.6)
ax.set_xlim(0, max(hi) * 1.22)
ax.set_xlabel("每多花 1 元人民币获得的性能提升 (综合分 / 元)", fontsize=11)
ax.set_title("性价比变化: 从 DeepAhead 模式升级到原装模型, 每多花 1 元的性能回报", fontsize=12.5, pad=10)
ax.grid(axis="x", alpha=0.25, ls=":")
ax.text(0.01, 0.03, "黄色实段 = 模式已达对标上限时的回报(≈0)；红段 = 模式仅达 DeepSeek 下界时的理论回报上限。DeepAhead 模式无独立公开基准, 区间为方法论外推。",
        transform=ax.transAxes, fontsize=8, color="#666")
fig.tight_layout()
fig.savefig(os.path.join(CHART_DIR, "chart2_marginal_gain.png"), bbox_inches="tight")
plt.close(fig)

# ---- 图3: 性价比分/元 ----------------------------------------------------------
fig, ax = plt.subplots(figsize=(12.5, 6.2), dpi=150)
rows = []
for k, p in PRICES.items():
    if p["score"] is None:
        continue
    rows.append((short_name(k), p["score"] / (model_costs[k] * FX), FAMILY[k], p["score"]))
rows.sort(key=lambda r: r[1])
labels = [r[0] for r in rows]
vals = [r[1] for r in rows]
y = np.arange(len(rows))
ax.barh(y, vals, color=[COLORS[r[2]] for r in rows], alpha=0.85)
for yy, v, r in zip(y, vals, rows):
    ax.annotate("%.1f 分/元\n(%s 分 @ ¥%.2f)" % (v, r[3], model_costs[[k for k in PRICES if PRICES[k]["name"] == r[0]][0]] * FX),
                (v, yy), textcoords="offset points", xytext=(5, -2), fontsize=8.2)
ax.set_yticks(y)
ax.set_yticklabels(labels, fontsize=9.6)
ax.set_xlabel("性能 / 成本 (综合分 ÷ 单任务元)", fontsize=11)
ax.set_title("性价比对比: 每元能换多少综合性能分", fontsize=12.5, pad=10)
ax.grid(axis="x", alpha=0.25, ls=":")
fig.tight_layout()
fig.savefig(os.path.join(CHART_DIR, "chart3_value_per_yuan.png"), bbox_inches="tight")
plt.close(fig)

# ---- 图4: DeepSWE v1.1 agentic 编码基准 ---------------------------------------
fig, ax = plt.subplots(figsize=(12.5, 6.0), dpi=150)
items = sorted(DEEPSWE.items(), key=lambda kv: kv[1], reverse=True)
row_labels = ["DSH / DSK / DSA / DSF（估算区间）"] + [short_name(k) for k, _ in items]
# 第一行: 模式区间带; 其余行: 模型柱
y = np.arange(len(row_labels))[::-1]  # 索引0 → 最高
mode_row = y[0]
for i, (k, v) in enumerate(items):
    yy = y[i + 1]
    ax.barh(yy, v, height=0.55, color=COLORS[FAMILY[k]], alpha=0.85)
    ax.annotate("%.1f%%" % v, (v, yy), textcoords="offset points", xytext=(5, -4), fontsize=9)
ax.add_patch(plt.Rectangle((0.5, mode_row - 0.35), 83.0 - 0.5, 0.7, facecolor=COLORS["mode"], alpha=0.18,
                           edgecolor=COLORS["mode"], lw=1.4))
ax.annotate("区间: 下界 ≈ V4 Flash 实测 54.4%, 上界 = 原装对标模型 (K3 67.5 / Astra 74.1 / Fable 5.1 67.4)",
            (3, mode_row), fontsize=8.6, color=COLORS["mode"], va="center")
ax.set_yticks(y)
ax.set_yticklabels(row_labels, fontsize=9.6)
ax.set_xlabel("DeepSWE v1.1 (仓库级智能体编码任务成功率, %)", fontsize=11)
ax.set_title("Agentic 编码视角: DeepSWE v1.1", fontsize=12.5, pad=10)
ax.grid(axis="x", alpha=0.25, ls=":")
fig.tight_layout()
fig.savefig(os.path.join(CHART_DIR, "chart4_deepswe.png"), bbox_inches="tight")
plt.close(fig)

print("charts written to", CHART_DIR)

# ----------------------------------------------------------------------------
# 5. 交互式 HTML (ECharts)
# ----------------------------------------------------------------------------
HTML_TEMPLATE = r"""<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>DeepAhead 四模式 × 原装模型 · 性价比分析 (0–50元)</title>
<script src="https://cdn.jsdelivr.net/npm/echarts@5.5.0/dist/echarts.min.js"></script>
<style>
  body { font-family: "Microsoft YaHei", "PingFang SC", sans-serif; margin: 0; background: #f6f7fb; color: #1f2937; }
  header { background: linear-gradient(90deg,#111827,#1e3a8a); color:#fff; padding: 22px 32px; }
  header h1 { margin: 0 0 6px; font-size: 21px; }
  header p { margin: 0; font-size: 13px; opacity: .85; }
  main { max-width: 1200px; margin: 0 auto; padding: 18px 12px; }
  .card { background: #fff; border-radius: 12px; padding: 14px 8px 6px; margin-bottom: 18px; box-shadow: 0 1px 4px rgba(0,0,0,.08); }
  .card h2 { font-size: 16px; margin: 6px 12px 2px; }
  .card .sub { font-size: 12px; color: #6b7280; margin: 0 12px 8px; }
  .chart { width: 100%; height: 460px; }
  .note { font-size: 12px; color: #6b7280; padding: 8px 16px 14px; line-height: 1.7; }
  .note b { color: #374151; }
  table { border-collapse: collapse; width: 100%; font-size: 12.5px; }
  th, td { border: 1px solid #e5e7eb; padding: 6px 8px; text-align: center; }
  th { background: #f3f4f6; }
  td.l { text-align: left; }
  .legend { display: flex; gap: 18px; flex-wrap: wrap; padding: 4px 16px 10px; font-size: 12px; }
  .legend span i { display:inline-block; width:12px; height:12px; border-radius:3px; margin-right:5px; vertical-align:-1px; }
  .toggle { margin: 0 12px 6px; font-size: 12px; }
  .toggle label { margin-right: 12px; cursor: pointer; }
</style>
</head>
<body>
<header>
  <h1>DeepAhead 四模式（DSH / DSK / DSA / DSF）× 原装模型 · 性能与价格差距 · 性价比变化</h1>
  <p>2026-09-08 · 性能分 = BenchLM 综合指数（2026-09-04）· 价格 = 官方 API 每百万 token 单价（USD，按 1 USD = 6.78 CNY）· 单任务成本 = 标准 Agent 负载（20 次调用 × 50K 输入、70% 前缀缓存命中 + 每次 2K 输出）</p>
</header>
<main>

<div class="card">
  <h2>① 价格 — 性能（0–50 元窗口）</h2>
  <p class="sub">深蓝色矩形 = DeepAhead 模式成本区间（V4 Flash 默认 ～ V4 Pro 升级档）但性能只有估算区间（原装引擎品质上限）——花 DeepSeek 的钱，买原装工作流的编排。</p>
  <div class="toggle">
    <label><input type="radio" name="axis" value="log" checked> 对数（推荐，看清 &lt;2 元战场）</label>
    <label><input type="radio" name="axis" value="value"> 线性 0–50</label>
  </div>
  <div id="c1" class="chart"></div>
  <div class="note"><b>读图：</b>原装旗舰（Fable 5.1 / Astra / Opus 5 / Sol）集中在右上（¥15–39，79.6–83 分）；Kimi K3 在 ¥11.6 / 74.9 分；DeepSeek V4 Pro 在 ¥1.14 / 66.4 分；四个 DeepAhead 模式挤在 ¥0.37–1.42 区间并向上顶到各自对标模型的分（红色框上沿 = 原装引擎上限，下沿 = DeepSeek 可比较下界）。DeepSeek V4 Flash 无公开综合排名，仅标价格。</div>
</div>

<div class="card">
  <h2>② 每多花 1 元 → 性能提升多少？（用户升级回报曲线）</h2>
  <p class="sub">从 DeepAhead 模式升级到对应原装模型：红段为理论回报上限，左端黄段为「模式已经跑满原装引擎品质」时的回报（≈0）。</p>
  <div id="c2" class="chart"></div>
  <div class="note"><b>读图：</b>从 DSK/DSA/DSF 换到任何原装旗舰，每多付 1 元最多换 0.4–0.9 个综合分，而 Fable 5 / Astra 相比 Fable 5.1 / Opus 5 等等的纯模型加价升级（Sol→Astra 多付 ¥23.2 只换 1.5 分 ≈ 0.07 分/元）回报更低。反过来说：DeepAhead 模式用原装 1/20~1/80 的价格就拿到了「同款编排」。<b>注意回报区间下界 = 0</b>：DeepAhead 模式尚未有独立公开基准，这是方法论外推，不是实测。</div>
</div>

<div class="card">
  <h2>③ 每元性价比（综合分 ÷ 单任务成本）</h2>
  <div id="c3" class="chart" style="height:400px"></div>
</div>

<div class="card">
  <h2>④ 代理编码视角：DeepSWE v1.1（仓库级任务成功率，%）</h2>
  <p class="sub">与综合指数互补的「智能体编码」单一基准；DeepSeek V4 Flash 54.4、Kimi K3 67.5、Fable 5.1 67.4、Opus 5 68.8、GPT-5.6 Sol 72.7、GPT-6 Astra 74.1。</p>
  <div id="c4" class="chart" style="height:360px"></div>
</div>

<div class="card">
  <h2>⑤ 数据总表</h2>
  <div style="overflow-x:auto; padding:0 12px 12px">
  <table id="tbl"></table>
  </div>
  <div class="note"><b>方法 / 假设：</b><br>
  ① 性能分：BenchLM 综合指数（0–100，2026-09-04 快照），覆盖全部 9 个有公开排名的模型；V4 Flash 未公开综合分，未进入主图。<br>
  ② 价格：官方 API 现价（DeepSeek 2026-07-31 / OpenAI 2026-07-30 降价后 / Moonshot / Anthropic），单位为 每百万 token USD，按 1 USD=6.78 CNY 折算（9月8日中间价 6.7804）。GPT-5.6 Sol 采用促销价 $4/$20（至少至 2026-11-21），原价 $5/$30 见附注。<br>
  ③ 单任务成本：标准 Agent 负载 = 20 次调用 × 每次 50K 输入（70% 前缀缓存命中、按各厂缓存价计费）+ 每次 2K 输出；与 Apidog「agentic coding loop」示例同形。真实成本随上下文长度、缓存命中率、推理 effort 与重试而变（OpenAI 5.6/6 系 reasoning 输出按输出价计费，未单独分档）。<br>
  ④ DeepAhead 模式成本：只烧 DeepSeek Token（docs/WORKFLOW-ENGINES.md），按规划/委派/审查阶段额外 1–5 次调用折算运算量：DSK +15%、DSA +20%、DSF +25%，DSH +0%；默认模型 deepseek-chat（→ V4 Flash），升级档 V4 Pro。<br>
  ⑤ DeepAhead 模式性能：<b>没有独立公开基准</b>。区间 = [DeepSeek V4 Pro 可比较综合分 66.4，原装对标模型综合分]（DSK→K3 74.9；DSA→Astra 81.1；DSF→Fable 5.1 83.0；DSH→66.4），为「引擎上限外推」，不是实测值。DeepSWE 视图中 Flash 行（54.4）为已发布实测，可作下界参考。</div>
</div>

</main>
<script>
const DATA = __DATA__;
const FX = DATA.fx;

function fmt(v){ return (Math.round(v*100)/100).toLocaleString('zh-CN'); }

// ---- 图1 ----
const famColor = {openai:'#10a37f', anthropic:'#d97757', moonshot:'#7c3aed', deepseek:'#2f6fed'};
function scatterSeries(){
  const pts = [];
  for (const [k,p] of Object.entries(DATA.models)){
    if (p.score==null) continue;
    pts.push({value:[p.cost_yuan, p.score], name:p.name});
  }
  return pts;
}
function familyLine(keys){
  return {
    type:'line', name:'加价链', data: keys.map(k=>[DATA.models[k].cost_yuan, DATA.models[k].score]),
    lineStyle:{type:'dashed', width:1.4, color: famColor[familyOf(keys[0])]}, symbol:'none', silent:true, z:1
  };
}
function familyOf(k){
  if (['luna','terra','sol','astra'].includes(k)) return 'openai';
  if (['opus5','fable5','fable51'].includes(k)) return 'anthropic';
  return 'deepseek';
}
const modeSeries = [];
for (const [mk,m] of Object.entries(DATA.modes)){
  modeSeries.push({
    type:'custom', name:mk, silent:true,
    renderItem:(p,api)=>{
      const x0=api.coord([m.cost_flash_yuan, m.score_floor])[0];
      const x1=api.coord([m.cost_pro_yuan, m.score_floor])[0];
      const y0=api.coord([m.cost_pro_yuan, m.score_floor])[1];
      const y1=api.coord([m.cost_pro_yuan, m.score_ceil])[1];
      return {type:'rect', shape:{x:x0,y:y0,width:x1-x0,height:y1-y0},
        style:{fill:'rgba(225,29,72,0.14)',stroke:'#e11d48',lineWidth:1.4}};
    }
  });
}
function makeC1(axisType){
  return {
    tooltip:{trigger:'item', formatter:(p)=>{ return p.name + (p.value ? (' ¥'+fmt(p.value[0])+' · '+p.value[1]+'分') : ''); }},
    legend:{top:0, data:['原装模型','家族加价链','DeepAhead 模式区间']},
    grid:{left:50,right:30,top:50,bottom:46},
    xAxis: axisType==='log'
      ? {type:'log', name:'单任务成本 (元, 对数)', min:0.32, max:52,
         axisLabel:{formatter:(v)=>v+'元'}}
      : {type:'value', name:'单任务成本 (元)', min:0, max:50,
         axisLabel:{formatter:(v)=>v+'元'}},
    yAxis:{type:'value', name:'BenchLM 综合分', min:52, max:100},
    series: [
      ...modeSeries,
      familyLine(['luna','terra','sol','astra']),
      familyLine(['opus5','fable5','fable51']),
      {type:'scatter', name:'原装模型', data:scatterSeries(),
       itemStyle:{borderColor:'#fff',borderWidth:1.2},
       label:{show:true, formatter:(p)=>p.name, position:'top', fontSize:10}},
      {type:'scatter', name:'DeepSeek V4 Flash（未排名）', symbolSize:8, itemStyle:{color:'#9ca3af'},
       data:[[DATA.models.v4flash.cost_yuan, 53.2]], label:{show:true, position:'top', fontSize:9,
         formatter:'V4 Flash\n未公开综合分'}}
    ]
  };
}
const c1 = echarts.init(document.getElementById('c1'));
c1.setOption(makeC1(document.querySelector('input[name=axis]:checked').value));
// ---- 图2 ----
const c2 = echarts.init(document.getElementById('c2'));
const marginal = DATA.marginal;
c2.setOption({
  tooltip:{trigger:'axis', axisPointer:{type:'shadow'},
    formatter:(ps)=>{ const m = marginal[ps[0].dataIndex];
      return m.label+'<br/>每多花1元回报: '+fmt(Math.max(0,m.gain_lo))+' ~ '+fmt(m.gain_hi)+' 分/元<br/>'+m.desc; }},
  grid:{left:190,right:130,top:36,bottom:40},
  xAxis:{type:'value', name:'分 / 元', max:1.05},
  yAxis:{type:'category', data:marginal.map(m=>m.label), axisLabel:{fontSize:11}},
  series:[
    {type:'bar', name:'保底回报(下界≈0)', stack:'g', barWidth:16,
     data:marginal.map(m=>Math.max(0,m.gain_lo)), itemStyle:{color:'#fbbf24'}},
    {type:'bar', name:'理论上限(至DeepSeek下界)', stack:'g', barWidth:16,
     data:marginal.map(m=>Math.max(0,m.gain_hi-Math.max(0,m.gain_lo))), itemStyle:{color:'#e11d48'},
     label:{show:true, position:'right', formatter:(p)=>fmt(p.value)+' 分/元'}}
  ],
  graphic: [
    {type:'text', left:200, top:8, style:{text:'下界(黄)≈0 =「模式已达原装引擎上限」; 上界(红) =「模式仅达 DeepSeek 下界」', fontSize:11, fill:'#6b7280'}}
  ]
});
// ---- 图3 ----
const c3 = echarts.init(document.getElementById('c3'));
const vp = Object.entries(DATA.models).filter(([k,p])=>p.score!=null)
  .map(([k,p])=>({name:p.name, val:p.score/p.cost_yuan})).sort((a,b)=>a.val-b.val);
c3.setOption({
  tooltip:{trigger:'axis', axisPointer:{type:'shadow'}},
  grid:{left:200,right:100,top:20,bottom:30},
  xAxis:{type:'value', name:'综合分 / 元'},
  yAxis:{type:'category', data:vp.map(v=>v.name), axisLabel:{fontSize:11}},
  series:[{type:'bar', data:vp.map((v,i)=>({
      value:v.val, itemStyle:{color: i<2?'#2f6fed': i<4?'#10a37f':'#d97757'}})),
    barWidth:15, label:{show:true, position:'right', formatter:(p)=>fmt(p.value)+' 分/元'}}]
});
// ---- 图4 ----
const c4 = echarts.init(document.getElementById('c4'));
const ds = Object.entries(DATA.deepSwe).map(([k,v])=>({name:DATA.models[k].name, val:v}));
c4.setOption({
  tooltip:{trigger:'axis', axisPointer:{type:'shadow'}},
  grid:{left:220,right:80,top:20,bottom:30},
  xAxis:{type:'value', name:'DeepSWE v1.1 (%)', max:85},
  yAxis:{type:'category', data:ds.map(d=>d.name).reverse(), axisLabel:{fontSize:11}},
  series:[{type:'bar', data: ds.map(d=>d.val).reverse(), barWidth:16,
    label:{show:true, position:'right', formatter:(p)=>p.value+'%'}}]
});
// ---- 图5 表格 ----
const rows = [];
rows.push('<tr><th class="l">模型 / 模式</th><th>输入 $/M</th><th>缓存 $/M</th><th>输出 $/M</th><th>上下文</th><th>综合分</th><th>排名</th><th>单任务成本 (元)</th></tr>');
for (const [k,p] of Object.entries(DATA.models)){
  rows.push(`<tr><td class="l">${p.name}</td><td>$${p.in}</td><td>$${p.cin}</td><td>$${p.out}</td><td>${p.ctx}</td>
    <td>${p.score==null?'未公开':p.score}</td><td>${p.rank}</td><td>¥${fmt(p.cost_yuan)}</td></tr>`);
}
for (const [mk,m] of Object.entries(DATA.modes)){
  rows.push(`<tr><td class="l">${m.name}</td><td colspan="3" style="color:#e11d48">仅 DeepSeek Token (V4 Flash/Pro)</td><td>1M</td>
    <td>${m.score_floor} ~ ${m.score_ceil} (估算)</td><td>—</td><td>¥${fmt(m.cost_flash_yuan)} ~ ¥${fmt(m.cost_pro_yuan)}</td></tr>`);
}
document.getElementById('tbl').innerHTML = rows.join('');

window.addEventListener('resize', ()=>{c1.resize();c2.resize();c3.resize();c4.resize();});
document.querySelectorAll('input[name=axis]').forEach(r=>r.addEventListener('change',()=>{
  c1.setOption(makeC1(document.querySelector('input[name=axis]:checked').value), true);
}));
</script>
</body>
</html>
"""

html = HTML_TEMPLATE.replace("__DATA__", json.dumps(out, ensure_ascii=False))
html_path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "模型性价比对比分析.html")
with open(html_path, "w", encoding="utf-8") as f:
    f.write(html)
print("html written to", html_path)

# 摘要输出
print("\n=== 单任务成本 (元) ===")
for k, v in sorted(model_costs.items(), key=lambda kv: kv[1] * FX):
    p = PRICES[k]
    print("%-22s ¥%.3f   (score=%s)" % (p["name"], v * FX, p["score"]))
print("\n=== DeepAhead 模式 ===")
for mk, m in MODES.items():
    print("%-4s ¥%.3f~%.3f  性能估算 %.1f~%.1f" % (mk, mode_costs[mk]["flash"] * FX, mode_costs[mk]["pro"] * FX,
                                                   mode_scores[mk]["floor"], mode_scores[mk]["ceil"]))
print("\n=== 边际 (分/元) ===")
for m in MARGINAL:
    print("%-16s %.3f ~ %.3f" % (m["label"], m["gain_lo"], m["gain_hi"]))
for c in CHAIN:
    if "gain" in c:
        print("%-16s %.3f" % (c["label"], c["gain"]))
