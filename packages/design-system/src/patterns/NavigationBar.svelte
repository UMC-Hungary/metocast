<script lang="ts" module>
  export interface NavItem<T extends string = string> {
    value: T;
    label: string;
    icon: IconName;
  }
</script>

<script lang="ts" generics="T extends string">
  import type { Snippet } from 'svelte';
  import Icon, { type IconName } from '../primitives/Icon.svelte';

  interface Props {
    items: NavItem<T>[];
    active: T;
    label: string;
    onselect?: (value: T) => void;
    /** Shown only in the rail, above the items — the reference's lockup and status line. */
    brand?: Snippet;
  }

  let { items, active, label, onselect, brand }: Props = $props();
</script>

<nav aria-label={label}>
  {#if brand}<div class="brand">{@render brand()}</div>{/if}
  {#each items as item (item.value)}
    <button
      type="button"
      class:active={active === item.value}
      aria-current={active === item.value ? 'page' : undefined}
      onclick={() => onselect?.(item.value)}
    >
      <Icon name={item.icon} size={22} stroke={active === item.value ? 1.6 : 1.3} />
      <span>{item.label}</span>
    </button>
  {/each}
</nav>

<style>
  nav {
    z-index: var(--z-nav);
    display: flex;
    justify-content: space-around;
    align-items: center;
    background: var(--surface-base);
    border-top: var(--ui-border-hairline) solid var(--border-hairline);
    padding: var(--c-nav-padding-top) var(--c-nav-padding-inline) var(--c-nav-padding-bottom);
    padding-bottom: max(var(--c-nav-padding-bottom), env(safe-area-inset-bottom));
  }

  .brand {
    display: none;
  }

  button {
    flex: 1;
    min-height: var(--ui-target-min);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--c-nav-item-gap);
    padding: var(--c-nav-item-padding);
    background: transparent;
    border: 0;
    cursor: pointer;
    color: var(--text-muted);
  }

  button.active {
    color: var(--text-primary);
  }

  button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: calc(var(--ui-focus-offset) * -1);
  }

  span {
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--type-label-sm-track);
    text-transform: var(--type-label-sm-transform);
    font-weight: var(--ui-weight-regular);
  }

  .active span {
    font-weight: var(--type-label-sm-weight);
  }

  /* The reference turns the bar into a rail here, and widens it again at the next threshold. */
  @media (min-width: 980px) {
    nav {
      flex-direction: column;
      align-items: stretch;
      justify-content: flex-start;
      gap: var(--c-nav-rail-gap);
      width: var(--c-nav-rail-width);
      height: 100%;
      padding: var(--c-nav-rail-padding-block) var(--ui-gutter-inset);
      border-top: 0;
      border-right: var(--ui-border-hairline) solid var(--border-hairline);
    }

    .brand {
      display: block;
      padding: var(--c-nav-brand-padding-top) var(--ui-stack) var(--c-nav-brand-padding-bottom);
      border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
      margin-bottom: var(--c-nav-brand-gap);
    }

    button {
      flex: 0 0 auto;
      flex-direction: row;
      justify-content: flex-start;
      gap: var(--c-nav-rail-item-gap);
      padding: var(--c-nav-rail-item-padding-block) var(--c-nav-rail-item-padding-inline);
      border-left: var(--ui-border-emphasis) solid transparent;
    }

    button.active {
      background: var(--surface-raised);
      border-left-color: var(--surface-inverse);
    }

    span {
      font-size: var(--type-label-size);
      letter-spacing: var(--c-nav-rail-track);
    }
  }

  @media (min-width: 1360px) {
    nav {
      width: var(--c-nav-rail-width-wide);
    }
  }
</style>
