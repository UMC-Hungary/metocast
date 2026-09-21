<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    value?: string;
    placeholder?: string;
    type?: 'text' | 'email' | 'url' | 'tel' | 'search' | 'password' | 'number';
    /** A quiet note under the input, associated through `aria-describedby`. */
    hint?: string;
    /** Replaces the hint and marks the field invalid. */
    error?: string;
    readonly?: boolean;
    disabled?: boolean;
    required?: boolean;
    id?: string;
    trailing?: Snippet;
  }

  let {
    label,
    value = $bindable(''),
    placeholder = '',
    type = 'text',
    hint = '',
    error = '',
    readonly = false,
    disabled = false,
    required = false,
    id = `sanctum-field-${crypto.randomUUID()}`,
    trailing,
  }: Props = $props();

  const noteId = $derived(`${id}-note`);
  const note = $derived(error || hint);
</script>

<div class="field" class:invalid={Boolean(error)}>
  <label for={id}>
    {label}
    {#if trailing}{@render trailing()}{/if}
  </label>
  <input
    {id}
    {type}
    {placeholder}
    {readonly}
    {disabled}
    {required}
    bind:value
    aria-invalid={error ? 'true' : undefined}
    aria-describedby={note ? noteId : undefined}
  />
  {#if note}
    <small id={noteId} class:error={Boolean(error)}>{note}</small>
  {/if}
</div>

<style>
  .field {
    display: block;
    padding: var(--c-field-padding-block) 0;
  }

  label {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--c-field-label-track);
    text-transform: var(--type-label-sm-transform);
    color: var(--text-muted);
    font-weight: var(--type-label-sm-weight);
  }

  input {
    width: 100%;
    min-height: var(--ui-target-min);
    padding: var(--c-text-field-padding-block) var(--ui-gutter-inset);
    margin-top: var(--c-field-input-gap);
    background: var(--surface-sunken);
    border: var(--ui-border-hairline) solid var(--border-control);
    border-radius: var(--ui-radius-square);
    font-family: var(--type-body-family);
    font-size: var(--type-body-size);
    color: var(--text-primary);
    letter-spacing: var(--type-body-track);
    caret-color: var(--text-primary);
  }

  input::placeholder {
    color: var(--text-faint);
  }

  input:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
    border-color: var(--accent);
  }

  .invalid input {
    border-color: var(--status-error);
  }

  input:disabled {
    color: var(--text-muted);
    cursor: not-allowed;
  }

  small {
    display: block;
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    color: var(--text-faint);
    letter-spacing: var(--c-field-hint-track);
    margin-top: var(--c-field-hint-gap);
  }

  small.error {
    color: var(--status-error);
  }
</style>
