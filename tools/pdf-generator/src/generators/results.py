
import json
import statistics
import glob
import os
import time
from pathlib import Path
from datetime import datetime

DATA_DIR = Path("data")  # Symlink to benchmark/results

def get_latest_benchmark(prefix: str, max_age_hours: int = 24) -> dict:
    """Finds latest JSON, checks freshness, returns data."""
    pattern = str(DATA_DIR / f"{prefix}_*.json")
    files = glob.glob(pattern)
    
    if not files:
        raise FileNotFoundError(f"No benchmark files found for '{prefix}' in {DATA_DIR}")
    
    latest_file = max(files, key=os.path.getctime)
    timestamp = os.path.getctime(latest_file)
    age_hours = (time.time() - timestamp) / 3600
    
    print(f"   Reading: {os.path.basename(latest_file)} (Age: {age_hours:.2f}h)")
    
    if age_hours > max_age_hours:
        raise TimeoutError(
            f"❌ STALE DATA ALERT: {latest_file} is {age_hours:.1f}h old. "
            f"Limit is {max_age_hours}h. Re-run benchmarks!"
        )
        
    with open(latest_file, 'r') as f:
        return json.load(f)

def get_all_benchmarks(prefix: str, max_age_hours: int = 48) -> dict:
    """Returns the latest benchmark for EACH model found."""
    pattern = str(DATA_DIR / f"{prefix}_*.json")
    files = glob.glob(pattern)
    
    if not files:
        return {}
        
    # Group by model
    model_files = {}
    print(f"DEBUG: Globbing pattern: {pattern}")
    print(f"DEBUG: Found {len(files)} files.")
    
    for fp in files:
        try:
            with open(fp, 'r') as f:
                # Read only first few bytes to avoid loading huge JSONs if possible? 
                # No, standard JSON parsers need full read. Let's just read it.
                data = json.load(f)
                model_n = data.get('model', 'unknown')
                print(f"DEBUG: Processing {fp}, Model: {model_n}")
                
                # Check age
                ts = os.path.getctime(fp)
                age = (time.time() - ts) / 3600
                if age > max_age_hours:
                    continue
                    
                if model_n not in model_files:
                    model_files[model_n] = (fp, ts, data)
                else:
                    # Keep newer
                    if ts > model_files[model_n][1]:
                        model_files[model_n] = (fp, ts, data)
        except Exception:
            continue
            
    return {m: d[2] for m, d in model_files.items()}


def normalize_benchmark_data(data: dict) -> dict:
    """Normalizes 'coding' benchmark format to match the structure expected by the paper generator.

    IMPORTANT: Coding benchmarks measure coverage (weighted keyword+file+symbol recall),
    NOT F1. We keep backward-compat keys ('f1') mapped to coverage_score for consumers
    that haven't been updated yet, but add explicit 'coverage_score' and 'gqi' keys.
    """
    if 'aggregate' in data and 'results' in data:
        return data

    # Handle 'coding' format which has 'tasks' instead of 'results'
    if 'tasks' in data:
        tasks = data['tasks']
        results = []

        grounded_composites = []
        grounded_hallucinations = 0
        blind_composites = []
        blind_hallucinations = 0

        for t in tasks:
            grounded = t.get('single_synapseed', {})
            blind = t.get('single_blind', {})

            g_res = {
                'latency_s': grounded.get('latency_s', 0),
                'tokens': grounded.get('tokens', 0),
                'composite': grounded.get('composite', grounded.get('coverage_score', 0)),
                'coverage_score': grounded.get('coverage_score', grounded.get('composite', 0)),
                'keyword_recall': grounded.get('keyword_recall', grounded.get('keyword_score', 0)),
                'file_recall': grounded.get('file_recall', grounded.get('file_score', 0)),
                'symbol_recall': grounded.get('symbol_recall', grounded.get('symbol_score', 0)),
                'citation_precision': grounded.get('citation_precision', grounded.get('grounding_rate', 0)),
                'hallucinations': grounded.get('hallucinations', 0),
                # Backward compat
                'keyword_score': grounded.get('keyword_score', grounded.get('keyword_recall', 0)),
                'file_score': grounded.get('file_score', grounded.get('file_recall', 0)),
                'symbol_score': grounded.get('symbol_score', grounded.get('symbol_recall', 0)),
                'grounding_rate': grounded.get('grounding_rate', grounded.get('citation_precision', 0)),
            }
            b_res = {
                'latency_s': blind.get('latency_s', 0),
                'tokens': blind.get('tokens', 0),
                'composite': blind.get('composite', blind.get('coverage_score', 0)),
                'coverage_score': blind.get('coverage_score', blind.get('composite', 0)),
                'keyword_recall': blind.get('keyword_recall', blind.get('keyword_score', 0)),
                'file_recall': blind.get('file_recall', blind.get('file_score', 0)),
                'symbol_recall': blind.get('symbol_recall', blind.get('symbol_score', 0)),
                'citation_precision': blind.get('citation_precision', blind.get('grounding_rate', 0)),
                'hallucinations': blind.get('hallucinations', 0),
                # Backward compat
                'keyword_score': blind.get('keyword_score', blind.get('keyword_recall', 0)),
                'file_score': blind.get('file_score', blind.get('file_recall', 0)),
                'symbol_score': blind.get('symbol_score', blind.get('symbol_recall', 0)),
                'grounding_rate': blind.get('grounding_rate', blind.get('citation_precision', 0)),
            }

            results.append({
                'task_id': t.get('task_id'),
                'grounded': g_res,
                'blind': b_res,
            })

            grounded_composites.append(g_res['composite'])
            grounded_hallucinations += g_res['hallucinations']
            blind_composites.append(b_res['composite'])
            blind_hallucinations += b_res['hallucinations']

        # Build aggregate — use coverage_score, maintain backward compat 'f1' key
        g_mean = statistics.mean(grounded_composites) if grounded_composites else 0
        b_mean = statistics.mean(blind_composites) if blind_composites else 0
        g_recall = statistics.mean([r['grounded']['keyword_recall'] for r in results]) if results else 0
        b_recall = statistics.mean([r['blind']['keyword_recall'] for r in results]) if results else 0

        agg = {
            'grounded_f1': {
                'f1': g_mean,  # backward compat key — actually coverage_score
                'coverage_score': g_mean,
                'recall': g_recall,
            },
            'grounded_hallucinations': grounded_hallucinations,
            'blind_f1': {
                'f1': b_mean,  # backward compat key — actually coverage_score
                'coverage_score': b_mean,
                'recall': b_recall,
            },
            'blind_hallucinations': blind_hallucinations,
        }

        return {
            'metadata': data.get('metadata', {}),
            'aggregate': agg,
            'results': results,
            '_source': 'coding_normalized',
        }

    return data

def load_benchmark_data(prefix: str) -> dict:
    """Loads benchmark data. Raises FileNotFoundError if not found — no silent fallback.

    Previously this silently fell back to 'coding' data when other benchmarks
    were missing, which produced semantically wrong metrics in the paper
    (e.g., coding composite reported as MRR).
    """
    data = get_latest_benchmark(prefix)
    return data

def load_search_metrics() -> dict:
    """Loads search metrics (MRR, Recall, etc).

    Handles dynamic keys like precision_at_10 / precision_at_5
    by scanning for the pattern.
    """
    data = load_benchmark_data("search")
    agg = data['aggregate']
    # Normalize dynamic keys: precision_at_N -> precision_at_10
    normalized = {'mrr': agg.get('mrr', 0)}
    for key, val in agg.items():
        if key.startswith('precision_at_'):
            normalized['precision_at_10'] = val
        elif key.startswith('recall_at_'):
            normalized['recall_at_10'] = val
        elif key.startswith('file_hit_at_'):
            normalized['file_hit_at_10'] = val
    return normalized

def load_grounding_metrics() -> dict:
    """Loads grounding metrics.

    Note: 'coverage_score' is the weighted recall (keyword+file+symbol),
    'citation_precision' is fraction of cited paths that exist on disk,
    'gqi' is their harmonic mean (NOT a standard F1 score).
    Backward compat: 'f1' maps to 'gqi', 'recall' maps to 'coverage_score'.
    """
    data = load_benchmark_data("grounding")
    agg = data['aggregate']
    results = data.get('results', [])

    # New-format aggregate has 'gqi', 'coverage_score' keys
    grounded = agg.get('grounded_f1', {})
    gqi = grounded.get('gqi', grounded.get('f1', 0))
    coverage = grounded.get('coverage_score', grounded.get('recall', 0))

    return {
        "gqi": gqi,
        "coverage_score": coverage,
        "hallucinations": agg.get('grounded_hallucinations', 0),
        # Backward compat keys
        "f1": gqi,
        "recall": coverage,
        "raw_results": [
            {
                "latency": r['grounded'].get('latency_s', 0),
                "file_score": r['grounded'].get('file_recall', r['grounded'].get('file_score', 0))
            }
            for r in results
        ]
    }

def load_all_grounding_metrics() -> dict:
    """Returns metrics for ALL models found, broken down by mode."""
    # Try grounding first, then coding
    all_data = get_all_benchmarks("grounding")
    if not all_data:
         all_data = get_all_benchmarks("coding")
         # Normalize all
         all_data = {m: normalize_benchmark_data(d) for m, d in all_data.items()}

    metrics_by_model = {}
    
    for model, data in all_data.items():
        if 'aggregate' not in data:
             data = normalize_benchmark_data(data)
             
        agg = data['aggregate']
        
        # Calculate raw averages for blind vs synapseed to allow mode comparison
        results = data.get('results', [])
        is_normalized = data.get('_source') == 'coding_normalized'

        def _get_score(r, mode):
            """Extract comparable score from a result entry.
            Coding normalized data has 'composite' (0-1).
            Raw grounding data has 'score' (0-3) which we normalize to 0-1.
            """
            entry = r.get(mode, {})
            if 'composite' in entry:
                return entry['composite']
            if 'score' in entry:
                return entry['score'] / 3.0
            return entry.get('keyword_score', 0)

        blind_scores = [_get_score(r, 'blind') for r in results if 'blind' in r]
        grounded_scores = [_get_score(r, 'grounded') for r in results if 'grounded' in r]
        
        blind_tokens = [r['blind'].get('tokens', 0) for r in results if 'blind' in r]
        grounded_tokens = [r['grounded'].get('tokens', 0) for r in results if 'grounded' in r]
        
        metrics_by_model[model] = {
            "f1": agg.get('grounded_f1', {}).get('f1', 0),
            "recall": agg.get('grounded_f1', {}).get('recall', 0),
            "hallucinations": agg.get('grounded_hallucinations', 0),
            "latency": statistics.mean([r['grounded'].get('latency_s', 0) for r in results if 'grounded' in r and r['grounded'].get('latency_s', 0) > 0]),
            "cost": statistics.mean(grounded_tokens) if grounded_tokens else 0,
            
            # Mode specific breakdown
            "modes": {
                "blind": {
                    "f1": statistics.mean(blind_scores) if blind_scores else 0,
                    "cost": statistics.mean(blind_tokens) if blind_tokens else 0
                },
                "grounded": {
                    "f1": statistics.mean(grounded_scores) if grounded_scores else 0,
                    "cost": statistics.mean(grounded_tokens) if grounded_tokens else 0
                }
            }
        }
    return metrics_by_model

def generate_search_table() -> str:
    """Generates Table 1: Search Quality (MRR, Precision, Recall)."""
    agg = load_search_metrics()
    version = "v4.23"

    # LaTeX Template
    tex = r"""
\begin{table}[h]
\centering
\begin{tabular}{lcccc}
\toprule
\textbf{Metric} & \textbf{MRR} & \textbf{P@10} & \textbf{R@10} & \textbf{File Hit} \\
\midrule
Synapseed %s & %.3f & %.3f & %.3f & %.3f \\
\bottomrule
\end{tabular}
\caption{Search Benchmark Performance. 12 Queries.}
\label{tab:search-perf}
\end{table}
""" % (
        version,
        agg.get('mrr', 0),
        agg.get('precision_at_10', 0),
        agg.get('recall_at_10', 0),
        agg.get('file_hit_at_10', 0),
    )
    return tex


def generate_grounding_table() -> str:
    """Generates Table 2: Grounding Impact (Blind vs Grounded).

    Metrics reported:
    - Coverage Score (CS): Weighted recall over ground-truth keywords/files/symbols.
    - GQI: Grounding Quality Index (harmonic mean of CS and citation precision).
    - Hallucinations: Count of non-existent cited paths + forbidden keywords.
    """
    data = load_benchmark_data("grounding")
    agg = data['aggregate']
    metrics = load_grounding_metrics()

    # Try new keys first, fall back to legacy
    blind_agg = agg.get('blind_f1', {})
    grounded_agg = agg.get('grounded_f1', {})

    blind_cs = blind_agg.get('coverage_score', blind_agg.get('f1', 0))
    grounded_cs = grounded_agg.get('coverage_score', grounded_agg.get('f1', 0))
    blind_gqi = blind_agg.get('gqi', blind_agg.get('f1', 0))
    grounded_gqi = grounded_agg.get('gqi', grounded_agg.get('f1', 0))

    # Statistics (if available from new-format aggregate)
    stats = agg.get('statistics', {})
    p_value = stats.get('wilcoxon_p', None)
    effect_d = stats.get('cohens_d', None)

    delta_cs = grounded_cs - blind_cs
    lift_cs = (delta_cs / blind_cs) * 100 if blind_cs > 0 else 0
    delta_gqi = grounded_gqi - blind_gqi
    lift_gqi = (delta_gqi / blind_gqi) * 100 if blind_gqi > 0 else 0

    # Build stat footnote
    stat_note = ""
    if p_value is not None:
        sig = "$p < 0.05$" if p_value < 0.05 else f"$p = {p_value:.3f}$"
        stat_note = f"Wilcoxon signed-rank test: {sig}"
        if effect_d is not None:
            stat_note += f", Cohen's $d = {effect_d:.2f}$"
        stat_note += "."

    tex = r"""
\begin{table}[h]
\centering
\begin{tabular}{lccc}
\toprule
\textbf{Pipeline} & \textbf{Coverage Score} & \textbf{GQI} & \textbf{Hallucinations} \\
\midrule
Baseline (Blind) & %.3f & %.3f & %d \\
Synapseed (Grounded) & \textbf{%.3f} & \textbf{%.3f} & \textbf{%d} \\
\midrule
\textit{Improvement} & \textit{+%.1f\%%} & \textit{+%.1f\%%} & - \\
\bottomrule
\end{tabular}
\caption{Grounding Benchmark: Blind vs Synapseed-guided generation. Coverage Score measures weighted recall over keyword/file/symbol ground truth. GQI is the Grounding Quality Index (harmonic mean of coverage and citation precision). %s}
\label{tab:grounding-perf}
\end{table}
""" % (
        blind_cs, blind_gqi, agg.get('blind_hallucinations', 0),
        grounded_cs, grounded_gqi, agg.get('grounded_hallucinations', 0),
        lift_cs, lift_gqi, stat_note
    )
    return tex

def generate_all_tables(output_dir: Path):
    """Orchestrates table creation."""
    print("🚀 Starting Data Ingestion pipeline...")
    
    try:
        t1 = generate_search_table()
        with open(output_dir / "table_search.tex", "w") as f:
            f.write(t1)
        print("   ✅ Table 1 (Search) generated.")

        t2 = generate_grounding_table()
        with open(output_dir / "table_grounding.tex", "w") as f:
            f.write(t2)
        print("   ✅ Table 2 (Grounding) generated.")
        
    except Exception as e:
        print(f"\n❌ FATAL: Pipeline failed. {str(e)}")
        # Allow exit(1) to propagate failure
        raise e
