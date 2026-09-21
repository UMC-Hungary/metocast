<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** The step number in its chip. Numbering is the caller's, not the component's. */
    number: string | number;
    label: string;
    /** A quiet note on the right of the header — "optional", "2 of 5". */
    hint?: string;
    /** Drops the bottom rule, for the last section in a form. */
    last?: boolean;
    children: Snippet;
  }

  let { number, label, hint = '', last = false, children }: Props = $props();

  const id = `sanctum-form-section-${crypto.randomUUID()}`;
</script>

<section class:last aria-labelledby={id}>
  <header>
    <div>
      <span class="number" aria-hidden="true">{number}</span>
      <h3 {id}>{label}</h3>
    </div>
    {#if hint}<p class="hint">{hint}</p>{/if}
  </header>
  <div class="body">{@render children()}</div>
</section>

<style>
  section {
    padding: var(--c-form-section-padding-block) 0;
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
  }

  section.last {
    border-bottom: 0;
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--c-form-section-header-gap);
    margin-bottom: var(--c-form-section-header-margin);
  }

  header > div {
    display: flex;
    align-items: baseline;
    gap: var(--c-form-section-header-gap);
  }

  .number {
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    color: var(--text-muted);
    letter-spacing: var(--c-form-section-number-track);
    font-variant-numeric: tabular-nums;
    font-weight: var(--type-label-sm-weight);
    padding: var(--c-form-section-number-padding-block) var(--c-form-section-number-padding-inline);
    border: var(--ui-border-hairline) solid var(--border-strong);
    border-radius: var(--ui-radius-chip);
  }

  h3 {
    margin: 0;
    font-family: var(--type-heading-family);
    font-size: var(--type-heading-size);
    font-style: italic;
    color: var(--text-primary);
    letter-spacing: var(--type-heading-track);
    font-weight: var(--type-heading-weight);
  }

  .hint {
    margin: 0;
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    color: var(--text-faint);
    letter-spacing: var(--c-form-section-hint-track);
    text-transform: var(--type-label-sm-transform);
  }

  .body {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--ui-stack);
  }
</style>
