"""Reporting utilities: Rich console output + JSON persistence.

Results are saved to benchmark/results/ as JSON with timestamps,
enabling git-tracked iteration and trend analysis.
"""

from __future__ import annotations

import json
import os
from datetime import datetime, timezone

from rich.console import Console
from rich.panel import Panel
from rich.table import Table


class Reporter:
    """Handles console output and JSON result persistence."""

    def __init__(self, benchmark_name: str, results_dir: str = "results"):
        self.benchmark_name = benchmark_name
        self.results_dir = os.path.join(
            os.path.dirname(os.path.dirname(__file__)), results_dir
        )
        self.console = Console()
        os.makedirs(self.results_dir, exist_ok=True)

    def header(self, subtitle: str = ""):
        """Print benchmark header."""
        title = f"SYNAPSEED Benchmark: {self.benchmark_name}"
        if subtitle:
            title += f" | {subtitle}"
        self.console.print(Panel(title, style="bold blue"))

    def table(
        self,
        title: str,
        columns: list[str],
        rows: list[list[str]],
        *,
        styles: list[str] | None = None,
    ):
        """Print a rich table."""
        t = Table(title=title)
        for i, col in enumerate(columns):
            style = styles[i] if styles and i < len(styles) else None
            t.add_column(col, style=style)
        for row in rows:
            t.add_row(*row)
        self.console.print(t)

    def metric(self, label: str, value: float, fmt: str = ".2f"):
        """Print a single metric."""
        self.console.print(f"  {label}: [bold]{value:{fmt}}[/bold]")

    def delta(self, label: str, blind: float, grounded: float, fmt: str = ".2f"):
        """Print a blind vs grounded comparison with delta."""
        d = grounded - blind
        color = "green" if d > 0 else ("red" if d < 0 else "white")
        self.console.print(
            f"  {label}: BLIND={blind:{fmt}}  GROUNDED={grounded:{fmt}}  "
            f"[{color}]delta={d:+{fmt}}[/{color}]"
        )

    def save(self, data: dict, *, suffix: str = "") -> str:
        """Save results as timestamped JSON. Returns the file path."""
        ts = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
        name = self.benchmark_name.lower().replace(" ", "_")
        if suffix:
            name += f"_{suffix}"
        filename = f"{name}_{ts}.json"
        path = os.path.join(self.results_dir, filename)

        # Add metadata envelope
        envelope = {
            "metadata": {
                "benchmark": self.benchmark_name,
                "timestamp": ts,
                "version": _get_synapseed_version(),
            },
            **data,
        }

        with open(path, "w") as f:
            json.dump(envelope, f, indent=2, default=str)

        self.console.print(f"\n  Results saved: [bold]{path}[/bold]")
        return path

    def summary_panel(self, lines: list[str], *, style: str = "green"):
        """Print a summary panel."""
        text = "\n".join(lines)
        self.console.print(Panel(text, title="Summary", style=style))


def _get_synapseed_version() -> str:
    """Read workspace version from Cargo.toml."""
    cargo = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(__file__))), "Cargo.toml")
    try:
        with open(cargo) as f:
            for line in f:
                if line.strip().startswith("version"):
                    return line.split('"')[1]
    except (FileNotFoundError, IndexError):
        pass
    return "unknown"
