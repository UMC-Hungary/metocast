import storybook from 'eslint-plugin-storybook';
import tseslint from 'typescript-eslint';
import svelte from 'eslint-plugin-svelte';
import svelteParser from 'svelte-eslint-parser';
import globals from 'globals';
import prettier from 'eslint-config-prettier';

export default tseslint.config(
  {
    ignores: [
      '**/build/',
      '**/.svelte-kit/',
      '**/storybook-static/',
      '**/node_modules/',
      'src-tauri/',
      'companion/dist/',
    ],
  },

  // TypeScript files
  {
    files: ['**/*.ts'],
    extends: tseslint.configs.recommended,
    languageOptions: {
      globals: { ...globals.browser, ...globals.node },
    },
    rules: {
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/consistent-type-imports': ['error', { prefer: 'type-imports' }],
      '@typescript-eslint/no-import-type-side-effects': 'error',
    },
  },

  // Svelte files — recommended rules
  ...svelte.configs['flat/recommended'],
  // Disable rules that incorrectly flag plain <a href> and goto() navigation
  { rules: { 'svelte/no-navigation-without-resolve': 'off' } },

  // Svelte files (incl. Svelte 5 rune modules `.svelte.ts`/`.svelte.js`) — TS parser
  {
    files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
    languageOptions: {
      parser: svelteParser,
      parserOptions: { parser: tseslint.parser },
      globals: { ...globals.browser },
    },
    plugins: { '@typescript-eslint': tseslint.plugin },
    rules: {
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/no-explicit-any': 'error',
    },
  },

  // ── Core client boundary ────────────────────────────────────────────────────
  // A rendering UI reaches the core only through @metocast/core-client. Tauri IPC,
  // raw fetch and raw WebSocket live inside that package so a UI stays portable
  // across desktop, browser and remote-client hosting. The rule targets every UI
  // (src today, ui/* after relocation); packages/core-client is exempt by not
  // being matched here, so a new UI is covered automatically.
  {
    files: ['src/**/*.{ts,svelte}', 'ui/**/*.{ts,svelte}'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['@tauri-apps/*', 'tauri-plugin-*'],
              message:
                'UI code must not use Tauri directly. Use @metocast/core-client (host capabilities are feature-detected there).',
            },
            {
              group: [
                '@metocast/core-client/api/*',
                '@metocast/core-client/ws/*',
                '@metocast/core-client/host/*',
              ],
              message:
                'Import from @metocast/core-client instead of reaching into its internal layers.',
            },
          ],
        },
      ],
      'no-restricted-globals': [
        'error',
        {
          name: 'fetch',
          message: 'Use @metocast/core-client instead of raw fetch.',
        },
        {
          name: 'WebSocket',
          message: 'Use connectWs/connectPresenterWs from @metocast/core-client instead.',
        },
      ],
      'no-restricted-syntax': [
        'error',
        {
          selector: "NewExpression[callee.name='WebSocket']",
          message: 'Use connectWs/connectPresenterWs from @metocast/core-client instead.',
        },
        {
          selector: "CallExpression[callee.name='fetch']",
          message: 'Use @metocast/core-client instead of raw fetch.',
        },
      ],
    },
  },

  // Prettier last — disables formatting rules that conflict
  prettier,

  ...storybook.configs['flat/recommended'],
);
