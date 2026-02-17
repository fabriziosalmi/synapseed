
# Python Environment
VENV = .venv
PYTHON = $(VENV)/bin/python
PIP = $(VENV)/bin/pip
SRC_DIR = src
OUT_DIR = assets

.PHONY: all paper tables clean setup

all: paper

setup:
	@echo "🔧 Setting up virtual environment..."
	python3 -m venv $(VENV)
	$(PIP) install -r requirements.txt

tables:
	@echo "📊 Generating LaTeX tables from benchmark data..."
	$(PYTHON) -m src.main --mode tables


VERSION := $(shell grep '^version =' ../../Cargo.toml | head -n 1 | cut -d '"' -f 2)
DATE := $(shell date +%Y%m%d)
PDF_NAME := Synapseed_$(VERSION)_$(DATE).pdf

paper: tables
	@echo "📝 Generating full assets..."
	$(PYTHON) -m src.main --mode full
	@echo "📚 Compiling PDF..."
	pdflatex -interaction=nonstopmode -output-directory $(OUT_DIR) $(SRC_DIR)/templates/main.tex
	bibtex $(OUT_DIR)/main
	pdflatex -interaction=nonstopmode -output-directory $(OUT_DIR) $(SRC_DIR)/templates/main.tex
	pdflatex -interaction=nonstopmode -output-directory $(OUT_DIR) $(SRC_DIR)/templates/main.tex
	mv $(OUT_DIR)/main.pdf $(OUT_DIR)/$(PDF_NAME)
	@echo "✅ Paper available at $(OUT_DIR)/$(PDF_NAME)"



verify:
	@echo "🔍 Verifying data integrity..."
	$(PYTHON) $(SRC_DIR)/verify.py

clean:
	rm -rf $(OUT_DIR)/*.tex $(OUT_DIR)/*.pdf
