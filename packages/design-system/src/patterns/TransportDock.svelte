<script lang="ts" module>
  export interface TransportAction {
    icon: IconName;
    label: string;
    variant?: 'default' | 'primary' | 'stop';
    disabled?: boolean;
    onclick: () => void;
  }
</script>

<script lang="ts">
  import Icon, { type IconName } from '../primitives/Icon.svelte';

  interface Props {
    /** The mono status line — "Presenting", "Standby". */
    status: string;
    /** What is on screen now. Truncates rather than wrapping. */
    current: string;
    /** The slide number, set as a display figure. */
    position: string | number;
    actions: TransportAction[];
    /** Names the toolbar so it can be reached from the landmark list. */
    label: string;
  }

  let { status, current, position, actions, label }: Props = $props();
</script>

<section class="dock" aria-label={label}>
  <div class="status">
    <span>{status}</span>
    <strong>{position}</strong>
    <em>{current}</em>
  </div>
  <div class="controls" role="toolbar" aria-label={label}>
    {#each actions as action (action.label)}
      <button
        type="button"
        class={action.variant ?? 'default'}
        aria-label={action.label}
        disabled={action.disabled}
        onclick={action.onclick}
      >
        <Icon name={action.icon} size={20} stroke={1.6} />
        <span>{action.label}</span>
      </button>
    {/each}
  </div>
</section>

<style>
  .dock {
    position: sticky;
    top: 0;
    z-index: var(--z-dock);
    margin-bottom: var(--c-dock-margin-bottom);
    padding: var(--c-dock-padding-top) var(--c-dock-padding-inline) var(--c-dock-padding-bottom);
    background: var(--surface-base);
    border-top: var(--ui-border-hairline) solid var(--border-strong);
    border-bottom: var(--ui-border-hairline) solid var(--border-strong);
  }

  .status {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: var(--c-dock-status-row-gap) var(--c-dock-status-column-gap);
    align-items: end;
    margin-bottom: var(--c-dock-status-margin);
  }

  .status > span {
    display: flex;
    align-items: center;
    gap: var(--ui-stack);
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--c-dock-status-track);
    text-transform: var(--type-label-sm-transform);
    color: var(--text-muted);
  }

  strong {
    grid-column: 2;
    grid-row: 1 / span 2;
    font-family: var(--type-title-family);
    font-size: var(--type-title-size);
    line-height: var(--type-title-leading);
    color: var(--text-primary);
    font-weight: var(--type-title-weight);
  }

  em {
    min-width: 0;
    color: var(--text-primary);
    font-family: var(--type-body-sm-family);
    font-size: var(--type-body-sm-size);
    font-style: normal;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .controls {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(0, 1fr));
    gap: var(--c-dock-controls-gap);
  }

  button {
    min-width: 0;
    min-height: var(--c-dock-button-height);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--c-dock-button-gap);
    background: var(--surface-raised);
    border: var(--ui-border-hairline) solid var(--border-control);
    color: var(--text-primary);
    cursor: pointer;
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: calc(var(--ui-focus-offset) * -1);
  }

  button.primary {
    background: var(--surface-inverse);
    color: var(--text-inverse);
  }

  button.stop {
    color: var(--status-error);
  }

  button span {
    font-family: var(--type-label-family);
    font-size: var(--c-dock-button-label-size);
    letter-spacing: var(--c-dock-button-label-track);
    text-transform: var(--type-label-transform);
  }

  @media (min-width: 760px) {
    .dock {
      top: var(--c-dock-sticky-offset);
      padding: var(--ui-gutter-inset);
      border: var(--ui-border-hairline) solid var(--border-strong);
    }
  }
</style>
