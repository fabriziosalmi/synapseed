/** Shared type definitions for the SYNAPSEED VS Code extension. */

// ── Architect report (matches Rust: ArchitectureReport, ModuleMetrics, Violation, Recommendation) ──
export interface ArchitectModule {
    module_name: string;
    file_path: string;
    efferent_coupling: number;
    afferent_coupling: number;
    instability: number;
    approx_complexity: number;
    fan_in: number;
    fan_out: number;
}

export interface ArchitectViolation {
    rule: string;
    description: string;
    severity: string;  // "warning" | "error" | "critical"
    modules: string[];
    suggestion: string;
}

export interface ArchitectRecommendation {
    priority: number;
    category: string;
    action: string;
    modules: string[];
}

export interface ArchitectReport {
    score: number;
    grade: string;
    module_count: number;
    edge_count: number;
    avg_instability: number;
    avg_complexity: number;
    max_coupling: number;
    topological_density: number;
    modules: ArchitectModule[];
    violations: ArchitectViolation[];
    recommendations: ArchitectRecommendation[];
}

// ── Analyze result (matches Rust: churn/risk analysis) ──────────────
export interface AnalyzeResult {
    churn_score?: number;
    risk?: string;
    convergence_rate?: number;
    fix_chain_count?: number;
}

// ── Telemetry ────────────────────────────────────────────────────────
export interface TelemetryHotspot {
    key: string;
    call_count: number;
    avg_duration_ms?: number;
    p95_duration_ms?: number;
}

export interface TelemetryData {
    total_spans: number;
    unique_locations: number;
    buffer_usage: string;
    hotspots: TelemetryHotspot[];
}

// ── Intent ───────────────────────────────────────────────────────────
export interface IntentCategory {
    cat: string;
    count: number;
}

// ── Security ─────────────────────────────────────────────────────────
export interface SecurityStats {
    dlp_scans: string;
    dlp_blocks: string;
    commands_allowed: string;
    commands_denied: string;
    errors_prevented: string;
}

// ── Benchmarks ───────────────────────────────────────────────────────
export interface BenchmarkMetadata {
    benchmark_type?: string;
    model?: string;
    timestamp?: string;
    [key: string]: unknown;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any -- benchmark JSON has deeply nested, variable structure
export interface BenchmarkFileData {
    metadata?: BenchmarkMetadata;
    results?: unknown[];
    summary?: Record<string, any>;
    [key: string]: unknown;
}

// ── Webview messages (discriminated union) ────────────────────────────
export type AskPanelMessage =
    | { type: 'ask'; query: string }
    | { type: 'askAboutFile'; path: string }
    | { type: 'copy'; text: string }
    | { type: 'export' }
    | { type: 'clear' }
    | { type: 'openFile'; path: string };

export type DashboardMessage =
    | { type: 'refresh' }
    | { type: 'openFile'; path: string }
    | { type: 'askAbout' }
    | { type: 'analyzeModule'; name: string };

export type BenchmarkMessage =
    | { type: 'refresh' }
    | { type: 'import' }
    | { type: 'openFile'; path: string };
