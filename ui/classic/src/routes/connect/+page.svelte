<script lang="ts">
  import { appMode } from '$lib/stores/mode.js';
  import { serverUrl, serverPort, localNetworkUrl } from '$lib/stores/server-url.js';
  import TokenDisplay from '$lib/components/connect/TokenDisplay.svelte';
  import ConnectionGuide from '$lib/components/connect/ConnectionGuide.svelte';
  import SshAccessCard from '$lib/components/connect/SshAccessCard.svelte';
  import PresenterInstallCard from '$lib/components/connect/PresenterInstallCard.svelte';

  let copiedNetworkUrl = $state(false);

  async function copyNetworkUrl() {
    await navigator.clipboard.writeText($localNetworkUrl);
    copiedNetworkUrl = true;
    setTimeout(() => {
      copiedNetworkUrl = false;
    }, 2000);
  }
</script>

<svelte:head>
  <title>Connect — Metocast</title>
</svelte:head>

<h1>Connection Info</h1>

<section class="info-card">
  <h2>Server</h2>
  <dl class="meta">
    <dt>Mode</dt>
    <dd>{$appMode}</dd>
    <dt>Local URL</dt>
    <dd><code>{$serverUrl}</code></dd>
    {#if $localNetworkUrl}
      <dt>Network URL</dt>
      <dd class="url-cell">
        <code>{$localNetworkUrl}</code>
        <button onclick={copyNetworkUrl}>{copiedNetworkUrl ? 'Copied!' : 'Copy'}</button>
      </dd>
    {/if}
    <dt>Port</dt>
    <dd>{$serverPort}</dd>
  </dl>
</section>

{#if $appMode === 'server'}
  <section>
    <h2>Auth Token</h2>
    <TokenDisplay />
  </section>

  <section>
    <ConnectionGuide />
  </section>

  <PresenterInstallCard />

  <SshAccessCard />
{:else}
  <p>In client mode. Connect to a server to manage events.</p>
{/if}

<style>
  h1 {
    margin-bottom: 1.5rem;
  }

  section {
    margin-bottom: 2rem;
  }

  h2 {
    margin: 0 0 1rem;
  }

  .info-card {
    padding: 1.25rem;
    background: var(--content-bg);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    max-width: 600px;
  }

  .url-cell {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .url-cell button {
    padding: 0.2rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    background: var(--glass-card-bg);
    cursor: pointer;
    font-size: 0.8rem;
    white-space: nowrap;
  }

  .url-cell button:hover {
    background: var(--nav-item-hover);
  }

  .meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.5rem 1rem;
    margin: 0;
  }

  .meta dt {
    font-weight: 600;
  }

  .meta dd {
    margin: 0;
  }
</style>
