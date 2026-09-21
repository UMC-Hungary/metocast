<script lang="ts" module>
  export interface FormFieldContext {
    controlId: string;
    describedby: string | undefined;
    invalid: boolean;
  }
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    /** A note under the control, associated through `aria-describedby`. */
    hint?: string;
    /** Replaces the hint, marks the control invalid, and is announced when it appears. */
    error?: string;
    required?: boolean;
    id?: string;
    /** Receives the ids and invalid flag the control must apply to itself. */
    children: Snippet<[FormFieldContext]>;
  }

  let {
    label,
    hint = '',
    error = '',
    required = false,
    id = `sanctum-form-field-${crypto.randomUUID()}`,
    children,
  }: Props = $props();

  const hintId = $derived(`${id}-hint`);
  const errorId = $derived(`${id}-error`);
  const describedby = $derived(
    [hint ? hintId : null, error ? errorId : null].filter(Boolean).join(' ') || undefined,
  );
</script>

<div class="form-field">
  <label for={id}>
    {label}
    {#if required}<span aria-hidden="true">*</span>{/if}
  </label>
  {@render children({ controlId: id, describedby, invalid: Boolean(error) })}
  {#if hint}<p id={hintId} class="hint">{hint}</p>{/if}
  {#if error}<p id={errorId} class="error" role="alert">{error}</p>{/if}
</div>

<style>
  .form-field {
    display: flex;
    flex-direction: column;
    gap: var(--c-form-field-gap);
  }

  label {
    font-family: var(--type-label-xs-family);
    font-size: var(--type-label-xs-size);
    letter-spacing: var(--type-label-xs-track);
    text-transform: var(--type-label-xs-transform);
    font-weight: var(--type-label-xs-weight);
    color: var(--text-muted);
  }

  .hint,
  .error {
    margin: 0;
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--c-field-hint-track);
    color: var(--text-faint);
  }

  .error {
    color: var(--status-error);
  }
</style>
