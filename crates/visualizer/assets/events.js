// SYNAPSEED Architecture Visualizer — Event Handling

// ── Hover Animations (HCI Req 7) ─────────────────────────────

function setupHoverAnimations() {
  if (!__cy) return;
  __cy.on('mouseover', 'node', function (e) {
    var node = e.target;
    if (node.isParent() && node.data('type') === 'cluster') return;
    node.stop();
    node.animate({
      style: {
        'border-width': (node.data('type') === 'file' ? 5 : 4),
        'overlay-opacity': 0.08,
      }
    }, { duration: 150, easing: 'ease-out-cubic' });
  });
  __cy.on('mouseout', 'node', function (e) {
    var node = e.target;
    if (node.isParent() && node.data('type') === 'cluster') return;
    node.stop();
    node.animate({
      style: {
        'border-width': (node.data('type') === 'file' ? 3 : 2),
        'overlay-opacity': 0,
      }
    }, { duration: 200, easing: 'ease-in-cubic' });
  });
}

// ── Tooltips ─────────────────────────────────────────────────

function setupTooltips() {
  if (!__cy) return;
  __cy.on('mouseover', 'node[type!="file"]', function (e) {
    const d = e.target.data();
    showTooltip(e, `
      <div class="tt-name">${esc(d.name || d.label)}</div>
      <div class="tt-kind">${esc(d.kind || d.type)}</div>
      <div class="tt-loc">L${d.lineStart}–L${d.lineEnd}</div>
    `);
  });

  __cy.on('mouseover', 'node[type="file"]', function (e) {
    const d = e.target.data();
    const n = e.target.children().length;
    var extra = '';
    if (d.instability != null) extra += '<div class="tt-kind">Instability: ' + d.instability.toFixed(2) + '</div>'; if (d.coupling > 0) extra += '<div class="tt-kind">Coupling: ' + d.coupling + ' (' + d.physicsClass + ')</div>'; if (d.inCycle) extra += '<div class="tt-kind" style="color:#f85149">In dependency cycle</div>';
    showTooltip(e, `
      <div class="tt-name">${esc(d.label)}</div>
      <div class="tt-kind">${esc(d.language || 'unknown')} — ${n} symbol${n !== 1 ? 's' : ''}</div>
      ${extra}
    `);
  });

  __cy.on('mouseout', 'node', function () {
    document.getElementById('tooltip').style.display = 'none';
  });
}

// ── Click Handlers ───────────────────────────────────────────

function setupClickHandlers() {
  if (!__cy) return;
  __cy.on('tap', 'node[type="file"]', function (e) {
    showFilePanel(e.target);
  });

  __cy.on('tap', 'node[type!="file"]', function (e) {
    showSymbolPanel(e.target);
  });

  __cy.on('dbltap', 'node[type="file"]', function (e) {
    toggleFileCollapse(e.target);
  });

  __cy.on('tap', function (e) {
    if (e.target === __cy) {
      document.getElementById('tooltip').style.display = 'none';
    }
  });

  window.addEventListener('resize', function () {
    if (__cy) {
      __cy.resize();
      __cy.fit(__cy.elements(), 50);
    }
  });
}

// ── WebSocket Event Handling ─────────────────────────────────

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
      handleEvent._heatDebounce = setTimeout(function () {
        refreshGraph();
        handleEvent._heatDebounce = null;
      }, 5000);
    }
  }
}

function applyHeatmap() {
  if (!__cy) return;
  __cy.nodes().forEach(function (node) {
    var heat = node.data('heatLevel');
    node.removeClass('heat-hot heat-warm heat-cool');
    if (heat === 'hot') node.addClass('heat-hot');
    else if (heat === 'warm') node.addClass('heat-warm');
    else if (heat === 'cool') node.addClass('heat-cool');
  });
}

function applyPhysics() {
  if (!__cy) return;
  __cy.nodes('[type="file"]').forEach(function (node) {
    var pc = node.data('physicsClass');
    node.removeClass('physics-rigid physics-gaseous physics-fluid');
    if (pc === 'rigid') node.addClass('physics-rigid');
    else if (pc === 'gaseous') node.addClass('physics-gaseous');
    else if (pc === 'fluid') node.addClass('physics-fluid');
  });
}

function highlightFile(path, kind) {
  if (!__cy) return;

  var fileName = path.split('/').pop();
  var fileNodes = __cy.nodes('[type="file"]').filter(function (n) {
    var fp = n.data('fullPath') || '';
    return fp === path || fp.endsWith('/' + fileName) || path.endsWith(fp);
  });

  if (fileNodes.length === 0) return;

  fileNodes.addClass('pulse');
  fileNodes.children().addClass('highlighted');

  setTimeout(function () {
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
  entry.innerHTML = '<span class="log-time">' + esc(time) + '</span> <span class="log-kind">' + esc(kind) + '</span> <span class="log-path">' + esc(shortPath) + '</span>';
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
