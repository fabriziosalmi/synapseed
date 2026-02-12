// SYNAPSEED Architecture Visualizer — API & WebSocket

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

    // Compound nodes + edges
    const nodeElements = data.elements.nodes || [];
    const edgeElements = data.elements.edges || [];
    const elements = nodeElements.concat(edgeElements);

    if (nodeElements.length === 0) {
      setStatus('No files indexed — is the project path correct?', true);
      return;
    }

    if (__cy) {
      __cy.destroy();
      __cy = null;
    }
    setStatus(null);

    // Auto-collapse on large graphs (>8 files) so only file boxes are visible
    var fileCount = nodeElements.filter(function(e) { return e.data && e.data.type === 'file'; }).length;
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
