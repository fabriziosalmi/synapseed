/** Shared HTML utilities for webview panels. */
import { GRADE_COLORS, COLORS, INSTABILITY_COLOR_THRESHOLDS } from './constants';

/** Escape HTML special characters to prevent XSS. */
export function escapeHtml(s: string): string {
    return String(s)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

/** Map architecture grade to display color. */
export function gradeColor(grade: string): string {
    return GRADE_COLORS[grade] ?? COLORS.ERROR;
}

/** Map a 0-100 percentage to red/orange/green color. */
export function metricColor(pct: number): string {
    if (pct > INSTABILITY_COLOR_THRESHOLDS.RED) { return COLORS.ERROR; }
    if (pct > INSTABILITY_COLOR_THRESHOLDS.ORANGE) { return COLORS.WARN; }
    return COLORS.PASS;
}

/** Generate a cryptographic nonce for webview CSP. */
export function getNonce(): string {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    let result = '';
    for (let i = 0; i < 32; i++) {
        result += chars.charAt(Math.floor(Math.random() * chars.length));
    }
    return result;
}
