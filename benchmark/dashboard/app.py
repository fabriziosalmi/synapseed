"""SYNAPSEED Benchmark Dashboard — local wandb-style metrics viewer.

Launch:
    cd benchmark
    source venv/bin/activate
    streamlit run dashboard/app.py

Reads all JSON results from benchmark/results/ and displays:
- Historical trends per benchmark type
- BLIND vs SYNAPSEED comparison
- Per-difficulty breakdowns
- Search quality metrics (MRR, P@K, R@K)
- NIAH heatmaps
"""

from __future__ import annotations

import json
import os
from datetime import datetime, timezone
from pathlib import Path

import plotly.express as px
import plotly.graph_objects as go
import streamlit as st

# ── Config ───────────────────────────────────────────────────────────

RESULTS_DIR = Path(__file__).parent.parent / "results"

st.set_page_config(
    page_title="SYNAPSEED Benchmarks",
    page_icon="🧠",
    layout="wide",
)


# ── Data Loading ─────────────────────────────────────────────────────

@st.cache_data(ttl=10)
def load_all_results() -> list[dict]:
    """Load all JSON result files, sorted by timestamp."""
    results = []
    if not RESULTS_DIR.exists():
        return results
    for f in sorted(RESULTS_DIR.glob("*.json")):
        try:
            data = json.loads(f.read_text())
            data["_file"] = f.name
            data["_path"] = str(f)
            results.append(data)
        except (json.JSONDecodeError, KeyError):
            continue
    return results


def classify_benchmark(result: dict) -> str:
    """Classify a result by benchmark type from metadata."""
    meta = result.get("metadata", {})
    bm = meta.get("benchmark", "").lower()
    if bm:
        return bm
    # Fallback: infer from filename
    name = result.get("_file", "")
    if "coding" in name:
        return "coding"
    if "grounding" in name:
        return "grounding"
    if "search" in name:
        return "search"
    if "niah" in name:
        return "niah"
    return "unknown"


def parse_timestamp(result: dict) -> datetime | None:
    """Extract timestamp from metadata."""
    ts = result.get("metadata", {}).get("timestamp", "")
    if not ts:
        return None
    try:
        return datetime.strptime(ts, "%Y%m%d_%H%M%S").replace(tzinfo=timezone.utc)
    except ValueError:
        return None


# ── Sidebar ──────────────────────────────────────────────────────────

st.sidebar.title("SYNAPSEED Benchmarks")

all_results = load_all_results()

if not all_results:
    st.warning("No results found in `benchmark/results/`. Run a benchmark first!")
    st.code("python -m benchmark.coding.run --quick", language="bash")
    st.stop()

# Classify results
by_type: dict[str, list[dict]] = {}
for r in all_results:
    t = classify_benchmark(r)
    by_type.setdefault(t, []).append(r)

available_types = sorted(by_type.keys())
selected_type = st.sidebar.selectbox(
    "Benchmark Type", available_types, index=0
)

st.sidebar.markdown("---")
st.sidebar.metric("Total Runs", len(all_results))
for t, runs in sorted(by_type.items()):
    st.sidebar.metric(f"{t.title()} Runs", len(runs))

st.sidebar.markdown("---")
st.sidebar.caption(
    f"Results dir: `{RESULTS_DIR}`\n\n"
    "Auto-refreshes every 10s."
)


# ── Main Content ─────────────────────────────────────────────────────

st.title(f"📊 {selected_type.title()} Benchmark")

runs = by_type.get(selected_type, [])
if not runs:
    st.info("No runs for this benchmark type yet.")
    st.stop()


# ── CODING BENCHMARK ─────────────────────────────────────────────────

if selected_type == "coding":
    # Historical trend
    trend_data = []
    for r in runs:
        ts = parse_timestamp(r)
        agg = r.get("aggregate", {})
        model = r.get("model", "unknown")
        version = r.get("metadata", {}).get("version", "?")
        if agg and ts:
            trend_data.append({
                "timestamp": ts,
                "BLIND": agg.get("blind_mean", 0),
                "SYNAPSEED": agg.get("synapseed_mean", 0),
                "Delta": agg.get("delta", 0),
                "model": model,
                "version": version,
            })

    if trend_data:
        st.subheader("Historical Trend")
        fig = go.Figure()
        fig.add_trace(go.Scatter(
            x=[d["timestamp"] for d in trend_data],
            y=[d["BLIND"] for d in trend_data],
            mode="lines+markers", name="BLIND",
            line=dict(color="#EF553B"),
            hovertext=[f"v{d['version']} | {d['model']}" for d in trend_data],
        ))
        fig.add_trace(go.Scatter(
            x=[d["timestamp"] for d in trend_data],
            y=[d["SYNAPSEED"] for d in trend_data],
            mode="lines+markers", name="SYNAPSEED",
            line=dict(color="#636EFA"),
            hovertext=[f"v{d['version']} | {d['model']}" for d in trend_data],
        ))
        fig.update_layout(
            yaxis_title="Mean Composite Score",
            xaxis_title="Run",
            yaxis_range=[0, 1],
            height=400,
        )
        st.plotly_chart(fig, use_container_width=True)

    # Latest run detail
    latest = runs[-1]
    agg = latest.get("aggregate", {})

    col1, col2, col3, col4 = st.columns(4)
    col1.metric("BLIND", f"{agg.get('blind_mean', 0):.3f}")
    col2.metric("SYNAPSEED", f"{agg.get('synapseed_mean', 0):.3f}")
    col3.metric("Delta", f"{agg.get('delta', 0):+.3f}")
    col4.metric("Hallucinations",
                f"B:{agg.get('blind_hallucinations', 0)} S:{agg.get('synapseed_hallucinations', 0)}")

    # Per-difficulty breakdown
    by_diff = agg.get("by_difficulty", {})
    if by_diff:
        st.subheader("By Difficulty (Latest Run)")
        diff_data = []
        for diff, scores in by_diff.items():
            diff_data.append({"Difficulty": diff, "BLIND": scores.get("blind", 0), "SYNAPSEED": scores.get("synapseed", 0)})
        fig = px.bar(
            diff_data, x="Difficulty", y=["BLIND", "SYNAPSEED"],
            barmode="group", color_discrete_map={"BLIND": "#EF553B", "SYNAPSEED": "#636EFA"},
            height=350,
        )
        fig.update_layout(yaxis_range=[0, 1], yaxis_title="Composite Score")
        st.plotly_chart(fig, use_container_width=True)

    # Per-task table
    tasks = latest.get("tasks", [])
    if tasks:
        st.subheader("Per-Task Results (Latest Run)")
        table_data = []
        for t in tasks:
            table_data.append({
                "Task": t["task_id"],
                "Diff": t["difficulty"],
                "BLIND": f"{t['single_blind']['composite']:.2f}",
                "SYNAPSEED": f"{t['single_synapseed']['composite']:.2f}",
                "Delta": f"{t['single_synapseed']['composite'] - t['single_blind']['composite']:+.2f}",
                "B Halluc": t["single_blind"]["hallucinations"],
                "S Halluc": t["single_synapseed"]["hallucinations"],
            })
        st.dataframe(table_data, use_container_width=True)


# ── GROUNDING BENCHMARK ──────────────────────────────────────────────

elif selected_type == "grounding":
    # F1 trend
    trend_data = []
    for r in runs:
        ts = parse_timestamp(r)
        agg = r.get("aggregate", {})
        model = r.get("model", "unknown")
        version = r.get("metadata", {}).get("version", "?")
        blind_f1 = agg.get("blind_f1", {})
        grounded_f1 = agg.get("grounded_f1", {})
        if ts and blind_f1:
            trend_data.append({
                "timestamp": ts,
                "BLIND F1": blind_f1.get("f1", 0),
                "GROUNDED F1": grounded_f1.get("f1", 0),
                "model": model,
                "version": version,
            })

    if trend_data:
        st.subheader("F1 Score Trend")
        fig = go.Figure()
        fig.add_trace(go.Scatter(
            x=[d["timestamp"] for d in trend_data],
            y=[d["BLIND F1"] for d in trend_data],
            mode="lines+markers", name="BLIND",
            line=dict(color="#EF553B"),
        ))
        fig.add_trace(go.Scatter(
            x=[d["timestamp"] for d in trend_data],
            y=[d["GROUNDED F1"] for d in trend_data],
            mode="lines+markers", name="GROUNDED",
            line=dict(color="#636EFA"),
        ))
        fig.update_layout(yaxis_title="F1 Score", yaxis_range=[0, 1], height=400)
        st.plotly_chart(fig, use_container_width=True)

    # Latest run
    latest = runs[-1]
    agg = latest.get("aggregate", {})
    blind_f1 = agg.get("blind_f1", {})
    grounded_f1 = agg.get("grounded_f1", {})

    col1, col2, col3 = st.columns(3)
    col1.metric("Precision", f"B:{blind_f1.get('precision', 0):.3f} → G:{grounded_f1.get('precision', 0):.3f}")
    col2.metric("Recall", f"B:{blind_f1.get('recall', 0):.3f} → G:{grounded_f1.get('recall', 0):.3f}")
    col3.metric("F1", f"B:{blind_f1.get('f1', 0):.3f} → G:{grounded_f1.get('f1', 0):.3f}")

    # Radar chart: BLIND vs GROUNDED per metric
    categories = ["Precision", "Recall", "F1"]
    fig = go.Figure()
    fig.add_trace(go.Scatterpolar(
        r=[blind_f1.get("precision", 0), blind_f1.get("recall", 0), blind_f1.get("f1", 0)],
        theta=categories, fill="toself", name="BLIND",
        line=dict(color="#EF553B"),
    ))
    fig.add_trace(go.Scatterpolar(
        r=[grounded_f1.get("precision", 0), grounded_f1.get("recall", 0), grounded_f1.get("f1", 0)],
        theta=categories, fill="toself", name="GROUNDED",
        line=dict(color="#636EFA"),
    ))
    fig.update_layout(polar=dict(radialaxis=dict(range=[0, 1])), height=400)
    st.plotly_chart(fig, use_container_width=True)

    # Per-question results
    results_list = latest.get("results", [])
    if results_list:
        st.subheader("Per-Question Results")
        table_data = []
        for r in results_list:
            table_data.append({
                "ID": r["id"],
                "Type": r["question_type"],
                "Diff": r["difficulty"],
                "BLIND": f"{r['blind']['score']:.1f}/3",
                "GROUNDED": f"{r['grounded']['score']:.1f}/3",
                "Delta": f"{r['delta']:+.1f}",
            })
        st.dataframe(table_data, use_container_width=True)


# ── SEARCH BENCHMARK ─────────────────────────────────────────────────

elif selected_type == "search":
    # MRR trend
    trend_data = []
    for r in runs:
        ts = parse_timestamp(r)
        agg = r.get("aggregate", {})
        version = r.get("metadata", {}).get("version", "?")
        if ts and agg:
            trend_data.append({
                "timestamp": ts,
                "MRR": agg.get("mrr", 0),
                "version": version,
                **{k: v for k, v in agg.items() if k.startswith("precision") or k.startswith("recall") or k.startswith("file")},
            })

    if trend_data:
        st.subheader("Search Quality Trend")
        metrics_to_plot = ["MRR"]
        # Find precision/recall keys dynamically
        for key in trend_data[0]:
            if key.startswith("precision") or key.startswith("recall"):
                metrics_to_plot.append(key)

        fig = go.Figure()
        colors = {"MRR": "#636EFA", "precision_at_10": "#EF553B", "recall_at_10": "#00CC96"}
        for metric in metrics_to_plot:
            fig.add_trace(go.Scatter(
                x=[d["timestamp"] for d in trend_data],
                y=[d.get(metric, 0) for d in trend_data],
                mode="lines+markers",
                name=metric.replace("_", " ").title(),
                line=dict(color=colors.get(metric)),
                hovertext=[f"v{d['version']}" for d in trend_data],
            ))
        fig.update_layout(yaxis_title="Score", yaxis_range=[0, 1], height=400)
        st.plotly_chart(fig, use_container_width=True)

    # Latest run metrics
    latest = runs[-1]
    agg = latest.get("aggregate", {})

    metric_cols = st.columns(4)
    metric_cols[0].metric("MRR", f"{agg.get('mrr', 0):.3f}")
    for i, (k, v) in enumerate(agg.items()):
        if k != "mrr" and i < 3:
            metric_cols[i + 1].metric(k.replace("_", " ").title(), f"{v:.3f}")

    # Per-query results
    queries = latest.get("queries", [])
    if queries:
        st.subheader("Per-Query Results")

        # Bar chart: MRR per query
        fig = px.bar(
            queries, x="id", y="mrr",
            color="difficulty",
            color_discrete_map={"medium": "#636EFA", "hard": "#EF553B"},
            height=350,
        )
        fig.update_layout(yaxis_range=[0, 1], yaxis_title="MRR")
        st.plotly_chart(fig, use_container_width=True)

        # Top results for selected query
        selected_q = st.selectbox("Inspect query", [q["id"] for q in queries])
        q_data = next((q for q in queries if q["id"] == selected_q), None)
        if q_data and q_data.get("top_results"):
            st.write(f"**Query:** `{q_data['query']}`")
            st.dataframe(q_data["top_results"], use_container_width=True)


# ── NIAH BENCHMARK ───────────────────────────────────────────────────

elif selected_type == "niah":
    latest = runs[-1]
    agg = latest.get("aggregate", {})

    col1, col2, col3 = st.columns(3)
    col1.metric("Found Both", f"{agg.get('found', 0)}/{agg.get('total', 0)}")
    col2.metric("Partial", str(agg.get("partial", 0)))
    col3.metric("Found Rate", f"{agg.get('found_rate', 0):.0%}")

    # Heatmap
    niah_results = latest.get("results", [])
    if niah_results:
        st.subheader("Depth × Context Length Heatmap")

        depths = sorted(set(r["depth"] for r in niah_results))
        lengths = sorted(set(r["context_length"] for r in niah_results))

        z = []
        for depth in depths:
            row = []
            for length in lengths:
                r = next(
                    (r for r in niah_results if r["depth"] == depth and r["context_length"] == length),
                    None,
                )
                if r:
                    val = 2 if r["found_both"] else (1 if r["partial"] else 0)
                else:
                    val = -1
                row.append(val)
            z.append(row)

        fig = go.Figure(data=go.Heatmap(
            z=z,
            x=[str(l) for l in lengths],
            y=[f"{d:.2f}" for d in depths],
            colorscale=[[0, "#EF553B"], [0.5, "#FECB52"], [1, "#00CC96"]],
            zmin=0, zmax=2,
            text=[[{0: "MISS", 1: "PARTIAL", 2: "FOUND"}.get(v, "?") for v in row] for row in z],
            texttemplate="%{text}",
            hovertemplate="Depth: %{y}<br>Context: %{x} tokens<br>%{text}<extra></extra>",
        ))
        fig.update_layout(
            xaxis_title="Context Length (tokens)",
            yaxis_title="Needle Depth",
            height=400,
        )
        st.plotly_chart(fig, use_container_width=True)

    # Trend across runs
    if len(runs) > 1:
        st.subheader("Found Rate Trend")
        trend_data = []
        for r in runs:
            ts = parse_timestamp(r)
            a = r.get("aggregate", {})
            if ts and a:
                trend_data.append({
                    "timestamp": ts,
                    "found_rate": a.get("found_rate", 0),
                    "model": r.get("model", "?"),
                })
        if trend_data:
            fig = px.line(
                trend_data, x="timestamp", y="found_rate",
                markers=True, height=350,
            )
            fig.update_layout(yaxis_range=[0, 1], yaxis_title="Found Rate")
            st.plotly_chart(fig, use_container_width=True)


# ── UNKNOWN / GENERIC ────────────────────────────────────────────────

else:
    st.subheader("Raw Results")
    for r in runs:
        with st.expander(r.get("_file", "unknown")):
            st.json(r)


# ── Footer ───────────────────────────────────────────────────────────

st.markdown("---")
st.caption(
    f"📁 {len(all_results)} results loaded from `benchmark/results/` · "
    f"Auto-refresh: 10s"
)
