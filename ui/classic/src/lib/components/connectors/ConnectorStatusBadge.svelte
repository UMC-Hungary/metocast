<script lang="ts">
  import type { ConnectorStatus } from '$lib/stores/connectors.js';

  interface Props {
    name: string;
    status: ConnectorStatus;
  }

  let { name, status }: Props = $props();

  const statusLabels: Record<ConnectorStatus, string> = {
    connected: 'Connected',
    connecting: 'Connecting',
    disconnected: 'Disconnected',
    error: 'Error',
  };
</script>

<span class="badge badge--{status}">
  <span class="dot" aria-hidden="true"></span>
  <span>{name}</span>
  <span class="status-label">{statusLabels[status]}</span>
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.2rem 0.6rem;
    border-radius: 9999px;
    font-size: 0.8rem;
    font-weight: 500;
    background: var(--content-bg);
    color: var(--text-primary);
  }

  .dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--text-tertiary);
    flex-shrink: 0;
  }

  .status-label {
    color: inherit;
    opacity: 0.75;
  }

  /* Connected — green */
  .badge--connected {
    background: var(--status-ok-bg);
    color: var(--status-ok-text);
  }
  .badge--connected .dot {
    background: var(--status-ok-dot);
  }

  /* Connecting — yellow */
  .badge--connecting {
    background: var(--status-warn-bg);
    color: var(--status-warn-text);
  }
  .badge--connecting .dot {
    background: var(--status-warn-dot);
  }

  /* Disconnected — grey */
  .badge--disconnected {
    background: var(--content-bg);
    color: var(--text-secondary);
  }
  .badge--disconnected .dot {
    background: var(--text-tertiary);
  }

  /* Error — red */
  .badge--error {
    background: var(--status-err-bg);
    color: var(--status-err-text);
  }
  .badge--error .dot {
    background: var(--status-err-dot);
  }
</style>
