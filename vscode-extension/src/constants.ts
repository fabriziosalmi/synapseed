/** Shared constants for the SYNAPSEED VS Code extension. */

// ── Cache TTLs (milliseconds) ────────────────────────────────────────
export const CACHE_TTL = {
    DEFAULT: 15_000,
    STATUS: 10_000,
    DIAGNOSTICS: 10_000,
    ARCHITECTURE: 60_000,
    TELEMETRY: 15_000,
} as const;

// ── CLI timeouts (milliseconds) ──────────────────────────────────────
export const TIMEOUT = {
    DEFAULT: 30_000,
    LONG: 60_000,
    TELEMETRY: 10_000,
} as const;

// ── CLI subprocess limits ────────────────────────────────────────────
export const MAX_EXEC_BUFFER = 10 * 1024 * 1024; // 10 MB

// ── Cache limits ─────────────────────────────────────────────────────
export const MAX_CACHE_ENTRIES = 100;

// ── UI thresholds ────────────────────────────────────────────────────
export const INSTABILITY_THRESHOLDS = {
    HIGH: 0.8,
    MEDIUM: 0.5,
} as const;

export const INSTABILITY_COLOR_THRESHOLDS = {
    RED: 80,
    ORANGE: 50,
} as const;

// ── Display limits ───────────────────────────────────────────────────
export const MAX_MODULES_DISPLAY = 12;
export const MAX_HOTSPOT_ITEMS = 8;
export const BLAME_CONTEXT_LINES = 5;

// ── Status bar priorities (higher = further left) ────────────────────
export const STATUS_BAR_PRIORITY = {
    GRADE: 53,
    DIAGNOSTICS: 52,
    SECURITY: 51,
    SESSION: 50,
} as const;

// ── Debounce ─────────────────────────────────────────────────────────
export const SAVE_DEBOUNCE_MS = 300;
export const PANEL_READY_DELAY_MS = 300;

// ── Color palette ────────────────────────────────────────────────────
export const COLORS = {
    PASS: '#4caf50',
    INFO: '#2196f3',
    WARN: '#ff9800',
    ERROR: '#f44336',
    PURPLE: '#7c4dff',
} as const;

export const GRADE_COLORS: Record<string, string> = {
    A: COLORS.PASS,
    B: COLORS.INFO,
    C: COLORS.WARN,
    D: COLORS.ERROR,
    F: COLORS.ERROR,
};

export const INTENT_COLORS: Record<string, string> = {
    fix: COLORS.ERROR,
    feature: COLORS.PASS,
    refactor: COLORS.INFO,
    security: COLORS.WARN,
    docs: COLORS.PURPLE,
};

// ── Language support ─────────────────────────────────────────────────
export const SUPPORTED_LANGUAGES = ['rust', 'python', 'typescript'] as const;
export const SUPPORTED_FILE_EXTENSIONS = /\.(rs|py|ts|js)$/;
