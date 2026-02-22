import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  server: {
    port: 3004,
    proxy: {
      '/api': {
        target: 'http://localhost:3003',
        changeOrigin: true,
        timeout: 120000,
        proxyTimeout: 120000,
      },
    },
    headers: {
      // Required for SharedArrayBuffer (OPFS persistence in DuckDB-WASM)
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    },
  },
  optimizeDeps: {
    exclude: ['@duckdb/duckdb-wasm'],
  },
});
