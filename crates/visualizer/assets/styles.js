// SYNAPSEED Architecture Visualizer — Cytoscape Stylesheet

function buildCytoscapeStyles() {
  return [
    // ── Base node (all nodes inherit) ──
    {
      selector: 'node',
      style: {
        'label': 'data(label)',
        'color': '#ffffff',
        'text-valign': 'bottom',
        'text-halign': 'center',
        'text-margin-y': 6,
        'font-size': '12px',
        'font-family': 'SF Mono, Fira Code, JetBrains Mono, monospace',
        'background-color': '#444',
        'border-width': 2,
        'border-color': '#555',
        'overlay-padding': '6px',
        'z-index': 10,
        'transition-property': 'background-color, border-color, border-width, opacity',
        'transition-duration': '0.3s',
      }
    },
    // ── FILE nodes (compound parents) — large rounded rectangles ──
    {
      selector: 'node[type="file"]',
      style: {
        'shape': 'round-rectangle',
        'width': 60,
        'height': 60,
        'background-color': '#0d1117',
        'border-color': '#58a6ff',
        'border-width': 3,
        'font-size': '14px',
        'font-weight': 'bold',
        'text-valign': 'top',
        'text-halign': 'center',
        'text-margin-y': 10,
        'text-background-opacity': 1,
        'text-background-color': '#0d1117',
        'text-background-padding': '4px',
        'text-background-shape': 'round-rectangle',
        'padding': '24px',
        'min-width': '140px',
      }
    },
    // Collapsed file nodes — readable labeled boxes
    {
      selector: 'node[type="file"].collapsed',
      style: {
        'width': 180,
        'height': 44,
        'padding': '6px',
        'min-width': '180px',
        'min-height': '44px',
        'border-style': 'solid',
        'border-width': 2,
        'border-color': '#58a6ff',
        'background-color': '#161b22',
        'text-valign': 'center',
        'text-halign': 'center',
        'text-margin-y': 0,
        'font-size': '13px',
      }
    },
    // ── SYMBOL nodes — colored circles by type ──
    {
      selector: 'node[type="function"], node[type="method"]',
      style: {
        'shape': 'ellipse',
        'width': 24,
        'height': 24,
        'background-color': '#238636',
        'border-color': '#7ee787',
        'border-width': 2,
      }
    },
    {
      selector: 'node[type="struct"], node[type="class"], node[type="interface"]',
      style: {
        'shape': 'ellipse',
        'width': 24,
        'height': 24,
        'background-color': '#1158c7',
        'border-color': '#58a6ff',
        'border-width': 2,
      }
    },
    {
      selector: 'node[type="enum"]',
      style: {
        'shape': 'ellipse',
        'width': 24,
        'height': 24,
        'background-color': '#6e40c9',
        'border-color': '#d2a8ff',
        'border-width': 2,
      }
    },
    {
      selector: 'node[type="module"], node[type="constant"]',
      style: {
        'shape': 'ellipse',
        'width': 24,
        'height': 24,
        'background-color': '#9e6a03',
        'border-color': '#f0883e',
        'border-width': 2,
      }
    },
    {
      selector: 'node[type="variable"], node[type="import"]',
      style: {
        'shape': 'ellipse',
        'width': 20,
        'height': 20,
        'background-color': '#333',
        'border-color': '#8b949e',
        'border-width': 1,
      }
    },
    // ── EDGES — subtle bezier arrows ──
    {
      selector: 'edge',
      style: {
        'width': 2,
        'curve-style': 'bezier',
        'line-color': '#30363d',
        'target-arrow-shape': 'triangle',
        'target-arrow-color': '#30363d',
        'arrow-scale': 0.8,
        'opacity': 0.5,
      }
    },
    // ── Cycle edges — red dashed ──
    {
      selector: 'edge[type="cycle"]',
      style: {
        'line-color': '#f85149',
        'target-arrow-color': '#f85149',
        'line-style': 'dashed',
        'width': 3,
        'opacity': 0.8,
        'arrow-scale': 1.0,
      }
    },
    // ── Cycle-involved nodes — red border ──
    {
      selector: 'node[inCycle]',
      style: {
        'border-color': '#f85149',
        'border-width': 4,
      }
    },
    // ── CLUSTER compound node (Unrecognized files group) ──
    {
      selector: 'node[type="cluster"]',
      style: {
        'shape': 'round-rectangle',
        'background-color': '#21262d',
        'border-color': '#484f58',
        'border-width': 2,
        'border-style': 'dashed',
        'font-size': '13px',
        'color': '#8b949e',
        'text-valign': 'top',
        'text-halign': 'center',
        'text-margin-y': 10,
        'padding': '20px',
        'min-width': '120px',
      }
    },
    // ── State classes ──
    { selector: '.sym-hidden', style: { 'display': 'none' } },
    { selector: '.search-dimmed', style: { 'opacity': 0.1, 'z-index': 0 } },
    { selector: '.search-match', style: { 'border-color': '#f0883e', 'border-width': 6, 'z-index': 999 } },
    { selector: '.highlighted', style: { 'border-color': '#f0883e', 'border-width': 5, 'z-index': 999 } },
    { selector: '.pulse', style: { 'border-color': '#f85149', 'background-color': '#550000', 'border-width': 5 } },
    // ── Heatmap ──
    { selector: '.heat-hot', style: { 'border-color': '#ff4433', 'border-width': 6, 'background-color': '#550000' } },
    { selector: '.heat-warm', style: { 'border-color': '#d29922', 'border-width': 4 } },
    { selector: '.heat-cool', style: { 'border-color': '#3fb950', 'border-width': 4 } },
    // ── Selected ──
    { selector: ':selected', style: { 'border-color': '#58a6ff', 'border-width': 4 } },
  ];
}
