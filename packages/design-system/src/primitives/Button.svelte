<script lang="ts">
  import type { Snippet } from 'svelte';
  import Spinner from './Spinner.svelte';

  interface Props {
    /**
     * `primary` is the ink fill the reference uses for the committing action; `secondary` is its
     * outlined counterpart; `quiet` is the mono text button it uses inside cards and toasts.
     */
    variant?: 'primary' | 'secondary' | 'quiet' | 'danger';
    type?: 'button' | 'submit' | 'reset';
    /** Fills the available width, as the reference's primary action does. */
    block?: boolean;
    /** The reference's small inline control (mono, uppercase) used inside rows — its `.mini`. */
    compact?: boolean;
    disabled?: boolean;
    /** Keeps the button's width while it works, and marks it busy. */
    loading?: boolean;
    loadingLabel?: string;
    href?: string;
    onclick?: (event: MouseEvent) => void;
    children: Snippet;
  }

  let {
    variant = 'secondary',
    type = 'button',
    block = false,
    compact = false,
    disabled = false,
    loading = false,
    loadingLabel = 'Working',
    href,
    onclick,
    children,
  }: Props = $props();
</script>

{#snippet content()}
  {#if loading}
    <span class="spinner"><Spinner label={loadingLabel} size={14} /></span>
  {/if}
  <span class:working={loading}>{@render children()}</span>
{/snippet}

{#if href}
  <a class="btn {variant}" class:block class:compact {href}>{@render content()}</a>
{:else}
  <button
    class="btn {variant}"
    class:block
    class:compact
    {type}
    disabled={disabled || loading}
    aria-busy={loading || undefined}
    {onclick}
  >
    {@render content()}
  </button>
{/if}

<style>
  .btn {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: var(--c-button-min-height);
    padding: 0 var(--c-button-padding-inline);
    cursor: pointer;
    font-family: var(--type-body-sm-family);
    font-weight: var(--type-body-sm-weight);
    letter-spacing: var(--c-button-track);
    white-space: nowrap;
    text-decoration: none;
    border-radius: var(--ui-radius-square);
    transition: background var(--duration-120);
  }

  .block {
    width: 100%;
  }

  /* The reference's `.mini`: a compact mono, uppercase inline control. */
  .btn.compact {
    min-height: 0;
    padding: var(--space-7, 7px) var(--space-10, 10px);
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    letter-spacing: var(--c-button-quiet-track);
    text-transform: var(--type-label-transform);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
  }

  .primary {
    background: var(--surface-inverse);
    color: var(--text-inverse);
    border: 0;
  }

  .secondary {
    background: transparent;
    color: var(--text-primary);
    border: var(--ui-border-hairline) solid var(--border-control);
  }

  .secondary:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .danger {
    background: transparent;
    color: var(--status-error);
    border: var(--ui-border-hairline) solid var(--status-error);
  }

  .danger:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .quiet {
    background: transparent;
    color: var(--text-primary);
    border: 0;
    min-height: var(--ui-target-min);
    padding: 0;
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    letter-spacing: var(--c-button-quiet-track);
    text-transform: var(--type-label-transform);
  }

  .quiet:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  .working {
    visibility: hidden;
  }

  .spinner {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }
</style>
