// Tauri has no Node server, so we ship a static SPA (adapter-static, SPA fallback).
import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    appDir: '_sanctum',
    adapter: adapter({
      fallback: 'index.html',
    }),
  },
};

export default config;
