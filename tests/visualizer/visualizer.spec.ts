/**
 * SYNAPSEED Architecture Visualizer — Playwright E2E Test Suite
 *
 * Tests cover:
 *  1. Page load & structure
 *  2. Graph rendering (Cytoscape)
 *  3. Footer stats
 *  4. Search / filter
 *  5. Detail panel (file click)
 *  6. Detail panel (symbol click)
 *  7. Panel close
 *  8. Double-click collapse/expand
 *  9. Activity log toggle
 * 10. Tooltip on hover
 * 11. Keyboard (Escape clears search)
 * 12. Responsive layout
 * 13. Heatmap classes
 * 14. WebSocket status indicator
 * 15. Graph status overlay
 * 16. Fit button
 * 17. Refresh button
 * 18. No console errors
 */

import { test, expect, Page } from '@playwright/test';

// ── Helpers ──────────────────────────────────────────────────

/** Wait for the Cytoscape graph to render (nodes present in the canvas). */
async function waitForGraph(page: Page) {
  // Wait for stat counters to populate (proves API loaded)
  await expect(page.locator('#stat-files')).not.toHaveText('—', { timeout: 10_000 });
  // Give Cytoscape layout animation time to settle
  await page.waitForTimeout(1200);
}

/** Get a count of Cytoscape nodes via the global `__cy` object. */
async function nodeCount(page: Page): Promise<number> {
  return page.evaluate(() => (window as any).__cy?.nodes().length ?? 0);
}

/** Tap a Cytoscape node by its data id. */
async function tapNode(page: Page, nodeId: string) {
  await page.evaluate((id) => {
    const c = (window as any).__cy;
    const node = c.getElementById(id);
    if (node.length) node.emit('tap');
  }, nodeId);
}

/** Double-tap a Cytoscape node by its data id. */
async function dbltapNode(page: Page, nodeId: string) {
  await page.evaluate((id) => {
    const c = (window as any).__cy;
    const node = c.getElementById(id);
    if (node.length) node.emit('dbltap');
  }, nodeId);
}

/** Trigger mouseover on a Cytoscape node. */
async function hoverNode(page: Page, nodeId: string) {
  await page.evaluate((id) => {
    const c = (window as any).__cy;
    const node = c.getElementById(id);
    if (node.length) node.emit('mouseover', { renderedPosition: { x: 300, y: 200 } });
  }, nodeId);
}

/** Trigger mouseout on a Cytoscape node. */
async function mouseoutNode(page: Page, nodeId: string) {
  await page.evaluate((id) => {
    const c = (window as any).__cy;
    const node = c.getElementById(id);
    if (node.length) node.emit('mouseout');
  }, nodeId);
}

// ── Tests ────────────────────────────────────────────────────

test.describe('Visualizer — Page Structure', () => {
  test('loads with correct title and layout', async ({ page }) => {
    await page.goto('/');
    await expect(page).toHaveTitle('SYNAPSEED — Architecture Visualizer');

    // Header elements
    await expect(page.locator('header h1')).toContainText('SYNAPSESEED');
    await expect(page.locator('#search-box')).toBeVisible();
    await expect(page.locator('header .controls button')).toHaveCount(3);

    // Main area
    await expect(page.locator('#cy')).toBeVisible();

    // Footer
    await expect(page.locator('footer')).toBeVisible();
    await expect(page.locator('#ws-status')).toBeVisible();
  });

  test('has no initial console errors (except WebSocket)', async ({ page }) => {
    const errors: string[] = [];
    page.on('console', (msg) => {
      if (msg.type() === 'error') {
        const text = msg.text();
        // Ignore expected WebSocket connection failure in test mode
        if (!text.includes('WebSocket') && !text.includes('ws://')) {
          errors.push(text);
        }
      }
    });

    await page.goto('/');
    await waitForGraph(page);

    expect(errors).toEqual([]);
  });
});

test.describe('Visualizer — Graph Rendering', () => {
  test('renders graph with correct node count', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    // Mock data has 6 nodes (2 files + 4 symbols)
    const count = await nodeCount(page);
    expect(count).toBe(6);
  });

  test('displays correct file and symbol stats in footer', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    await expect(page.locator('#stat-files')).toHaveText('2');
    await expect(page.locator('#stat-symbols')).toHaveText('4');
    await expect(page.locator('#stat-events')).toHaveText('0');
  });

  test('graph status overlay disappears after load', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    const status = page.locator('#graph-status');
    await expect(status).toHaveClass(/hidden/);
  });
});

test.describe('Visualizer — Search', () => {
  test('filters nodes by typing in search box', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    const searchBox = page.locator('#search-box');
    await searchBox.fill('main');

    // Wait for debounce (150ms)
    await page.waitForTimeout(300);

    // Check that matching nodes have search-match class
    const matchCount = await page.evaluate(() => {
      return (window as any).__cy.nodes('.search-match').length;
    });
    expect(matchCount).toBeGreaterThan(0);

    // Non-matching nodes should be dimmed
    const dimmedCount = await page.evaluate(() => {
      return (window as any).__cy.nodes('.search-dimmed').length;
    });
    expect(dimmedCount).toBeGreaterThan(0);
  });

  test('Escape key clears search and restores all nodes', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    const searchBox = page.locator('#search-box');
    await searchBox.fill('main');
    await page.waitForTimeout(300);

    // Press Escape
    await searchBox.press('Escape');
    await page.waitForTimeout(200);

    // Search box should be empty
    await expect(searchBox).toHaveValue('');

    // No nodes should have search classes
    const dimmed = await page.evaluate(() => {
      return (window as any).__cy.nodes('.search-dimmed').length;
    });
    expect(dimmed).toBe(0);
  });

  test('shows "no results" status for unmatched search', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    const searchBox = page.locator('#search-box');
    await searchBox.fill('zzzznonexistent');
    await page.waitForTimeout(300);

    const status = page.locator('#graph-status');
    await expect(status).toContainText('No results');
  });
});

test.describe('Visualizer — Detail Panel', () => {
  test('clicking a file node opens the detail panel', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    await tapNode(page, 'file:src/main.rs');
    await page.waitForTimeout(200);

    const panel = page.locator('#detail-panel');
    await expect(panel).toHaveClass(/visible/);

    // Panel title = file label
    await expect(page.locator('#panel-title')).toHaveText('main.rs');

    // Panel body shows path
    await expect(page.locator('#panel-body')).toContainText('src/main.rs');

    // Panel body shows language
    await expect(page.locator('#panel-body')).toContainText('Rust');

    // Panel body lists symbols
    await expect(page.locator('#panel-body .panel-symbol-list li')).toHaveCount(2);
  });

  test('clicking a symbol node opens symbol detail panel', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    await tapNode(page, 'sym:src/main.rs:main');
    await page.waitForTimeout(200);

    const panel = page.locator('#detail-panel');
    await expect(panel).toHaveClass(/visible/);

    await expect(page.locator('#panel-title')).toHaveText('main');
    await expect(page.locator('#panel-body')).toContainText('Function');
    await expect(page.locator('#panel-body')).toContainText('L1');
    await expect(page.locator('#panel-body')).toContainText('L20');
  });

  test('close button dismisses the panel', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    // Open panel
    await tapNode(page, 'file:src/main.rs');
    await page.waitForTimeout(200);
    await expect(page.locator('#detail-panel')).toHaveClass(/visible/);

    // Close it
    await page.locator('.panel-close').click();
    await page.waitForTimeout(100);

    // Panel hidden
    const panel = page.locator('#detail-panel');
    const classes = await panel.getAttribute('class');
    expect(classes).not.toContain('visible');
  });

  test('symbol list item in file panel focuses the symbol', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    // Open file panel
    await tapNode(page, 'file:src/main.rs');
    await page.waitForTimeout(200);

    // Click the first symbol in the list
    await page.locator('#panel-body .panel-symbol-list li').first().click();
    await page.waitForTimeout(500);

    // Panel should now show symbol details (title changes)
    const title = await page.locator('#panel-title').textContent();
    expect(title === 'main' || title === 'Config').toBeTruthy();
  });
});

test.describe('Visualizer — Collapse / Expand', () => {
  test('double-click collapses file children', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    // Before collapse — children visible
    const beforeHidden = await page.evaluate(() => {
      return (window as any).__cy.getElementById('sym:src/main.rs:main').hasClass('sym-hidden');
    });
    expect(beforeHidden).toBe(false);

    // Double-click to collapse
    await dbltapNode(page, 'file:src/main.rs');
    await page.waitForTimeout(300);

    // After collapse — children hidden
    const afterHidden = await page.evaluate(() => {
      return (window as any).__cy.getElementById('sym:src/main.rs:main').hasClass('sym-hidden');
    });
    expect(afterHidden).toBe(true);

    // File node should have collapsed class
    const isCollapsed = await page.evaluate(() => {
      return (window as any).__cy.getElementById('file:src/main.rs').hasClass('collapsed');
    });
    expect(isCollapsed).toBe(true);
  });

  test('double-click again re-expands file children', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    // Collapse
    await dbltapNode(page, 'file:src/main.rs');
    await page.waitForTimeout(200);

    // Expand
    await dbltapNode(page, 'file:src/main.rs');
    await page.waitForTimeout(200);

    const isHidden = await page.evaluate(() => {
      return (window as any).__cy.getElementById('sym:src/main.rs:main').hasClass('sym-hidden');
    });
    expect(isHidden).toBe(false);

    const isCollapsed = await page.evaluate(() => {
      return (window as any).__cy.getElementById('file:src/main.rs').hasClass('collapsed');
    });
    expect(isCollapsed).toBe(false);
  });
});

test.describe('Visualizer — Tooltip', () => {
  test('hovering a symbol shows tooltip with name, kind, lines', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    await hoverNode(page, 'sym:src/main.rs:main');
    await page.waitForTimeout(100);

    const tooltip = page.locator('#tooltip');
    await expect(tooltip).toBeVisible();
    await expect(tooltip.locator('.tt-name')).toContainText('main');
    await expect(tooltip.locator('.tt-kind')).toContainText('Function');
    await expect(tooltip.locator('.tt-loc')).toContainText('L1');
  });

  test('hovering a file shows tooltip with label and symbol count', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    await hoverNode(page, 'file:src/main.rs');
    await page.waitForTimeout(100);

    const tooltip = page.locator('#tooltip');
    await expect(tooltip).toBeVisible();
    await expect(tooltip.locator('.tt-name')).toContainText('main.rs');
    await expect(tooltip.locator('.tt-kind')).toContainText('Rust');
    await expect(tooltip.locator('.tt-kind')).toContainText('2 symbols');
  });

  test('mouseout hides tooltip', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    await hoverNode(page, 'sym:src/main.rs:main');
    await page.waitForTimeout(100);
    await expect(page.locator('#tooltip')).toBeVisible();

    await mouseoutNode(page, 'sym:src/main.rs:main');
    await page.waitForTimeout(100);

    await expect(page.locator('#tooltip')).not.toBeVisible();
  });
});

test.describe('Visualizer — Activity Log', () => {
  test('activity log toggles on button click', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    const log = page.locator('#activity-log');
    const btn = page.locator('#btn-activity');

    // Initially hidden
    const classes = await log.getAttribute('class');
    expect(classes || '').not.toContain('visible');

    // Click to show
    await btn.click();
    await expect(log).toHaveClass(/visible/);
    await expect(btn).toHaveClass(/active/);

    // Click to hide
    await btn.click();
    await page.waitForTimeout(100);
    const afterClasses = await log.getAttribute('class') || '';
    expect(afterClasses).not.toContain('visible');
  });
});

test.describe('Visualizer — Heatmap', () => {
  test('hot nodes have heat-hot class', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    const isHot = await page.evaluate(() => {
      return (window as any).__cy.getElementById('sym:src/lib.rs:process').hasClass('heat-hot');
    });
    expect(isHot).toBe(true);
  });

  test('warm nodes have heat-warm class', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    const isWarm = await page.evaluate(() => {
      return (window as any).__cy.getElementById('sym:src/main.rs:Config').hasClass('heat-warm');
    });
    expect(isWarm).toBe(true);
  });

  test('cold nodes have no heat class', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    const hasHeat = await page.evaluate(() => {
      const node = (window as any).__cy.getElementById('sym:src/main.rs:main');
      return node.hasClass('heat-hot') || node.hasClass('heat-warm') || node.hasClass('heat-cool');
    });
    expect(hasHeat).toBe(false);
  });
});

test.describe('Visualizer — Controls', () => {
  test('Fit button calls __cy.fit', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    // Zoom in first
    await page.evaluate(() => {
      (window as any).__cy.zoom(4);
    });

    const zoomBefore = await page.evaluate(() => (window as any).__cy.zoom());
    expect(zoomBefore).toBeCloseTo(4, 0);

    // Click Fit
    await page.locator('button', { hasText: 'Fit' }).click();
    await page.waitForTimeout(300);

    const zoomAfter = await page.evaluate(() => (window as any).__cy.zoom());
    // Should be different from 4 (fit to viewport)
    expect(zoomAfter).not.toBeCloseTo(4, 0);
  });

  test('Refresh button reloads graph data', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    // Track fetch calls
    let fetchCount = 0;
    page.on('request', (req) => {
      if (req.url().includes('/api/graph')) fetchCount++;
    });

    await page.locator('button', { hasText: 'Refresh' }).click();
    await page.waitForTimeout(2000);

    expect(fetchCount).toBeGreaterThanOrEqual(1);
  });
});

test.describe('Visualizer — WebSocket Status', () => {
  test('shows connecting/disconnected state', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    // In test mode, WS will fail — should show Disconnected or Error
    const label = page.locator('#ws-label');
    const text = await label.textContent();
    expect(
      text === 'Disconnected' || text === 'Error' || text === 'Connecting...'
    ).toBeTruthy();
  });

  test('WebSocket dot has correct class', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    // Wait for WS to fail
    await page.waitForTimeout(1500);

    const dot = page.locator('#ws-dot');
    // Should not be 'connected' in test mode
    const classes = await dot.getAttribute('class');
    expect(classes).not.toContain('connected');
  });
});

test.describe('Visualizer — Responsive', () => {
  test('header title hides on narrow viewport', async ({ page }) => {
    await page.setViewportSize({ width: 500, height: 800 });
    await page.goto('/');
    await waitForGraph(page);

    // h1 should be hidden by media query
    await expect(page.locator('header h1')).not.toBeVisible();
  });

  test('search box expands on narrow viewport', async ({ page }) => {
    await page.setViewportSize({ width: 500, height: 800 });
    await page.goto('/');

    const searchBox = page.locator('#search-box');
    await expect(searchBox).toBeVisible();
  });
});

test.describe('Visualizer — WebSocket Events', () => {
  test('simulated file_changed event increments counter and highlights', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    // Simulate receiving a WS event via the global handleEvent function
    await page.evaluate(() => {
      (window as any).handleEvent({
        type: 'file_changed',
        path: 'src/main.rs',
        kind: 'modified',
      });
    });

    await page.waitForTimeout(200);

    // Event counter incremented
    await expect(page.locator('#stat-events')).toHaveText('1');

    // File node should have pulse class briefly
    const hasPulse = await page.evaluate(() => {
      return (window as any).__cy.getElementById('file:src/main.rs').hasClass('pulse');
    });
    expect(hasPulse).toBe(true);
  });

  test('activity log shows entry after event', async ({ page }) => {
    await page.goto('/');
    await waitForGraph(page);

    // Open log
    await page.locator('#btn-activity').click();

    // Simulate event
    await page.evaluate(() => {
      (window as any).handleEvent({
        type: 'file_changed',
        path: 'src/lib.rs',
        kind: 'created',
      });
    });

    await page.waitForTimeout(200);

    const log = page.locator('#activity-log');
    await expect(log.locator('.log-entry')).toHaveCount(1);
    await expect(log.locator('.log-kind')).toContainText('created');
    await expect(log.locator('.log-path')).toContainText('lib.rs');
  });
});

test.describe('Visualizer — Error Handling', () => {
  test('shows error status when API fails', async ({ page }) => {
    // Intercept the API to return an error
    await page.route('**/api/graph', (route) => {
      route.fulfill({ status: 500, body: 'Internal Server Error' });
    });

    await page.goto('/');
    await page.waitForTimeout(2000);

    const status = page.locator('#graph-status');
    await expect(status).toBeVisible();
    await expect(status).toHaveClass(/error/);
    await expect(status).toContainText('API error');
  });
});
