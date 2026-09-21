<script lang="ts" module>
  export interface Device {
    id: string;
    name: string;
    /** The address, set in the mono face. */
    address: string;
    /** The model or kind, set in the body face. */
    kind?: string;
  }
</script>

<script lang="ts">
  interface Props {
    devices: Device[];
    label: string;
    /** The action offered on each device — "Pair". */
    actionLabel: string;
    emptyMessage: string;
    onselect?: (device: Device) => void;
  }

  let { devices, label, actionLabel, emptyMessage, onselect }: Props = $props();
</script>

{#if devices.length === 0}
  <p class="empty">{emptyMessage}</p>
{:else}
  <ul aria-label={label}>
    {#each devices as device (device.id)}
      <li>
        <span class="name">{device.name}</span>
        <code>{device.address}</code>
        {#if device.kind}<em>{device.kind}</em>{/if}
        <button
          type="button"
          aria-label={`${actionLabel}: ${device.name}`}
          onclick={() => onselect?.(device)}
        >
          {actionLabel}
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: var(--c-device-row-gap) var(--c-device-column-gap);
    align-items: center;
    padding: var(--c-device-padding-block) 0;
    border-bottom: var(--ui-border-hairline) solid var(--border-hairline);
  }

  li:last-child {
    border-bottom: 0;
  }

  .name {
    font-family: var(--type-body-sm-family);
    font-size: var(--c-device-name-size);
    color: var(--text-primary);
    font-weight: var(--type-body-sm-weight);
  }

  code,
  em {
    grid-column: 1;
    color: var(--text-muted);
    font-family: var(--type-label-sm-family);
    font-size: var(--type-label-sm-size);
    letter-spacing: var(--c-device-address-track);
    font-style: normal;
  }

  em {
    font-family: var(--type-body-family);
    font-size: var(--c-device-kind-size);
    letter-spacing: var(--ui-track-none);
    color: var(--text-secondary);
  }

  button {
    grid-row: 1 / span 3;
    grid-column: 2;
    min-height: var(--ui-target-min);
    padding: 0 var(--c-discovery-button-padding);
    background: transparent;
    color: var(--text-primary);
    border: var(--ui-border-hairline) solid var(--border-control);
    cursor: pointer;
    font-family: var(--type-label-sm-family);
    font-size: var(--c-discovery-title-size);
    letter-spacing: var(--c-discovery-button-track);
    text-transform: var(--type-label-sm-transform);
  }

  button:focus-visible {
    outline: var(--ui-focus-width) solid var(--accent);
    outline-offset: var(--ui-focus-offset);
  }

  .empty {
    margin: 0;
    padding: var(--c-discovery-status-padding) 0;
    font-family: var(--type-quote-family);
    font-style: italic;
    font-size: var(--type-quote-size);
    line-height: var(--type-body-sm-leading);
    color: var(--text-muted);
  }

  /* The reference collapses the action onto its own line below this width. */
  @media (max-width: 420px) {
    li {
      grid-template-columns: 1fr;
    }

    button {
      grid-row: auto;
      grid-column: 1;
      margin-top: var(--ui-stack);
    }
  }
</style>
