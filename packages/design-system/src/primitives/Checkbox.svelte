<script lang="ts">
  import Icon from './Icon.svelte';

  interface Props {
    checked?: boolean;
    /** Neither checked nor unchecked — a parent whose children disagree. */
    indeterminate?: boolean;
    label: string;
    /** Hides the label visually while leaving it to assistive technology. */
    labelHidden?: boolean;
    disabled?: boolean;
    describedby?: string;
    id?: string;
    onchange?: (checked: boolean) => void;
  }

  let {
    checked = $bindable(false),
    indeterminate = false,
    label,
    labelHidden = false,
    disabled = false,
    describedby,
    id = `sanctum-checkbox-${crypto.randomUUID()}`,
    onchange,
  }: Props = $props();
</script>

<div class="checkbox">
  <input
    {id}
    type="checkbox"
    bind:checked
    {disabled}
    {indeterminate}
    aria-describedby={describedby}
    onchange={() => onchange?.(checked)}
  />
  <span class="box" aria-hidden="true">
    {#if indeterminate}
      <Icon name="minus" size={14} stroke={1.8} />
    {:else if checked}
      <Icon name="check" size={14} stroke={1.8} />
    {/if}
  </span>
  <label for={id} class:visually-hidden={labelHidden}>{label}</label>
</div>

<style>
  .checkbox {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--c-checkbox-gap);
    min-height: var(--ui-target-min);
  }

  /* The native control stays where it is and stays operable; the box is what is drawn. */
  input {
    position: absolute;
    width: var(--c-checkbox-size);
    height: var(--c-checkbox-size);
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }

  .box {
    width: var(--c-checkbox-size);
    height: var(--c-checkbox-size);
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: center;
    border: var(--ui-border-hairline) solid var(--border-control);
    color: var(--text-inverse);
    background: transparent;
  }

  input:checked ~ .box,
  input:indeterminate ~ .box {
    background: var(--surface-inverse);
    border-color: var(--surface-inverse);
  }

  input:focus-visible ~ .box {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
  }

  input:disabled ~ .box,
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
