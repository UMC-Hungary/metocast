<script lang="ts">
  import Dot from '../primitives/Dot.svelte';
  import Icon from '../primitives/Icon.svelte';
  import type { Status } from '../primitives/StatusDot.svelte';

  interface Props {
    /** Already interpolated by the caller — "Notifications, 3 unread". */
    label: string;
    unread?: number;
    /** The worst tier waiting. `off` shows no mark at all. */
    tier?: Status;
    onclick?: () => void;
  }

  let { label, unread = 0, tier = 'off', onclick }: Props = $props();

  const colors: Record<Status, string | null> = {
    live: 'var(--status-live)',
    error: 'var(--status-error)',
    warn: 'var(--status-warn)',
    ok: 'var(--text-secondary)',
    off: null,
  };

  const color = $derived(colors[tier]);
</script>

<button type="button" aria-label={label} {onclick}>
  <Icon name="bell" size={20} />
  {#if unread > 0 && (tier === 'live' || tier === 'error')}
    <em aria-hidden="true">{unread}</em>
  {:else if color}
    <span class="mark"><Dot {color} size={8} pulse={tier === 'live'} /></span>
  {/if}
</button>

<style>
  button {
    background: transparent;
    border: 0;
    min-width: var(--ui-target-min);
    min-height: var(--ui-target-min);
    padding: var(--c-icon-button-padding);
    cursor: pointer;
    position: relative;
    color: var(--text-primary);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
  }

  .mark {
    position: absolute;
    display: flex;
    box-sizing: content-box;
    top: var(--c-bell-mark-offset);
    right: var(--c-bell-mark-offset);
    width: var(--c-bell-mark-size);
    height: var(--c-bell-mark-size);
    border-radius: var(--ui-radius-circle);
    border: var(--ui-border-emphasis) solid var(--surface-base);
  }

  em {
    position: absolute;
    top: var(--c-bell-count-top);
    right: var(--c-bell-count-right);
    font-family: var(--type-label-family);
    font-size: var(--c-bell-count-size);
    color: var(--text-inverse);
    background: var(--status-error);
    padding: var(--c-bell-count-padding-block) var(--c-bell-count-padding-inline);
    border-radius: var(--ui-radius-pill);
    min-width: var(--c-bell-count-min-width);
    text-align: center;
    line-height: var(--type-label-leading);
    border: var(--c-bell-count-border) solid var(--surface-base);
    font-style: normal;
  }
</style>
