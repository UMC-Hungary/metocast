import type { StorybookConfig } from '@storybook/svelte-vite';

const config: StorybookConfig = {
  stories: ['../stories/*.stories.ts'],
  addons: [
    '@storybook/addon-docs',
    '@storybook/addon-a11y',
    '@storybook/addon-vitest',
    '@storybook/addon-mcp',
  ],
  framework: { name: '@storybook/svelte-vite', options: {} },
  // The woff2 subsets are served from here, so no story ever reaches the network for a font.
  staticDirs: ['../static'],
};

export default config;
