
import argparse
from pathlib import Path
from .generators import results, methodology, performance, discussion, formalization, plots, bibliography, case_study


def _try_generate(label: str, fn, *args):
    """Run a generator function, print result, skip on missing data."""
    try:
        result = fn(*args)
        print(f"   ✅ {label}")
        return result
    except FileNotFoundError as e:
        print(f"   ⚠️  {label} SKIPPED (missing data): {e}")
        return None


def main():
    parser = argparse.ArgumentParser(description="Synapseed ArXiv Generator")
    parser.add_argument("--mode", choices=["tables", "full"], required=True)
    args = parser.parse_args()

    # Asset directory
    asset_dir = Path("assets")
    asset_dir.mkdir(exist_ok=True)

    def _write(filename: str, content: str | None):
        if content is not None:
            with open(asset_dir / filename, "w") as f:
                f.write(content)

    if args.mode == "tables":
        print("📊 Generating Benchmark Tables...")
        results.generate_all_tables(asset_dir)

        print("🏗️ Generating Methodology Stats...")
        _write("methodology.tex", methodology.generate_methodology_tex())

        print("📐 Generating Formalization...")
        _write("formalization.tex", formalization.generate_formalization_tex())

        print("📚 Generating Bibliography...")
        _write("references.bib", bibliography.generate_bibtex())

        print("🚀 Generating Performance Metrics...")
        perf_metrics = _try_generate("Performance data loaded", performance.load_performance_metrics)
        if perf_metrics:
            _write("table3_performance.tex", performance.generate_performance_table(perf_metrics))
        else:
            _write("table3_performance.tex", "% No performance data available (grounding benchmark not yet run)\n")

        print("📊 Generating Visualization Plots...")
        plot_lines = _try_generate("Plots generated", plots.generate_plots, asset_dir)
        comp_lines = _try_generate("Comparison plots generated", plots.generate_comparison_plots, asset_dir)
        all_lines = (plot_lines or []) + (["", ""] + comp_lines if comp_lines else [])
        _write("plots.tex", "\n".join(all_lines) if all_lines else "% No plot data available\n")

        print("✅ Tables mode complete.")

    elif args.mode == "full":
        print("📝 Generating Narrative Sections...")

        abstract = _try_generate("Abstract", discussion.generate_abstract)
        _write("abstract.tex", abstract)

        _write("introduction.tex", discussion.generate_introduction())

        discuss = _try_generate("Discussion", discussion.generate_discussion)
        _write("discussion.tex", discuss)

        _write("related_work.tex", discussion.generate_related_work())
        _write("conclusion.tex", discussion.generate_conclusion())

        print("✅ Narrative assets generated.")

        # Tables
        print("📊 Ensure Tables are fresh...")
        results.generate_all_tables(asset_dir)

        _write("methodology.tex", methodology.generate_methodology_tex())
        _write("references.bib", bibliography.generate_bibtex())

        print("🕵️ Generating Case Study...")
        _write("case_study.tex", case_study.generate_case_study())

        # Performance
        perf = _try_generate("Performance data loaded", performance.load_performance_metrics)
        if perf:
            _write("table3_performance.tex", performance.generate_performance_table(perf))
        else:
            _write("table3_performance.tex", "% No performance data available\n")

        print("📊 Generating Visualization Plots...")
        plot_lines = _try_generate("Plots generated", plots.generate_plots, asset_dir)
        comp_lines = _try_generate("Comparison plots generated", plots.generate_comparison_plots, asset_dir)
        all_lines = (plot_lines or []) + (["", ""] + comp_lines if comp_lines else [])
        _write("plots.tex", "\n".join(all_lines) if all_lines else "% No plot data available\n")

        print("🎉 All assets ready for PDF compilation.")


if __name__ == "__main__":
    main()
