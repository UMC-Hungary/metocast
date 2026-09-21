import { createRawSnippet } from 'svelte';

/**
 * Story content for a snippet prop. Storybook args are plain values, and a Svelte 5 snippet is
 * not — `createRawSnippet` is the bridge, so a story can pass slot content without a wrapper
 * component per story.
 */
export const s = (text: string) =>
  createRawSnippet(() => ({
    render: () => (text.startsWith('<') ? text : `<span>${text}</span>`),
  }));
