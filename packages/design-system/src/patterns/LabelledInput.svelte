<script lang="ts">
  interface Props {
    label: string;
    value?: string;
    placeholder?: string;
    type?: 'text' | 'email' | 'url' | 'tel' | 'search' | 'number';
    error?: string;
    disabled?: boolean;
    required?: boolean;
    id?: string;
  }

  let {
    label,
    value = $bindable(''),
    placeholder = '',
    type = 'text',
    error = '',
    disabled = false,
    required = false,
    id = `sanctum-labelled-input-${crypto.randomUUID()}`,
  }: Props = $props();

  const errorId = $derived(`${id}-error`);
</script>

<div class="chamber" class:invalid={Boolean(error)}>
  <label for={id}>{label}</label>
  <input
    {id}
    {type}
    {placeholder}
    {disabled}
    {required}
    bind:value
    aria-invalid={error ? 'true' : undefined}
    aria-describedby={error ? errorId : undefined}
  />
  {#if error}<p id={errorId} class="error">{error}</p>{/if}
</div>

<style>
  .chamber {
    display: block;
    background: var(--surface-sunken);
    border: var(--ui-border-hairline) solid var(--border-control);
    padding: var(--c-labelled-input-padding-top) var(--ui-gutter-inset)
      var(--c-labelled-input-padding-bottom);
    transition: border var(--motion-fast);
  }

  .chamber:focus-within {
    border-color: var(--accent);
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
  }

  .invalid {
    border-color: var(--status-error);
  }

  label {
    display: block;
    font-family: var(--type-label-xs-family);
    font-size: var(--type-label-xs-size);
    letter-spacing: var(--type-label-xs-track);
    text-transform: var(--type-label-xs-transform);
    color: var(--text-muted);
    font-weight: var(--type-label-xs-weight);
    margin-bottom: var(--c-labelled-input-label-gap);
  }

  input {
    width: 100%;
    border: 0;
    background: transparent;
    font-family: var(--type-body-strong-family);
    font-size: var(--type-body-strong-size);
    color: var(--text-primary);
    padding: 0;
    letter-spacing: var(--type-body-strong-track);
    font-weight: var(--type-body-strong-weight);
    line-height: var(--type-body-strong-leading);
    caret-color: var(--text-primary);
  }

  /* The container carries the focus indicator, so the input's own outline would double it. */
  input:focus {
    outline: 0;
  }

  input:disabled {
    color: var(--text-muted);
    cursor: not-allowed;
  }

  .error {
    margin: var(--c-labelled-input-label-gap) 0 0;
    font-family: var(--type-label-xs-family);
    font-size: var(--type-label-xs-size);
    letter-spacing: var(--type-label-xs-track);
    text-transform: var(--type-label-xs-transform);
    color: var(--status-error);
  }
</style>
