<script lang="ts">
  interface Props {
    value?: string;
    type?: 'text' | 'email' | 'url' | 'tel' | 'search' | 'password' | 'number';
    placeholder?: string;
    disabled?: boolean;
    readonly?: boolean;
    required?: boolean;
    invalid?: boolean;
    /** Supplied by `FormField`, which owns the label and the notes. */
    id?: string;
    describedby?: string;
    /** Only when the field stands outside a `FormField`. */
    label?: string;
    /** Completions in the caller's order; the first that extends the value is offered inline. */
    suggestions?: string[];
    /** The word beside the Tab key cap — "accept". */
    acceptHint?: string;
  }

  let {
    value = $bindable(''),
    type = 'text',
    placeholder = '',
    disabled = false,
    readonly = false,
    required = false,
    invalid = false,
    id,
    describedby,
    label,
    suggestions = [],
    acceptHint = '',
  }: Props = $props();

  const hintId = `sanctum-text-field-${crypto.randomUUID()}-hint`;
  let focused = $state(false);
  let dismissed = $state(false);

  const suggestion = $derived(
    focused && !dismissed && !readonly && !disabled
      ? suggestions.find(
          (s) => s.length > value.length && s.toLowerCase().startsWith(value.toLowerCase()),
        )
      : undefined,
  );

  function onkeydown(e: KeyboardEvent) {
    if (!suggestion || e.shiftKey || e.altKey || e.ctrlKey || e.metaKey) return;
    if (e.key === 'Tab') {
      e.preventDefault();
      value = suggestion;
    } else if (e.key === 'Escape') {
      e.preventDefault();
      dismissed = true;
    }
  }
</script>

<div class="field">
  <input
    {id}
    {type}
    placeholder={suggestion ? '' : placeholder}
    {disabled}
    {readonly}
    {required}
    bind:value
    autocomplete={suggestions.length ? 'off' : undefined}
    aria-label={label}
    aria-invalid={invalid ? 'true' : undefined}
    aria-describedby={[describedby, suggestion && hintId].filter(Boolean).join(' ') || undefined}
    class:invalid
    {onkeydown}
    oninput={() => (dismissed = false)}
    onfocus={() => (focused = true)}
    onblur={() => (focused = false)}
  />
  {#if suggestion}
    <p class="ghost" id={hintId}>
      <span class="typed" aria-hidden="true">{value}</span><span aria-hidden="true"
        >{suggestion.slice(value.length)}</span
      >
      <span class="visually-hidden">{suggestion}</span>
      <span class="hint"><kbd>Tab</kbd>{acceptHint}</span>
    </p>
  {/if}
</div>

<style>
  .field {
    position: relative;
  }

  input {
    width: 100%;
    min-height: var(--ui-target-min);
    padding: var(--c-text-field-padding-block) var(--ui-gutter-inset);
    background: var(--surface-sunken);
    border: var(--ui-border-hairline) solid var(--border-control);
    border-radius: var(--ui-radius-square);
    font-family: var(--type-body-strong-family);
    font-size: var(--type-body-strong-size);
    font-weight: var(--type-body-strong-weight);
    letter-spacing: var(--type-body-strong-track);
    color: var(--text-primary);
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

  .invalid {
    border-color: var(--status-error);
  }

  input:disabled {
    color: var(--text-muted);
    cursor: not-allowed;
  }

  .ghost {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    margin: 0;
    padding: 0 var(--ui-gutter-inset);
    border: var(--ui-border-hairline) solid transparent;
    pointer-events: none;
    white-space: pre;
    overflow: hidden;
    font-family: var(--type-body-strong-family);
    font-size: var(--type-body-strong-size);
    font-weight: var(--type-body-strong-weight);
    letter-spacing: var(--type-body-strong-track);
    color: var(--text-faint);
  }

  .typed {
    visibility: hidden;
  }

  .hint {
    display: inline-flex;
    align-items: center;
    gap: var(--ui-stack);
    margin-left: auto;
    padding-left: var(--ui-stack);
    font-family: var(--type-label-xs-family);
    font-size: var(--type-label-xs-size);
    font-weight: var(--type-label-xs-weight);
    letter-spacing: var(--type-label-xs-track);
    text-transform: var(--type-label-xs-transform);
    color: var(--text-muted);
  }

  kbd {
    font-family: inherit;
    padding: var(--c-form-section-number-padding-block) var(--c-form-section-number-padding-inline);
    border: var(--ui-border-hairline) solid var(--border-strong);
    border-radius: var(--ui-radius-chip);
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
    border: 0;
  }
</style>
