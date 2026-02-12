// SYNAPSEED Architecture Visualizer — Boot & Orchestration
//
// Dependencies (loaded via <script> tags before this file):
//   constants.js, styles.js, layout.js, panels.js,
//   events.js, search.js, xray.js, api.js

// ── Initialize Cytoscape ─────────────────────────────────────

function initCytoscape(elements, autoCollapse) {
  try {
    __cy = cytoscape({
      container: document.getElementById('cy'),
      elements: elements,
      style: buildCytoscapeStyles(),
      layout: { name: 'preset' },
      minZoom: 0.2,
      maxZoom: 4.0,
      wheelSensitivity: 0.3,
    });

    applyCollapseState(autoCollapse);
    applyHeatmap();
    setupHoverAnimations();
    setupTooltips();
    setupClickHandlers();
    runLayout();
    initXray();

  } catch (err) {
    console.error('Cytoscape init failed:', err);
    setStatus('Graph init failed: ' + err.message, true);
  }
}

// ── Boot ─────────────────────────────────────────────────

initSearch();
loadGraph();
connectWebSocket();
