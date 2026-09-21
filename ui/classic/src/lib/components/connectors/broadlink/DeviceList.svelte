<script lang="ts">
  import { onMount } from 'svelte';
  import {
    fetchDevices,
    addDevice,
    removeDevice,
    triggerDiscover,
    type BroadlinkDevice,
  } from '@metocast/core-client';
  import { broadlinkDiscoveredDevices } from '$lib/stores/broadlink.js';

  let devices = $state<BroadlinkDevice[]>([]);
  let loading = $state(false);
  let error = $state('');

  let newName = $state('');
  let newHost = $state('');
  let newMac = $state('');
  let newType = $state('0x520b');
  let addError = $state('');

  let discovering = $state(false);
  let discoverError = $state('');

  onMount(async () => {
    await reload();
  });

  async function reload() {
    loading = true;
    error = '';
    try {
      devices = await fetchDevices();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function handleAdd() {
    addError = '';
    try {
      const dev = await addDevice({
        name: newName,
        host: newHost,
        mac: newMac,
        deviceType: newType,
      });
      devices = [...devices, dev];
      newName = '';
      newHost = '';
      newMac = '';
      newType = '0x520b';
    } catch (e) {
      addError = String(e);
    }
  }

  async function handleRemove(id: string) {
    try {
      await removeDevice(id);
      devices = devices.filter((d) => d.id !== id);
    } catch (e) {
      error = String(e);
    }
  }

  async function handleDiscover() {
    discoverError = '';
    discovering = true;
    broadlinkDiscoveredDevices.set([]);
    try {
      await triggerDiscover();
      // Results come in via WS; wait for a few seconds then reload
      await new Promise((r) => setTimeout(r, 7000));
      await reload();
    } catch (e) {
      discoverError = String(e);
    } finally {
      discovering = false;
    }
  }
</script>

<div>
  <div style="display:flex; gap:0.5rem; margin-bottom:0.75rem;">
    <button onclick={handleDiscover} disabled={discovering}>
      {discovering ? 'Discovering…' : 'Discover Devices'}
    </button>
    {#if discoverError}
      <span style="color:var(--status-err-text)">{discoverError}</span>
    {/if}
  </div>

  {#if loading}
    <p>Loading…</p>
  {:else if error}
    <p style="color:var(--status-err-text)">{error}</p>
  {:else if devices.length === 0}
    <p>No devices. Discover or add manually below.</p>
  {:else}
    <ul style="list-style:none; padding:0; margin:0 0 0.75rem;">
      {#each devices as dev (dev.id)}
        <li
          style="display:flex; justify-content:space-between; align-items:center; padding:0.25rem 0; border-bottom:1px solid var(--border);"
        >
          <span><strong>{dev.name}</strong> — {dev.host} ({dev.mac})</span>
          <button onclick={() => handleRemove(dev.id)}>Remove</button>
        </li>
      {/each}
    </ul>
  {/if}

  <details>
    <summary>Add device manually</summary>
    <div style="margin-top:0.5rem; display:flex; flex-direction:column; gap:0.5rem;">
      <input placeholder="Name" bind:value={newName} />
      <input placeholder="Host (IP)" bind:value={newHost} />
      <input placeholder="MAC (aa:bb:cc:dd:ee:ff)" bind:value={newMac} />
      <input placeholder="Device type (e.g. 0x520b)" bind:value={newType} />
      {#if addError}
        <span style="color:var(--status-err-text)">{addError}</span>
      {/if}
      <button onclick={handleAdd} disabled={!newName || !newHost || !newMac}>Add Device</button>
    </div>
  </details>
</div>
