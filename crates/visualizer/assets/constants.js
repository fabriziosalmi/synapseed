// SYNAPSEED Architecture Visualizer — Constants & Utilities

// HCI Req 7: Perceptually uniform palette — WCAG AA contrast on #0d1117.
// Semantic: blue=structural, green=actions, purple=types, orange=enums, gray=deps.
const NODE_COLORS = {
  file:     { bg: '#161b22', border: '#58a6ff', text: '#c9d1d9' },
  function: { bg: '#0e4429', border: '#7ee787', text: '#7ee787' },
  method:   { bg: '#0e4429', border: '#56d364', text: '#56d364' },
  struct:   { bg: '#1a1040', border: '#d2a8ff', text: '#d2a8ff' },
  class:    { bg: '#1a1040', border: '#bc8cff', text: '#bc8cff' },
  enum:     { bg: '#3a1d0c', border: '#f0883e', text: '#f0883e' },
  module:   { bg: '#0d1d3a', border: '#79c0ff', text: '#79c0ff' },
  import:   { bg: '#1c2128', border: '#8b949e', text: '#8b949e' },
  variable: { bg: '#2a1800', border: '#ffa657', text: '#ffa657' },
  constant: { bg: '#2a0e0e', border: '#ff7b72', text: '#ff7b72' },
  interface:{ bg: '#1a1040', border: '#d2a8ff', text: '#d2a8ff' },
  cluster:  { bg: '#21262d', border: '#484f58', text: '#8b949e' },
};

// Globals use `var` so they're accessible as window properties (for testing).
// Prefixed with __ to avoid conflict with DOM element id="cy".
var __cy = null;
var __eventCount = 0;
var __collapsedFiles = new Set();

// ── Status Overlay ──────────────────────────────────────────

function setStatus(text, isError) {
  const el = document.getElementById('graph-status');
  if (!el) return;
  if (!text) {
    el.className = 'hidden';
    el.textContent = '';
    return;
  }
  el.className = isError ? 'error' : '';
  el.textContent = text;
}

// ── XSS Safety ──────────────────────────────────────────────

function esc(str) {
  if (!str) return '';
  const el = document.createElement('span');
  el.textContent = str;
  return el.innerHTML;
}
