// SYNAPSEED Architecture Visualizer — X-Ray Mode (HCI Req 10)

var __xrayOverlay = null;
var __xrayTimeout = null;

function initXray() {
  if (!__cy) return;

  __cy.on('mouseover', 'node[type="file"]', function(e) {
    if (!e.originalEvent.shiftKey) return;
    clearTimeout(__xrayTimeout);
    var nodeId = e.target.data('fullPath') || e.target.id();
    var pos = e.renderedPosition;
    __xrayTimeout = setTimeout(function() {
      fetch('/api/xray?node=' + encodeURIComponent(nodeId))
        .then(function(r) { return r.json(); })
        .then(function(data) { showXrayOverlay(pos, data); })
        .catch(function() { /* silently fail */ });
    }, 200);
  });

  __cy.on('mouseout', 'node[type="file"]', function() {
    clearTimeout(__xrayTimeout);
    hideXrayOverlay();
  });

  // Also hide on background click
  __cy.on('tap', function(e) {
    if (e.target === __cy) hideXrayOverlay();
  });
}

function showXrayOverlay(pos, data) {
  hideXrayOverlay();

  var overlay = document.createElement('div');
  overlay.className = 'xray-overlay';

  // Position near cursor, clamped inside viewport
  var x = Math.min(pos.x + 24, window.innerWidth - 380);
  var y = Math.max(pos.y - 20, 10);
  if (y + 360 > window.innerHeight) y = window.innerHeight - 370;
  overlay.style.left = x + 'px';
  overlay.style.top = y + 'px';

  var html = '<div class="xray-title">' + esc(data.file || '?') + '</div>';

  if (data.imports && data.imports.length > 0) {
    html += '<div class="xray-section"><span class="xray-label">Imports (' + data.imports.length + ')</span>';
    data.imports.slice(0, 8).forEach(function(imp) {
      html += '<div class="xray-item">&rarr; ' + esc(imp) + '</div>';
    });
    if (data.imports.length > 8) html += '<div class="xray-item" style="color:#484f58">...and ' + (data.imports.length - 8) + ' more</div>';
    html += '</div>';
  }

  if (data.importers && data.importers.length > 0) {
    html += '<div class="xray-section"><span class="xray-label">Imported by (' + data.importers.length + ')</span>';
    data.importers.slice(0, 8).forEach(function(imp) {
      html += '<div class="xray-item">&larr; ' + esc(imp) + '</div>';
    });
    if (data.importers.length > 8) html += '<div class="xray-item" style="color:#484f58">...and ' + (data.importers.length - 8) + ' more</div>';
    html += '</div>';
  }

  if (data.preview) {
    html += '<div class="xray-section"><span class="xray-label">Preview</span>';
    html += '<pre class="xray-code">' + esc(data.preview) + '</pre>';
    html += '</div>';
  }

  if ((!data.imports || data.imports.length === 0) && (!data.importers || data.importers.length === 0) && !data.preview) {
    html += '<div class="xray-section" style="color:#484f58">No dependency or source data available</div>';
  }

  overlay.innerHTML = html;
  document.body.appendChild(overlay);
  __xrayOverlay = overlay;
}

function hideXrayOverlay() {
  if (__xrayOverlay) {
    __xrayOverlay.remove();
    __xrayOverlay = null;
  }
}
