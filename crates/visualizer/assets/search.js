// SYNAPSEED Architecture Visualizer — Search / Filter

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
