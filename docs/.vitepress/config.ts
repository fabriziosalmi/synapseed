import { defineConfig } from 'vitepress'
import { withMermaid } from 'vitepress-plugin-mermaid'

export default withMermaid(defineConfig({
  title: 'SYNAPSEED',
  description: 'High-Performance Semantic AI Middleware — The Thinking Layer Between You and the LLM',
  lang: 'en-US',
  base: '/synapseed/',

  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/logo.svg' }],
    ['meta', { name: 'theme-color', content: '#7ee787' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:title', content: 'SYNAPSEED' }],
    ['meta', { property: 'og:description', content: 'High-Performance Semantic AI Middleware' }],
  ],

  themeConfig: {
    logo: '/logo.svg',
    siteTitle: 'SYNAPSEED',

    nav: [
      { text: 'Guide', link: '/guide/introduction' },
      { text: 'Architecture', link: '/architecture/overview' },
      { text: 'Features', link: '/features/cortex' },
      { text: 'Reference', link: '/reference/cli' },
      {
        text: 'v3.9.0',
        items: [
          { text: 'Changelog', link: 'https://github.com/fabriziosalmi/synapseed/releases' },
          { text: 'Contributing', link: 'https://github.com/fabriziosalmi/synapseed' },
        ],
      },
    ],

    sidebar: {
      '/guide/': [
        {
          text: 'Getting Started',
          items: [
            { text: 'Introduction', link: '/guide/introduction' },
            { text: 'Installation', link: '/guide/installation' },
            { text: 'Quick Start', link: '/guide/quickstart' },
            { text: 'Configuration', link: '/guide/configuration' },
          ],
        },
      ],
      '/architecture/': [
        {
          text: 'Architecture',
          items: [
            { text: 'Overview', link: '/architecture/overview' },
            { text: 'Plugin System', link: '/architecture/plugin-system' },
            { text: 'Event Bus', link: '/architecture/event-bus' },
            { text: 'Crate Map', link: '/architecture/crates' },
          ],
        },
      ],
      '/features/': [
        {
          text: 'Core Subsystems',
          items: [
            { text: 'Cortex — AST Engine', link: '/features/cortex' },
            { text: 'Husk — DLP Shield', link: '/features/husk' },
            { text: 'Root — Command Sentinel', link: '/features/root' },
            { text: 'Chronos — Git Time-Travel', link: '/features/chronos' },
          ],
        },
        {
          text: 'Advanced',
          items: [
            { text: 'Search — Semantic Index', link: '/features/search' },
            { text: 'Shadow — Live Compiler', link: '/features/shadow-check' },
            { text: 'Visualizer — Live Dashboard', link: '/features/visualizer' },
            { text: 'Whisper — Intent Router', link: '/features/whisper' },
            { text: 'Telemetry — OTLP Receiver', link: '/features/telemetry' },
          ],
        },
      ],
      '/reference/': [
        {
          text: 'CLI',
          items: [
            { text: 'Commands', link: '/reference/cli' },
          ],
        },
        {
          text: 'MCP Protocol',
          items: [
            { text: 'Tools', link: '/reference/mcp-tools' },
            { text: 'Resources', link: '/reference/mcp-resources' },
            { text: 'Prompts', link: '/reference/mcp-prompts' },
          ],
        },
      ],
      '/integration/': [
        {
          text: 'Integration',
          items: [
            { text: 'Claude Desktop', link: '/integration/claude-desktop' },
            { text: 'Claude Code', link: '/integration/claude-code' },
            { text: 'VS Code / Cursor', link: '/integration/vscode' },
            { text: 'Self-Telemetry', link: '/integration/self-telemetry' },
          ],
        },
      ],
      '/security/': [
        {
          text: 'Security',
          items: [
            { text: 'Security Model', link: '/security/model' },
            { text: 'DLP Reference', link: '/security/dlp' },
          ],
        },
      ],
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/fabriziosalmi/synapseed' },
    ],

    search: {
      provider: 'local',
    },

    footer: {
      message: 'Released under the Apache License 2.0.',
      copyright: 'Copyright 2024-present Fabrizio Salmi',
    },

    editLink: {
      pattern: 'https://github.com/fabriziosalmi/synapseed/edit/main/docs/:path',
      text: 'Edit this page on GitHub',
    },
  },
}))
