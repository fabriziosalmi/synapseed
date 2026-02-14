# The Architect (Structural Analysis)

The Architect module provides **high-level structural insights** into your codebase. While Cortex understands syntax, Architect understands **system design**.

## Core Capabilities

### 1. Dependency Graphing
Constructs a graph where nodes are modules/crates and edges are imports/dependencies. This allows SYNAPSEED to understand the "shape" of your application.

### 2. Coupling Metrics
Calculates software engineering metrics to assess code quality:
- **Instability (I)**: Ratio of efferent (outgoing) to total coupling.
- **Abstractness (A)**: Ratio of abstract types (traits) to implementation types.
- **Distance from Main Sequence (D)**: Balance between stability and abstractness.

### 3. Anti-Pattern Detection
Identifies architectural smells:
- **Cyclic Dependencies**: `A -> B -> C -> A`. These make code hard to test and refactor.
- **God Objects**: Files or structs with excessive responsibility (high fan-in/fan-out, high line count).
- **Layer Violations**: e.g., Domain layer importing from Infrastructure layer (in Clean Architecture).

### 4. Architecture Score
Synthesizes all metrics into a single grade (**A** through **F**) to give a quick health check of the project structure.

## Usage

### MCP Tool: `architect`
Trigger a fresh analysis.
```bash
synapseed architect --refresh
```

### MCP Tool: `consult`
Ask architectural questions backed by the project's "DNA" (configuration rules).
```bash
synapseed consult "Can I import the database layer from the UI?"
```

## Configuration

You can define architectural rules in `.synapseed/dna.yaml`. For example, strictly forbidding circular dependencies or defining valid layer interactions.
