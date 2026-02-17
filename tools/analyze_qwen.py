#!/usr/bin/env python3
"""
Analisi comparativa dei risultati benchmark per i modelli Qwen
"""
import json
import statistics
from pathlib import Path
from typing import Dict, List

def load_benchmark(path: Path) -> dict:
    """Carica un file benchmark JSON"""
    with open(path) as f:
        return json.load(f)

def analyze_coding_benchmark(data: dict) -> dict:
    """Analizza un benchmark coding e calcola metriche aggregate"""
    model = data.get('model', 'unknown')
    tasks = data.get('tasks', [])
    
    # Condizioni da analizzare
    conditions = [
        'single_blind',
        'single_synapseed', 
        'single_blind_opt',
        'single_synapseed_opt',
        'single_synapseed_nothink',
        'single_synapseed_opt_nothink'
    ]
    
    results = {}
    
    for cond in conditions:
        # Raccogli metriche per questa condizione
        composites = []
        tokens = []
        latencies = []
        grounding_rates = []
        
        for task in tasks:
            if cond in task:
                t = task[cond]
                if 'composite' in t:
                    composites.append(t['composite'])
                if 'tokens' in t:
                    tokens.append(t['tokens'])
                if 'latency_s' in t:
                    latencies.append(t['latency_s'])
                if 'grounding_rate' in t:
                    grounding_rates.append(t['grounding_rate'])
        
        if composites:
            results[cond] = {
                'f1_mean': statistics.mean(composites),
                'f1_median': statistics.median(composites),
                'tokens_mean': statistics.mean(tokens) if tokens else 0,
                'latency_mean': statistics.mean(latencies) if latencies else 0,
                'grounding_mean': statistics.mean(grounding_rates) if grounding_rates else 0,
                'count': len(composites)
            }
    
    return {
        'model': model,
        'type': 'coding',
        'timestamp': data.get('metadata', {}).get('timestamp', ''),
        'results': results
    }

def analyze_grounding_benchmark(data: dict) -> dict:
    """Analizza un benchmark grounding"""
    model = data.get('model', 'unknown')
    results_data = data.get('results', [])
    
    blind_f1s = []
    grounded_f1s = []
    blind_tokens = []
    grounded_tokens = []
    blind_latencies = []
    grounded_latencies = []
    
    for r in results_data:
        if 'blind' in r:
            blind_f1s.append(r['blind'].get('f1', 0))
            blind_tokens.append(r['blind'].get('tokens', 0))
            blind_latencies.append(r['blind'].get('latency_s', 0))
        if 'grounded' in r:
            grounded_f1s.append(r['grounded'].get('f1', 0))
            grounded_tokens.append(r['grounded'].get('tokens', 0))
            grounded_latencies.append(r['grounded'].get('latency_s', 0))
    
    return {
        'model': model,
        'type': 'grounding',
        'timestamp': data.get('metadata', {}).get('timestamp', ''),
        'results': {
            'blind': {
                'f1_mean': statistics.mean(blind_f1s) if blind_f1s else 0,
                'tokens_mean': statistics.mean(blind_tokens) if blind_tokens else 0,
                'latency_mean': statistics.mean(blind_latencies) if blind_latencies else 0,
                'count': len(blind_f1s)
            },
            'grounded': {
                'f1_mean': statistics.mean(grounded_f1s) if grounded_f1s else 0,
                'tokens_mean': statistics.mean(grounded_tokens) if grounded_tokens else 0,
                'latency_mean': statistics.mean(grounded_latencies) if grounded_latencies else 0,
                'count': len(grounded_f1s)
            }
        }
    }

def main():
    # Trova tutti i file benchmark qwen
    benchmark_dir = Path('/Users/fab/Documents/git/synapseed/benchmark/results')
    
    coding_files = sorted(benchmark_dir.glob('coding_*qwen*.json'))
    grounding_files = sorted(benchmark_dir.glob('grounding_*qwen*.json'))
    
    print("=" * 80)
    print("ANALISI BENCHMARK QWEN - CODING")
    print("=" * 80)
    
    coding_results = []
    for f in coding_files:
        data = load_benchmark(f)
        analysis = analyze_coding_benchmark(data)
        coding_results.append(analysis)
        
        print(f"\n📊 Modello: {analysis['model']}")
        print(f"   Timestamp: {analysis['timestamp']}")
        print(f"   File: {f.name}")
        print()
        
        # Confronto blind vs synapseed
        if 'single_blind' in analysis['results'] and 'single_synapseed' in analysis['results']:
            blind = analysis['results']['single_blind']
            synapseed = analysis['results']['single_synapseed']
            
            f1_improvement = ((synapseed['f1_mean'] - blind['f1_mean']) / blind['f1_mean'] * 100) if blind['f1_mean'] > 0 else 0
            token_increase = ((synapseed['tokens_mean'] - blind['tokens_mean']) / blind['tokens_mean'] * 100) if blind['tokens_mean'] > 0 else 0
            
            print(f"   🎯 F1 Score:")
            print(f"      Blind:     {blind['f1_mean']:.3f}")
            print(f"      Synapseed: {synapseed['f1_mean']:.3f} ({f1_improvement:+.1f}%)")
            print(f"   💰 Tokens:")
            print(f"      Blind:     {blind['tokens_mean']:.0f}")
            print(f"      Synapseed: {synapseed['tokens_mean']:.0f} ({token_increase:+.1f}%)")
            print(f"   ⚡ Latency:")
            print(f"      Blind:     {blind['latency_mean']:.2f}s")
            print(f"      Synapseed: {synapseed['latency_mean']:.2f}s")
            print(f"   📈 Grounding Rate: {synapseed['grounding_mean']:.3f}")
        
        # Mostra tutte le condizioni
        print(f"\n   📋 Tutte le condizioni:")
        for cond, metrics in sorted(analysis['results'].items()):
            print(f"      {cond:30s} F1={metrics['f1_mean']:.3f} Tokens={metrics['tokens_mean']:.0f}")
    
    print("\n" + "=" * 80)
    print("ANALISI BENCHMARK QWEN - GROUNDING")
    print("=" * 80)
    
    grounding_results = []
    for f in grounding_files:
        data = load_benchmark(f)
        analysis = analyze_grounding_benchmark(data)
        grounding_results.append(analysis)
        
        print(f"\n📊 Modello: {analysis['model']}")
        print(f"   Timestamp: {analysis['timestamp']}")
        print(f"   File: {f.name}")
        print()
        
        blind = analysis['results']['blind']
        grounded = analysis['results']['grounded']
        
        if blind['f1_mean'] > 0:
            f1_improvement = ((grounded['f1_mean'] - blind['f1_mean']) / blind['f1_mean'] * 100)
            token_change = ((grounded['tokens_mean'] - blind['tokens_mean']) / blind['tokens_mean'] * 100) if blind['tokens_mean'] > 0 else 0
            
            print(f"   🎯 F1 Score:")
            print(f"      Blind:    {blind['f1_mean']:.3f}")
            print(f"      Grounded: {grounded['f1_mean']:.3f} ({f1_improvement:+.1f}%)")
            print(f"   💰 Tokens:")
            print(f"      Blind:    {blind['tokens_mean']:.0f}")
            print(f"      Grounded: {grounded['tokens_mean']:.0f} ({token_change:+.1f}%)")
            print(f"   ⚡ Latency:")
            print(f"      Blind:    {blind['latency_mean']:.2f}s")
            print(f"      Grounded: {grounded['latency_mean']:.2f}s")
    
    # Confronto tra modelli
    print("\n" + "=" * 80)
    print("CONFRONTO TRA MODELLI (CODING - SYNAPSEED)")
    print("=" * 80)
    print()
    print(f"{'Modello':<25} {'F1 Score':>10} {'Tokens':>10} {'Latency':>10} {'Grounding':>10}")
    print("-" * 80)
    
    for analysis in sorted(coding_results, key=lambda x: x['model']):
        if 'single_synapseed' in analysis['results']:
            r = analysis['results']['single_synapseed']
            print(f"{analysis['model']:<25} {r['f1_mean']:>10.3f} {r['tokens_mean']:>10.0f} {r['latency_mean']:>10.2f}s {r['grounding_mean']:>10.3f}")
    
    print("\n" + "=" * 80)
    print("CONFRONTO TRA MODELLI (GROUNDING)")
    print("=" * 80)
    print()
    print(f"{'Modello':<25} {'F1 (Blind)':>12} {'F1 (Grounded)':>15} {'Improvement':>12}")
    print("-" * 80)
    
    for analysis in sorted(grounding_results, key=lambda x: x['model']):
        blind_f1 = analysis['results']['blind']['f1_mean']
        grounded_f1 = analysis['results']['grounded']['f1_mean']
        improvement = ((grounded_f1 - blind_f1) / blind_f1 * 100) if blind_f1 > 0 else 0
        print(f"{analysis['model']:<25} {blind_f1:>12.3f} {grounded_f1:>15.3f} {improvement:>11.1f}%")

if __name__ == '__main__':
    main()
