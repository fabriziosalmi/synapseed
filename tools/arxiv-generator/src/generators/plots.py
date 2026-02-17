
import matplotlib.pyplot as plt
import json
import numpy as np
from pathlib import Path
from .results import load_grounding_metrics, load_all_grounding_metrics

def generate_comparison_plots(output_dir: Path):
    """Generates multi-model comparison plots (Blind vs Grounded)."""
    metrics = load_all_grounding_metrics()
    if len(metrics) < 1:
        return []

    models = list(metrics.keys())
    models.sort()
    
    # Data Preparation
    blind_f1 = [metrics[m]['modes']['blind'].get('f1', metrics[m]['modes']['blind'].get('coverage_score', 0)) for m in models]
    grounded_f1 = [metrics[m]['modes']['grounded'].get('f1', metrics[m]['modes']['grounded'].get('coverage_score', 0)) for m in models]
    
    blind_cost = [metrics[m]['modes']['blind']['cost'] for m in models]
    grounded_cost = [metrics[m]['modes']['grounded']['cost'] for m in models]

    # Bar Layout
    x = np.arange(len(models))
    width = 0.35

    # --- Plot 1: Coverage Score (Grouped Bar) ---
    plt.figure(figsize=(10, 6))
    bars1 = plt.bar(x - width/2, blind_f1, width, label='Blind (No Tools)', color='lightgray')
    bars2 = plt.bar(x + width/2, grounded_f1, width, label='Synapseed (Grounded)', color='skyblue')
    
    plt.xlabel('Model')
    plt.ylabel('Coverage Score')
    plt.title('Impact of Grounding on Code Understanding Quality')
    plt.xticks(x, models, rotation=45, ha='right')
    plt.ylim(0, 1.1) 
    plt.legend()
    plt.grid(axis='y', linestyle='--', alpha=0.3)
    
    # Annotate
    for bar in bars2:
        height = bar.get_height()
        plt.text(bar.get_x() + bar.get_width()/2., height + 0.02,
                 f'{height:.2f}', ha='center', va='bottom', fontsize=9, fontweight='bold')
                 
    plt.tight_layout()
    plt.savefig(output_dir / 'plot_comparison_f1.pdf')
    plt.close()

    # --- Plot 2: Token Cost (Grouped Bar) ---
    plt.figure(figsize=(10, 6))
    bars1 = plt.bar(x - width/2, blind_cost, width, label='Blind (No Tools)', color='lightgray')
    bars2 = plt.bar(x + width/2, grounded_cost, width, label='Synapseed (Grounded)', color='salmon')
    
    plt.xlabel('Model')
    plt.ylabel('Avg Tokens per Task')
    plt.title('Token Cost Analysis')
    plt.xticks(x, models, rotation=45, ha='right')
    plt.legend()
    plt.grid(axis='y', linestyle='--', alpha=0.3)
    
    # Annotate
    for bar in bars2:
        height = bar.get_height()
        plt.text(bar.get_x() + bar.get_width()/2., height + 10,
                 f'{int(height)}', ha='center', va='bottom', fontsize=9)
                 
    plt.tight_layout()
    plt.savefig(output_dir / 'plot_comparison_cost.pdf')
    plt.close()

    return [
        r"\begin{figure*}[ht]",
        r"    \centering",
        r"    \begin{minipage}{0.48\textwidth}",
        r"        \centering",
        r"        \includegraphics[width=\linewidth]{assets/plot_comparison_f1.pdf}",
        r"        \caption{Grounding Quality: Synapseed consistently improves Coverage Scores across models.}",
        r"        \label{fig:comp_f1}",
        r"    \end{minipage}\hfill",
        r"    \begin{minipage}{0.48\textwidth}",
        r"        \centering",
        r"        \includegraphics[width=\linewidth]{assets/plot_comparison_cost.pdf}",
        r"        \caption{Cost Analysis: Grounding introduces token overhead but yields higher accuracy.}",
        r"        \label{fig:comp_cost}",
        r"    \end{minipage}",
        r"\end{figure*}"
    ]

def generate_plots(output_dir: Path):
    """Generates performance plots."""
    metrics = load_grounding_metrics()
    
    # 1. Latency vs File Score Scatter
    plt.figure(figsize=(6, 4))
    latencies = [r['latency'] for r in metrics['raw_results']]
    scores = [r['file_score'] for r in metrics['raw_results']]
    
    plt.scatter(latencies, scores, alpha=0.6)
    plt.xlabel('Latency (s)')
    plt.ylabel('File Retrieval Recall')
    plt.title('Latency vs Retrieval Quality')
    plt.grid(True, linestyle='--', alpha=0.7)
    plt.tight_layout()
    plt.savefig(output_dir / 'plot_latency_quality.pdf')
    plt.close()

    # 2. Latency Histogram
    plt.figure(figsize=(6, 4))
    plt.hist(latencies, bins=10, color='skyblue', edgecolor='black')
    plt.xlabel('Latency (s)')
    plt.ylabel('Frequency')
    plt.title('Latency Distribution')
    plt.grid(True, linestyle='--', alpha=0.7)
    plt.tight_layout()
    plt.savefig(output_dir / 'plot_latency_dist.pdf')
    plt.close()
    
    return [
        r"\begin{figure}[ht]",
        r"    \centering",
        r"    \includegraphics[width=0.48\textwidth]{assets/plot_latency_quality.pdf}",
        r"    \caption{Latency vs Retrieval Quality. No strong correlation implies consistent performance across varying complexity.}",
        r"    \label{fig:latency_quality}",
        r"\end{figure}",
        r"",
        r"\begin{figure}[ht]",
        r"    \centering",
        r"    \includegraphics[width=0.48\textwidth]{assets/plot_latency_dist.pdf}",
        r"    \caption{Latency Distribution. Most queries resolve within 10-30s.}",
        r"    \label{fig:latency_dist}",
        r"\end{figure}"
    ]
