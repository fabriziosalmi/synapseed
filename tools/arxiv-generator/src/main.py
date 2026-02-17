
import argparse
from pathlib import Path
from .generators import results, methodology, performance, discussion, formalization, plots, bibliography, case_study

def main():
    parser = argparse.ArgumentParser(description="Synapseed ArXiv Generator")
    parser.add_argument("--mode", choices=["tables", "full"], required=True)
    args = parser.parse_args()

    # Asset directory
    asset_dir = Path("assets")
    asset_dir.mkdir(exist_ok=True)


    if args.mode == "tables":
        print("📊 Generating Benchmark Tables...")
        results.generate_all_tables(asset_dir)
        
        print("🏗️ Generating Methodology Stats...")
        # methodology imported at top
        with open(asset_dir / "methodology.tex", "w") as f:
            f.write(methodology.generate_methodology_tex())
            
        print("📐 Generating Formalization...") 
        with open(asset_dir / "formalization.tex", "w") as f:
            f.write(formalization.generate_formalization_tex())

        print("📚 Generating Bibliography...")
        with open(asset_dir / "references.bib", "w") as f:
            f.write(bibliography.generate_bibtex())
            
        print("🚀 Generating Performance Metrics...")
        # performance imported at top
        perf_metrics = performance.load_performance_metrics()
        perf_tex = performance.generate_performance_table(perf_metrics)
        with open(asset_dir / "table3_performance.tex", "w") as f:
            f.write(perf_tex)
            
        print("📊 Generating Visualization Plots...")
        plot_tex_lines = plots.generate_plots(asset_dir)
        
        # Add Comparison Plots
        comp_lines = plots.generate_comparison_plots(asset_dir)
        if comp_lines:
            plot_tex_lines.extend(["", ""] + comp_lines)
            
        with open(asset_dir / "plots.tex", "w") as f:
            f.write("\n".join(plot_tex_lines))
            
        print("✅ All assets generated.")
    elif args.mode == "full":
        print("📝 Generating Narrative Sections...")
        # discussion imported at top

        # Manually trigger generation since discussion.py is designed to be run directly or imported.
        # But wait, discussion.py's functions just return strings.
        # Let's adjust discussion.py to have a generate_all function or just write files here.
        
        # Actually, simpler: just call the function if main is modified to return content
        # But current discussion.py writes files when run as main.
        # Let's import and call the functions.
        
        abstract = discussion.generate_abstract()
        intro = discussion.generate_introduction()
        discuss = discussion.generate_discussion()
        related = discussion.generate_related_work()
        conclusion = discussion.generate_conclusion()
        
        with open(asset_dir / "abstract.tex", "w") as f: f.write(abstract)
        with open(asset_dir / "introduction.tex", "w") as f: f.write(intro)
        with open(asset_dir / "discussion.tex", "w") as f: f.write(discuss)
        with open(asset_dir / "related_work.tex", "w") as f: f.write(related)
        with open(asset_dir / "conclusion.tex", "w") as f: f.write(conclusion)
        
        print("✅ Narrative assets generated.")

        
        # Also run tables generation if not already done
        print("📊 Ensure Tables are fresh...")
        results.generate_all_tables(asset_dir)
        
        with open(asset_dir / "methodology.tex", "w") as f:
            f.write(methodology.generate_methodology_tex())

        # Also Bibliography
        with open(asset_dir / "references.bib", "w") as f:
            f.write(bibliography.generate_bibtex())

        print("🕵️ Generating Case Study...")
        with open(asset_dir / "case_study.tex", "w") as f:
            f.write(case_study.generate_case_study())

        # Also Performance
        perf = performance.load_performance_metrics()
        with open(asset_dir / "table3_performance.tex", "w") as f:
            f.write(performance.generate_performance_table(perf))
            
        print("📊 Generating Visualization Plots...")
        plot_tex_lines = plots.generate_plots(asset_dir)
        
        # Add Comparison Plots
        comp_lines = plots.generate_comparison_plots(asset_dir)
        if comp_lines:
            plot_tex_lines.extend(["", ""] + comp_lines)

        with open(asset_dir / "plots.tex", "w") as f:
            f.write("\n".join(plot_tex_lines))
            
        print("🎉 All assets ready for PDF compilation.")


if __name__ == "__main__":
    main()
