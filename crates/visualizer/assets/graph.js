// SYNAPSEED Architecture Visualizer — Frontend Logic

const NODE_COLORS = {
  file:     { bg: '#1c2128', border: '#30363d', text: '#c9d1d9' },
  function: { bg: '#0e4429', border: '#7ee787', text: '#7ee787' },
  method:   { bg: '#0c3a3a', border: '#56d4dd', text: '#56d4dd' },
  struct:   { bg: '#0d1d3a', border: '#58a6ff', text: '#58a6ff' },
  class:    { bg: '#0d1d3a', border: '#58a6ff', text: '#58a6ff' },
  enum:     { bg: '#2a1446', border: '#d2a8ff', text: '#d2a8ff' },
  module:   { bg: '#3a1d0c', border: '#f0883e', text: '#f0883e' },
  import:   { bg: '#1c2128', border: '#484f58', text: '#8b949e' },
  variable: { bg: '#2a2400', border: '#d29922', text: '#d29922' },
  constant: { bg: '#3a1d0c', border: '#f0883e', text: '#f0883e' },
  interface:{ bg: '#0d1d3a', border: '#58a6ff', text: '#58a6ff' },
};

let cy = null;
let eventCount = 0;

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

// ── Initialize Cytoscape ─────────────────────────────────────

function initCytoscape(elements) {
  cy = cytoscape({
    container: document.getElementById('cy'),
    elements: elements,
    style: [
      // File nodes (compound parents)
      {
        selector: 'node[type="file"]',
        style: {
          'background-color': '#161b22',
          'border-color': '#30363d',
          'border-width': 2,
          'label': 'data(label)',
          'color': '#c9d1d9',
          'font-size': '12px',
          'font-family': 'SF Mono, Fira Code, monospace',
          'text-valign': 'top',
          'text-halign': 'center',
          'text-margin-y': 10,
          'padding': '24px',
          'shape': 'roundrectangle',
          'min-width': '140px',
        }
      },
      // Symbol nodes
      {
        selector: 'node[type!="file"]',
        style: {
          'width': 32,
          'height': 32,
          'label': 'data(label)',
          'font-size': '10px',
          'font-family': 'SF Mono, Fira Code, monospace',
          'text-valign': 'bottom',
          'text-halign': 'center',
          'text-margin-y': 8,
          'text-max-width': '120px',
          'text-wrap': 'ellipsis',
          'border-width': 2,
          'transition-property': 'background-color, border-color, width, height',
          'transition-duration': '0.3s',
        }
      },
      // Highlighted nodes (on file change)
      {
        selector: '.highlighted',
        style: {
          'border-color': '#f0883e',
          'border-width': 4,
          'z-index': 999,
        }
      },
      {
        selector: '.pulse',
        style: {
          'border-color': '#f85149',
          'background-color': '#3a0f0f',
          'border-width': 4,
        }
      },
      // Telemetry heatmap levels
      {
        selector: '.heat-hot',
        style: {
          'border-color': '#f85149',
          'border-width': 4,
        }
      },
      {
        selector: '.heat-warm',
        style: {
          'border-color': '#d29922',
          'border-width': 3,
        }
      },
      {
        selector: '.heat-cool',
        style: {
          'border-color': '#7ee787',
          'border-width': 2,
        }
      },
    ],
    layout: {
      name: 'cose',
      animate: true,
      animationDuration: 800,
      nodeRepulsion: function() { return 10000; },
      idealEdgeLength: function() { return 100; },
      gravity: 0.25,
      padding: 50,
      nodeDimensionsIncludeLabels: true,
      fit: true,
    },
    minZoom: 0.1,
    maxZoom: 5,
    wheelSensitivity: 0.3,
  });

  // Apply individual symbol colors
  Object.entries(NODE_COLORS).forEach(([type, colors]) => {
    if (type === 'file') return;
    cy.nodes(`[type="${type}"]`).style({
      'background-color': colors.bg,
      'border-color': colors.border,
      'color': colors.text,
    });
  });

  // Apply telemetry heatmap from data
  applyHeatmap();

  // Tooltip on hover
  cy.on('mouseover', 'node[type!="file"]', function(e) {
    const node = e.target;
    const d = node.data();
    const tooltip = document.getElementById('tooltip');
    tooltip.innerHTML = `
      <div class="tt-name">${d.name || d.label}</div>
      <div class="tt-kind">${d.kind || d.type}</div>
      <div class="tt-loc">L${d.lineStart}–L${d.lineEnd}</div>
    `;
    tooltip.style.display = 'block';
    const pos = e.renderedPosition || e.position;
    tooltip.style.left = (pos.x + 20) + 'px';
    tooltip.style.top = (pos.y - 10) + 'px';
  });

  cy.on('mouseout', 'node', function() {
    document.getElementById('tooltip').style.display = 'none';
  });

  cy.on('mousemove', function(e) {
    const tooltip = document.getElementById('tooltip');
    if (tooltip.style.display === 'block') {
      tooltip.style.left = (e.renderedPosition.x + 20) + 'px';
      tooltip.style.top = (e.renderedPosition.y - 10) + 'px';
    }
  });

  // Click handler: show full details panel
  cy.on('tap', 'node[type!="file"]', function(e) {
    const d = e.target.data();
    const parent = e.target.parent();
    const filePath = parent.length ? parent.data('fullPath') || parent.data('label') : '—';
    const tooltip = document.getElementById('tooltip');
    tooltip.innerHTML = `
      <div class="tt-name">${d.name || d.label}</div>
      <div class="tt-kind">${d.kind || d.type}</div>
      <div class="tt-loc">L${d.lineStart}–L${d.lineEnd}</div>
      <div class="tt-kind" style="margin-top:8px;color:#8b949e">${filePath}</div>
      <div class="tt-kind" style="margin-top:4px;color:#58a6ff">${d.label}</div>
    `;
    tooltip.style.display = 'block';
    const pos = e.renderedPosition || e.position;
    tooltip.style.left = (pos.x + 20) + 'px';
    tooltip.style.top = (pos.y - 10) + 'px';
  });

  // Click on file node: show full path
  cy.on('tap', 'node[type="file"]', function(e) {
    const d = e.target.data();
    const childCount = e.target.children().length;
    const tooltip = document.getElementById('tooltip');
    tooltip.innerHTML = `
      <div class="tt-name">${d.label}</div>
      <div class="tt-kind">${d.language || 'unknown'} — ${childCount} symbol${childCount !== 1 ? 's' : ''}</div>
      <div class="tt-kind" style="margin-top:8px;color:#8b949e">${d.fullPath}</div>
    `;
    tooltip.style.display = 'block';
    const pos = e.renderedPosition || e.position;
    tooltip.style.left = (pos.x + 20) + 'px';
    tooltip.style.top = (pos.y - 10) + 'px';
  });

  // Click on background: dismiss tooltip
  cy.on('tap', function(e) {
    if (e.target === cy) {
      document.getElementById('tooltip').style.display = 'none';
    }
  });
}

// ── Graph Data Loading ───────────────────────────────────────

async function loadGraph() {
  setStatus('Loading graph...', false);
  try {
    const res = await fetch('/api/graph');
    if (!res.ok) {
      const text = await res.text();
      setStatus(`API error: ${res.status} — ${text}`, true);
      console.error('API error:', res.status, text);
      return;
    }
    const data = await res.json();

    document.getElementById('stat-files').textContent = data.stats.files;
    document.getElementById('stat-symbols').textContent = data.stats.symbols;

    // Compound nodes only — containment is via "parent" field, no edges needed
    const elements = data.elements.nodes || [];

    if (elements.length === 0) {
      setStatus('No files indexed — is the project path correct?', true);
      return;
    }

    if (cy) {
      cy.destroy();
    }
    setStatus(null);
    initCytoscape(elements);
  } catch (e) {
    setStatus(`Failed to load graph: ${e.message}`, true);
    console.error('Failed to load graph:', e);
  }
}

function refreshGraph() {
  loadGraph();
}

function fitGraph() {
  if (cy) cy.fit(undefined, 40);
}

// ── WebSocket Connection ─────────────────────────────────────

function connectWebSocket() {
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
  const ws = new WebSocket(`${protocol}//${location.host}/ws`);

  const dot = document.getElementById('ws-dot');
  const label = document.getElementById('ws-label');

  dot.className = 'dot connecting';
  label.textContent = 'Connecting...';

  ws.onopen = () => {
    dot.className = 'dot connected';
    label.textContent = 'Live';
  };

  ws.onclose = () => {
    dot.className = 'dot';
    label.textContent = 'Disconnected';
    // Reconnect after 3 seconds
    setTimeout(connectWebSocket, 3000);
  };

  ws.onerror = () => {
    dot.className = 'dot';
    label.textContent = 'Error';
  };

  ws.onmessage = (msg) => {
    try {
      const event = JSON.parse(msg.data);
      handleEvent(event);
    } catch (e) {
      console.warn('Invalid WS message:', msg.data);
    }
  };
}

// ── Event Handling ───────────────────────────────────────────

function handleEvent(event) {
  eventCount++;
  document.getElementById('stat-events').textContent = eventCount;

  if (event.type === 'file_changed') {
    highlightFile(event.path, event.kind);
    addLogEntry(event.path, event.kind);
  } else if (event.type === 'telemetry_update') {
    // Refresh graph to pick up new heatmap data
    if (event.hotspot_file) {
      addLogEntry(event.hotspot_file, 'hotspot');
    }
    // Debounce: only refresh every 5 seconds max
    if (!handleEvent._heatDebounce) {
      handleEvent._heatDebounce = setTimeout(() => {
        refreshGraph();
        handleEvent._heatDebounce = null;
      }, 5000);
    }
  }
}

function applyHeatmap() {
  if (!cy) return;
  cy.nodes().forEach(node => {
    const heat = node.data('heatLevel');
    node.removeClass('heat-hot heat-warm heat-cool');
    if (heat === 'hot') node.addClass('heat-hot');
    else if (heat === 'warm') node.addClass('heat-warm');
    else if (heat === 'cool') node.addClass('heat-cool');
  });
}

function highlightFile(path, kind) {
  if (!cy) return;

  // Find the file node matching this path
  const fileNodes = cy.nodes('[type="file"]').filter(n => {
    const fp = n.data('fullPath') || '';
    return path.endsWith(fp) || fp.endsWith(path.split('/').pop());
  });

  if (fileNodes.length === 0) return;

  // Pulse animation
  fileNodes.addClass('pulse');
  fileNodes.children().addClass('highlighted');

  setTimeout(() => {
    fileNodes.removeClass('pulse');
    fileNodes.children().removeClass('highlighted');
  }, 2000);

  // If a file was created or deleted, refresh the graph
  if (kind === 'created' || kind === 'deleted') {
    setTimeout(refreshGraph, 500);
  }
}

function addLogEntry(path, kind) {
  const log = document.getElementById('activity-log');
  const entry = document.createElement('div');
  entry.className = 'log-entry';
  const time = new Date().toLocaleTimeString();
  const shortPath = path.split('/').slice(-2).join('/');
  entry.innerHTML = `<span class="log-time">${time}</span> <span class="log-kind">${kind}</span> <span class="log-path">${shortPath}</span>`;
  log.prepend(entry);

  // Keep only last 50 entries
  while (log.children.length > 50) {
    log.removeChild(log.lastChild);
  }
}

function toggleLog() {
  document.getElementById('activity-log').classList.toggle('visible');
}

// ── Boot ─────────────────────────────────────────────────────

loadGraph();
connectWebSocket();
