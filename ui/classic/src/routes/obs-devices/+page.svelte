<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import {
    obsAvailableDevices,
    obsDeviceListeners,
    obsDeviceListenerStatuses,
  } from '$lib/stores/obs-devices.js';
  import { obsStatus } from '$lib/stores/connectors.js';
  import { sendWsCommand } from '@metocast/core-client';
  import type {
    ObsAvailableDevices,
    DeviceListener,
    DeviceListenerStatus,
  } from '@metocast/core-client/schemas/ws-messages';

  // ── Add listener form state ──────────────────────────────────────────────
  let showAddForm = $state(false);
  let addCategory = $state('');
  let addDeviceValue = $state('');
  let addDeviceName = $state('');
  let addFriendlyName = $state('');

  // ── Edit state ───────────────────────────────────────────────────────────
  let editingId = $state<string | null>(null);
  let editFriendlyName = $state('');

  // ── Derived ─────────────────────────────────────────────────────────────
  const isObsConnected = $derived($obsStatus === 'connected');

  type CategoryKey = 'displays' | 'audioInputs' | 'audioOutputs' | 'videoInputs' | 'captureCards';

  const categoryMap: Record<string, CategoryKey> = {
    display: 'displays',
    audio_input: 'audioInputs',
    audio_output: 'audioOutputs',
    video_input: 'videoInputs',
    capture_card: 'captureCards',
  };

  function devicesForCategory(devices: ObsAvailableDevices, category: string) {
    const key = categoryMap[category];
    return key ? devices[key] : [];
  }

  function statusForListener(id: string, statuses: DeviceListenerStatus[]) {
    return statuses.find((s) => s.listenerId === id);
  }

  function categoryLabel(category: string): string {
    const key = category.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase()) as
      | 'display'
      | 'audioInput'
      | 'audioOutput'
      | 'videoInput'
      | 'captureCard';
    return $_(`obsDevices.categories.${key}`);
  }

  // ── Handlers ─────────────────────────────────────────────────────────────

  function handleScanNow() {
    sendWsCommand('obs.devices.scan');
  }

  function openAddForm() {
    addCategory = '';
    addDeviceValue = '';
    addDeviceName = '';
    addFriendlyName = '';
    showAddForm = true;
  }

  function handleCategoryChange() {
    addDeviceValue = '';
    addDeviceName = '';
    addFriendlyName = '';
  }

  function handleDeviceSelect(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    addDeviceValue = select.value;
    const devices = $obsAvailableDevices
      ? devicesForCategory($obsAvailableDevices, addCategory)
      : [];
    const device = devices.find((d) => d.itemValue === select.value);
    addDeviceName = device?.itemName ?? '';
    if (!addFriendlyName) {
      addFriendlyName = device?.itemName ?? '';
    }
  }

  function handleAddSubmit(event: SubmitEvent) {
    event.preventDefault();
    if (!addCategory || !addDeviceValue || !addFriendlyName) return;
    sendWsCommand('obs.listeners.create', {
      connectorType: 'obs',
      category: addCategory,
      deviceItemValue: addDeviceValue,
      deviceItemName: addDeviceName,
      friendlyName: addFriendlyName,
    });
    showAddForm = false;
  }

  function startEdit(listener: DeviceListener) {
    editingId = listener.id;
    editFriendlyName = listener.friendlyName;
  }

  function cancelEdit() {
    editingId = null;
    editFriendlyName = '';
  }

  function submitEdit(id: string) {
    if (!editFriendlyName.trim()) return;
    sendWsCommand('obs.listeners.update', { id, friendlyName: editFriendlyName });
    editingId = null;
  }

  function handleDelete(id: string) {
    sendWsCommand('obs.listeners.delete', { id });
  }

  onMount(() => {
    sendWsCommand('obs.listeners.list');
  });
</script>

<svelte:head>
  <title>{$_('obsDevices.title')} — Metocast</title>
</svelte:head>

<h1>{$_('obsDevices.title')}</h1>

<!-- ── Available Devices Panel ─────────────────────────────────────────────── -->
<section class="panel">
  <div class="panel-header">
    <h2>{$_('obsDevices.availableDevices')}</h2>
    {#if $obsAvailableDevices}
      <span class="scan-time">
        {new Date($obsAvailableDevices.scannedAt).toLocaleTimeString()}
      </span>
    {/if}
    <button class="btn-secondary" onclick={handleScanNow} disabled={!isObsConnected}>
      {$_('obsDevices.scanNow')}
    </button>
  </div>

  {#if !isObsConnected}
    <p class="info-banner">{$_('obsDevices.notConnected')}</p>
  {:else if !$obsAvailableDevices}
    <p class="muted">{$_('obsDevices.loadingObs')}</p>
  {:else}
    <div class="device-grid">
      <div class="device-category">
        <h3>{$_('obsDevices.categories.display')}</h3>
        {#if $obsAvailableDevices.displays.length === 0}
          <p class="muted">—</p>
        {:else}
          <ul>
            {#each $obsAvailableDevices.displays as d (d.itemValue)}
              <li>{d.itemName}</li>
            {/each}
          </ul>
        {/if}
      </div>
      <div class="device-category">
        <h3>{$_('obsDevices.categories.audioInput')}</h3>
        {#if $obsAvailableDevices.audioInputs.length === 0}
          <p class="muted">—</p>
        {:else}
          <ul>
            {#each $obsAvailableDevices.audioInputs as d (d.itemValue)}
              <li>{d.itemName}</li>
            {/each}
          </ul>
        {/if}
      </div>
      <div class="device-category">
        <h3>{$_('obsDevices.categories.audioOutput')}</h3>
        {#if $obsAvailableDevices.audioOutputs.length === 0}
          <p class="muted">—</p>
        {:else}
          <ul>
            {#each $obsAvailableDevices.audioOutputs as d (d.itemValue)}
              <li>{d.itemName}</li>
            {/each}
          </ul>
        {/if}
      </div>
      <div class="device-category">
        <h3>{$_('obsDevices.categories.videoInput')}</h3>
        {#if $obsAvailableDevices.videoInputs.length === 0}
          <p class="muted">—</p>
        {:else}
          <ul>
            {#each $obsAvailableDevices.videoInputs as d (d.itemValue)}
              <li>{d.itemName}</li>
            {/each}
          </ul>
        {/if}
      </div>
      <div class="device-category">
        <h3>{$_('obsDevices.categories.captureCard')}</h3>
        {#if $obsAvailableDevices.captureCards.length === 0}
          <p class="muted">—</p>
        {:else}
          <ul>
            {#each $obsAvailableDevices.captureCards as d (d.itemValue)}
              <li>{d.itemName}</li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>
  {/if}
</section>

<!-- ── Listeners Panel ─────────────────────────────────────────────────────── -->
<section class="panel">
  <div class="panel-header">
    <h2>{$_('obsDevices.listeners')}</h2>
    <button class="btn-primary" onclick={openAddForm}>
      {$_('obsDevices.addListener')}
    </button>
  </div>

  {#if showAddForm}
    <form class="add-form" onsubmit={handleAddSubmit}>
      <div class="form-row">
        <label for="add-category">{$_('obsDevices.listenerForm.category')}</label>
        <select id="add-category" bind:value={addCategory} onchange={handleCategoryChange} required>
          <option value="">—</option>
          <option value="display">{$_('obsDevices.categories.display')}</option>
          <option value="audio_input">{$_('obsDevices.categories.audioInput')}</option>
          <option value="audio_output">{$_('obsDevices.categories.audioOutput')}</option>
          <option value="video_input">{$_('obsDevices.categories.videoInput')}</option>
          <option value="capture_card">{$_('obsDevices.categories.captureCard')}</option>
        </select>
      </div>
      {#if addCategory && $obsAvailableDevices}
        {@const devices = devicesForCategory($obsAvailableDevices, addCategory)}
        <div class="form-row">
          <label for="add-device">{$_('obsDevices.listenerForm.device')}</label>
          <select id="add-device" value={addDeviceValue} onchange={handleDeviceSelect} required>
            <option value="">—</option>
            {#each devices as d (d.itemValue)}
              <option value={d.itemValue}>{d.itemName}</option>
            {/each}
          </select>
        </div>
      {/if}
      <div class="form-row">
        <label for="add-name">{$_('obsDevices.listenerForm.name')}</label>
        <input id="add-name" type="text" bind:value={addFriendlyName} required />
      </div>
      <div class="form-actions">
        <button type="submit" class="btn-primary">{$_('obsDevices.listenerForm.save')}</button>
        <button type="button" class="btn-secondary" onclick={() => (showAddForm = false)}>
          {$_('obsDevices.listenerForm.cancel')}
        </button>
      </div>
    </form>
  {/if}

  {#if $obsDeviceListeners.length === 0}
    <p class="muted">No listeners configured.</p>
  {:else}
    <table class="listeners-table">
      <thead>
        <tr>
          <th>{$_('obsDevices.listenerForm.name')}</th>
          <th>{$_('obsDevices.listenerForm.category')}</th>
          <th>{$_('obsDevices.listenerForm.device')}</th>
          <th>Status</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each $obsDeviceListeners as listener (listener.id)}
          {@const status = statusForListener(listener.id, $obsDeviceListenerStatuses)}
          <tr>
            <td>
              {#if editingId === listener.id}
                <input
                  type="text"
                  class="inline-edit"
                  bind:value={editFriendlyName}
                  onkeydown={(e) => {
                    if (e.key === 'Enter') submitEdit(listener.id);
                    if (e.key === 'Escape') cancelEdit();
                  }}
                />
              {:else}
                {listener.friendlyName}
              {/if}
            </td>
            <td>{categoryLabel(listener.category)}</td>
            <td>{listener.deviceItemName}</td>
            <td>
              {#if status}
                <span
                  class="status-badge"
                  class:available={status.available}
                  class:unavailable={!status.available}
                >
                  {status.available
                    ? $_('obsDevices.status.available')
                    : $_('obsDevices.status.unavailable')}
                </span>
              {:else}
                <span class="status-badge">—</span>
              {/if}
            </td>
            <td class="actions">
              {#if editingId === listener.id}
                <button class="btn-xs btn-primary" onclick={() => submitEdit(listener.id)}>
                  {$_('obsDevices.listenerForm.save')}
                </button>
                <button class="btn-xs btn-secondary" onclick={cancelEdit}>
                  {$_('obsDevices.listenerForm.cancel')}
                </button>
              {:else}
                <button class="btn-xs btn-secondary" onclick={() => startEdit(listener)}>
                  Edit
                </button>
                <button class="btn-xs btn-danger" onclick={() => handleDelete(listener.id)}>
                  Delete
                </button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</section>

<style>
  h1 {
    margin: 0 0 1.5rem;
    font-size: 1.5rem;
    font-weight: 700;
    color: var(--text-primary);
  }

  .panel {
    background: var(--glass-card-bg);
    border: 1px solid var(--glass-border);
    border-radius: 12px;
    padding: 1.25rem;
    margin-bottom: 1.5rem;
  }

  .panel-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 1rem;
  }

  .panel-header h2 {
    margin: 0;
    flex: 1;
    font-size: 1rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .scan-time {
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .info-banner {
    padding: 0.75rem 1rem;
    background: var(--glass-card-bg);
    border: 1px solid var(--glass-border);
    border-radius: 8px;
    color: var(--text-secondary);
    font-size: 0.875rem;
    margin: 0;
  }

  .muted {
    color: var(--text-secondary);
    font-size: 0.875rem;
    margin: 0;
  }

  .device-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 1rem;
  }

  .device-category h3 {
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--text-secondary);
    margin: 0 0 0.5rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .device-category ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .device-category li {
    font-size: 0.875rem;
    color: var(--text-primary);
    padding: 0.25rem 0;
    border-bottom: 1px solid var(--glass-border);
  }

  .add-form {
    background: var(--glass-card-bg);
    border: 1px solid var(--glass-border);
    border-radius: 8px;
    padding: 1rem;
    margin-bottom: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .form-row {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .form-row label {
    font-size: 0.8125rem;
    font-weight: 500;
    color: var(--text-secondary);
  }

  .form-row input,
  .form-row select {
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--glass-border);
    border-radius: 6px;
    background: var(--input-bg, var(--glass-card-bg));
    color: var(--text-primary);
    font-size: 0.875rem;
  }

  .form-actions {
    display: flex;
    gap: 0.5rem;
  }

  .listeners-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.875rem;
  }

  .listeners-table th {
    text-align: left;
    padding: 0.5rem 0.75rem;
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--glass-border);
  }

  .listeners-table td {
    padding: 0.625rem 0.75rem;
    border-bottom: 1px solid var(--glass-border);
    color: var(--text-primary);
  }

  .inline-edit {
    padding: 0.25rem 0.5rem;
    border: 1px solid var(--glass-border);
    border-radius: 4px;
    background: var(--input-bg, var(--glass-card-bg));
    color: var(--text-primary);
    font-size: 0.875rem;
    width: 100%;
  }

  .status-badge {
    display: inline-block;
    padding: 0.2rem 0.5rem;
    border-radius: 4px;
    font-size: 0.75rem;
    font-weight: 600;
    background: var(--glass-card-bg);
    border: 1px solid var(--glass-border);
    color: var(--text-secondary);
  }

  .status-badge.available {
    background: color-mix(in srgb, green 15%, transparent);
    border-color: color-mix(in srgb, green 30%, transparent);
    color: green;
  }

  .status-badge.unavailable {
    background: color-mix(in srgb, red 15%, transparent);
    border-color: color-mix(in srgb, red 30%, transparent);
    color: red;
  }

  .actions {
    display: flex;
    gap: 0.375rem;
  }

  .btn-primary {
    padding: 0.5rem 1rem;
    border: none;
    border-radius: 6px;
    background: var(--accent);
    color: white;
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
  }

  .btn-primary:hover {
    opacity: 0.9;
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-secondary {
    padding: 0.5rem 1rem;
    border: 1px solid var(--glass-border);
    border-radius: 6px;
    background: transparent;
    color: var(--text-primary);
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
  }

  .btn-secondary:hover {
    background: var(--nav-item-hover);
  }

  .btn-secondary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-xs {
    padding: 0.25rem 0.625rem;
    font-size: 0.75rem;
  }

  .btn-danger {
    padding: 0.5rem 1rem;
    border: 1px solid color-mix(in srgb, red 40%, transparent);
    border-radius: 6px;
    background: color-mix(in srgb, red 10%, transparent);
    color: red;
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
  }

  .btn-danger:hover {
    background: color-mix(in srgb, red 20%, transparent);
  }
</style>
