<script lang="ts">
  import Toggle from '../primitives/Toggle.svelte';

  interface Props {
    label: string;
    /** A second line explaining what the switch does. */
    sub?: string;
    checked?: boolean;
    disabled?: boolean;
    onchange?: (checked: boolean) => void;
  }

  let { label, sub = '', checked = $bindable(false), disabled = false, onchange }: Props = $props();

  const id = `sanctum-toggle-row-${crypto.randomUUID()}`;
</script>

<div class="row">
  <p>
    <strong id={`${id}-label`}>{label}</strong>
    {#if sub}<span id={`${id}-sub`}>{sub}</span>{/if}
  </p>
  <Toggle
    bind:checked
    {disabled}
    {onchange}
    labelledby={`${id}-label`}
    describedby={sub ? `${id}-sub` : undefined}
  />
</div>

<style>
  .row {
    background: var(--surface-sunken);
    padding: var(--c-toggle-row-padding-block) var(--ui-gutter-inset);
    display: flex;
    align-items: center;
    gap: var(--c-toggle-row-gap);
    min-height: var(--ui-target-min);
  }

  p {
    flex: 1;
    min-width: 0;
    margin: 0;
  }

  strong {
    display: block;
    font-family: var(--type-body-sm-family);
    font-size: var(--type-body-sm-size);
    color: var(--text-primary);
    font-weight: var(--type-body-sm-weight);
    letter-spacing: var(--type-body-sm-track);
  }

  span {
    display: block;
    font-size: var(--type-caption-size);
    color: var(--text-muted);
    margin-top: var(--c-toggle-row-sub-gap);
    line-height: var(--type-body-sm-leading);
  }
</style>
