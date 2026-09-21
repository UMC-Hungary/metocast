<script lang="ts">
  interface Props {
    value?: string;
    placeholder?: string;
    rows?: number;
    disabled?: boolean;
    readonly?: boolean;
    required?: boolean;
    invalid?: boolean;
    id?: string;
    describedby?: string;
    label?: string;
  }

  let {
    value = $bindable(''),
    placeholder = '',
    rows = 4,
    disabled = false,
    readonly = false,
    required = false,
    invalid = false,
    id,
    describedby,
    label,
  }: Props = $props();
</script>

<textarea
  {id}
  {placeholder}
  {rows}
  {disabled}
  {readonly}
  {required}
  bind:value
  aria-label={label}
  aria-invalid={invalid ? 'true' : undefined}
  aria-describedby={describedby}
  class:invalid
></textarea>

<style>
  textarea {
    width: 100%;
    padding: var(--c-text-field-padding-block) var(--ui-gutter-inset);
    background: var(--surface-sunken);
    border: var(--ui-border-hairline) solid var(--border-control);
    border-radius: var(--ui-radius-square);
    font-family: var(--type-body-family);
    font-size: var(--type-body-size);
    line-height: var(--type-caption-leading);
    letter-spacing: var(--type-body-track);
    color: var(--text-primary);
    caret-color: var(--text-primary);
    resize: vertical;
  }

  textarea::placeholder {
    color: var(--text-faint);
  }

  textarea:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
    border-color: var(--accent);
  }

  .invalid {
    border-color: var(--status-error);
  }

  textarea:disabled {
    color: var(--text-muted);
    cursor: not-allowed;
  }
</style>
