<script lang="ts">
  import {
    obsConfig,
    obsState,
    vmixConfig,
    vmixState,
    atemConfig,
    atemState,
    broadlinkConfig,
    broadlinkState,
    youtubeConfig,
    youtubeState,
    facebookConfig,
    facebookState,
    discordConfig,
    szentirasConfig,
    discordState,
    szentirasState,
  } from '$lib/stores/connectors.js';
  import { authToken } from '$lib/stores/server-url.js';
  import { appMode } from '$lib/stores/mode.js';
  import { findConnector } from '$lib/connectors/registry.js';
  import ConnectorStatusBadge from './ConnectorStatusBadge.svelte';
  import type { BaseConfig, ConnectorState } from '$lib/connectors/types.js';
  import {
    fetchDevices,
    fetchCommands,
    sendCommand,
    type BroadlinkDevice,
    type BroadlinkCommand,
  } from '@metocast/core-client';

  const CATEGORY_LABELS: Record<string, string> = {
    projector: 'Projector',
    screen: 'Screen',
    hvac: 'HVAC',
    lighting: 'Lighting',
    audio: 'Audio',
    other: 'Other',
  };
  const CATEGORY_ORDER = ['projector', 'screen', 'lighting', 'audio', 'hvac', 'other'];

  interface Props {
    connectorId: string;
    compact?: boolean;
  }

  let { connectorId, compact = true }: Props = $props();

  const def = $derived(findConnector(connectorId));

  const config = $derived.by((): BaseConfig => {
    if (connectorId === 'obs') return $obsConfig;
    if (connectorId === 'vmix') return $vmixConfig;
    if (connectorId === 'atem') return $atemConfig;
    if (connectorId === 'broadlink') return $broadlinkConfig;
    if (connectorId === 'youtube') return $youtubeConfig;
    if (connectorId === 'facebook') return $facebookConfig;
    if (connectorId === 'discord') return $discordConfig;
    if (connectorId === 'szentiras') return $szentirasConfig;
    return { enabled: false };
  });

  const connState = $derived.by((): ConnectorState => {
    if (connectorId === 'obs') return $obsState;
    if (connectorId === 'vmix') return $vmixState;
    if (connectorId === 'atem') return $atemState;
    if (connectorId === 'broadlink') return $broadlinkState;
    if (connectorId === 'youtube') return $youtubeState;
    if (connectorId === 'facebook') return $facebookState;
    if (connectorId === 'discord') return $discordState;
    if (connectorId === 'szentiras') return $szentirasState;
    return { connection: 'disconnected' };
  });

  const isConfigured = $derived(def ? def.isConfigured(config) : false);
  const isConnected = $derived(connState.connection === 'connected');

  const hasFlags = $derived(
    isConnected &&
      (!!def?.capabilities.streaming || !!def?.capabilities.recording || !!def?.capabilities.live),
  );

  // ── Broadlink command panel ───────────────────────────────────────────────

  let blDevices = $state<BroadlinkDevice[]>([]);
  let blCommands = $state<BroadlinkCommand[]>([]);
  let blSelectedId = $state<string | null>(null);
  let blLoading = $state(false);
  let blSendingId = $state<string | null>(null);
  let blSendError = $state('');

  $effect(() => {
    if (connectorId === 'broadlink' && $authToken && $broadlinkConfig.enabled) {
      loadBroadlink();
    }
  });

  async function loadBroadlink() {
    blLoading = true;
    try {
      blDevices = await fetchDevices();
      const first = blDevices.find((d) => d.isDefault) ?? blDevices.find(() => true);
      if (first && blSelectedId === null) blSelectedId = first.id;
      blCommands = await fetchCommands(blSelectedId ?? undefined);
    } catch {
      // server may not be ready yet
    } finally {
      blLoading = false;
    }
  }

  async function onDeviceChange(id: string) {
    blSelectedId = id;
    try {
      blCommands = await fetchCommands(id);
    } catch {
      blCommands = [];
    }
  }

  async function blSend(cmd: BroadlinkCommand) {
    blSendingId = cmd.id;
    blSendError = '';
    try {
      await sendCommand(cmd.id);
    } catch (e) {
      blSendError = String(e);
    } finally {
      blSendingId = null;
    }
  }

  const blGrouped = $derived.by(() => {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local grouping inside a derived, never mutated after
    const map = new Map<string, BroadlinkCommand[]>();
    for (const cmd of blCommands) {
      const cat = cmd.category || 'other';
      if (!map.has(cat)) map.set(cat, []);
      map.get(cat)!.push(cmd);
    }
    return CATEGORY_ORDER.filter((cat) => map.has(cat)).map((cat) => ({
      category: cat,
      label: CATEGORY_LABELS[cat] ?? cat,
      cmds: map.get(cat)!,
    }));
  });
</script>

{#if def && (isConfigured || $appMode === 'client')}
  <div
    class="widget"
    class:widget--compact={compact}
    class:widget--broadlink={connectorId === 'broadlink'}
  >
    <ConnectorStatusBadge name={def.name} status={connState.connection} />

    {#if hasFlags}
      <div class="flag-row">
        {#if def.capabilities.streaming}
          <span class="flag" class:flag--active={connState.isStreaming}>
            {connState.isStreaming ? 'Streaming' : 'Not Streaming'}
          </span>
        {/if}
        {#if def.capabilities.recording}
          <span class="flag" class:flag--active={connState.isRecording}>
            {connState.isRecording ? 'Recording' : 'Not Recording'}
          </span>
        {/if}
        {#if def.capabilities.live}
          <span class="flag" class:flag--active={connState.isLive}>
            {connState.isLive ? 'Live' : 'Not Live'}
          </span>
        {/if}
      </div>
    {/if}

    {#if connectorId === 'broadlink'}
      {#if blDevices.length > 1}
        <select
          class="bl-device-select"
          value={blSelectedId}
          onchange={(e) => onDeviceChange((e.currentTarget as HTMLSelectElement).value)}
          aria-label="Select device"
        >
          {#each blDevices as dev (dev.id)}
            <option value={dev.id}>{dev.name}</option>
          {/each}
        </select>
      {:else if blDevices.length === 1}
        <span class="bl-device-name">{blDevices.at(0)?.name}</span>
      {/if}

      {#if blLoading}
        <p class="bl-hint">Loading…</p>
      {:else if blCommands.length === 0 && blDevices.length === 0}
        <p class="bl-hint">No devices. <a href="/settings">Add one →</a></p>
      {:else if blCommands.length === 0}
        <p class="bl-hint">No commands. <a href="/rf-ir">Add one →</a></p>
      {:else}
        <div class="bl-categories">
          {#each blGrouped as group (group.category)}
            <div class="bl-category">
              <span class="bl-category-label">{group.label}</span>
              <div class="bl-cmd-row">
                {#each group.cmds as cmd (cmd.id)}
                  <button
                    class="bl-cmd-btn"
                    class:bl-cmd-btn--sending={blSendingId === cmd.id}
                    onclick={() => blSend(cmd)}
                    disabled={blSendingId !== null}
                    title={cmd.name}
                  >
                    {cmd.name}
                  </button>
                {/each}
              </div>
            </div>
          {/each}
        </div>
      {/if}

      {#if blSendError}
        <p class="bl-error" role="alert">{blSendError}</p>
      {/if}

      <a href="/rf-ir" class="bl-manage-link">Manage →</a>
    {/if}
  </div>
{/if}

<style>
  .widget {
    padding: 0.875rem 1rem;
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    align-items: flex-start;
  }

  .widget--broadlink {
    align-items: stretch;
  }

  .flag-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
  }

  .flag {
    display: inline-flex;
    align-items: center;
    padding: 0.15rem 0.5rem;
    border-radius: 9999px;
    font-size: 0.75rem;
    font-weight: 500;
    background: var(--content-bg);
    color: var(--text-secondary);
  }

  .flag--active {
    background: var(--status-ok-bg);
    color: var(--status-ok-text);
  }

  /* ── Broadlink ── */

  .bl-device-select {
    font-size: 0.8125rem;
    padding: 0.25rem 0.5rem;
    border: 1px solid var(--input-border);
    border-radius: 0.3rem;
    background: var(--input-bg);
    color: var(--text-primary);
    cursor: pointer;
    width: 100%;
  }

  .bl-device-name {
    font-size: 0.8125rem;
    color: var(--text-secondary);
  }

  .bl-hint {
    font-size: 0.8125rem;
    color: var(--text-secondary);
    margin: 0;
  }

  .bl-categories {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    width: 100%;
  }

  .bl-category {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .bl-category-label {
    font-size: 0.6875rem;
    font-weight: 600;
    color: var(--text-tertiary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .bl-cmd-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
  }

  .bl-cmd-btn {
    padding: 0.3rem 0.65rem;
    font-size: 0.8125rem;
    border: 1px solid var(--border);
    border-radius: 0.3rem;
    background: var(--glass-card-bg);
    color: var(--text-primary);
    cursor: pointer;
    white-space: nowrap;
  }

  .bl-cmd-btn:hover:not(:disabled) {
    background: var(--nav-item-hover);
    border-color: var(--border);
  }

  .bl-cmd-btn--sending {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }

  .bl-cmd-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .bl-error {
    font-size: 0.8125rem;
    color: var(--status-err-text);
    margin: 0;
  }

  .bl-manage-link {
    font-size: 0.8125rem;
    color: var(--text-secondary);
    text-decoration: none;
    align-self: flex-end;
  }

  .bl-manage-link:hover {
    color: var(--text-primary);
    text-decoration: underline;
  }
</style>
