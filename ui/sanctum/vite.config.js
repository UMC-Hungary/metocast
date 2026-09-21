import { sveltekit } from '@sveltejs/kit/vite';
import { searchForWorkspaceRoot } from 'vite';

export default {
  plugins: [sveltekit()],
  server: {
    // Matches tauri.conf devUrl (1420); tauri dev serves one UI at a time.
    port: 1420,
    strictPort: true,
    host: process.env.TAURI_DEV_HOST || false,
    // @metocast/design-system is a pnpm symlink; its fonts.css url()s resolve to the
    // package's realpath in packages/, which /@fs 403s unless the workspace root is allowed.
    fs: { allow: [searchForWorkspaceRoot(process.cwd())] },
  },
};
