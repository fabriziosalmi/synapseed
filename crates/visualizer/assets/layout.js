// SYNAPSEED Architecture Visualizer — Layout & Collapse

// ── Collapse State ───────────────────────────────────────────

function applyCollapseState(autoCollapse) {
  if (!__cy) return;
  if (autoCollapse) {
    __cy.nodes('[type="file"]').forEach(function(fileNode) {
      __collapsedFiles.add(fileNode.id());
      fileNode.addClass('collapsed');
      fileNode.children().addClass('sym-hidden');
    });
  } else {
    __collapsedFiles.forEach(fileId => {
      const fileNode = __cy.getElementById(fileId);
      if (fileNode.length) {
        fileNode.addClass('collapsed');
        fileNode.children().addClass('sym-hidden');
      }
    });
  }
}

// ── Run Layout ───────────────────────────────────────────

function runLayout() {
  if (!__cy) return;

  var visibleNodes = __cy.nodes(':visible');
  var visibleCount = visibleNodes.length;
  var hasVisibleSymbols = visibleNodes.filter('[type!="file"]').length > 0;

  // Strategy: grid for collapsed-only overview, COSE for expanded/mixed
  var useGrid = visibleCount > 20 && !hasVisibleSymbols;

  var layoutOpts;

  if (useGrid) {
    // ── Grid layout: clean, organized rows for collapsed file boxes ──
    layoutOpts = {
      name: 'grid',
      fit: true,
      padding: 40,
      avoidOverlap: true,
      avoidOverlapPadding: 20,
      nodeDimensionsIncludeLabels: true,
      condense: true,
      sort: function(a, b) {
        return (a.data('label') || '').localeCompare(b.data('label') || '');
      },
      animate: false,
      stop: function() {
        // Clamp zoom so labels stay readable
        if (__cy.zoom() > 1.5) { __cy.zoom(1.5); __cy.center(); }
        if (__cy.zoom() < 0.5) { __cy.zoom(0.5); __cy.center(); }
      }
    };
  } else {
    // ── COSE force-directed for expanded or small graphs ──
    var isLarge = visibleCount > 100;
    layoutOpts = {
      name: 'cose',
      animate: !isLarge,             // no animation on huge graphs (prevents zoom flicker)
      animationDuration: 600,
      animationEasing: 'ease-out-cubic',
      randomize: true,
      componentSpacing: isLarge ? 20 : 60,
      nodeRepulsion: function() { return isLarge ? 600 : 2000; },
      nodeOverlap: 20,
      idealEdgeLength: function() { return isLarge ? 40 : 50; },
      edgeElasticity: function() { return 100; },
      nestingFactor: isLarge ? 12 : 5,
      gravity: isLarge ? 80 : 1.2,
      numIter: isLarge ? 500 : 1000,
      initialTemp: isLarge ? 100 : 200,
      coolingFactor: 0.95,
      minTemp: 1.0,
      nodeDimensionsIncludeLabels: true,
      fit: false,                    // we handle fit in stop callback only
      padding: 30,
      stop: function() {
        __cy.fit(__cy.elements(':visible'), 30);
        if (__cy.zoom() > 1.5) { __cy.zoom(1.5); __cy.center(); }
        if (__cy.zoom() < 0.4) { __cy.zoom(0.4); __cy.center(); }
      }
    };
  }

  var layout = __cy.layout(layoutOpts);
  layout.run();
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

  // Re-run layout to adapt (may switch between grid <-> COSE)
  runLayout();
}
