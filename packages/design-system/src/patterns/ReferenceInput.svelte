<script lang="ts" module>
  export interface Verse {
    n: string | number;
    text: string;
  }

  export interface ReferenceResult {
    invalid?: boolean;
    verses?: Verse[];
    translation?: string;
  }
</script>

<script lang="ts">
  import Dot from '../primitives/Dot.svelte';

  interface Props {
    label: string;
    /** The position of this reference among the others — "primary", "second reading". */
    rank: string;
    value?: string;
    placeholder?: string;
    /** The lookup's outcome. `null` while nothing has been looked up yet. */
    result?: ReferenceResult | null;
    /** Announced when a lookup fails; the reference shows this only as prose. */
    errorMessage: string;
    /** "{n} verses · {translation}", already interpolated and translated by the caller. */
    statusMessage?: string;
    notFoundLabel: string;
    id?: string;
  }

  let {
    label,
    rank,
    value = $bindable(''),
    placeholder = '',
    result = null,
    errorMessage,
    statusMessage = '',
    notFoundLabel,
    id = `sanctum-reference-input-${crypto.randomUUID()}`,
  }: Props = $props();

  const verses = $derived(result && !result.invalid ? (result.verses ?? []) : []);
  const invalid = $derived(Boolean(result?.invalid));
  const statusId = $derived(`${id}-status`);
</script>

<section class:invalid>
  <header>
    <p>
      <label for={id}>{label}</label>
      <em>{rank}</em>
    </p>
    {#if invalid}
      <strong class="bad"><Dot color="var(--status-error)" size={4} />{notFoundLabel}</strong>
    {:else if verses.length > 0 && statusMessage}
      <strong><Dot color="var(--status-ok)" size={4} />{statusMessage}</strong>
    {/if}
  </header>
  <input
    {id}
    bind:value
    {placeholder}
    aria-invalid={invalid ? 'true' : undefined}
    aria-describedby={invalid || verses.length > 0 ? statusId : undefined}
  />
  <div id={statusId} role="status">
    {#if invalid}
      <p class="error">{errorMessage}</p>
    {:else if verses.length > 0}
      <div class="verses">
        {#each verses as verse (verse.n)}
          <p><code>{verse.n}</code>{verse.text}</p>
        {/each}
      </div>
    {/if}
  </div>
</section>

<style>
  section {
    background: var(--surface-sunken);
    border: var(--ui-border-hairline) solid var(--border-control);
    padding: var(--c-reference-input-padding-top) var(--ui-gutter-inset)
      var(--c-reference-input-padding-bottom);
  }

  section:focus-within {
    border-color: var(--accent);
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
  }

  .invalid {
    border-color: var(--status-error);
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--ui-stack);
    margin-bottom: var(--c-reference-input-header-gap);
  }

  header p {
    display: flex;
    gap: var(--ui-stack);
    margin: 0;
  }

  label,
  em,
  strong {
    font-family: var(--type-label-xs-family);
    font-size: var(--type-label-xs-size);
    letter-spacing: var(--c-reference-input-label-track);
    text-transform: var(--type-label-xs-transform);
    color: var(--text-muted);
    font-style: normal;
  }

  strong {
    display: inline-flex;
    align-items: center;
    gap: var(--c-reference-input-status-gap);
    color: var(--status-ok);
  }

  .bad {
    color: var(--status-error);
  }

  input {
    width: 100%;
    border: 0;
    background: transparent;
    font-family: var(--type-body-strong-family);
    font-size: var(--type-body-strong-size);
    color: var(--text-primary);
    padding: 0;
    font-weight: var(--type-body-strong-weight);
    caret-color: var(--text-primary);
  }

  input:focus {
    outline: 0;
  }

  .error {
    margin-top: var(--c-reference-input-block-gap);
    padding: var(--c-reference-input-quote-padding-block)
      var(--c-reference-input-quote-padding-inline);
    border-left: var(--ui-border-emphasis) solid var(--status-error);
    color: var(--text-secondary);
    font-family: var(--type-quote-family);
    font-style: italic;
    font-size: var(--type-quote-size);
  }

  .verses {
    margin-top: var(--c-reference-input-block-gap);
    border-left: var(--ui-border-emphasis) solid var(--surface-inverse);
  }

  .verses p {
    display: flex;
    gap: var(--c-reference-input-block-gap);
    margin: 0;
    padding: var(--c-reference-input-quote-padding-block)
      var(--c-reference-input-quote-padding-inline);
    border-top: var(--ui-border-hairline) solid var(--border-hairline);
    font-family: var(--type-quote-family);
    font-size: var(--c-reference-input-verse-size);
    font-style: italic;
    color: var(--text-secondary);
    line-height: var(--type-quote-leading);
  }

  .verses p:first-child {
    border-top: 0;
  }

  code {
    min-width: var(--c-reference-input-verse-number-width);
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    color: var(--text-muted);
    font-style: normal;
  }
</style>
