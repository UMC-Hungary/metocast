<script lang="ts">
  import { onDestroy } from 'svelte';
  import { broadlinkDiscoveredDevices } from '$lib/stores/broadlink.js';
  import {
    triggerDiscover,
    addDevice,
    fetchDevices,
    type BroadlinkDevice,
  } from '@metocast/core-client';

  interface Props {
    /** Called when a device is successfully added so the parent can refresh its list. */
    onDeviceAdded?: (device: BroadlinkDevice) => void;
  }

  let { onDeviceAdded }: Props = $props();

  type ScanState = 'idle' | 'scanning' | 'done';

  let scanState = $state<ScanState>('idle');
  let scanError = $state('');
  /** MACs of devices already persisted in the database. */
  let savedMacs = $state<Set<string>>(new Set());
  /** MACs that the user just added during this session. */
  let addedMacs = $state<Set<string>>(new Set());
  let addingMac = $state<string | null>(null);
  let addErrors = $state<Record<string, string>>({});

  let scanTimer: ReturnType<typeof setTimeout> | null = null;

  const discovered = broadlinkDiscoveredDevices;

  onDestroy(() => {
    if (scanTimer !== null) clearTimeout(scanTimer);
  });

  async function startScan() {
    scanError = '';
    addErrors = {};
    addedMacs = new Set();
    broadlinkDiscoveredDevices.set([]);

    // Load currently saved devices so we can mark them
    try {
      const existing = await fetchDevices();
      savedMacs = new Set(existing.map((d) => d.mac));
    } catch {
      savedMacs = new Set();
    }

    scanState = 'scanning';

    try {
      await triggerDiscover();
    } catch (e) {
      scanError = String(e);
      scanState = 'idle';
      return;
    }

    // Server resolves after the discovery timeout (default 5 s). Give a
    // few extra seconds for the WS messages to arrive, then flip to done.
    scanTimer = setTimeout(() => {
      scanState = 'done';
      scanTimer = null;
    }, 8000);
  }

  function stopScan() {
    if (scanTimer !== null) {
      clearTimeout(scanTimer);
      scanTimer = null;
    }
    scanState = 'done';
  }

  async function handleAdd(dev: {
    name: string;
    host: string;
    mac: string;
    deviceType: string;
    model: string | null;
  }) {
    addingMac = dev.mac;
    const rest = { ...addErrors };
    delete rest[dev.mac];
    addErrors = rest;

    try {
      const body: Parameters<typeof addDevice>[0] = {
        name: dev.name || dev.model || 'Broadlink Device',
        host: dev.host,
        mac: dev.mac,
        deviceType: dev.deviceType,
      };
      if (dev.model) body.model = dev.model;
      const saved = await addDevice(body);
      addedMacs = new Set([...addedMacs, dev.mac]);
      savedMacs = new Set([...savedMacs, dev.mac]);
      onDeviceAdded?.(saved);
    } catch (e) {
      addErrors = { ...addErrors, [dev.mac]: String(e) };
    } finally {
      addingMac = null;
    }
  }
</script>

<div class="discovery-panel">
  <div class="discovery-header">
    <span class="discovery-title">Device Discovery</span>
    {#if scanState === 'idle'}
      <button class="btn-discover" onclick={startScan}>Scan Network</button>
    {:else if scanState === 'scanning'}
      <div class="scan-active">
        <span class="spinner" aria-hidden="true"></span>
        <span>Scanning…</span>
        <button class="btn-stop" onclick={stopScan}>Stop</button>
      </div>
    {:else}
      <div class="scan-active">
        <span class="done-mark" aria-hidden="true">✓</span>
        <span>Scan complete</span>
        <button class="btn-discover" onclick={startScan}>Scan Again</button>
      </div>
    {/if}
  </div>

  {#if scanError}
    <p class="scan-error" role="alert">{scanError}</p>
  {/if}

  {#if scanState !== 'idle'}
    <div class="device-list" role="list">
      {#if $discovered.length === 0}
        {#if scanState === 'scanning'}
          <p class="empty-hint">Waiting for devices to respond…</p>
        {:else}
          <p class="empty-hint">No devices found on this network.</p>
        {/if}
      {:else}
        {#each $discovered as dev (dev.mac)}
          <div class="device-row" role="listitem">
            <div class="device-info">
              <span class="device-name">{dev.name || dev.model || 'Unknown Device'}</span>
              <span class="device-meta">{dev.host} &middot; {dev.mac}</span>
              {#if dev.model}
                <span class="device-model">{dev.model} ({dev.deviceType})</span>
              {/if}
              {#if addErrors[dev.mac]}
                <span class="device-error">{addErrors[dev.mac]}</span>
              {/if}
            </div>
            <div class="device-action">
              {#if addedMacs.has(dev.mac)}
                <span class="badge badge--added">Added</span>
              {:else if savedMacs.has(dev.mac)}
                <span class="badge badge--saved">Already saved</span>
              {:else}
                <button
                  class="btn-add"
                  onclick={() => handleAdd(dev)}
                  disabled={addingMac === dev.mac}
                >
                  {addingMac === dev.mac ? 'Adding…' : 'Add'}
                </button>
              {/if}
            </div>
          </div>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .discovery-panel {
    margin-top: 1rem;
    border: 1px solid var(--border);
    border-radius: 0.375rem;
    overflow: hidden;
  }

  .discovery-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.625rem 0.875rem;
    background: var(--content-bg);
    border-bottom: 1px solid var(--border);
  }

  .discovery-title {
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .scan-active {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8125rem;
    color: var(--text-primary);
  }

  .spinner {
    display: inline-block;
    width: 0.875rem;
    height: 0.875rem;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.75s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .done-mark {
    color: var(--status-ok-dot);
    font-weight: 700;
  }

  .scan-error {
    margin: 0;
    padding: 0.5rem 0.875rem;
    font-size: 0.8125rem;
    color: var(--status-err-text);
    background: var(--status-err-bg);
    border-bottom: 1px solid var(--border);
  }

  .device-list {
    padding: 0.25rem 0;
  }

  .empty-hint {
    margin: 0;
    padding: 0.75rem 0.875rem;
    font-size: 0.8125rem;
    color: var(--text-secondary);
  }

  .device-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.625rem 0.875rem;
    border-bottom: 1px solid var(--content-bg);
  }

  .device-row:last-child {
    border-bottom: none;
  }

  .device-info {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    min-width: 0;
  }

  .device-name {
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .device-meta {
    font-size: 0.75rem;
    color: var(--text-secondary);
    font-family: monospace;
  }

  .device-model {
    font-size: 0.75rem;
    color: var(--text-tertiary);
  }

  .device-error {
    font-size: 0.75rem;
    color: var(--status-err-text);
  }

  .device-action {
    flex-shrink: 0;
  }

  .badge {
    display: inline-block;
    padding: 0.2rem 0.5rem;
    border-radius: 99px;
    font-size: 0.75rem;
    font-weight: 500;
  }

  .badge--added {
    background: var(--status-ok-bg);
    color: var(--status-ok-text);
  }

  .badge--saved {
    background: var(--content-bg);
    color: var(--text-secondary);
  }

  .btn-discover,
  .btn-add,
  .btn-stop {
    padding: 0.3rem 0.65rem;
    border-radius: 0.3rem;
    font-size: 0.8125rem;
    cursor: pointer;
    border: 1px solid transparent;
  }

  .btn-discover {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }

  .btn-discover:hover {
    filter: brightness(0.9);
  }

  .btn-add {
    background: var(--glass-card-bg);
    color: var(--accent);
    border-color: var(--accent);
  }

  .btn-add:hover:not(:disabled) {
    background: var(--accent-subtle);
  }

  .btn-stop {
    background: var(--glass-card-bg);
    color: var(--text-secondary);
    border-color: var(--border);
  }

  .btn-stop:hover {
    background: var(--content-bg);
  }

  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
