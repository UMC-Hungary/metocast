<script lang="ts" module>
  export interface RadioOption<T extends string = string> {
    value: T;
    label: string;
    hint?: string;
    disabled?: boolean;
  }
</script>

<script lang="ts" generics="T extends string">
  interface Props {
    options: RadioOption<T>[];
    value: T;
    label: string;
    labelHidden?: boolean;
    onchange?: (value: T) => void;
  }

  let { options, value = $bindable(), label, labelHidden = false, onchange }: Props = $props();

  const name = `sanctum-radio-${crypto.randomUUID()}`;
</script>

<fieldset>
  <legend class:visually-hidden={labelHidden}>{label}</legend>
  {#each options as option (option.value)}
    <div class="option">
      <input
        type="radio"
        id={`${name}-${option.value}`}
        {name}
        value={option.value}
        checked={value === option.value}
        disabled={option.disabled}
        aria-describedby={option.hint ? `${name}-${option.value}-hint` : undefined}
        onchange={() => {
          value = option.value;
          onchange?.(option.value);
        }}
      />
      <span class="mark" aria-hidden="true"></span>
      <label for={`${name}-${option.value}`}>
        {option.label}
        {#if option.hint}<span id={`${name}-${option.value}-hint`} class="hint">{option.hint}</span
          >{/if}
      </label>
    </div>
  {/each}
</fieldset>

<style>
  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
    display: flex;
    flex-direction: column;
    gap: var(--ui-stack);
  }

  legend {
    padding: 0;
    margin-bottom: var(--c-radio-legend-gap);
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--type-label-sm-track);
    text-transform: var(--type-label-sm-transform);
    color: var(--text-muted);
  }

  .option {
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: var(--c-checkbox-gap);
    min-height: var(--ui-target-min);
  }

  input {
    position: absolute;
    top: var(--c-radio-mark-offset);
    width: var(--c-checkbox-size);
    height: var(--c-checkbox-size);
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }

  .mark {
    width: var(--c-checkbox-size);
    height: var(--c-checkbox-size);
    flex: 0 0 auto;
    margin-top: var(--c-radio-mark-offset);
    border: var(--ui-border-hairline) solid var(--border-control);
    border-radius: var(--ui-radius-circle);
    background: transparent;
  }

  input:checked ~ .mark {
    border-width: var(--c-radio-mark-thickness);
    border-color: var(--surface-inverse);
  }

  input:focus-visible ~ .mark {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
  }

  input:disabled ~ .mark,
  input:disabled ~ label {
    opacity: 0.5;
    cursor: not-allowed;
  }

  label {
    font-family: var(--type-body-sm-family);
    font-size: var(--type-body-sm-size);
    font-weight: var(--type-body-sm-weight);
    letter-spacing: var(--type-body-sm-track);
    color: var(--text-primary);
    cursor: pointer;
  }

  .hint {
    display: block;
    font-family: var(--type-caption-family);
    font-size: var(--type-caption-size);
    font-weight: var(--type-caption-weight);
    letter-spacing: var(--type-caption-track);
    line-height: var(--type-caption-leading);
    color: var(--text-muted);
    margin-top: var(--c-toggle-row-sub-gap);
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
