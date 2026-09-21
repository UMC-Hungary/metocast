<script lang="ts">
  import { onMount } from 'svelte';
  import {
    fetchCommands,
    removeCommand,
    sendCommand,
    type BroadlinkCommand,
    type BroadlinkDevice,
  } from '@metocast/core-client';
  import LearnDialog from './LearnDialog.svelte';
  import CodeEntryDialog from './CodeEntryDialog.svelte';
  import ImportDialog from './ImportDialog.svelte';

  interface Props {
    device: BroadlinkDevice;
    onCommandAdded?: () => void;
  }

  let { device, onCommandAdded }: Props = $props();

  let commands = $state<BroadlinkCommand[]>([]);
  let loading = $state(false);
  let error = $state('');
  let showLearn = $state(false);
  let showCodeEntry = $state(false);
  let showImport = $state(false);

  onMount(async () => {
    await reload();
  });

  async function reload() {
    loading = true;
    error = '';
    try {
      commands = await fetchCommands(device.id);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function handleRemove(id: string) {
    try {
      await removeCommand(id);
      commands = commands.filter((c) => c.id !== id);
    } catch (e) {
      error = String(e);
    }
  }

  async function handleSend(id: string) {
    try {
      await sendCommand(id);
    } catch (e) {
      error = String(e);
    }
  }

  async function handleLearnDone() {
    showLearn = false;
    await reload();
    onCommandAdded?.();
  }

  async function handleCodeEntryDone() {
    showCodeEntry = false;
    await reload();
    onCommandAdded?.();
  }
</script>

<div>
  <div style="display:flex; gap:0.5rem; margin-bottom:0.5rem;">
    <button onclick={() => (showLearn = true)}>Learn IR/RF</button>
    <button onclick={() => (showCodeEntry = true)}>Enter Code Manually</button>
    <button onclick={() => (showImport = true)}>Import Presets</button>
  </div>

  {#if loading}
    <p>Loading commands…</p>
  {:else if error}
    <p style="color:var(--status-err-text)">{error}</p>
  {:else if commands.length === 0}
    <p>No commands yet.</p>
  {:else}
    <ul style="list-style:none; padding:0; margin:0;">
      {#each commands as cmd (cmd.id)}
        <li
          style="display:flex; justify-content:space-between; align-items:center; padding:0.25rem 0; border-bottom:1px solid var(--border);"
        >
          <span><strong>{cmd.name}</strong> <em>({cmd.category})</em> [{cmd.codeType}]</span>
          <div style="display:flex; gap:0.25rem;">
            <button onclick={() => handleSend(cmd.id)}>Send</button>
            <button onclick={() => handleRemove(cmd.id)}>Remove</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  {#if showLearn}
    <LearnDialog {device} onDone={handleLearnDone} onCancel={() => (showLearn = false)} />
  {/if}

  {#if showCodeEntry}
    <CodeEntryDialog
      {device}
      onDone={handleCodeEntryDone}
      onCancel={() => (showCodeEntry = false)}
    />
  {/if}

  {#if showImport}
    <ImportDialog
      {device}
      onDone={async () => {
        showImport = false;
        await reload();
        onCommandAdded?.();
      }}
      onCancel={() => (showImport = false)}
    />
  {/if}
</div>
