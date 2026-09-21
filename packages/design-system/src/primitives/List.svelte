<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** Nudges the list down when it follows another list rather than a section label. */
    flush?: boolean;
    /**
     * Renders the rows as list items when they are a set of like things. Rows that are each a
     * navigation target should stay unlisted, as they are in the reference, and be wrapped in
     * a nav by the screen.
     */
    as?: 'div' | 'ul';
    'aria-label'?: string;
    'aria-labelledby'?: string;
    children: Snippet;
  }

  let {
    flush = false,
    as = 'div',
    'aria-label': ariaLabel,
    'aria-labelledby': ariaLabelledby,
    children,
  }: Props = $props();
</script>

{#if as === 'ul'}
  <ul class:flush aria-label={ariaLabel} aria-labelledby={ariaLabelledby}>{@render children()}</ul>
{:else}
  <div class:flush aria-label={ariaLabel} aria-labelledby={ariaLabelledby}>
    {@render children()}
  </div>
{/if}

<style>
  div,
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    background: var(--surface-raised);
    border-top: var(--ui-border-hairline) solid var(--border-hairline);
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
  }

  .flush {
    margin-top: var(--c-list-flush-gap);
  }
</style>
