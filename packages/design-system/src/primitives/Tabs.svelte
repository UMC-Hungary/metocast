<script lang="ts" module>
  export interface Tab<T extends string = string> {
    value: T;
    label: string;
    disabled?: boolean;
  }
</script>

<script lang="ts" generics="T extends string">
  import type { Snippet } from 'svelte';
  import { rovingTabindex } from '../a11y/roving-tabindex.js';

  interface Props {
    tabs: Tab<T>[];
    value: T;
    label: string;
    onchange?: (value: T) => void;
    /** Rendered inside the panel for the selected tab. */
    children: Snippet<[T]>;
  }

  let { tabs, value = $bindable(), label, onchange, children }: Props = $props();

  const id = `sanctum-tabs-${crypto.randomUUID()}`;

  function select(tab: Tab<T>) {
    if (tab.disabled) return;
    value = tab.value;
    onchange?.(tab.value);
  }

  /** APG tabs, automatic activation: arrowing to a tab selects it. */
  function onmove(index: number) {
    const tab = tabs[index];
    if (tab) select(tab);
  }
</script>

<div class="tabs">
  <div
    role="tablist"
    aria-label={label}
    use:rovingTabindex={{ selector: '[data-roving]', orientation: 'horizontal', onmove }}
  >
    {#each tabs as tab (tab.value)}
      <button
        type="button"
        role="tab"
        data-roving
        id={`${id}-tab-${tab.value}`}
        aria-selected={value === tab.value}
        aria-controls={`${id}-panel-${tab.value}`}
        aria-disabled={tab.disabled || undefined}
        tabindex={value === tab.value ? 0 : -1}
        class:selected={value === tab.value}
        onclick={() => select(tab)}
      >
        {tab.label}
      </button>
    {/each}
  </div>
  <div
    role="tabpanel"
    id={`${id}-panel-${value}`}
    aria-labelledby={`${id}-tab-${value}`}
    tabindex="0"
  >
    {@render children(value)}
  </div>
</div>

<style>
  [role='tablist'] {
    display: flex;
    gap: var(--c-tabs-gap);
    padding: 0 var(--ui-gutter);
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
    overflow-x: auto;
  }

  button {
    flex: 0 0 auto;
    min-height: var(--ui-target-min);
    padding: var(--c-tabs-padding-block) 0;
    background: transparent;
    border: 0;
    border-bottom: var(--ui-border-emphasis) solid transparent;
    color: var(--text-muted);
    cursor: pointer;
    font-family: var(--type-label-family);
    font-size: var(--type-label-size);
    letter-spacing: var(--c-tabs-track);
    text-transform: var(--type-label-transform);
    font-weight: var(--type-label-weight);
    white-space: nowrap;
  }

  .selected {
    color: var(--text-primary);
    border-bottom-color: var(--surface-inverse);
  }

  button[aria-disabled='true'] {
    color: var(--text-faint);
    cursor: not-allowed;
  }

  button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: calc(var(--ui-focus-offset) * -1);
  }

  [role='tabpanel']:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: calc(var(--ui-focus-offset) * -1);
  }
</style>
