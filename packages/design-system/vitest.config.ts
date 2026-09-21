import { storybookTest } from '@storybook/addon-vitest/vitest-plugin';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { playwright } from '@vitest/browser-playwright';
import { defineConfig } from 'vitest/config';

/**
 * Runs every story in a real browser and asserts the accessibility checks the a11y addon
 * configures. `a11y: { test: 'error' }` in .storybook/preview.ts is what makes a violation
 * fail rather than merely report, so this is the gate the change asks for.
 */
export default defineConfig({
  test: {
    projects: [
      {
        // Storybook's own renderer ships `.svelte` sources, so the plugin has to reach into
        // node_modules as well as the package's own files.
        plugins: [svelte({ include: ['**/*.svelte'] }), storybookTest({ configDir: '.storybook' })],
        optimizeDeps: { exclude: ['@storybook/svelte', '@storybook/svelte-vite'] },
        test: {
          name: 'storybook',
          setupFiles: ['./.storybook/vitest.setup.ts'],
          browser: {
            enabled: true,
            headless: true,
            provider: playwright(),
            instances: [{ browser: 'chromium' }],
          },
        },
      },
    ],
  },
});
