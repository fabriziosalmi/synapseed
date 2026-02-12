// SYNAPSEED Architecture Visualizer — Frontend Logic

const NODE_COLORS = {
  file:     { bg: '#161b22', border: '#30363d', text: '#c9d1d9' },
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

// ── Initialize Cytoscape ─────────────────────────────────────

function initCytoscape(elements, autoCollapse) {
  try {
    __cy = cytoscape({
      container: document.getElementById('cy'),
      elements: elements,
      style: [
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
        // Collapsed file nodes — compact labeled boxes
        {
          selector: 'node[type="file"].collapsed',
          style: {
            'width': 140,
            'height': 36,
            'padding': '6px',
            'min-width': '140px',
            'min-height': '36px',
            'border-style': 'solid',
            'border-width': 2,
            'border-color': '#58a6ff',
            'background-color': '#161b22',
            'text-valign': 'center',
            'text-halign': 'center',
            'text-margin-y': 0,
            'font-size': '11px',
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
      ],
      layout: { name: 'preset' },
      minZoom: 0.1,
      maxZoom: 5.0,
      wheelSensitivity: 0.2,
    });

    // Auto-collapse all files on large graphs (reduces 900+ nodes to ~56)
    if (autoCollapse) {
      __cy.nodes('[type="file"]').forEach(function(fileNode) {
        __collapsedFiles.add(fileNode.id());
        fileNode.addClass('collapsed');
        fileNode.children().addClass('sym-hidden');
      });
    } else {
      // Re-apply collapsed state from previous render
      __collapsedFiles.forEach(fileId => {
        const fileNode = __cy.getElementById(fileId);
        if (fileNode.length) {
          fileNode.addClass('collapsed');
          fileNode.children().addClass('sym-hidden');
        }
      });
    }

    // Apply telemetry heatmap from data
    applyHeatmap();

    // ── Hover tooltip ──
    __cy.on('mouseover', 'node[type!="file"]', function(e) {
      const d = e.target.data();
      showTooltip(e, `
        <div class="tt-name">${esc(d.name || d.label)}</div>
        <div class="tt-kind">${esc(d.kind || d.type)}</div>
        <div class="tt-loc">L${d.lineStart}–L${d.lineEnd}</div>
      `);
    });

    __cy.on('mouseover', 'node[type="file"]', function(e) {
      const d = e.target.data();
      const n = e.target.children().length;
      showTooltip(e, `
        <div class="tt-name">${esc(d.label)}</div>
        <div class="tt-kind">${esc(d.language || 'unknown')} — ${n} symbol${n !== 1 ? 's' : ''}</div>
      `);
    });

    __cy.on('mouseout', 'node', function() {
      document.getElementById('tooltip').style.display = 'none';
    });

    // ── Single click → detail panel ──
    __cy.on('tap', 'node[type="file"]', function(e) {
      showFilePanel(e.target);
    });

    __cy.on('tap', 'node[type!="file"]', function(e) {
      showSymbolPanel(e.target);
    });

    // ── Double click → collapse/expand file ──
    __cy.on('dbltap', 'node[type="file"]', function(e) {
      toggleFileCollapse(e.target);
    });

    // ── Click background → dismiss ──
    __cy.on('tap', function(e) {
      if (e.target === __cy) {
        document.getElementById('tooltip').style.display = 'none';
      }
    });

    // ── Auto-resize handler ──
    window.addEventListener('resize', function() {
      if (__cy) {
        __cy.resize();
        __cy.fit(__cy.elements(), 50);
      }
    });

    // Run the force-directed layout after all setup is done
    runLayout();

  } catch (err) {
    console.error('Cytoscape init failed:', err);
    setStatus('Graph init failed: ' + err.message, true);
  }
}

// ── Run Layout ───────────────────────────────────────────

function runLayout() {
  if (!__cy) return;
  // Adapt physics based on visible (non-hidden) node count
  var visibleCount = __cy.nodes(':visible').length;
  var isLarge = visibleCount > 30;

  var layout = __cy.layout({
    name: 'cose',
    animate: true,
    animationDuration: isLarge ? 500 : 800,
    randomize: true,
    // Physics — scale to graph size
    componentSpacing: isLarge ? 30 : 60,
    nodeRepulsion: function() { return isLarge ? 400 : 2000; },
    nodeOverlap: 20,
    idealEdgeLength: function() { return isLarge ? 30 : 50; },
    edgeElasticity: function() { return 100; },
    nestingFactor: isLarge ? 10 : 5,
    gravity: isLarge ? 80 : 1.2,
    numIter: 1000,
    initialTemp: isLarge ? 50 : 200,
    coolingFactor: 0.95,
    minTemp: 1.0,
    nodeDimensionsIncludeLabels: true,
    fit: true,
    padding: 40,
    // Force fit + zoom clamp when layout finishes
    stop: function() {
      __cy.fit(__cy.elements(':visible'), 40);
      // Clamp zoom to a readable range
      if (__cy.zoom() > 1.5) { __cy.zoom(1.5); __cy.center(); }
      if (__cy.zoom() < 0.3) { __cy.zoom(0.3); __cy.center(); }
    }
  });
  layout.run();
}

// ── Tooltip Helper ────────────────────────────────────────

function showTooltip(e, html) {
  const tooltip = document.getElementById('tooltip');
  tooltip.innerHTML = html;
  tooltip.style.display = 'block';
  const pos = e.renderedPosition;
  if (pos) {
    // Keep tooltip inside viewport
    const x = Math.min(pos.x + 20, window.innerWidth - 440);
    const y = Math.max(pos.y - 10, 10);
    tooltip.style.left = x + 'px';
    tooltip.style.top = y + 'px';
  }
}

function esc(str) {
  if (!str) return '';
  const el = document.createElement('span');
  el.textContent = str;
  return el.innerHTML;
}

// ── Detail Panel ──────────────────────────────────────────

function showFilePanel(node) {
  const d = node.data();
  const children = node.children();
  const panel = document.getElementById('detail-panel');
  const title = document.getElementById('panel-title');
  const body = document.getElementById('panel-body');

  title.textContent = d.label;

  let symbolsHtml = '';
  children.forEach(child => {
    const cd = child.data();
    const color = (NODE_COLORS[cd.type] || NODE_COLORS.function).border;
    symbolsHtml += `
      <li data-id="${esc(child.id())}" onclick="focusNode('${esc(child.id())}')">
        <span class="sym-dot" style="background:${color}"></span>
        <span>${esc(cd.name || cd.label)}</span>
        <span class="sym-lines">L${cd.lineStart}–L${cd.lineEnd}</span>
      </li>`;
  });

  const isCollapsed = __collapsedFiles.has(node.id());

  body.innerHTML = `
    <div class="panel-section">
      <div class="panel-label">PATH</div>
      <div class="panel-value">${esc(d.fullPath)}</div>
    </div>
    <div class="panel-section">
      <div class="panel-label">LANGUAGE</div>
      <div class="panel-value accent">${esc(d.language || 'unknown')}</div>
    </div>
    <div class="panel-section">
      <div class="panel-label">SYMBOLS (${children.length})</div>
      <div class="panel-value" style="font-size:11px;color:#484f58;margin-bottom:4px">
        Double-click file to ${isCollapsed ? 'expand' : 'collapse'}
      </div>
      <ul class="panel-symbol-list">${symbolsHtml || '<li style="color:#484f58">No symbols</li>'}</ul>
    </div>
    ${d.heatLevel !== 'none' ? `
    <div class="panel-section">
      <div class="panel-label">HEAT</div>
      <div class="panel-value ${d.heatLevel === 'hot' ? 'orange' : 'green'}">${d.heatLevel} (${d.heatMs.toFixed(1)}ms avg)</div>
    </div>` : ''}
  `;

  panel.classList.add('visible');
}

function showSymbolPanel(node) {
  const d = node.data();
  const parent = node.parent();
  const filePath = parent.length ? parent.data('fullPath') || parent.data('label') : '—';
  const panel = document.getElementById('detail-panel');
  const title = document.getElementById('panel-title');
  const body = document.getElementById('panel-body');
  const color = (NODE_COLORS[d.type] || NODE_COLORS.function).border;

  title.textContent = d.name || d.label;

  body.innerHTML = `
    <div class="panel-section">
      <div class="panel-label">KIND</div>
      <div class="panel-value"><span class="sym-dot" style="background:${color};display:inline-block;vertical-align:middle;margin-right:6px"></span>${esc(d.kind || d.type)}</div>
    </div>
    <div class="panel-section">
      <div class="panel-label">LINES</div>
      <div class="panel-value green">L${d.lineStart} – L${d.lineEnd}</div>
    </div>
    <div class="panel-section">
      <div class="panel-label">SIGNATURE</div>
      <div class="panel-value" style="font-size:12px;word-break:break-all">${esc(d.label)}</div>
    </div>
    <div class="panel-section">
      <div class="panel-label">FILE</div>
      <div class="panel-value" style="font-size:12px">${esc(filePath)}</div>
    </div>
    ${d.heatLevel !== 'none' ? `
    <div class="panel-section">
      <div class="panel-label">HEAT</div>
      <div class="panel-value ${d.heatLevel === 'hot' ? 'orange' : 'green'}">${d.heatLevel} (${d.heatMs.toFixed(1)}ms avg)</div>
    </div>` : ''}
  `;

  panel.classList.add('visible');
}

function closePanel() {
  document.getElementById('detail-panel').classList.remove('visible');
}

function focusNode(nodeId) {
  if (!__cy) return;
  const node = __cy.getElementById(nodeId);
  if (node.length) {
    // If hidden (collapsed parent), expand first
    if (node.hasClass('sym-hidden')) {
      const parent = node.parent();
      if (parent.length) toggleFileCollapse(parent);
    }
    __cy.animate({ center: { eles: node }, zoom: 2 }, { duration: 400 });
    node.select();
    showSymbolPanel(node);
  }
}

// ── Expand / Collapse ─────────────────────────────────────

function toggleFileCollapse(fileNode) {
  const id = fileNode.id();
  const children = fileNode.children();

  if (__collapsedFiles.has(id)) {
    // Expand
    __collapsedFiles.delete(id);
    fileNode.removeClass('collapsed');
    children.removeClass('sym-hidden');
  } else {
    // Collapse
    __collapsedFiles.add(id);
    fileNode.addClass('collapsed');
    children.addClass('sym-hidden');
  }
}

// ── Search / Filter ───────────────────────────────────────

var __searchTimeout = null;

function initSearch() {
  const input = document.getElementById('search-box');
  if (!input) return;

  input.addEventListener('input', function() {
    clearTimeout(__searchTimeout);
    __searchTimeout = setTimeout(() => applySearch(input.value.trim()), 150);
  });

  // Escape clears search
  input.addEventListener('keydown', function(e) {
    if (e.key === 'Escape') {
      input.value = '';
      applySearch('');
      input.blur();
    }
  });
}

function applySearch(query) {
  if (!__cy) return;

  // Clear previous search classes
  __cy.nodes().removeClass('search-dimmed search-match');

  if (!query) return;

  const q = query.toLowerCase();
  let matchCount = 0;

  __cy.nodes().forEach(node => {
    const d = node.data();
    const searchable = [
      d.label, d.name, d.type, d.kind, d.fullPath, d.language
    ].filter(Boolean).join(' ').toLowerCase();

    if (searchable.includes(q)) {
      node.addClass('search-match');
      // Also highlight parent file if a symbol matches
      if (d.type !== 'file') {
        node.parent().addClass('search-match');
      }
      matchCount++;
    } else {
      node.addClass('search-dimmed');
    }
  });

  // Remove dimmed from matched file parents
  __cy.nodes('.search-match').removeClass('search-dimmed');

  // Update status briefly
  if (matchCount === 0) {
    setStatus(`No results for "${query}"`, true);
    setTimeout(() => setStatus(null), 2000);
  }
}

// ── Graph Data Loading ───────────────────────────────────

async function loadGraph() {
  setStatus('Loading graph...', false);
  try {
    const res = await fetch('/api/graph');
    if (!res.ok) {
      const text = await res.text();
      setStatus('API error: ' + res.status + ' — ' + text, true);
      console.error('API error:', res.status, text);
      return;
    }
    const data = await res.json();

    document.getElementById('stat-files').textContent = data.stats.files;
    document.getElementById('stat-symbols').textContent = data.stats.symbols;

    // Compound nodes only — containment is via "parent" field
    const elements = data.elements.nodes || [];

    if (elements.length === 0) {
      setStatus('No files indexed — is the project path correct?', true);
      return;
    }

    if (__cy) {
      __cy.destroy();
      __cy = null;
    }
    setStatus(null);

    // Auto-collapse on large graphs (>8 files) so only file boxes are visible
    var fileCount = elements.filter(function(e) { return e.data && e.data.type === 'file'; }).length;
    var autoCollapse = fileCount > 8;
    initCytoscape(elements, autoCollapse);

    // Re-apply search if active
    const searchVal = document.getElementById('search-box').value.trim();
    if (searchVal) applySearch(searchVal);

  } catch (e) {
    setStatus('Failed to load graph: ' + e.message, true);
    console.error('Failed to load graph:', e);
  }
}

function refreshGraph() {
  loadGraph();
}

function fitGraph() {
  if (__cy) __cy.fit(undefined, 40);
}

// ── WebSocket Connection ─────────────────────────────────

function connectWebSocket() {
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
  let ws;
  try {
    ws = new WebSocket(protocol + '//' + location.host + '/ws');
  } catch (e) {
    console.warn('WebSocket creation failed:', e);
    return;
  }

  const dot = document.getElementById('ws-dot');
  const label = document.getElementById('ws-label');

  dot.className = 'dot connecting';
  label.textContent = 'Connecting...';

  ws.onopen = function() {
    dot.className = 'dot connected';
    label.textContent = 'Live';
  };

  ws.onclose = function() {
    dot.className = 'dot';
    label.textContent = 'Disconnected';
    setTimeout(connectWebSocket, 3000);
  };

  ws.onerror = function() {
    dot.className = 'dot';
    label.textContent = 'Error';
  };

  ws.onmessage = function(msg) {
    try {
      var event = JSON.parse(msg.data);
      handleEvent(event);
    } catch (e) {
      // silently ignore malformed messages
    }
  };
}

// ── Event Handling ───────────────────────────────────────

function handleEvent(event) {
  __eventCount++;
  document.getElementById('stat-events').textContent = __eventCount;

  if (event.type === 'file_changed') {
    highlightFile(event.path, event.kind);
    addLogEntry(event.path, event.kind);
  } else if (event.type === 'telemetry_update') {
    if (event.hotspot_file) {
      addLogEntry(event.hotspot_file, 'hotspot');
    }
    // Debounce: only refresh every 5 seconds max
    if (!handleEvent._heatDebounce) {
      handleEvent._heatDebounce = setTimeout(function() {
        refreshGraph();
        handleEvent._heatDebounce = null;
      }, 5000);
    }
  }
}

function applyHeatmap() {
  if (!__cy) return;
  __cy.nodes().forEach(function(node) {
    var heat = node.data('heatLevel');
    node.removeClass('heat-hot heat-warm heat-cool');
    if (heat === 'hot') node.addClass('heat-hot');
    else if (heat === 'warm') node.addClass('heat-warm');
    else if (heat === 'cool') node.addClass('heat-cool');
  });
}

function highlightFile(path, kind) {
  if (!__cy) return;

  var fileName = path.split('/').pop();
  var fileNodes = __cy.nodes('[type="file"]').filter(function(n) {
    var fp = n.data('fullPath') || '';
    return fp === path || fp.endsWith('/' + fileName) || path.endsWith(fp);
  });

  if (fileNodes.length === 0) return;

  fileNodes.addClass('pulse');
  fileNodes.children().addClass('highlighted');

  setTimeout(function() {
    fileNodes.removeClass('pulse');
    fileNodes.children().removeClass('highlighted');
  }, 2000);

  if (kind === 'created' || kind === 'deleted') {
    setTimeout(refreshGraph, 500);
  }
}

function addLogEntry(path, kind) {
  var log = document.getElementById('activity-log');
  var entry = document.createElement('div');
  entry.className = 'log-entry';
  var time = new Date().toLocaleTimeString();
  var shortPath = path.split('/').slice(-2).join('/');
  entry.innerHTML = '<span class="log-time">' + time + '</span> <span class="log-kind">' + kind + '</span> <span class="log-path">' + shortPath + '</span>';
  log.prepend(entry);

  while (log.children.length > 50) {
    log.removeChild(log.lastChild);
  }
}

function toggleLog() {
  var log = document.getElementById('activity-log');
  var btn = document.getElementById('btn-activity');
  log.classList.toggle('visible');
  btn.classList.toggle('active');
}

// ── Boot ─────────────────────────────────────────────────

initSearch();
loadGraph();
connectWebSocket();
