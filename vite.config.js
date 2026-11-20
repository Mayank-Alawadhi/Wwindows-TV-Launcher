import { defineConfig } from 'vite';
import { cpSync } from 'node:fs';
import { resolve } from 'node:path';

export default defineConfig({
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1420, strictPort: true },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: {
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    copyPublicDir: false
  },
  plugins: [{
    name: 'copy-launcher-sites',
    closeBundle() {
      const out = resolve('dist');
      cpSync(resolve('sites.json'), resolve(out, 'sites.json'));
      cpSync(resolve('icons'), resolve(out, 'icons'), { recursive: true });
      cpSync(resolve('assets'), resolve(out, 'assets'), { recursive: true });
    }
  }]
});
