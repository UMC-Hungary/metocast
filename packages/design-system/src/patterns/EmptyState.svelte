<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** The serif italic line the reference uses — "The booth is quiet." */
    title: string;
    /** The mono micro-label under it — "Connectors will speak up here". */
    hint?: string;
    action?: Snippet;
  }

  let { title, hint = '', action }: Props = $props();
</script>

<div class="empty">
  <p>{title}</p>
  {#if hint}<span>{hint}</span>{/if}
  {#if action}<div class="action">{@render action()}</div>{/if}
</div>

<style>
  .empty {
    padding: var(--c-empty-state-padding-top) var(--ui-gutter) var(--c-empty-state-padding-bottom);
    text-align: center;
  }

  p {
    margin: 0;
    font-family: var(--type-quote-family);
    font-style: italic;
    font-size: var(--c-empty-state-title-size);
    color: var(--text-muted);
  }

  span {
    display: block;
    font-family: var(--type-label-xs-family);
    font-size: var(--c-empty-state-hint-size);
    letter-spacing: var(--c-empty-state-hint-track);
    text-transform: var(--type-label-xs-transform);
    color: var(--text-faint);
    margin-top: var(--c-empty-state-hint-gap);
  }

  .action {
    display: flex;
    justify-content: center;
    margin-top: var(--c-empty-state-hint-gap);
  }
</style>
