// SYNAPSEED Architecture Visualizer — Detail Panels

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
      <div class="panel-value ${d.heatLevel === 'hot' ? 'orange' : 'green'}">${esc(d.heatLevel)} (${d.heatMs.toFixed(1)}ms avg)</div>
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
      <div class="panel-value ${d.heatLevel === 'hot' ? 'orange' : 'green'}">${esc(d.heatLevel)} (${d.heatMs.toFixed(1)}ms avg)</div>
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
