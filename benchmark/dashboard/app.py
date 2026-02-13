"""SYNAPSEED Benchmark Dashboard — local metrics viewer.

Launch:
    cd benchmark
    source venv/bin/activate
    streamlit run dashboard/app.py
"""

from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path

import plotly.graph_objects as go
import streamlit as st

# ── Config ───────────────────────────────────────────────────────────

RESULTS_DIR = Path(__file__).resolve().parent.parent / "results"

PLOT_LAYOUT = dict(
    template="plotly_dark",
    paper_bgcolor="rgba(0,0,0,0)",
    plot_bgcolor="rgba(0,0,0,0)",
    font=dict(family="Inter, system-ui, sans-serif"),
    margin=dict(l=40, r=20, t=40, b=40),
)

# Colors
C_BLIND = "#ef4444"     # red
C_SYNAPSEED = "#3b82f6" # blue
C_DELTA = "#22c55e"     # green
C_WARN = "#f59e0b"      # amber
C_MUTED = "#6b7280"     # gray

st.set_page_config(
    page_title="SYNAPSEED Benchmarks",
    page_icon="S",
    layout="wide",
    initial_sidebar_state="collapsed",
)

# ── Custom CSS ───────────────────────────────────────────────────────

st.markdown("""
<style>
    /* Dark clean theme */
    .stApp { background-color: #0f1117; }
    [data-testid="stSidebar"] { background-color: #1a1b26; }

    /* Metric cards */
    [data-testid="stMetric"] {
        background: linear-gradient(135deg, #1e1e2e 0%, #1a1b26 100%);
        border: 1px solid #2d2d3d;
        border-radius: 12px;
        padding: 16px 20px;
    }
    [data-testid="stMetricLabel"] {
        font-size: 0.75rem !important;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        color: #9ca3af !important;
    }
    [data-testid="stMetricValue"] {
        font-size: 1.8rem !important;
        font-weight: 700 !important;
    }
    [data-testid="stMetricDelta"] {
        font-size: 0.9rem !important;
    }

    /* Headers */
    h1 { font-weight: 800 !important; letter-spacing: -0.02em; }
    h2 { font-weight: 700 !important; color: #e5e7eb !important; }
    h3 { font-weight: 600 !important; color: #9ca3af !important; font-size: 0.9rem !important; text-transform: uppercase; letter-spacing: 0.05em; }

    /* Tables */
    [data-testid="stDataFrame"] { border-radius: 8px; overflow: hidden; }

    /* Tabs */
    .stTabs [data-baseweb="tab-list"] { gap: 4px; }
    .stTabs [data-baseweb="tab"] {
        background: #1e1e2e;
        border-radius: 8px 8px 0 0;
        padding: 8px 20px;
        font-weight: 600;
    }

    /* Score badge */
    .score-badge {
        display: inline-block;
        padding: 4px 12px;
        border-radius: 20px;
        font-weight: 700;
        font-size: 0.85rem;
    }
    .score-good { background: #16a34a22; color: #22c55e; border: 1px solid #22c55e44; }
    .score-ok { background: #f59e0b22; color: #f59e0b; border: 1px solid #f59e0b44; }
    .score-bad { background: #ef444422; color: #ef4444; border: 1px solid #ef444444; }
</style>
""", unsafe_allow_html=True)


# ── Data Loading ─────────────────────────────────────────────────────

@st.cache_data(ttl=10)
def load_all_results() -> list[dict]:
    results = []
    if not RESULTS_DIR.exists():
        return results
    for f in sorted(RESULTS_DIR.glob("*.json")):
        try:
            data = json.loads(f.read_text())
            data["_file"] = f.name
            results.append(data)
        except (json.JSONDecodeError, KeyError):
            continue
    return results


def classify(result: dict) -> str:
    bm = result.get("metadata", {}).get("benchmark", "").lower()
    if bm:
        return bm
    name = result.get("_file", "")
    for t in ("coding", "grounding", "search", "niah"):
        if t in name:
            return t
    return "unknown"


def parse_ts(result: dict) -> datetime | None:
    ts = result.get("metadata", {}).get("timestamp", "")
    if not ts:
        return None
    try:
        return datetime.strptime(ts, "%Y%m%d_%H%M%S").replace(tzinfo=timezone.utc)
    except ValueError:
        return None


def score_badge(val: float, thresholds=(0.7, 0.4)) -> str:
    good, ok = thresholds
    if val >= good:
        cls = "score-good"
    elif val >= ok:
        cls = "score-ok"
    else:
        cls = "score-bad"
    return f'<span class="score-badge {cls}">{val:.0%}</span>'


def make_gauge(value: float, title: str, color: str = C_SYNAPSEED) -> go.Figure:
    fig = go.Figure(go.Indicator(
        mode="gauge+number",
        value=value * 100,
        number={"suffix": "%", "font": {"size": 36, "color": "white"}},
        gauge={
            "axis": {"range": [0, 100], "tickwidth": 0, "tickcolor": "rgba(0,0,0,0)"},
            "bar": {"color": color, "thickness": 0.7},
            "bgcolor": "#1e1e2e",
            "borderwidth": 0,
            "steps": [
                {"range": [0, 40], "color": "#1e1e2e"},
                {"range": [40, 70], "color": "#1e1e2e"},
                {"range": [70, 100], "color": "#1e1e2e"},
            ],
            "threshold": {
                "line": {"color": C_WARN, "width": 2},
                "thickness": 0.8,
                "value": 70,
            },
        },
    ))
    gauge_layout = {k: v for k, v in PLOT_LAYOUT.items() if k != "margin"}
    fig.update_layout(
        **gauge_layout,
        height=180,
        margin=dict(l=20, r=20, t=10, b=10),
    )
    return fig


# ── Load Data ────────────────────────────────────────────────────────

all_results = load_all_results()

if not all_results:
    st.markdown("# SYNAPSEED Benchmark Dashboard")
    st.markdown("---")
    st.info("No results yet. Run a benchmark to get started:")
    st.code("""cd benchmark && source venv/bin/activate
python run.py coding --quick
python run.py grounding --quick
python run.py search""", language="bash")
    st.stop()

by_type: dict[str, list[dict]] = {}
for r in all_results:
    by_type.setdefault(classify(r), []).append(r)


# ── Header ───────────────────────────────────────────────────────────

col_title, col_meta = st.columns([3, 1])
with col_title:
    st.markdown("# SYNAPSEED Benchmarks")
with col_meta:
    latest_version = "?"
    for r in reversed(all_results):
        v = r.get("metadata", {}).get("version", "")
        if v:
            latest_version = v
            break
    st.markdown(
        f"<div style='text-align:right; padding-top:12px; color:#6b7280'>"
        f"<b style='color:#e5e7eb'>v{latest_version}</b> &middot; "
        f"{len(all_results)} runs</div>",
        unsafe_allow_html=True,
    )

st.markdown("---")


# ── Overview Cards ───────────────────────────────────────────────────

def overview_card(bm_type: str, runs: list[dict]) -> dict:
    """Extract key metric for overview display."""
    latest = runs[-1]
    agg = latest.get("aggregate", {})
    model = latest.get("model", "")
    ts = parse_ts(latest)
    time_str = ts.strftime("%H:%M") if ts else "?"

    if bm_type == "coding":
        blind = agg.get("blind_mean", 0)
        syn = agg.get("synapseed_mean", 0)
        delta = syn - blind
        return {"label": "Coding", "value": syn, "delta": delta,
                "detail": f"BLIND {blind:.0%}", "model": model, "time": time_str}
    elif bm_type == "grounding":
        gf1 = agg.get("grounded_f1", {}).get("f1", 0)
        bf1 = agg.get("blind_f1", {}).get("f1", 0)
        return {"label": "Grounding F1", "value": gf1, "delta": gf1 - bf1,
                "detail": f"BLIND {bf1:.0%}", "model": model, "time": time_str}
    elif bm_type == "search":
        mrr = agg.get("mrr", 0)
        return {"label": "Search MRR", "value": mrr, "delta": None,
                "detail": f"File Hit {agg.get('file_hit_at_10', agg.get('file_hit_at_5', 0)):.0%}",
                "model": model, "time": time_str}
    elif bm_type == "niah":
        rate = agg.get("found_rate", 0)
        return {"label": "NIAH Found", "value": rate, "delta": None,
                "detail": f"{agg.get('found', 0)}/{agg.get('total', 0)}",
                "model": model, "time": time_str}
    return {"label": bm_type, "value": 0, "delta": None, "detail": "", "model": "", "time": ""}


cards = []
for t in ("coding", "grounding", "search", "niah"):
    if t in by_type:
        cards.append((t, overview_card(t, by_type[t])))

if cards:
    cols = st.columns(len(cards))
    for col, (bm_type, card) in zip(cols, cards):
        with col:
            delta_str = f"{card['delta']:+.0%}" if card["delta"] is not None else None
            st.metric(
                label=card["label"],
                value=f"{card['value']:.0%}",
                delta=delta_str,
                help=f"Model: {card['model']} | {card['detail']}",
            )
            st.caption(f"{card['detail']} | {card['model']} | {card['time']}")

st.markdown("")


# ── Tabs ─────────────────────────────────────────────────────────────

available = [t for t in ("coding", "grounding", "search", "niah") if t in by_type]
tab_labels = [t.upper() for t in available]
tabs = st.tabs(tab_labels)


for tab, bm_type in zip(tabs, available):
    runs = by_type[bm_type]
    latest = runs[-1]
    agg = latest.get("aggregate", {})

    with tab:

        # ── CODING ──────────────────────────────────────────────
        if bm_type == "coding":
            st.markdown("### Does SYNAPSEED context improve LLM answers?")

            blind_mean = agg.get("blind_mean", 0)
            syn_mean = agg.get("synapseed_mean", 0)
            delta = syn_mean - blind_mean

            # Key metrics row
            c1, c2, c3, c4 = st.columns(4)
            c1.metric("BLIND Score", f"{blind_mean:.0%}")
            c2.metric("SYNAPSEED Score", f"{syn_mean:.0%}",
                       delta=f"{delta:+.0%} vs BLIND")
            c3.metric("Hallucinations (B/S)",
                       f"{agg.get('blind_hallucinations', 0)} / {agg.get('synapseed_hallucinations', 0)}")
            c4.metric("Tasks", str(len(latest.get("tasks", []))))

            # Gauge + Trend side by side
            g_col, t_col = st.columns([1, 2])

            with g_col:
                st.markdown("#### SYNAPSEED Accuracy")
                st.plotly_chart(make_gauge(syn_mean, "SYNAPSEED"), use_container_width=True)

            with t_col:
                if len(runs) > 1:
                    st.markdown("#### Historical Trend")
                    fig = go.Figure()
                    xs, blind_ys, syn_ys, labels = [], [], [], []
                    for r in runs:
                        ts = parse_ts(r)
                        a = r.get("aggregate", {})
                        if ts and a:
                            model = r.get("model", "?")
                            version = r.get("metadata", {}).get("version", "?")
                            xs.append(ts)
                            blind_ys.append(a.get("blind_mean", 0))
                            syn_ys.append(a.get("synapseed_mean", 0))
                            labels.append(f"v{version} | {model}")
                    fig.add_trace(go.Scatter(x=xs, y=blind_ys, name="BLIND",
                        mode="lines+markers", line=dict(color=C_BLIND, width=2),
                        hovertext=labels, marker=dict(size=8)))
                    fig.add_trace(go.Scatter(x=xs, y=syn_ys, name="SYNAPSEED",
                        mode="lines+markers", line=dict(color=C_SYNAPSEED, width=2),
                        hovertext=labels, marker=dict(size=8)))
                    fig.update_layout(**PLOT_LAYOUT, height=220,
                        yaxis=dict(range=[0, 1], title="Score"),
                        legend=dict(orientation="h", y=1.15))
                    st.plotly_chart(fig, use_container_width=True)
                else:
                    # Single run — show difficulty breakdown as bar chart
                    by_diff = agg.get("by_difficulty", {})
                    if by_diff:
                        st.markdown("#### By Difficulty")
                        diffs, blind_vals, syn_vals = [], [], []
                        for d, s in by_diff.items():
                            diffs.append(d.upper())
                            blind_vals.append(s.get("blind", 0))
                            syn_vals.append(s.get("synapseed", 0))
                        fig = go.Figure()
                        fig.add_trace(go.Bar(x=diffs, y=blind_vals, name="BLIND",
                            marker_color=C_BLIND, opacity=0.85))
                        fig.add_trace(go.Bar(x=diffs, y=syn_vals, name="SYNAPSEED",
                            marker_color=C_SYNAPSEED, opacity=0.85))
                        fig.update_layout(**PLOT_LAYOUT, barmode="group", height=220,
                            yaxis=dict(range=[0, 1], title="Score"),
                            legend=dict(orientation="h", y=1.15))
                        st.plotly_chart(fig, use_container_width=True)

            # Per-task detail
            tasks = latest.get("tasks", [])
            if tasks:
                st.markdown("#### Per-Task Breakdown")
                task_ids, blind_scores, syn_scores, difficulties = [], [], [], []
                for t in tasks:
                    task_ids.append(t["task_id"].replace("_", " ").title())
                    b = t.get("single_blind", {}).get("composite", 0)
                    s = t.get("single_synapseed", {}).get("composite", 0)
                    blind_scores.append(b)
                    syn_scores.append(s)
                    difficulties.append(t["difficulty"])

                fig = go.Figure()
                fig.add_trace(go.Bar(
                    y=task_ids, x=blind_scores, name="BLIND",
                    orientation="h", marker_color=C_BLIND, opacity=0.8,
                ))
                fig.add_trace(go.Bar(
                    y=task_ids, x=syn_scores, name="SYNAPSEED",
                    orientation="h", marker_color=C_SYNAPSEED, opacity=0.8,
                ))
                fig.update_layout(**PLOT_LAYOUT, barmode="group",
                    height=max(200, len(tasks) * 50),
                    xaxis=dict(range=[0, 1], title="Composite Score"),
                    legend=dict(orientation="h", y=1.08))
                st.plotly_chart(fig, use_container_width=True)


        # ── GROUNDING ───────────────────────────────────────────
        elif bm_type == "grounding":
            st.markdown("### How well does MCP context ground LLM answers?")

            bf1 = agg.get("blind_f1", {})
            gf1 = agg.get("grounded_f1", {})

            c1, c2, c3, c4 = st.columns(4)
            c1.metric("Grounded F1", f"{gf1.get('f1', 0):.0%}",
                       delta=f"{gf1.get('f1', 0) - bf1.get('f1', 0):+.0%} vs BLIND")
            c2.metric("Precision", f"{gf1.get('precision', 0):.0%}",
                       delta=f"{gf1.get('precision', 0) - bf1.get('precision', 0):+.0%}")
            c3.metric("Recall", f"{gf1.get('recall', 0):.0%}",
                       delta=f"{gf1.get('recall', 0) - bf1.get('recall', 0):+.0%}")
            perfect = sum(1 for r in latest.get("results", [])
                         if r.get("grounded", {}).get("score", 0) == 3.0)
            c4.metric("Perfect Scores", f"{perfect}/{len(latest.get('results', []))}")

            # Gauges row
            g1, g2, g3 = st.columns(3)
            with g1:
                st.plotly_chart(make_gauge(gf1.get("precision", 0), "Precision"), use_container_width=True)
                st.caption("Precision")
            with g2:
                st.plotly_chart(make_gauge(gf1.get("recall", 0), "Recall"), use_container_width=True)
                st.caption("Recall")
            with g3:
                st.plotly_chart(make_gauge(gf1.get("f1", 0), "F1"), use_container_width=True)
                st.caption("F1")

            # Per-question waterfall
            results_list = latest.get("results", [])
            if results_list:
                st.markdown("#### Per-Question Scores")
                ids, blind_sc, ground_sc, colors = [], [], [], []
                for r in results_list:
                    ids.append(r["id"])
                    b = r.get("blind", {}).get("score", 0)
                    g = r.get("grounded", {}).get("score", 0)
                    blind_sc.append(b)
                    ground_sc.append(g)

                fig = go.Figure()
                fig.add_trace(go.Bar(
                    x=ids, y=blind_sc, name="BLIND",
                    marker_color=C_BLIND, opacity=0.7,
                ))
                fig.add_trace(go.Bar(
                    x=ids, y=ground_sc, name="GROUNDED",
                    marker_color=C_SYNAPSEED, opacity=0.85,
                ))
                fig.add_hline(y=3.0, line_dash="dot", line_color=C_MUTED,
                             annotation_text="Perfect", annotation_position="top right")
                fig.update_layout(**PLOT_LAYOUT, barmode="group", height=300,
                    yaxis=dict(range=[0, 3.2], title="Score (0-3)"),
                    legend=dict(orientation="h", y=1.08))
                st.plotly_chart(fig, use_container_width=True)


        # ── SEARCH ──────────────────────────────────────────────
        elif bm_type == "search":
            st.markdown("### How well does Tantivy find the right symbols?")

            mrr = agg.get("mrr", 0)
            # Find precision/recall keys dynamically
            p_key = next((k for k in agg if k.startswith("precision")), None)
            r_key = next((k for k in agg if k.startswith("recall")), None)
            fh_key = next((k for k in agg if k.startswith("file_hit")), None)

            c1, c2, c3, c4 = st.columns(4)
            c1.metric("MRR", f"{mrr:.0%}",
                       help="Mean Reciprocal Rank: how high the first correct result ranks")
            if p_key:
                c2.metric("Precision", f"{agg[p_key]:.0%}",
                           help="Fraction of top-K results that are relevant")
            if r_key:
                c3.metric("Recall", f"{agg[r_key]:.0%}",
                           help="Fraction of relevant symbols found in top-K")
            if fh_key:
                c4.metric("File Hit", f"{agg[fh_key]:.0%}",
                           help="Fraction of correct files found in top-K")

            # Per-query chart
            queries = latest.get("queries", [])
            if queries:
                g_col, d_col = st.columns([1, 2])

                with g_col:
                    st.markdown("#### Overall MRR")
                    st.plotly_chart(make_gauge(mrr, "MRR"), use_container_width=True)

                with d_col:
                    st.markdown("#### MRR by Query")
                    qids = [q["id"].replace("s", "S").replace("_", " ") for q in queries]
                    mrrs = [q.get("mrr", 0) for q in queries]
                    bar_colors = [C_SYNAPSEED if m >= 0.5 else (C_WARN if m > 0 else C_BLIND)
                                  for m in mrrs]
                    fig = go.Figure(go.Bar(
                        y=qids, x=mrrs, orientation="h",
                        marker_color=bar_colors, opacity=0.85,
                        text=[f"{m:.0%}" for m in mrrs],
                        textposition="auto",
                    ))
                    fig.add_vline(x=0.5, line_dash="dot", line_color=C_MUTED)
                    fig.update_layout(**PLOT_LAYOUT,
                        height=max(250, len(queries) * 32),
                        xaxis=dict(range=[0, 1], title="MRR"),
                        showlegend=False)
                    st.plotly_chart(fig, use_container_width=True)

                # Failed queries highlight
                failed = [q for q in queries if q.get("mrr", 0) == 0]
                if failed:
                    st.markdown("#### Needs Improvement")
                    for q in failed:
                        st.markdown(
                            f"- **{q['id']}** `{q['query']}` — "
                            f"{q.get('results_count', 0)} results but no relevant symbol in top-K"
                        )


        # ── NIAH ────────────────────────────────────────────────
        elif bm_type == "niah":
            st.markdown("### Can the LLM find specific facts in the context?")

            found_rate = agg.get("found_rate", 0)
            found = agg.get("found", 0)
            total = agg.get("total", 0)
            partial = agg.get("partial", 0)

            c1, c2, c3 = st.columns(3)
            c1.metric("Found Rate", f"{found_rate:.0%}")
            c2.metric("Found / Total", f"{found} / {total}")
            c3.metric("Partial", str(partial))

            # Heatmap
            niah_results = latest.get("results", [])
            if niah_results:
                st.markdown("#### Depth x Context Heatmap")
                depths = sorted(set(r["depth"] for r in niah_results))
                lengths = sorted(set(r["context_length"] for r in niah_results))

                z, text_matrix = [], []
                for depth in depths:
                    row, text_row = [], []
                    for length in lengths:
                        r = next(
                            (r for r in niah_results
                             if r["depth"] == depth and r["context_length"] == length), None)
                        if r:
                            val = 2 if r["found_both"] else (1 if r["partial"] else 0)
                        else:
                            val = -1
                        row.append(val)
                        text_row.append({0: "MISS", 1: "PARTIAL", 2: "FOUND"}.get(val, "?"))
                    z.append(row)
                    text_matrix.append(text_row)

                fig = go.Figure(go.Heatmap(
                    z=z,
                    x=[f"{l:,}" for l in lengths],
                    y=[f"{d:.0%}" for d in depths],
                    colorscale=[[0, C_BLIND], [0.5, C_WARN], [1, C_DELTA]],
                    zmin=0, zmax=2,
                    text=text_matrix, texttemplate="%{text}",
                    textfont=dict(size=14, color="white"),
                    hovertemplate="Depth: %{y}<br>Context: %{x} tokens<br>%{text}<extra></extra>",
                    showscale=False,
                ))
                fig.update_layout(**PLOT_LAYOUT, height=max(200, len(depths) * 60),
                    xaxis_title="Context Length (tokens)",
                    yaxis_title="Needle Depth")
                st.plotly_chart(fig, use_container_width=True)


# ── Footer ───────────────────────────────────────────────────────────

st.markdown("---")
st.markdown(
    f"<div style='text-align:center; color:#4b5563; font-size:0.8rem'>"
    f"{len(all_results)} result(s) &middot; Auto-refresh 10s &middot; "
    f"<code>{RESULTS_DIR}</code></div>",
    unsafe_allow_html=True,
)
