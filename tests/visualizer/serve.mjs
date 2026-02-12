// Minimal HTTP server that serves the visualizer assets + mock /api/graph.
// Used by Playwright webServer config.

import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ASSETS = join(__dirname, '..', '..', 'crates', 'visualizer', 'assets');
const PORT = 4400;

// Mock graph payload — 2 files, 4 symbols (enough to test all interactions)
const MOCK_GRAPH = JSON.stringify({
  elements: {
    nodes: [
      { data: { id: 'file:src/main.rs', label: 'main.rs', type: 'file', language: 'Rust', fullPath: 'src/main.rs', heatLevel: 'none', heatMs: 0 } },
      { data: { id: 'sym:src/main.rs:main', label: 'fn main()', type: 'function', parent: 'file:src/main.rs', name: 'main', kind: 'Function', lineStart: 1, lineEnd: 20, heatLevel: 'none', heatMs: 0 } },
      { data: { id: 'sym:src/main.rs:Config', label: 'struct Config', type: 'struct', parent: 'file:src/main.rs', name: 'Config', kind: 'Struct', lineStart: 22, lineEnd: 30, heatLevel: 'warm', heatMs: 75 } },
      { data: { id: 'file:src/lib.rs', label: 'lib.rs', type: 'file', language: 'Rust', fullPath: 'src/lib.rs', heatLevel: 'hot', heatMs: 250 } },
      { data: { id: 'sym:src/lib.rs:process', label: 'fn process()', type: 'function', parent: 'file:src/lib.rs', name: 'process', kind: 'Function', lineStart: 1, lineEnd: 50, heatLevel: 'hot', heatMs: 250 } },
      { data: { id: 'sym:src/lib.rs:Status', label: 'enum Status', type: 'enum', parent: 'file:src/lib.rs', name: 'Status', kind: 'Enum', lineStart: 52, lineEnd: 58, heatLevel: 'none', heatMs: 0 } },
    ],
  },
  stats: { files: 2, symbols: 4 },
});

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
};

const server = createServer((req, res) => {
  const url = new URL(req.url, `http://localhost:${PORT}`);

  // Mock API
  if (url.pathname === '/api/graph') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(MOCK_GRAPH);
    return;
  }

  // Mock WebSocket upgrade — just reject (Playwright tests mock WS via page)
  if (url.pathname === '/ws') {
    res.writeHead(400);
    res.end('WebSocket not available in test mode');
    return;
  }

  // Static assets
  let filePath;
  if (url.pathname === '/' || url.pathname === '/index.html') {
    filePath = join(ASSETS, 'index.html');
  } else if (url.pathname === '/graph.js') {
    filePath = join(ASSETS, 'graph.js');
  } else {
    res.writeHead(404);
    res.end('Not found');
    return;
  }

  try {
    const data = readFileSync(filePath);
    const ext = filePath.slice(filePath.lastIndexOf('.'));
    res.writeHead(200, { 'Content-Type': MIME[ext] || 'application/octet-stream' });
    res.end(data);
  } catch {
    res.writeHead(404);
    res.end('Not found');
  }
});

server.listen(PORT, () => {
  console.log(`Visualizer test server on http://localhost:${PORT}`);
});
