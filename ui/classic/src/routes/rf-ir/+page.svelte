<script lang="ts">
  import { onMount } from 'svelte';
  import { fetchDevices, type BroadlinkDevice } from '@metocast/core-client';
  import { broadlinkStatus } from '$lib/stores/connectors.js';
  import DeviceList from '$lib/components/connectors/broadlink/DeviceList.svelte';
  import CommandList from '$lib/components/connectors/broadlink/CommandList.svelte';

  let devices = $state<BroadlinkDevice[]>([]);
  let selectedDevice = $state<BroadlinkDevice | null>(null);
  let loading = $state(false);
  let error = $state('');

  onMount(async () => {
    await reload();
  });

  async function reload() {
    loading = true;
    error = '';
    try {
      devices = await fetchDevices();
      if (devices.length > 0 && !selectedDevice) {
        selectedDevice = devices[0] ?? null;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
</script>

<svelte:head>
  <title>RF/IR Remote — Broadlink</title>
</svelte:head>

<div style="padding:1.5rem; max-width:900px; margin:0 auto;">
  <div style="display:flex; align-items:center; gap:1rem; margin-bottom:1.5rem;">
    <h1 style="margin:0;">RF/IR Remote Control</h1>
    <span
      style="padding:0.2rem 0.6rem; border-radius:99px; font-size:0.8rem; background:{$broadlinkStatus ===
      'connected'
        ? 'var(--status-ok-bg)'
        : 'var(--status-err-bg)'}; color:{$broadlinkStatus === 'connected'
        ? 'var(--status-ok-text)'
        : 'var(--status-err-text)'};"
    >
      {$broadlinkStatus}
    </span>
  </div>

  <section style="margin-bottom:2rem;">
    <h2>Devices</h2>
    {#if error}
      <p style="color:var(--status-err-text)">{error}</p>
    {:else}
      <DeviceList />
    {/if}
  </section>

  {#if loading}
    <p>Loading devices…</p>
  {:else if devices.length > 0}
    <section>
      <h2>Commands</h2>
      <div style="display:flex; gap:0.5rem; flex-wrap:wrap; margin-bottom:1rem;">
        {#each devices as dev (dev.id)}
          <button
            onclick={() => (selectedDevice = dev)}
            style="padding:0.4rem 0.8rem; border-radius:0.25rem; background:{selectedDevice?.id ===
            dev.id
              ? 'var(--accent)'
              : 'var(--border)'}; color:{selectedDevice?.id === dev.id
              ? 'white'
              : 'var(--text-primary)'};"
          >
            {dev.name}
          </button>
        {/each}
      </div>

      {#if selectedDevice}
        <CommandList device={selectedDevice} onCommandAdded={reload} />
      {/if}
    </section>
  {/if}
</div>
