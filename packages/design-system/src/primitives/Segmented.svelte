<script lang="ts" module>
  export interface SegmentedOption<T extends string = string> {
    value: T;
    label: string;
    /** A display-face character above the label, as in the reference. */
    glyph?: string;
    /** A second, quieter line under the label. */
    hint?: string;
    disabled?: boolean;
  }
</script>

<script lang="ts" generics="T extends string">
  import { rovingTabindex } from '../a11y/roving-tabindex.js';

  interface Props {
    options: SegmentedOption<T>[];
    value: T;
    /** Required unless the group is labelled by another element through `labelledby`. */
    label?: string;
    labelledby?: string;
    /** The reference's compact inline tabs (as in Settings) rather than the full control. */
    compact?: boolean;
    onchange?: (value: T) => void;
  }

  let {
    options,
    value = $bindable(),
    label,
    labelledby,
    compact = false,
    onchange,
  }: Props = $props();

  function select(option: SegmentedOption<T>) {
    if (option.disabled) return;
    value = option.value;
    onchange?.(option.value);
  }

  /** APG radiogroup: the arrows both move focus and change the selection. */
  function onmove(index: number) {
    const option = options[index];
    if (option) select(option);
  }
</script>

<div
  class="seg"
  class:compact
  role="radiogroup"
  aria-label={label}
  aria-labelledby={labelledby}
  style:--sanctum-seg-columns={options.length}
  use:rovingTabindex={{ selector: '[data-roving]', orientation: 'horizontal', onmove }}
>
  {#each options as option (option.value)}
    <button
      type="button"
      role="radio"
      data-roving
      aria-checked={value === option.value}
      aria-disabled={option.disabled || undefined}
      tabindex={value === option.value ? 0 : -1}
      class:active={value === option.value}
      onclick={() => select(option)}
    >
      {#if option.glyph}<span class="glyph" aria-hidden="true">{option.glyph}</span>{/if}
      <strong>{option.label}</strong>
      {#if option.hint}<small>{option.hint}</small>{/if}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: grid;
    grid-template-columns: repeat(var(--sanctum-seg-columns), 1fr);
    border: var(--ui-border-hairline) solid var(--border-control);
  }

  button {
    padding: var(--c-segmented-padding-block) var(--ui-stack);
    min-height: var(--ui-target-min);
    cursor: pointer;
    background: transparent;
    color: var(--text-primary);
    border: 0;
    border-right: var(--ui-border-hairline) solid var(--border-control);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--c-segmented-gap);
  }

  button:last-child {
    border-right: 0;
  }

  /* The reference's inline tabs: compact, no 44px target. */
  .compact button {
    min-height: 0;
    padding: var(--space-7, 7px) var(--space-10, 10px);
  }

  button[aria-disabled='true'] {
    cursor: not-allowed;
    color: var(--text-muted);
  }

  button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: calc(var(--ui-focus-offset) * -1);
  }

  .active {
    background: var(--surface-inverse);
    color: var(--text-inverse);
  }

  .glyph {
    font-family: var(--type-heading-family);
    font-size: var(--c-segmented-glyph-size);
    line-height: var(--type-numeral-leading);
  }

  strong {
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    letter-spacing: var(--c-segmented-label-track);
    text-transform: var(--type-label-transform);
    font-weight: var(--type-label-weight);
  }

  small {
    font-size: var(--type-label-size);
    color: var(--text-muted);
  }

  .active small {
    color: var(--text-inverse-muted);
  }
</style>
