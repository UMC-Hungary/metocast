<script lang="ts">
  import { appMode } from '$lib/stores/mode.js';
  import { serverUrl, authToken, localNetworkUrl } from '$lib/stores/server-url.js';
  import { wsStatus } from '$lib/stores/ws.js';
  import { CONNECTORS } from '$lib/connectors/registry.js';
  import ConnectorDashboardWidget from '$lib/components/connectors/ConnectorDashboardWidget.svelte';
</script>

<svelte:head>
  <title>Metocast</title>
</svelte:head>

<h1>Metocast</h1>

<section class="status-grid">
  <!-- System info: mode + WebSocket + server URL in one card -->
  <div class="card system-card">
    <h2>System</h2>
    <dl>
      <dt>Mode</dt>
      <dd><span class="badge">{$appMode}</span></dd>

      <dt>WebSocket</dt>
      <dd><span class="badge badge--{$wsStatus}">{$wsStatus}</span></dd>

      {#if $appMode === 'server'}
        <dt>Server URL</dt>
        <dd>
          <code>{$serverUrl}</code>
          {#if $localNetworkUrl}
            <br /><code class="url-secondary">{$localNetworkUrl}</code>
            <span class="label">network</span>
          {/if}
        </dd>
      {/if}
    </dl>
  </div>

  <!-- One widget per configured connector -->
  {#each CONNECTORS.filter((c) => c.id !== 'broadlink') as def (def.id)}
    <ConnectorDashboardWidget connectorId={def.id} />
  {/each}
</section>

<ConnectorDashboardWidget connectorId="broadlink" />

<section class="quick-links">
  <h2>Quick Links</h2>
  <ul>
    <li><a href="/events">View all events</a></li>
    <li><a href="/events/new">Create new event</a></li>
    <li><a href="/connect">Connection info &amp; token</a></li>
    <li><a href="/logs">Application logs</a></li>
    <li><a href="/settings">Connector settings</a></li>
  </ul>
</section>

{#if $appMode === 'server' && $authToken}
  <section class="token-preview">
    <h2>Auth Token</h2>
    <p>Use this token in the <code>Authorization: Bearer &lt;token&gt;</code> header.</p>
    <a href="/connect">Manage token &rarr;</a>
  </section>
{/if}

<style>
  h1 {
    margin-bottom: 1.5rem;
  }

  .status-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 1rem;
    margin-bottom: 2rem;
    align-items: start;
  }

  /* System card spans full width on small screens, auto-width otherwise */
  .system-card {
    grid-column: 1 / -1;
    max-width: 340px;
  }

  .card {
    padding: 1rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
  }

  .card h2 {
    margin: 0 0 0.75rem;
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    column-gap: 0.75rem;
    row-gap: 0.4rem;
    margin: 0;
  }

  dt {
    font-size: 0.8125rem;
    color: var(--text-secondary);
    align-self: center;
    white-space: nowrap;
  }

  dd {
    margin: 0;
    font-size: 0.875rem;
    align-self: center;
  }

  .badge {
    display: inline-block;
    padding: 0.2rem 0.6rem;
    border-radius: 9999px;
    font-size: 0.8125rem;
    background: var(--content-bg);
    color: var(--text-primary);
  }

  .badge--connected {
    background: var(--status-ok-bg);
    color: var(--status-ok-text);
  }
  .badge--connecting {
    background: var(--status-warn-bg);
    color: var(--status-warn-text);
  }
  .badge--disconnected,
  .badge--error {
    background: var(--status-err-bg);
    color: var(--status-err-text);
  }

  code {
    font-size: 0.8125rem;
    font-family: ui-monospace, monospace;
    color: var(--text-primary);
  }

  .url-secondary {
    color: var(--text-secondary);
  }

  .label {
    font-size: 0.7rem;
    padding: 0.1rem 0.4rem;
    background: var(--accent-subtle);
    color: var(--accent);
    border-radius: 9999px;
    margin-left: 0.25rem;
    vertical-align: middle;
  }

  .quick-links {
    margin-bottom: 2rem;
  }
  .quick-links ul {
    padding-left: 1.25rem;
  }
  .quick-links li {
    margin-bottom: 0.5rem;
  }

  .token-preview {
    padding: 1rem;
    background: var(--accent-subtle);
    border-radius: 0.5rem;
  }
</style>
