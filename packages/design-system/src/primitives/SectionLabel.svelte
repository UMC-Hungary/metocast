<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** A trailing note, rendered one tracking step tighter and one tone lighter. */
    hint?: string;
    /**
     * Renders as a heading at this level when the label introduces a region. Left off, it is a
     * plain label — the reference's own markup, which is right when the list below is not a
     * section of its own.
     */
    headingLevel?: 2 | 3 | 4;
    id?: string;
    children: Snippet;
  }

  let { hint = '', headingLevel, id, children }: Props = $props();
</script>

<div class="label">
  {#if headingLevel === 2}
    <h2 {id}>{@render children()}</h2>
  {:else if headingLevel === 3}
    <h3 {id}>{@render children()}</h3>
  {:else if headingLevel === 4}
    <h4 {id}>{@render children()}</h4>
  {:else}
    <div {id}>{@render children()}</div>
  {/if}
  {#if hint}<span>{hint}</span>{/if}
</div>

<style>
  .label {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    padding: var(--c-section-label-padding-top) var(--ui-gutter)
      var(--c-section-label-padding-bottom);
  }

  div,
  h2,
  h3,
  h4,
  span {
    margin: 0;
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    letter-spacing: var(--type-label-track);
    text-transform: var(--type-label-transform);
    color: var(--text-muted);
    font-weight: var(--type-label-weight);
  }

  span {
    color: var(--text-faint);
    letter-spacing: var(--c-section-label-hint-track);
  }
</style>
