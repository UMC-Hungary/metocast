<script lang="ts">
  import type { Snippet } from 'svelte';
  import Spinner from '../primitives/Spinner.svelte';

  interface Props {
    /** The mono heading — "Broadlink devices". */
    title: string;
    /** The prose line under it, set in the reference's serif italic. */
    description?: string;
    scanLabel: string;
    scanning?: boolean;
    /** Announced while a scan runs, and shown beside the spinner. */
    scanningLabel?: string;
    onscan?: () => void;
    children?: Snippet;
  }

  let {
    title,
    description = '',
    scanLabel,
    scanning = false,
    scanningLabel = 'Scanning',
    onscan,
    children,
  }: Props = $props();
</script>

<section class="panel">
  <header>
    <div>
      <strong>{title}</strong>
      {#if description}<span>{description}</span>{/if}
    </div>
    <button type="button" disabled={scanning} onclick={onscan}>{scanLabel}</button>
  </header>
  <div class="body">
    {#if scanning}
      <p class="status"><Spinner size={14} label={scanningLabel} />{scanningLabel}</p>
    {:else}
      {@render children?.()}
    {/if}
  </div>
</section>

<style>
  .panel {
    padding: var(--ui-gutter-inset);
    border: var(--ui-border-hairline) solid var(--border-hairline);
    background: var(--surface-raised);
  }

  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--ui-gutter-tight);
    margin-bottom: var(--c-discovery-header-gap);
  }

  strong {
    display: block;
    font-family: var(--type-label-sm-family);
    font-size: var(--c-discovery-title-size);
    letter-spacing: var(--c-discovery-title-track);
    text-transform: var(--type-label-sm-transform);
    color: var(--text-muted);
  }

  header span {
    display: block;
    margin-top: var(--c-row-meta-gap);
    font-family: var(--type-quote-family);
    font-size: var(--type-quote-size);
    font-style: italic;
    line-height: var(--type-body-sm-leading);
    color: var(--text-muted);
  }

  header button {
    flex-shrink: 0;
    min-height: var(--ui-target-min);
    padding: 0 var(--c-discovery-button-padding);
    background: var(--surface-inverse);
    color: var(--text-inverse);
    border: 0;
    cursor: pointer;
    font-family: var(--type-label-sm-family);
    font-size: var(--c-discovery-title-size);
    letter-spacing: var(--c-discovery-button-track);
    text-transform: var(--type-label-sm-transform);
  }

  header button:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  header button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
  }

  .body {
    border-top: var(--ui-border-hairline) solid var(--border-hairline);
    padding-top: var(--c-discovery-body-padding);
  }

  .status {
    display: flex;
    align-items: center;
    gap: var(--ui-stack);
    margin: 0;
    padding: var(--c-discovery-status-padding) 0;
    color: var(--text-secondary);
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    letter-spacing: var(--c-field-hint-track);
    text-transform: var(--type-label-transform);
  }

  /* The reference stacks the header and the action below this width. */
  @media (max-width: 420px) {
    header {
      display: grid;
    }

    header button {
      width: 100%;
    }
  }
</style>
