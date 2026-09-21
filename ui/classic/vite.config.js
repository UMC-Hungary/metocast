import { sveltekit } from '@sveltejs/kit/vite';
import { existsSync } from 'fs';
import { resolve } from 'path';

const liquidGlassInstalled = existsSync(resolve('node_modules/tauri-plugin-liquid-glass-api'));

export default {
  plugins: [sveltekit()],
  server: {
    port: 1420,
    strictPort: true,
    host: process.env.TAURI_DEV_HOST || false,
  },
  resolve: {
    alias: liquidGlassInstalled
      ? {}
      : { 'tauri-plugin-liquid-glass-api': resolve('./src/lib/stubs/liquid-glass-stub.ts') },
  },
  ssr: {
    noExternal: [
      '@tauri-apps/api',
      'svelte-sonner',
      ...(liquidGlassInstalled ? ['tauri-plugin-liquid-glass-api'] : []),
    ],
  },
};
