import argparse
import math
import os
import sys
import time
import json
import subprocess
import re
import random
from datetime import datetime
from typing import List, Dict, Any

# Try to import external dependencies
try:
    from openai import OpenAI
    import tiktoken
    from rich.console import Console
    from rich.table import Table
    from rich.panel import Panel
    from rich.progress import Progress, SpinnerColumn, TextColumn
    from rich import print as rprint
    from rich.layout import Layout
    from rich.live import Live
    # Advanced Metrics
    from rouge_score import rouge_scorer
    import textstat
except ImportError:
    print("Please install dependencies: pip install openai rich tiktoken rouge-score textstat")
    sys.exit(1)

console = Console()

# Configuration (defaults)
DEFAULT_MODEL = "qwen/qwen3-4b" 
DEFAULT_API_BASE = "http://127.0.0.1:1234/v1" 
DEFAULT_API_KEY = "lm-studio"

class VanguardRunner:
    def __init__(self, model: str, api_base: str, api_key: str, dry_run: bool = False):
        self.model = model
        self.api_base = api_base
        self.api_key = api_key
        self.dry_run = dry_run
        
        self.client = OpenAI(base_url=self.api_base, api_key=self.api_key)
        self.base_dir = os.path.dirname(os.path.abspath(__file__))
        self.targets_dir = os.path.join(self.base_dir, "targets")
        self.results_dir = os.path.join(self.base_dir, "results")
        
        # Initialize Metrics Engines
        self.rouge_scorer = rouge_scorer.RougeScorer(['rouge1', 'rouge2', 'rougeL'], use_stemmer=True)
        
        # Initialize token encoder 
        try:
            self.encoder = tiktoken.encoding_for_model(self.model)
        except KeyError:
            self.encoder = tiktoken.get_encoding("cl100k_base")

        # Fixed seed for reproducible baselines
        random.seed(42)

        os.makedirs(self.targets_dir, exist_ok=True)
        os.makedirs(self.results_dir, exist_ok=True)

    def count_tokens(self, text: str) -> int:
        return len(self.encoder.encode(text))

    def prepare_repo(self, repo_url: str, repo_name: str) -> str:
        """Clones or updates the target repository."""
        repo_path = os.path.join(self.targets_dir, repo_name)
        if not os.path.exists(repo_path):
            console.print(f"[bold yellow]Cloning {repo_name}...[/bold yellow]")
            subprocess.run(["git", "clone", repo_url, repo_path], check=True)
            console.print(f"[green]Cloned successfully.[/green]")
        else:
            # console.print(f"[bold yellow]Updating {repo_name}...[/bold yellow]")
            # subprocess.run(["git", "pull"], cwd=repo_path, check=False) 
            pass
        return repo_path

    def get_random_files_content(self, repo_path: str, count: int = 10, max_lines: int = 500) -> str:
        """Simulates a poor RAG retrieval: random files + file list."""
        context = "Files in repository:\n"
        
        # Get all files
        all_files = []
        for root, dirs, files in os.walk(repo_path):
            if '.git' in dirs: dirs.remove('.git')
            for file in files:
                if file.endswith(('.rs', '.py', '.md', '.txt', '.toml')):
                    all_files.append(os.path.join(root, file))
        
        # List context (first 200 files to avoid overflow)
        rel_files = [os.path.relpath(f, repo_path) for f in all_files[:200]]
        context += "\n".join(rel_files) + "\n\n"

        # Content context (Random selection)
        if len(all_files) > 0:
            selected_files = random.sample(all_files, min(count, len(all_files)))
            for f_path in selected_files:
                try:
                    rel_name = os.path.relpath(f_path, repo_path)
                    with open(f_path, 'r', errors='ignore') as f:
                        lines = f.readlines()
                        content = "".join(lines[:max_lines])
                        context += f"\n--- Start of {rel_name} ---\n{content}\n--- End of {rel_name} ---\n"
                except Exception:
                    pass
        
        return context

    def get_synapseed_context(self, repo_path: str, query: str) -> str:
        """Gets context from Synapseed: targets + actual source code at those locations."""
        try:
            subprocess.run(["synapseed", "init"], cwd=repo_path, capture_output=True)
            subprocess.run(["synapseed", "hoist", "."], cwd=repo_path, capture_output=True)
        except Exception as e:
            console.print(f"[yellow]Synapseed Init/Hoist Warning: {e}[/yellow]")

        env = os.environ.copy()
        env["RUST_LOG"] = "off"

        model_lower = self.model.lower()
        if "1.7b" in model_lower or "1b" in model_lower:
            env["SYNAPSEED_MODEL_TIER"] = "atomic"
        elif "4b" in model_lower or "3b" in model_lower:
            env["SYNAPSEED_MODEL_TIER"] = "molecular"
        else:
            env["SYNAPSEED_MODEL_TIER"] = "galactic"

        cmd = ["synapseed", "ask", query, "--raw"]

        try:
            result = subprocess.run(
                cmd, cwd=repo_path, capture_output=True, text=True,
                timeout=60, env=env
            )

            if result.returncode != 0:
                console.print(f"[red]Synapseed Error: {result.stderr}[/red]")
                return "Error running Synapseed."

            try:
                stdout = result.stdout.strip()
                # The CLI outputs: smart_context\n\n--- Full Context ---\n{json}
                # We must find the marker first, since smart_context may contain '{' (Rust code).
                marker = "--- Full Context ---\n"
                marker_pos = stdout.find(marker)
                if marker_pos >= 0:
                    json_str = stdout[marker_pos + len(marker):]
                    data = json.loads(json_str)
                    return self._build_source_context(data, repo_path)
                # Fallback: no marker found, try raw JSON parse
                brace_start = stdout.find('{')
                brace_end = stdout.rfind('}')
                if brace_start >= 0 and brace_end > brace_start:
                    data = json.loads(stdout[brace_start:brace_end + 1])
                    return self._build_source_context(data, repo_path)
                return stdout

            except json.JSONDecodeError:
                return result.stdout

        except Exception as e:
            return f"Error executing synapseed: {e}"

    def _read_file_range(self, file_path: str, start: int, end: int, context_lines: int = 10) -> str:
        """Reads a range of lines from a file, with optional surrounding context."""
        try:
            with open(file_path, 'r', errors='ignore') as f:
                all_lines = f.readlines()

            # Expand range with context, clamped to file bounds
            actual_start = max(0, start - 1 - context_lines)
            actual_end = min(len(all_lines), end + context_lines)

            numbered = []
            for i in range(actual_start, actual_end):
                prefix = ">" if start - 1 <= i < end else " "
                numbered.append(f"{prefix} {i + 1:4d} | {all_lines[i].rstrip()}")
            return "\n".join(numbered)
        except Exception:
            return ""

    def _build_source_context(self, data: dict, repo_path: str) -> str:
        """Builds context with actual source code from Synapseed-identified locations.

        Priority: raw_sources (pre-extracted by Synapseed) > file reads from targets/symbols.
        """
        if not data or not isinstance(data, dict):
            return json.dumps(data) if data else ""

        parts = []

        # ── Priority 1: Use raw_sources if available (structured, no disk I/O) ──
        raw_sources = data.get("raw_sources") or []
        if raw_sources:
            for src in raw_sources:
                fp = src.get("file_path", "unknown")
                start = src.get("line_start", 0)
                end = src.get("line_end", 0)
                source = src.get("source", "")
                if not source:
                    continue
                header = f"FILE: {fp} (lines {start}-{end})"
                parts.append(f"--- {header} ---\n{source}\n--- end ---")
            if parts:
                return "\n\n".join(parts)

        # ── Priority 2: Fall back to reading files from targets + symbols ──
        locations = []

        for t in (data.get("targets") or []):
            fp = t.get("file_path", "")
            if fp and t.get("line_start"):
                locations.append({
                    "file": fp, "start": t["line_start"],
                    "end": t.get("line_end", t["line_start"] + 20),
                    "name": t.get("name", ""), "kind": t.get("kind", "")
                })

        for s in (data.get("code_context") or {}).get("symbols", []):
            fp = s.get("file_path", "")
            if fp and s.get("line_start"):
                locations.append({
                    "file": fp, "start": s["line_start"],
                    "end": s.get("line_end", s["line_start"] + 20),
                    "name": s.get("name", ""), "kind": s.get("kind", "")
                })

        # Deduplicate by (file, start)
        seen = set()
        unique_locations = []
        for loc in locations:
            key = (loc["file"], loc["start"])
            if key not in seen:
                seen.add(key)
                unique_locations.append(loc)

        if not unique_locations:
            return data.get("smart_context", json.dumps(data, indent=2))

        for loc in unique_locations:
            fp = loc["file"]
            if not os.path.isabs(fp):
                fp = os.path.join(repo_path, fp)

            rel_path = os.path.relpath(fp, repo_path)
            header = f"FILE: {rel_path}"
            if loc["name"]:
                header += f" ({loc['kind']}: {loc['name']})"

            source = self._read_file_range(fp, loc["start"], loc["end"])
            if source:
                parts.append(f"--- {header} ---\n{source}\n--- end ---")

        if not parts:
            return data.get("smart_context", json.dumps(data, indent=2))

        return "\n\n".join(parts)

    def run_inference(self, system_prompt: str, user_prompt: str, scenario_name: str,
                       thinking: bool = True) -> str:
        """Runs the LLM generation with streaming feedback."""
        if self.dry_run:
            time.sleep(1)
            return "Simulated LLM Response: The file is src/lib.rs"

        # For models that support thinking mode (Qwen3), control it via prompt
        if not thinking:
            user_prompt = "/no_think\n" + user_prompt

        try:
            stream = self.client.chat.completions.create(
                model=self.model,
                messages=[
                    {"role": "system", "content": system_prompt},
                    {"role": "user", "content": user_prompt}
                ],
                temperature=0.0,
                timeout=120,
                stream=True
            )

            content = ""
            with Live(console=console, refresh_per_second=4) as live:
                live.update(f"[cyan]Generating Answer for {scenario_name}...[/cyan]")

                for chunk in stream:
                    if chunk.choices[0].delta.content:
                        text = chunk.choices[0].delta.content
                        content += text
                        preview = content[-200:].replace("[", "\\[")
                        live.update(f"[cyan]{scenario_name}[/cyan] | Generating: ...{preview}")

            return content

        except Exception as e:
            console.print(f"[red]LLM Error: {e}[/red]")
            return f"LLM Error: {e}"

    @staticmethod
    def strip_thinking(text: str) -> str:
        """Removes <think>...</think> blocks from model output for clean evaluation."""
        return re.sub(r'<think>.*?</think>', '', text, flags=re.DOTALL).strip()

    def detect_hallucinations(self, text: str, repo_path: str) -> Dict[str, Any]:
        """SCIENTIFIC METRIC: Truth Anchor. Verifies if cited files actually exist."""
        # Regex to capture potential file paths (e.g. src/main.rs, lib/utils.py)
        # We look for common code extensions
        pattern = r'\b[\w\-\./]+\.(rs|py|js|ts|go|c|cpp|h|md|json|toml|yaml)\b'
        full_matches = [m.group(0) for m in re.finditer(pattern, text)]
        
        # Deduplicate
        full_matches = list(set(full_matches))
        
        if not full_matches:
            return {"rate": 0.0, "valid": 0, "total": 0, "ghosts": []}
            
        valid_count = 0
        ghosts = []
        
        for path in full_matches:
            # Check relative to repo root
            full_abs_path = os.path.join(repo_path, path)
            # Also check if it's just a filename match somewhere (lazier check)
            # For strict science, we check exact path or basename existence
            
            exists = False
            if os.path.exists(full_abs_path):
                exists = True
            else:
                # Fallback: check if basename exists anywhere (to be charitable to small models)
                for root, _, files in os.walk(repo_path):
                    if os.path.basename(path) in files:
                        exists = True
                        break
            
            if exists:
                valid_count += 1
            else:
                ghosts.append(path)
                
        return {
            "rate": (len(ghosts) / len(full_matches)) * 100, # Hallucination Rate %
            "valid": valid_count,
            "total": len(full_matches),
            "ghosts": ghosts
        }

    def evaluate_ground_truth(self, response: str, ground_truth: Dict) -> Dict:
        """Calculates recall, ROUGE, and readability against ground truth."""
        response_lower = response.lower()

        # 1. File Recall
        files_found = 0
        for f in ground_truth['key_files']:
            if f.lower() in response_lower or os.path.basename(f).lower() in response_lower:
                files_found += 1
        file_recall = files_found / len(ground_truth['key_files']) if ground_truth['key_files'] else 0

        # 2. Symbol Recall
        symbols_found = 0
        for s in ground_truth['key_symbols']:
            if s.lower() in response_lower:
                symbols_found += 1
        symbol_recall = symbols_found / len(ground_truth['key_symbols']) if ground_truth['key_symbols'] else 0

        # 3. Composite Recall (average of file + symbol recall)
        recall = (file_recall + symbol_recall) / 2.0

        # 4. ROUGE against synthetic reference (concept + key files + symbols)
        reference_text = f"{ground_truth['concept']} " + " ".join(ground_truth['key_files']) + " " + " ".join(ground_truth['key_symbols'])
        rouge_scores = self.rouge_scorer.score(reference_text, response)

        # 5. Readability
        readability = textstat.flesch_reading_ease(response)
        complexity = textstat.text_standard(response, float_output=True)

        return {
            "recall": recall,
            "file_recall": file_recall,
            "symbol_recall": symbol_recall,
            "files_found": files_found,
            "symbols_found": symbols_found,
            "rouge_1": rouge_scores['rouge1'].fmeasure,
            "rouge_l": rouge_scores['rougeL'].fmeasure,
            "readability": readability,
            "grade_level": complexity
        }


    def _compute_metrics(self, eval_data: Dict, hal_data: Dict, tokens_in: int,
                          raw_response: str) -> Dict:
        """Computes per-run metrics from evaluation and hallucination data."""
        tokens_out = self.count_tokens(raw_response)
        ctx_eff = (eval_data['recall'] / (tokens_in / 1000)) if tokens_in > 0 else 0
        grounding = ((hal_data['valid'] / hal_data['total']) * 100) if hal_data['total'] > 0 else 100.0
        cost = (tokens_in / 1_000_000) * 0.50

        return {
            "tokens_in": tokens_in,
            "tokens_out": tokens_out,
            "recall": eval_data['recall'],
            "file_recall": eval_data['file_recall'],
            "symbol_recall": eval_data['symbol_recall'],
            "hallucination_rate": hal_data['rate'],
            "grounding_rate": grounding,
            "ghost_files": hal_data['ghosts'],
            "context_efficiency": ctx_eff,
            "rouge_l": eval_data['rouge_l'],
            "cost_per_run": cost,
        }

    def _run_pair(self, system_prompt: str, context_a: str, context_b: str,
                  query: str, ground_truth: Dict, repo_path: str,
                  thinking: bool, scenario_id: str,
                  system_prompt_syn: str = None) -> Dict:
        """Runs one baseline+synapseed pair and returns the result dict."""
        think_label = "think" if thinking else "no_think"

        # --- Baseline ---
        prompt_a = f"Context:\n{context_a}\n\nQuestion: {query}"
        start = time.time()
        raw_a = self.run_inference(system_prompt, prompt_a, f"Baseline/{think_label}", thinking=thinking)
        latency_a = time.time() - start
        tokens_a = self.count_tokens(prompt_a)
        resp_a = self.strip_thinking(raw_a)

        eval_a = self.evaluate_ground_truth(resp_a, ground_truth)
        hal_a = self.detect_hallucinations(resp_a, repo_path)

        # --- Synapseed ---
        syn_prompt = system_prompt_syn or system_prompt
        prompt_b = f"Context:\n{context_b}\n\nQuestion: {query}"
        start = time.time()
        raw_b = self.run_inference(syn_prompt, prompt_b, f"Synapseed/{think_label}", thinking=thinking)
        latency_b = time.time() - start
        tokens_b = self.count_tokens(prompt_b)
        resp_b = self.strip_thinking(raw_b)

        eval_b = self.evaluate_ground_truth(resp_b, ground_truth)
        hal_b = self.detect_hallucinations(resp_b, repo_path)

        # --- Derived metrics ---
        metrics_a = self._compute_metrics(eval_a, hal_a, tokens_a, resp_a)
        metrics_b = self._compute_metrics(eval_b, hal_b, tokens_b, resp_b)

        compression = tokens_a / tokens_b if tokens_b > 0 else 0
        savings = ((metrics_a['cost_per_run'] - metrics_b['cost_per_run']) / metrics_a['cost_per_run'] * 100) if metrics_a['cost_per_run'] > 0 else 0
        speed_up = latency_a / latency_b if latency_b > 0 else 0

        if eval_a['recall'] > 0:
            accuracy_ratio = eval_b['recall'] / eval_a['recall']
        elif eval_b['recall'] > 0:
            # Synapseed found something, baseline found nothing → big win
            accuracy_ratio = eval_b['recall'] / 0.01
        else:
            # Both scored 0 → no demonstrated improvement
            accuracy_ratio = 0.0
        cog_mult = max(accuracy_ratio * math.log2(max(compression, 1.0)), 0)

        metrics_a["latency_s"] = latency_a
        metrics_b.update({
            "latency_s": latency_b,
            "savings_pct": savings,
            "compression": compression,
            "cognitive_multiplier": cog_mult,
            "speed_up": speed_up,
        })

        result = {
            "scenario": scenario_id,
            "thinking": thinking,
            "baseline": metrics_a,
            "synapseed": metrics_b,
        }

        # --- Console Report ---
        table = Table(title=f"{scenario_id} [{think_label}]")
        table.add_column("Metric", style="cyan")
        table.add_column("Baseline", style="magenta")
        table.add_column("Synapseed", style="green")
        table.add_column("Delta", style="bold yellow")

        table.add_row("Input Tokens", f"{tokens_a}", f"{tokens_b}", f"{compression:.1f}x compression")
        table.add_row("Recall", f"{eval_a['recall']:.2f}", f"{eval_b['recall']:.2f}",
                       f"file: {eval_b['file_recall']:.0%} sym: {eval_b['symbol_recall']:.0%}")
        table.add_row("Grounding Rate", f"{metrics_a['grounding_rate']:.0f}%", f"{metrics_b['grounding_rate']:.0f}%",
                       f"Ghosts: {len(hal_a['ghosts'])} vs {len(hal_b['ghosts'])}")
        table.add_row("Context Efficiency", f"{metrics_a['context_efficiency']:.4f}", f"{metrics_b['context_efficiency']:.4f}",
                       "Recall per 1k tok")
        table.add_row("ROUGE-L", f"{eval_a['rouge_l']:.2f}", f"{eval_b['rouge_l']:.2f}", "Relevance overlap")
        table.add_row("Cog. Multiplier", "-", f"{cog_mult:.2f}x", "accuracy * log2(compression)")
        table.add_row("Latency", f"{latency_a:.1f}s", f"{latency_b:.1f}s", f"{speed_up:.1f}x faster")
        table.add_row("Est. Cost (1M)", f"${metrics_a['cost_per_run'] * 1e6:.2f}",
                       f"${metrics_b['cost_per_run'] * 1e6:.2f}", f"{savings:.1f}% cheaper")
        console.print(table)

        return result

    def run(self, ground_truth_file: str):
        with open(ground_truth_file, 'r') as f:
            data = json.load(f)

        console.print(f"[bold purple]VANGUARD PROTOCOL: {self.model}[/bold purple]")

        system_prompt = (
            "You are a code analysis assistant. Answer ONLY based on the provided context. "
            "Cite exact file paths and symbols from the context when possible. "
            "If the information is not in the context, say 'NOT FOUND'."
        )

        # Synapseed-specific system prompt: stricter grounding rules for augmented context
        system_prompt_syn = (
            "You are a high-precision Code Analyst. "
            "You are provided with RAW SOURCE CODE chunks delimited by @@@ markers. "
            "RULES:\n"
            "1. Use ONLY the provided files and line numbers.\n"
            "2. If you cite a file not present in the context, your answer is INVALID.\n"
            "3. Be technical and precise. Use the exact signatures provided.\n"
            "4. If the information is not in the context, say 'NOT FOUND'."
        )

        results = []

        for scenario in data['scenarios']:
            repo_name = scenario['target_repo'].split('/')[-1]
            repo_path = self.prepare_repo(scenario['target_url'], repo_name)

            console.print(Panel(f"Scenario: {scenario['id']}", style="bold blue"))

            # Prepare contexts once (reused across thinking modes)
            context_a = self.get_random_files_content(repo_path)
            context_b = self.get_synapseed_context(repo_path, scenario['query'])

            # Run both thinking modes
            for thinking in [True, False]:
                mode = "THINK" if thinking else "NO_THINK"
                console.print(f"[cyan]--- Mode: {mode} ---[/cyan]")

                result = self._run_pair(
                    system_prompt, context_a, context_b,
                    scenario['query'], scenario['ground_truth'],
                    repo_path, thinking, scenario['id'],
                    system_prompt_syn=system_prompt_syn,
                )
                results.append(result)

        # Export
        ts = datetime.now().strftime("%Y%m%d_%H%M%S")
        out_file = os.path.join(self.results_dir, f"vanguard_{self.model.replace('/', '_')}_{ts}.json")
        with open(out_file, 'w') as f:
            json.dump(results, f, indent=2)
        console.print(f"[bold green]Vanguard Report Saved: {out_file}[/bold green]")

        return results

def generate_arxiv_report(all_data: Dict[str, List[Dict]], output_path: str):
    """Generates the SCIENCE_REPORT.md."""
    with open(output_path, 'w') as f:
        f.write("# Synapseed Vanguard Benchmark Report\n\n")
        f.write("## Comparative Matrix\n\n")
        f.write("| Model | Scenario | Think | Recall (Base) | Recall (Syn) | Grounding | Compr. | Cog. Mult |\n")
        f.write("|-------|----------|-------|---------------|--------------|-----------|--------|-----------|\n")

        for model, scenarios in all_data.items():
            for s in scenarios:
                base = s['baseline']
                syn = s['synapseed']
                think_mode = "on" if s.get('thinking', True) else "off"
                f.write(
                    f"| {model} | {s['scenario']} | {think_mode} "
                    f"| {base['recall']:.2f} | {syn['recall']:.2f} "
                    f"| {syn['grounding_rate']:.0f}% "
                    f"| {syn.get('compression', 0):.1f}x "
                    f"| **{syn.get('cognitive_multiplier', 0):.2f}x** |\n"
                )

        f.write("\n## Thinking vs No-Think Analysis\n\n")
        for model, scenarios in all_data.items():
            think_runs = [s for s in scenarios if s.get('thinking', True)]
            nothink_runs = [s for s in scenarios if not s.get('thinking', True)]
            if think_runs and nothink_runs:
                f.write(f"### {model}\n")
                for t_run in think_runs:
                    nt_run = next((x for x in nothink_runs if x['scenario'] == t_run['scenario']), None)
                    if nt_run:
                        f.write(f"**{t_run['scenario']}**: ")
                        f.write(f"Think recall={t_run['synapseed']['recall']:.2f}, ")
                        f.write(f"NoThink recall={nt_run['synapseed']['recall']:.2f}, ")
                        delta = nt_run['synapseed']['recall'] - t_run['synapseed']['recall']
                        f.write(f"delta={delta:+.2f}\n\n")

        f.write("\n## Small vs Large Model Comparison\n")
        f.write("Hypothesis: 1.7B + Synapseed >= 4B Vanilla.\n\n")

        qwen17 = all_data.get("qwen/qwen3-1.7b", [])
        qwen4 = all_data.get("qwen/qwen3-4b", [])

        if qwen17 and qwen4:
            for s17 in qwen17:
                s4 = next((x for x in qwen4 if x['scenario'] == s17['scenario']
                           and x.get('thinking') == s17.get('thinking')), None)
                if s4:
                    syn_recall = s17['synapseed']['recall']
                    base_recall = s4['baseline']['recall']
                    proven = syn_recall >= base_recall
                    think_mode = "think" if s17.get('thinking', True) else "no_think"
                    f.write(f"### {s17['scenario']} [{think_mode}]\n")
                    f.write(f"- **Qwen 1.7B + Synapseed**: Recall {syn_recall:.2f}, Grounding {s17['synapseed']['grounding_rate']:.0f}%\n")
                    f.write(f"- **Qwen 4B Vanilla**: Recall {base_recall:.2f}, Grounding {s4['baseline']['grounding_rate']:.0f}%\n")
                    f.write(f"- **Outcome**: {'PROVEN' if proven else 'FAILED'}\n\n")

    console.print(f"[bold blue]Arxiv Report Generated: {output_path}[/bold blue]")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Vanguard Protocol Runner")
    parser.add_argument("--model", default=DEFAULT_MODEL)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    
    if args.model == "matrix":
        # Matrix Configuration (1.7B, 4B, Llama-1B)
        matrix_models = [
            "qwen/qwen3-1.7b",
            "qwen/qwen3-4b",
            "llama-3.2-1b-instruct"
        ]
        console.print(f"[bold yellow]🚀 Launching SCIENCE MATRIX: {matrix_models}[/bold yellow]")
        ground_truth = os.path.join(os.path.dirname(__file__), "vanguard_lab", "ground_truth.json")
        
        aggregated_results = {}
        
        for model in matrix_models:
            console.print(f"\n[bold purple]>>> TARGET ACQUIRED: {model}[/bold purple]")
            try:
                runner = VanguardRunner(model, DEFAULT_API_BASE, DEFAULT_API_KEY, args.dry_run)
                res = runner.run(ground_truth)
                aggregated_results[model] = res
            except Exception as e:
                console.print(f"[red]Failed to run {model}: {e}[/red]")
        
        # Generate Science Report
        report_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "results", "SCIENCE_REPORT.md")
        generate_arxiv_report(aggregated_results, report_path)
        
    else:
        runner = VanguardRunner(args.model, DEFAULT_API_BASE, DEFAULT_API_KEY, args.dry_run)
        runner.run(os.path.join(os.path.dirname(__file__), "vanguard_lab", "ground_truth.json"))
