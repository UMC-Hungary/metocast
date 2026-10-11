import { sveltekit } from '@sveltejs/kit/vite';

export default {
  plugins: [sveltekit()],
  server: {
    port: 1420,
    strictPort: true,
    host: process.env.TAURI_DEV_HOST || false,
  },
  ssr: {
    noExternal: ['@tauri-apps/api', 'svelte-sonner'],
  },
};
